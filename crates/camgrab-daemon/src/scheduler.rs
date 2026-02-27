use anyhow::Result;
use chrono::{DateTime, Utc};
use cron::Schedule;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::str::FromStr;
use std::time::Duration as StdDuration;
use thiserror::Error;
use tokio::time::{sleep, Duration};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum SchedulerError {
    #[error("Invalid cron expression: {0}")]
    InvalidCronExpression(String),

    #[error("Job not found: {0}")]
    JobNotFound(Uuid),

    #[error("Job execution failed: {0}")]
    ExecutionFailed(String),

    #[error("Invalid job action: {0}")]
    InvalidAction(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduledJob {
    pub id: Uuid,
    pub name: String,
    pub cron_expr: String,
    pub action: JobAction,
    pub camera: String,
    pub enabled: bool,
    pub last_run: Option<DateTime<Utc>>,
    pub next_run: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum JobAction {
    Snap {
        output_dir: PathBuf,
        format: String,
    },
    Clip {
        output_dir: PathBuf,
        duration: Duration,
        format: String,
    },
    HealthCheck,
    Custom {
        command: String,
    },
}

impl ScheduledJob {
    pub fn new(
        name: String,
        cron_expr: String,
        action: JobAction,
        camera: String,
    ) -> Result<Self, SchedulerError> {
        // Validate cron expression
        Schedule::from_str(&cron_expr)
            .map_err(|e| SchedulerError::InvalidCronExpression(e.to_string()))?;

        let mut job = Self {
            id: Uuid::new_v4(),
            name,
            cron_expr,
            action,
            camera,
            enabled: true,
            last_run: None,
            next_run: None,
        };

        job.update_next_run();
        Ok(job)
    }

    pub fn update_next_run(&mut self) {
        if !self.enabled {
            self.next_run = None;
            return;
        }

        match Schedule::from_str(&self.cron_expr) {
            Ok(schedule) => {
                self.next_run = schedule.upcoming(Utc).next();
            }
            Err(e) => {
                error!(
                    "Failed to parse cron expression '{}': {}",
                    self.cron_expr, e
                );
                self.next_run = None;
            }
        }
    }

    pub fn is_due(&self, now: DateTime<Utc>) -> bool {
        if !self.enabled {
            return false;
        }

        self.next_run.map(|next| next <= now).unwrap_or(false)
    }

    pub fn mark_executed(&mut self) {
        self.last_run = Some(Utc::now());
        self.update_next_run();
    }
}

pub struct Scheduler {
    jobs: HashMap<Uuid, ScheduledJob>,
    config_path: PathBuf,
}

impl Scheduler {
    pub fn new() -> Self {
        Self {
            jobs: HashMap::new(),
            config_path: camgrab_core::config::default_config_path(),
        }
    }

    pub fn with_config_path(config_path: PathBuf) -> Self {
        Self {
            jobs: HashMap::new(),
            config_path,
        }
    }

    pub fn add_job(&mut self, job: ScheduledJob) -> Uuid {
        let id = job.id;
        info!(
            "Adding scheduled job '{}' ({}): {} on camera '{}'",
            job.name, id, job.cron_expr, job.camera
        );
        self.jobs.insert(id, job);
        id
    }

    pub fn remove_job(&mut self, id: &Uuid) -> bool {
        match self.jobs.remove(id) {
            Some(job) => {
                info!("Removed scheduled job '{}' ({})", job.name, id);
                true
            }
            None => {
                warn!("Attempted to remove non-existent job: {}", id);
                false
            }
        }
    }

    pub fn get_job(&self, id: &Uuid) -> Option<&ScheduledJob> {
        self.jobs.get(id)
    }

    pub fn get_job_mut(&mut self, id: &Uuid) -> Option<&mut ScheduledJob> {
        self.jobs.get_mut(id)
    }

    pub fn list_jobs(&self) -> Vec<&ScheduledJob> {
        self.jobs.values().collect()
    }

    pub fn enable_job(&mut self, id: &Uuid) -> Result<(), SchedulerError> {
        let job = self
            .jobs
            .get_mut(id)
            .ok_or(SchedulerError::JobNotFound(*id))?;

        job.enabled = true;
        job.update_next_run();
        info!("Enabled job '{}' ({})", job.name, id);
        Ok(())
    }

    pub fn disable_job(&mut self, id: &Uuid) -> Result<(), SchedulerError> {
        let job = self
            .jobs
            .get_mut(id)
            .ok_or(SchedulerError::JobNotFound(*id))?;

        job.enabled = false;
        job.next_run = None;
        info!("Disabled job '{}' ({})", job.name, id);
        Ok(())
    }

    pub fn next_tick(&self) -> Option<DateTime<Utc>> {
        self.jobs
            .values()
            .filter(|job| job.enabled)
            .filter_map(|job| job.next_run)
            .min()
    }

    pub async fn run(&mut self, mut shutdown: tokio::sync::watch::Receiver<bool>) {
        info!("Scheduler started with {} jobs", self.jobs.len());

        loop {
            let now = Utc::now();
            let mut due_jobs = Vec::new();

            // Find all jobs that are due
            for job in self.jobs.values() {
                if job.is_due(now) {
                    due_jobs.push(job.id);
                }
            }

            // Execute due jobs
            for job_id in due_jobs {
                // Clone the job data to avoid borrowing issues
                if let Some(job) = self.jobs.get(&job_id) {
                    info!(
                        "Executing scheduled job '{}' ({}): {:?} on camera '{}'",
                        job.name, job.id, job.action, job.camera
                    );

                    let job_clone = job.clone();

                    // Execute the job action
                    if let Err(e) = self.execute_job(&job_clone).await {
                        error!(
                            "Failed to execute job '{}' ({}): {}",
                            job_clone.name, job_clone.id, e
                        );
                    }

                    // Now get mutable borrow to mark as executed
                    if let Some(job) = self.jobs.get_mut(&job_id) {
                        job.mark_executed();
                    }
                }
            }

            // Calculate next wake time
            let next_tick = self.next_tick();
            let sleep_duration = if let Some(next) = next_tick {
                let duration = (next - Utc::now())
                    .to_std()
                    .unwrap_or(StdDuration::from_secs(1));
                // Cap at 60 seconds to ensure we check for shutdown regularly
                duration.min(StdDuration::from_secs(60))
            } else {
                // No jobs scheduled, check again in 60 seconds
                StdDuration::from_secs(60)
            };

            debug!("Scheduler sleeping for {:?}", sleep_duration);

            // Sleep until next tick or shutdown signal
            tokio::select! {
                _ = sleep(sleep_duration) => {
                    // Continue to next iteration
                }
                _ = shutdown.changed() => {
                    if *shutdown.borrow() {
                        info!("Scheduler received shutdown signal");
                        break;
                    }
                }
            }
        }

        info!("Scheduler stopped");
    }

    async fn execute_job(&self, job: &ScheduledJob) -> Result<(), SchedulerError> {
        match &job.action {
            JobAction::Snap { output_dir, format } => {
                info!("Executing snap job for camera '{}'", job.camera);

                let app_config = camgrab_core::config::load(&self.config_path)
                    .map_err(|e| SchedulerError::ExecutionFailed(format!("Config load: {}", e)))?;
                let cam_config = camgrab_core::config::find_camera(&app_config, &job.camera)
                    .ok_or_else(|| {
                        SchedulerError::ExecutionFailed(format!(
                            "Camera '{}' not found in config",
                            job.camera
                        ))
                    })?;

                tokio::fs::create_dir_all(output_dir).await.map_err(|e| {
                    SchedulerError::ExecutionFailed(format!("Create output dir: {}", e))
                })?;

                let filename = format!(
                    "{}-{}.{}",
                    job.camera,
                    chrono::Utc::now().format("%Y%m%d-%H%M%S"),
                    format
                );
                let output_path = output_dir.join(&filename);

                let cam = camgrab_core::camera::Camera::from_config(cam_config);
                let mut client = camgrab_core::rtsp::client::RtspClient::new(&cam)
                    .map_err(|e| SchedulerError::ExecutionFailed(format!("RTSP client: {}", e)))?;
                client
                    .connect()
                    .await
                    .map_err(|e| SchedulerError::ExecutionFailed(format!("Connect: {}", e)))?;
                let result = client
                    .snap(&output_path)
                    .await
                    .map_err(|e| SchedulerError::ExecutionFailed(format!("Snap: {}", e)))?;

                info!(
                    "Snap job complete: {} ({} bytes)",
                    result.path.display(),
                    result.size_bytes
                );
            }
            JobAction::Clip {
                output_dir,
                duration,
                format,
            } => {
                info!(
                    "Executing clip job for camera '{}' ({:?})",
                    job.camera, duration
                );

                let app_config = camgrab_core::config::load(&self.config_path)
                    .map_err(|e| SchedulerError::ExecutionFailed(format!("Config load: {}", e)))?;
                let cam_config = camgrab_core::config::find_camera(&app_config, &job.camera)
                    .ok_or_else(|| {
                        SchedulerError::ExecutionFailed(format!(
                            "Camera '{}' not found in config",
                            job.camera
                        ))
                    })?;

                tokio::fs::create_dir_all(output_dir).await.map_err(|e| {
                    SchedulerError::ExecutionFailed(format!("Create output dir: {}", e))
                })?;

                let filename = format!(
                    "{}-{}.{}",
                    job.camera,
                    chrono::Utc::now().format("%Y%m%d-%H%M%S"),
                    format
                );
                let output_path = output_dir.join(&filename);

                let clip_duration = std::time::Duration::from_secs(duration.as_secs());
                let clip_options = camgrab_core::rtsp::client::ClipOptions {
                    include_audio: true,
                    audio_codec_override: None,
                    container_format: camgrab_core::rtsp::client::ContainerFormat::Mp4,
                    max_file_size: 0,
                };

                let cam = camgrab_core::camera::Camera::from_config(cam_config);
                let mut client = camgrab_core::rtsp::client::RtspClient::new(&cam)
                    .map_err(|e| SchedulerError::ExecutionFailed(format!("RTSP client: {}", e)))?;
                client
                    .connect()
                    .await
                    .map_err(|e| SchedulerError::ExecutionFailed(format!("Connect: {}", e)))?;
                let result = client
                    .clip(&output_path, clip_duration, clip_options)
                    .await
                    .map_err(|e| SchedulerError::ExecutionFailed(format!("Clip: {}", e)))?;

                info!(
                    "Clip job complete: {} ({} bytes, {:.1}s)",
                    result.path.display(),
                    result.size_bytes,
                    result.duration.as_secs_f64()
                );
            }
            JobAction::HealthCheck => {
                info!("Executing health check for camera '{}'", job.camera);

                let app_config = camgrab_core::config::load(&self.config_path)
                    .map_err(|e| SchedulerError::ExecutionFailed(format!("Config load: {}", e)))?;
                let cam_config = camgrab_core::config::find_camera(&app_config, &job.camera)
                    .ok_or_else(|| {
                        SchedulerError::ExecutionFailed(format!(
                            "Camera '{}' not found in config",
                            job.camera
                        ))
                    })?;

                let cam = camgrab_core::camera::Camera::from_config(cam_config);
                let mut client = camgrab_core::rtsp::client::RtspClient::new(&cam)
                    .map_err(|e| SchedulerError::ExecutionFailed(format!("RTSP client: {}", e)))?;
                client.connect().await.map_err(|e| {
                    SchedulerError::ExecutionFailed(format!("Health check connect: {}", e))
                })?;
                client.disconnect().await;

                info!("Health check passed for camera '{}'", job.camera);
            }
            JobAction::Custom { command } => {
                info!("Executing custom command: {}", command);

                let output = tokio::process::Command::new("sh")
                    .arg("-c")
                    .arg(command)
                    .output()
                    .await
                    .map_err(|e| SchedulerError::ExecutionFailed(format!("Command exec: {}", e)))?;

                if output.status.success() {
                    info!("Custom command succeeded (exit code 0)");
                } else {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    warn!(
                        "Custom command exited with code {:?}: {}",
                        output.status.code(),
                        stderr.trim()
                    );
                }
            }
        }

        Ok(())
    }
}

impl Default for Scheduler {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_job_creation() {
        let job = ScheduledJob::new(
            "Test Job".to_string(),
            "0 0 * * * *".to_string(), // Every hour
            JobAction::HealthCheck,
            "lobby".to_string(),
        );

        assert!(job.is_ok());
        let job = job.unwrap();
        assert_eq!(job.name, "Test Job");
        assert_eq!(job.camera, "lobby");
        assert!(job.enabled);
        assert!(job.next_run.is_some());
    }

    #[test]
    fn test_invalid_cron_expression() {
        let job = ScheduledJob::new(
            "Invalid Job".to_string(),
            "invalid cron".to_string(),
            JobAction::HealthCheck,
            "lobby".to_string(),
        );

        assert!(job.is_err());
        match job {
            Err(SchedulerError::InvalidCronExpression(_)) => {}
            _ => panic!("Expected InvalidCronExpression error"),
        }
    }

    #[test]
    fn test_scheduler_add_remove_job() {
        let mut scheduler = Scheduler::new();

        let job = ScheduledJob::new(
            "Test Job".to_string(),
            "0 0 * * * *".to_string(),
            JobAction::HealthCheck,
            "lobby".to_string(),
        )
        .unwrap();

        let id = job.id;
        scheduler.add_job(job);

        assert_eq!(scheduler.list_jobs().len(), 1);
        assert!(scheduler.get_job(&id).is_some());

        assert!(scheduler.remove_job(&id));
        assert_eq!(scheduler.list_jobs().len(), 0);
        assert!(scheduler.get_job(&id).is_none());
    }

    #[test]
    fn test_scheduler_enable_disable() {
        let mut scheduler = Scheduler::new();

        let job = ScheduledJob::new(
            "Test Job".to_string(),
            "0 0 * * * *".to_string(),
            JobAction::HealthCheck,
            "lobby".to_string(),
        )
        .unwrap();

        let id = job.id;
        scheduler.add_job(job);

        assert!(scheduler.get_job(&id).unwrap().enabled);
        assert!(scheduler.get_job(&id).unwrap().next_run.is_some());

        scheduler.disable_job(&id).unwrap();
        assert!(!scheduler.get_job(&id).unwrap().enabled);
        assert!(scheduler.get_job(&id).unwrap().next_run.is_none());

        scheduler.enable_job(&id).unwrap();
        assert!(scheduler.get_job(&id).unwrap().enabled);
        assert!(scheduler.get_job(&id).unwrap().next_run.is_some());
    }

    #[test]
    fn test_next_tick() {
        let mut scheduler = Scheduler::new();

        let job1 = ScheduledJob::new(
            "Job 1".to_string(),
            "0 0 * * * *".to_string(), // Every hour
            JobAction::HealthCheck,
            "camera1".to_string(),
        )
        .unwrap();

        let job2 = ScheduledJob::new(
            "Job 2".to_string(),
            "0 * * * * *".to_string(), // Every minute
            JobAction::HealthCheck,
            "camera2".to_string(),
        )
        .unwrap();

        let next1 = job1.next_run;
        let next2 = job2.next_run;

        scheduler.add_job(job1);
        scheduler.add_job(job2);

        let next_tick = scheduler.next_tick();
        assert!(next_tick.is_some());

        // The next tick should be the earlier of the two jobs
        assert_eq!(next_tick, next1.min(next2));
    }

    #[test]
    fn test_job_action_serialization() {
        let snap_action = JobAction::Snap {
            output_dir: PathBuf::from("/tmp/snaps"),
            format: "jpg".to_string(),
        };

        let json = serde_json::to_string(&snap_action).unwrap();
        let deserialized: JobAction = serde_json::from_str(&json).unwrap();

        match deserialized {
            JobAction::Snap { output_dir, format } => {
                assert_eq!(output_dir, PathBuf::from("/tmp/snaps"));
                assert_eq!(format, "jpg");
            }
            _ => panic!("Wrong action type"),
        }
    }

    #[test]
    fn test_clip_action_serialization() {
        let clip_action = JobAction::Clip {
            output_dir: PathBuf::from("/tmp/clips"),
            duration: Duration::from_secs(30),
            format: "mp4".to_string(),
        };

        let json = serde_json::to_string(&clip_action).unwrap();
        let deserialized: JobAction = serde_json::from_str(&json).unwrap();

        match deserialized {
            JobAction::Clip {
                output_dir,
                duration,
                format,
            } => {
                assert_eq!(output_dir, PathBuf::from("/tmp/clips"));
                assert_eq!(duration, Duration::from_secs(30));
                assert_eq!(format, "mp4");
            }
            _ => panic!("Wrong action type"),
        }
    }

    #[test]
    fn test_health_check_action_serialization() {
        let action = JobAction::HealthCheck;
        let json = serde_json::to_string(&action).unwrap();
        let deserialized: JobAction = serde_json::from_str(&json).unwrap();

        match deserialized {
            JobAction::HealthCheck => {}
            _ => panic!("Wrong action type"),
        }
    }

    #[test]
    fn test_custom_action_serialization() {
        let action = JobAction::Custom {
            command: "echo 'hello'".to_string(),
        };

        let json = serde_json::to_string(&action).unwrap();
        let deserialized: JobAction = serde_json::from_str(&json).unwrap();

        match deserialized {
            JobAction::Custom { command } => {
                assert_eq!(command, "echo 'hello'");
            }
            _ => panic!("Wrong action type"),
        }
    }

    #[test]
    fn test_job_is_due() {
        let mut job = ScheduledJob::new(
            "Test Job".to_string(),
            "0 0 1 1 * *".to_string(), // Jan 1st at midnight
            JobAction::HealthCheck,
            "lobby".to_string(),
        )
        .unwrap();

        // Job should not be due now
        let now = Utc::now();
        assert!(!job.is_due(now));

        // Manually set next_run to the past
        job.next_run = Some(now - chrono::Duration::hours(1));
        assert!(job.is_due(now));

        // Disabled job should never be due
        job.enabled = false;
        assert!(!job.is_due(now));
    }

    #[test]
    fn test_job_mark_executed() {
        let mut job = ScheduledJob::new(
            "Test Job".to_string(),
            "0 0 * * * *".to_string(),
            JobAction::HealthCheck,
            "lobby".to_string(),
        )
        .unwrap();

        assert!(job.last_run.is_none());

        job.mark_executed();

        assert!(job.last_run.is_some());
        // next_run should still be set after marking executed
        assert!(job.next_run.is_some());
    }
}
