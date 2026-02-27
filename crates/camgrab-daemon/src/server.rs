use anyhow::{Context, Result};
use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    middleware,
    response::{IntoResponse, Response},
    routing::{delete, get, post},
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::scheduler::{JobAction, ScheduledJob, Scheduler};

const DEFAULT_BIND_ADDRESS: &str = "127.0.0.1:9847";
const MAX_EVENT_BUFFER_SIZE: usize = 1000;

#[derive(Debug, Error)]
pub enum DaemonError {
    #[error("Authentication failed")]
    AuthenticationFailed,

    #[error("Camera not found: {0}")]
    CameraNotFound(String),

    #[error("Invalid request: {0}")]
    InvalidRequest(String),

    #[error("Internal server error: {0}")]
    InternalError(String),

    #[error("Job not found: {0}")]
    JobNotFound(Uuid),
}

impl IntoResponse for DaemonError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            DaemonError::AuthenticationFailed => (StatusCode::UNAUTHORIZED, self.to_string()),
            DaemonError::CameraNotFound(_) => (StatusCode::NOT_FOUND, self.to_string()),
            DaemonError::InvalidRequest(_) => (StatusCode::BAD_REQUEST, self.to_string()),
            DaemonError::JobNotFound(_) => (StatusCode::NOT_FOUND, self.to_string()),
            DaemonError::InternalError(_) => (StatusCode::INTERNAL_SERVER_ERROR, self.to_string()),
        };

        let body = Json(serde_json::json!({
            "error": message,
        }));

        (status, body).into_response()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonConfig {
    pub bind_address: String,
    pub auth_token: Option<String>,
    pub config_path: Option<PathBuf>,
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            bind_address: DEFAULT_BIND_ADDRESS.to_string(),
            auth_token: None,
            config_path: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    pub pid: u32,
    pub port: u16,
    pub token: Option<String>,
    pub started_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MotionEvent {
    pub id: Uuid,
    pub camera: String,
    pub timestamp: DateTime<Utc>,
    pub confidence: f32,
    pub region: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CameraStatus {
    pub name: String,
    pub connected: bool,
    pub watching: bool,
    pub last_seen: Option<DateTime<Utc>>,
}

struct DaemonState {
    config: DaemonConfig,
    started_at: DateTime<Utc>,
    scheduler: Scheduler,
    cameras: Vec<CameraStatus>,
    motion_events: VecDeque<MotionEvent>,
    #[allow(dead_code)]
    shutdown_tx: tokio::sync::watch::Sender<bool>,
}

impl DaemonState {
    fn new(config: DaemonConfig, shutdown_tx: tokio::sync::watch::Sender<bool>) -> Self {
        Self {
            config,
            started_at: Utc::now(),
            scheduler: Scheduler::new(),
            cameras: Vec::new(),
            motion_events: VecDeque::with_capacity(MAX_EVENT_BUFFER_SIZE),
            shutdown_tx,
        }
    }

    #[allow(dead_code)]
    fn add_motion_event(&mut self, event: MotionEvent) {
        if self.motion_events.len() >= MAX_EVENT_BUFFER_SIZE {
            self.motion_events.pop_front();
        }
        self.motion_events.push_back(event);
    }

    fn get_camera(&self, name: &str) -> Option<&CameraStatus> {
        self.cameras.iter().find(|c| c.name == name)
    }

    fn get_camera_mut(&mut self, name: &str) -> Option<&mut CameraStatus> {
        self.cameras.iter_mut().find(|c| c.name == name)
    }
}

pub struct Daemon {
    config: DaemonConfig,
    state: Arc<RwLock<DaemonState>>,
    shutdown_tx: tokio::sync::watch::Sender<bool>,
    shutdown_rx: tokio::sync::watch::Receiver<bool>,
}

impl Daemon {
    pub fn new(config: DaemonConfig) -> Result<Self> {
        let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
        let state = Arc::new(RwLock::new(DaemonState::new(
            config.clone(),
            shutdown_tx.clone(),
        )));

        Ok(Self {
            config,
            state,
            shutdown_tx,
            shutdown_rx,
        })
    }

    pub async fn run(&self) -> Result<()> {
        info!("Starting camgrab daemon on {}", self.config.bind_address);

        // Write session file
        if let Err(e) = self.write_session_file().await {
            warn!("Failed to write session file: {}", e);
        }

        // Build HTTP API router
        let app = self.build_router();

        // Parse bind address
        let addr: SocketAddr = self
            .config
            .bind_address
            .parse()
            .context("Failed to parse bind address")?;

        // Start scheduler in background with config path
        let scheduler_state = self.state.clone();
        let scheduler_shutdown = self.shutdown_rx.clone();
        let config_path = self
            .config
            .config_path
            .clone()
            .unwrap_or_else(camgrab_core::config::default_config_path);
        let scheduler_handle = tokio::spawn(async move {
            let mut scheduler = {
                let _state = scheduler_state.write().await;
                Scheduler::with_config_path(config_path)
            };
            scheduler.run(scheduler_shutdown).await;
        });

        // Setup signal handlers
        let shutdown_signal = Self::shutdown_signal();

        // Start HTTP server
        info!("Daemon listening on {}", addr);
        let listener = tokio::net::TcpListener::bind(&addr)
            .await
            .context("Failed to bind to address")?;

        let server = axum::serve(listener, app);

        // Wait for shutdown signal
        tokio::select! {
            result = server => {
                if let Err(e) = result {
                    error!("Server error: {}", e);
                }
            }
            _ = shutdown_signal => {
                info!("Received shutdown signal");
            }
        }

        // Initiate graceful shutdown
        info!("Shutting down daemon...");
        let _ = self.shutdown_tx.send(true);

        // Wait for scheduler to finish
        let _ = tokio::time::timeout(tokio::time::Duration::from_secs(5), scheduler_handle).await;

        // Clean up session file
        if let Err(e) = self.remove_session_file().await {
            warn!("Failed to remove session file: {}", e);
        }

        info!("Daemon stopped");
        Ok(())
    }

    fn build_router(&self) -> Router {
        let state = self.state.clone();

        Router::new()
            .route("/api/status", get(status_handler))
            .route("/api/cameras", get(cameras_handler))
            .route("/api/snap/:camera", post(snap_handler))
            .route("/api/watch/:camera/start", post(watch_start_handler))
            .route("/api/watch/:camera/stop", post(watch_stop_handler))
            .route("/api/events", get(events_handler))
            .route("/api/schedule", get(schedule_list_handler))
            .route("/api/schedule", post(schedule_add_handler))
            .route("/api/schedule/:id", delete(schedule_remove_handler))
            .layer(middleware::from_fn_with_state(
                state.clone(),
                auth_middleware,
            ))
            .with_state(state)
    }

    async fn shutdown_signal() {
        use tokio::signal;

        let ctrl_c = async {
            signal::ctrl_c()
                .await
                .expect("Failed to install Ctrl+C handler");
        };

        #[cfg(unix)]
        let terminate = async {
            signal::unix::signal(signal::unix::SignalKind::terminate())
                .expect("Failed to install SIGTERM handler")
                .recv()
                .await;
        };

        #[cfg(not(unix))]
        let terminate = std::future::pending::<()>();

        tokio::select! {
            _ = ctrl_c => {},
            _ = terminate => {},
        }

        info!("Shutdown signal received");
    }

    async fn write_session_file(&self) -> Result<()> {
        let home = dirs::home_dir().context("Failed to get home directory")?;
        let config_dir = home.join(".camgrab");
        tokio::fs::create_dir_all(&config_dir)
            .await
            .context("Failed to create config directory")?;

        let session_file = config_dir.join("daemon.json");

        let addr: SocketAddr = self.config.bind_address.parse()?;
        let session_info = SessionInfo {
            pid: std::process::id(),
            port: addr.port(),
            token: self.config.auth_token.clone(),
            started_at: Utc::now(),
        };

        let json = serde_json::to_string_pretty(&session_info)?;
        tokio::fs::write(&session_file, json)
            .await
            .context("Failed to write session file")?;

        info!("Session file written to {}", session_file.display());
        Ok(())
    }

    async fn remove_session_file(&self) -> Result<()> {
        let home = dirs::home_dir().context("Failed to get home directory")?;
        let session_file = home.join(".camgrab").join("daemon.json");

        if tokio::fs::try_exists(&session_file).await? {
            tokio::fs::remove_file(&session_file)
                .await
                .context("Failed to remove session file")?;
            info!("Session file removed");
        }

        Ok(())
    }
}

// Middleware for bearer token authentication
async fn auth_middleware(
    State(state): State<Arc<RwLock<DaemonState>>>,
    headers: HeaderMap,
    request: axum::extract::Request,
    next: middleware::Next,
) -> Result<Response, DaemonError> {
    let state_guard = state.read().await;

    // If no token is configured, skip authentication
    if state_guard.config.auth_token.is_none() {
        drop(state_guard);
        return Ok(next.run(request).await);
    }

    // Extract bearer token from Authorization header
    let auth_header = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok());

    if let Some(auth_value) = auth_header {
        if let Some(token) = auth_value.strip_prefix("Bearer ") {
            if Some(token) == state_guard.config.auth_token.as_deref() {
                drop(state_guard);
                return Ok(next.run(request).await);
            }
        }
    }

    Err(DaemonError::AuthenticationFailed)
}

// API Handlers

#[derive(Debug, Serialize)]
struct StatusResponse {
    status: String,
    uptime_seconds: i64,
    started_at: DateTime<Utc>,
    active_cameras: usize,
    scheduled_jobs: usize,
}

async fn status_handler(
    State(state): State<Arc<RwLock<DaemonState>>>,
) -> Result<Json<StatusResponse>, DaemonError> {
    let state = state.read().await;
    let uptime = (Utc::now() - state.started_at).num_seconds();

    Ok(Json(StatusResponse {
        status: "running".to_string(),
        uptime_seconds: uptime,
        started_at: state.started_at,
        active_cameras: state.cameras.iter().filter(|c| c.connected).count(),
        scheduled_jobs: state.scheduler.list_jobs().len(),
    }))
}

async fn cameras_handler(
    State(state): State<Arc<RwLock<DaemonState>>>,
) -> Result<Json<Vec<CameraStatus>>, DaemonError> {
    let state = state.read().await;
    Ok(Json(state.cameras.clone()))
}

#[derive(Debug, Deserialize)]
struct SnapRequest {
    output_dir: Option<PathBuf>,
    format: Option<String>,
}

#[derive(Debug, Serialize)]
struct SnapResponse {
    camera: String,
    status: String,
    message: String,
}

async fn snap_handler(
    State(state): State<Arc<RwLock<DaemonState>>>,
    Path(camera): Path<String>,
    Json(payload): Json<SnapRequest>,
) -> Result<Json<SnapResponse>, DaemonError> {
    let state = state.read().await;

    // Verify camera exists
    state
        .get_camera(&camera)
        .ok_or_else(|| DaemonError::CameraNotFound(camera.clone()))?;

    info!(
        "Triggering snapshot for camera '{}' (output_dir: {:?}, format: {:?})",
        camera, payload.output_dir, payload.format
    );

    // Resolve config path and load camera config
    let config_path = state
        .config
        .config_path
        .clone()
        .unwrap_or_else(camgrab_core::config::default_config_path);
    let app_config = camgrab_core::config::load(&config_path)
        .map_err(|e| DaemonError::InternalError(format!("Config load error: {}", e)))?;
    let cam_config = camgrab_core::config::find_camera(&app_config, &camera)
        .ok_or_else(|| DaemonError::CameraNotFound(camera.clone()))?;

    // Build output path
    let output_dir = payload.output_dir.unwrap_or_else(std::env::temp_dir);
    let ext = payload.format.as_deref().unwrap_or("jpg");
    let filename = format!("{}-{}.{}", camera, Utc::now().format("%Y%m%d-%H%M%S"), ext);
    let output_path = output_dir.join(&filename);

    // Connect and snap
    let cam = camgrab_core::camera::Camera::from_config(cam_config);
    let mut client = camgrab_core::rtsp::client::RtspClient::new(&cam)
        .map_err(|e| DaemonError::InternalError(format!("RTSP client error: {}", e)))?;
    client
        .connect()
        .await
        .map_err(|e| DaemonError::InternalError(format!("Connection failed: {}", e)))?;
    let snap_result = client
        .snap(&output_path)
        .await
        .map_err(|e| DaemonError::InternalError(format!("Snapshot failed: {}", e)))?;

    info!(
        "Snapshot captured: {} ({} bytes)",
        snap_result.path.display(),
        snap_result.size_bytes
    );

    Ok(Json(SnapResponse {
        camera: camera.clone(),
        status: "success".to_string(),
        message: format!(
            "Snapshot saved to {} ({} bytes)",
            snap_result.path.display(),
            snap_result.size_bytes
        ),
    }))
}

async fn watch_start_handler(
    State(state): State<Arc<RwLock<DaemonState>>>,
    Path(camera): Path<String>,
) -> Result<Json<serde_json::Value>, DaemonError> {
    let mut state = state.write().await;

    // Check camera exists and isn't already watching
    let camera_status = state
        .get_camera(&camera)
        .ok_or_else(|| DaemonError::CameraNotFound(camera.clone()))?;

    if camera_status.watching {
        return Ok(Json(serde_json::json!({
            "status": "already_watching",
            "camera": camera,
        })));
    }

    // Validate camera exists in config and can connect
    let config_path = state
        .config
        .config_path
        .clone()
        .unwrap_or_else(camgrab_core::config::default_config_path);
    let app_config = camgrab_core::config::load(&config_path)
        .map_err(|e| DaemonError::InternalError(format!("Config load error: {}", e)))?;
    let cam_config = camgrab_core::config::find_camera(&app_config, &camera)
        .ok_or_else(|| DaemonError::CameraNotFound(camera.clone()))?;

    // Test RTSP connection before marking as watching
    let cam = camgrab_core::camera::Camera::from_config(cam_config);
    let mut client = camgrab_core::rtsp::client::RtspClient::new(&cam)
        .map_err(|e| DaemonError::InternalError(format!("RTSP client error: {}", e)))?;
    client
        .connect()
        .await
        .map_err(|e| DaemonError::InternalError(format!("Connection failed: {}", e)))?;
    client.disconnect().await;

    // Now set watching flag
    let camera_status = state
        .get_camera_mut(&camera)
        .ok_or_else(|| DaemonError::CameraNotFound(camera.clone()))?;
    camera_status.watching = true;
    info!("Started motion monitoring for camera '{}'", camera);

    // NOTE: Real continuous motion detection requires a persistent streaming session.
    // The current implementation validates connectivity and sets the watching flag.
    // A background streaming task would need to be spawned for true real-time detection.

    Ok(Json(serde_json::json!({
        "status": "started",
        "camera": camera,
    })))
}

async fn watch_stop_handler(
    State(state): State<Arc<RwLock<DaemonState>>>,
    Path(camera): Path<String>,
) -> Result<Json<serde_json::Value>, DaemonError> {
    let mut state = state.write().await;

    let camera_status = state
        .get_camera_mut(&camera)
        .ok_or_else(|| DaemonError::CameraNotFound(camera.clone()))?;

    if !camera_status.watching {
        return Ok(Json(serde_json::json!({
            "status": "not_watching",
            "camera": camera,
        })));
    }

    camera_status.watching = false;
    info!("Stopped motion monitoring for camera '{}'", camera);

    // NOTE: When real continuous streaming is implemented, this would
    // signal the background streaming task to stop.

    Ok(Json(serde_json::json!({
        "status": "stopped",
        "camera": camera,
    })))
}

#[derive(Debug, Serialize)]
struct EventsResponse {
    events: Vec<MotionEvent>,
    count: usize,
}

async fn events_handler(
    State(state): State<Arc<RwLock<DaemonState>>>,
) -> Result<Json<EventsResponse>, DaemonError> {
    let state = state.read().await;
    let events: Vec<MotionEvent> = state.motion_events.iter().cloned().collect();

    Ok(Json(EventsResponse {
        count: events.len(),
        events,
    }))
}

async fn schedule_list_handler(
    State(state): State<Arc<RwLock<DaemonState>>>,
) -> Result<Json<Vec<ScheduledJob>>, DaemonError> {
    let state = state.read().await;
    let jobs = state.scheduler.list_jobs().into_iter().cloned().collect();

    Ok(Json(jobs))
}

#[derive(Debug, Deserialize)]
struct AddJobRequest {
    name: String,
    cron_expr: String,
    action: JobAction,
    camera: String,
}

#[derive(Debug, Serialize)]
struct AddJobResponse {
    id: Uuid,
    message: String,
}

async fn schedule_add_handler(
    State(state): State<Arc<RwLock<DaemonState>>>,
    Json(payload): Json<AddJobRequest>,
) -> Result<Json<AddJobResponse>, DaemonError> {
    let mut state = state.write().await;

    let job = ScheduledJob::new(
        payload.name,
        payload.cron_expr,
        payload.action,
        payload.camera,
    )
    .map_err(|e| DaemonError::InvalidRequest(e.to_string()))?;

    let id = state.scheduler.add_job(job);

    Ok(Json(AddJobResponse {
        id,
        message: format!("Job {} added successfully", id),
    }))
}

#[derive(Debug, Serialize)]
struct RemoveJobResponse {
    message: String,
}

async fn schedule_remove_handler(
    State(state): State<Arc<RwLock<DaemonState>>>,
    Path(id): Path<Uuid>,
) -> Result<Json<RemoveJobResponse>, DaemonError> {
    let mut state = state.write().await;

    if state.scheduler.remove_job(&id) {
        Ok(Json(RemoveJobResponse {
            message: format!("Job {} removed successfully", id),
        }))
    } else {
        Err(DaemonError::JobNotFound(id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_daemon_config_default() {
        let config = DaemonConfig::default();
        assert_eq!(config.bind_address, DEFAULT_BIND_ADDRESS);
        assert!(config.auth_token.is_none());
        assert!(config.config_path.is_none());
    }

    #[test]
    fn test_session_info_serialization() {
        let session = SessionInfo {
            pid: 12345,
            port: 9847,
            token: Some("test-token".to_string()),
            started_at: Utc::now(),
        };

        let json = serde_json::to_string(&session).unwrap();
        let deserialized: SessionInfo = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.pid, 12345);
        assert_eq!(deserialized.port, 9847);
        assert_eq!(deserialized.token, Some("test-token".to_string()));
    }

    #[test]
    fn test_motion_event_buffer() {
        let (shutdown_tx, _) = tokio::sync::watch::channel(false);
        let mut state = DaemonState::new(DaemonConfig::default(), shutdown_tx);

        // Add events up to capacity
        for i in 0..MAX_EVENT_BUFFER_SIZE {
            state.add_motion_event(MotionEvent {
                id: Uuid::new_v4(),
                camera: format!("camera{}", i),
                timestamp: Utc::now(),
                confidence: 0.95,
                region: None,
            });
        }

        assert_eq!(state.motion_events.len(), MAX_EVENT_BUFFER_SIZE);

        // Add one more - should evict the oldest
        state.add_motion_event(MotionEvent {
            id: Uuid::new_v4(),
            camera: "new_camera".to_string(),
            timestamp: Utc::now(),
            confidence: 0.95,
            region: None,
        });

        assert_eq!(state.motion_events.len(), MAX_EVENT_BUFFER_SIZE);
        assert_eq!(state.motion_events.back().unwrap().camera, "new_camera");
        assert_ne!(state.motion_events.front().unwrap().camera, "camera0");
    }

    #[test]
    fn test_camera_status() {
        let (shutdown_tx, _) = tokio::sync::watch::channel(false);
        let mut state = DaemonState::new(DaemonConfig::default(), shutdown_tx);

        state.cameras.push(CameraStatus {
            name: "lobby".to_string(),
            connected: true,
            watching: false,
            last_seen: Some(Utc::now()),
        });

        assert!(state.get_camera("lobby").is_some());
        assert!(state.get_camera("nonexistent").is_none());

        let camera = state.get_camera_mut("lobby").unwrap();
        camera.watching = true;

        assert!(state.get_camera("lobby").unwrap().watching);
    }
}
