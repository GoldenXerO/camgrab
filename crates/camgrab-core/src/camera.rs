//! Camera abstraction layer for camgrab
//!
//! This module provides the core camera abstraction, including protocol definitions,
//! transport types, authentication methods, and camera runtime representation.

use std::fmt;
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// RTSP protocol variants
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Protocol {
    /// Standard RTSP over TCP
    #[default]
    Rtsp,
    /// RTSP over TLS (secure)
    Rtsps,
}

impl fmt::Display for Protocol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Rtsp => write!(f, "rtsp"),
            Self::Rtsps => write!(f, "rtsps"),
        }
    }
}

/// Network transport protocol
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Transport {
    /// TCP transport (reliable, connection-oriented)
    #[default]
    Tcp,
    /// UDP transport (faster, connectionless)
    Udp,
}

impl fmt::Display for Transport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Tcp => write!(f, "tcp"),
            Self::Udp => write!(f, "udp"),
        }
    }
}

/// Camera stream type
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum StreamType {
    /// Main/primary stream (typically higher resolution)
    #[default]
    Main,
    /// Sub/secondary stream (typically lower resolution)
    Sub,
    /// Custom stream path
    Custom(String),
}

impl fmt::Display for StreamType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Main => write!(f, "main"),
            Self::Sub => write!(f, "sub"),
            Self::Custom(path) => write!(f, "custom({path})"),
        }
    }
}

/// Authentication method for RTSP
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum AuthMethod {
    /// Automatically detect authentication method
    #[default]
    Auto,
    /// HTTP Basic authentication
    Basic,
    /// HTTP Digest authentication (more secure)
    Digest,
}

impl fmt::Display for AuthMethod {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Auto => write!(f, "auto"),
            Self::Basic => write!(f, "basic"),
            Self::Digest => write!(f, "digest"),
        }
    }
}

/// Runtime representation of a connected camera
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Camera {
    /// Camera name/identifier
    pub name: String,
    /// Hostname or IP address
    pub host: String,
    /// RTSP port
    pub port: u16,
    /// Username for authentication
    pub username: Option<String>,
    /// Password for authentication
    pub password: Option<String>,
    /// RTSP protocol (rtsp or rtsps)
    pub protocol: Protocol,
    /// Network transport (tcp or udp)
    pub transport: Transport,
    /// Stream type (main, sub, or custom)
    pub stream: StreamType,
    /// Custom stream path override
    pub custom_path: Option<String>,
    /// Whether to enable audio in the stream
    pub audio_enabled: bool,
    /// Authentication method
    pub auth_method: AuthMethod,
    /// Connection timeout
    pub timeout: Duration,
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            name: String::new(),
            host: "localhost".to_string(),
            port: 554,
            username: None,
            password: None,
            protocol: Protocol::default(),
            transport: Transport::default(),
            stream: StreamType::default(),
            custom_path: None,
            audio_enabled: false,
            auth_method: AuthMethod::default(),
            timeout: Duration::from_secs(10),
        }
    }
}

impl Camera {
    /// Create a new camera from configuration
    ///
    /// # Arguments
    /// * `config` - The camera configuration
    ///
    /// # Returns
    /// A new `Camera` instance
    pub fn from_config(config: &crate::config::CameraConfig) -> Self {
        Self {
            name: config.name.clone(),
            host: config.host.clone(),
            port: config.port.unwrap_or(554),
            username: config.username.clone(),
            password: config.password.clone(),
            protocol: config.protocol.unwrap_or_default(),
            transport: config.transport.unwrap_or_default(),
            stream: config.stream_type.clone().unwrap_or_default(),
            custom_path: config.custom_path.clone(),
            audio_enabled: config.audio_enabled.unwrap_or(false),
            auth_method: config.auth_method.unwrap_or_default(),
            timeout: Duration::from_secs(config.timeout_secs.unwrap_or(10)),
        }
    }

    /// Build the full RTSP URL with credentials
    ///
    /// # Returns
    /// A complete RTSP URL string in the format:
    /// `rtsp://[username:password@]host:port/path`
    pub fn rtsp_url(&self) -> String {
        let auth = match (&self.username, &self.password) {
            (Some(user), Some(pass)) => format!("{user}:{pass}@"),
            (Some(user), None) => format!("{user}@"),
            _ => String::new(),
        };

        format!(
            "{}://{}{}:{}/{}",
            self.protocol,
            auth,
            self.host,
            self.port,
            self.stream_path()
        )
    }

    /// Build the RTSP URL with password redacted
    ///
    /// # Returns
    /// An RTSP URL string with the password replaced by asterisks
    pub fn rtsp_url_redacted(&self) -> String {
        let auth = match (&self.username, &self.password) {
            (Some(user), Some(_)) => format!("{user}:***@"),
            (Some(user), None) => format!("{user}@"),
            _ => String::new(),
        };

        format!(
            "{}://{}{}:{}/{}",
            self.protocol,
            auth,
            self.host,
            self.port,
            self.stream_path()
        )
    }

    /// Get the stream path based on the stream type
    ///
    /// # Returns
    /// The stream path segment of the RTSP URL
    pub fn stream_path(&self) -> &str {
        if let Some(custom) = &self.custom_path {
            return custom.as_str();
        }

        match &self.stream {
            StreamType::Main => "stream/main",
            StreamType::Sub => "stream/sub",
            StreamType::Custom(path) => path.as_str(),
        }
    }

    /// Get the display name of the camera
    ///
    /// # Returns
    /// The camera's configured name
    pub fn display_name(&self) -> &str {
        &self.name
    }
}

/// Camera-related errors
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum CameraError {
    /// Invalid hostname or IP address
    #[error("Invalid host: {0}")]
    InvalidHost(String),

    /// Invalid port number
    #[error("Invalid port: {0}")]
    InvalidPort(u16),

    /// Authentication failed
    #[error("Authentication failed for camera: {0}")]
    AuthenticationFailed(String),

    /// Connection timeout
    #[error("Connection timeout after {0}ms")]
    ConnectionTimeout(u64),

    /// Stream not found at the specified path
    #[error("Stream not found: {0}")]
    StreamNotFound(String),

    /// Connection refused by the camera
    #[error("Connection refused by camera at {0}:{1}")]
    ConnectionRefused(String, u16),

    /// Unknown error
    #[error("Unknown camera error: {0}")]
    Unknown(String),
}

/// Camera status for health checks
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CameraStatus {
    /// Whether the camera is reachable via network
    pub reachable: bool,
    /// Whether RTSP connection is successful
    pub rtsp_ok: bool,
    /// Network latency in milliseconds
    pub latency_ms: Option<u64>,
    /// Error if any occurred during health check
    pub error: Option<CameraError>,
    /// Timestamp of last health check
    pub last_checked: DateTime<Utc>,
}

impl CameraStatus {
    /// Create a new healthy camera status
    pub fn healthy(latency_ms: u64) -> Self {
        Self {
            reachable: true,
            rtsp_ok: true,
            latency_ms: Some(latency_ms),
            error: None,
            last_checked: Utc::now(),
        }
    }

    /// Create a new unhealthy camera status with an error
    pub fn unhealthy(error: CameraError) -> Self {
        Self {
            reachable: false,
            rtsp_ok: false,
            latency_ms: None,
            error: Some(error),
            last_checked: Utc::now(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::CameraConfig;

    #[test]
    fn test_protocol_display() {
        assert_eq!(Protocol::Rtsp.to_string(), "rtsp");
        assert_eq!(Protocol::Rtsps.to_string(), "rtsps");
    }

    #[test]
    fn test_transport_display() {
        assert_eq!(Transport::Tcp.to_string(), "tcp");
        assert_eq!(Transport::Udp.to_string(), "udp");
    }

    #[test]
    fn test_stream_type_display() {
        assert_eq!(StreamType::Main.to_string(), "main");
        assert_eq!(StreamType::Sub.to_string(), "sub");
        assert_eq!(
            StreamType::Custom("custom/path".to_string()).to_string(),
            "custom(custom/path)"
        );
    }

    #[test]
    fn test_rtsp_url_without_auth() {
        let camera = Camera {
            name: "test-cam".to_string(),
            host: "192.168.1.100".to_string(),
            port: 554,
            username: None,
            password: None,
            protocol: Protocol::Rtsp,
            transport: Transport::Tcp,
            stream: StreamType::Main,
            custom_path: None,
            audio_enabled: false,
            auth_method: AuthMethod::Auto,
            timeout: Duration::from_secs(10),
        };

        assert_eq!(camera.rtsp_url(), "rtsp://192.168.1.100:554/stream/main");
    }

    #[test]
    fn test_rtsp_url_with_auth() {
        let camera = Camera {
            name: "test-cam".to_string(),
            host: "192.168.1.100".to_string(),
            port: 554,
            username: Some("admin".to_string()),
            password: Some("password123".to_string()),
            protocol: Protocol::Rtsp,
            transport: Transport::Tcp,
            stream: StreamType::Main,
            custom_path: None,
            audio_enabled: false,
            auth_method: AuthMethod::Digest,
            timeout: Duration::from_secs(10),
        };

        assert_eq!(
            camera.rtsp_url(),
            "rtsp://admin:password123@192.168.1.100:554/stream/main"
        );
    }

    #[test]
    fn test_rtsp_url_with_username_only() {
        let camera = Camera {
            name: "test-cam".to_string(),
            host: "camera.local".to_string(),
            port: 8554,
            username: Some("viewer".to_string()),
            password: None,
            protocol: Protocol::Rtsps,
            transport: Transport::Udp,
            stream: StreamType::Sub,
            custom_path: None,
            audio_enabled: true,
            auth_method: AuthMethod::Basic,
            timeout: Duration::from_secs(15),
        };

        assert_eq!(
            camera.rtsp_url(),
            "rtsps://viewer@camera.local:8554/stream/sub"
        );
    }

    #[test]
    fn test_rtsp_url_redacted() {
        let camera = Camera {
            name: "secure-cam".to_string(),
            host: "10.0.0.50".to_string(),
            port: 554,
            username: Some("admin".to_string()),
            password: Some("super-secret-password".to_string()),
            protocol: Protocol::Rtsp,
            transport: Transport::Tcp,
            stream: StreamType::Main,
            custom_path: None,
            audio_enabled: false,
            auth_method: AuthMethod::Digest,
            timeout: Duration::from_secs(10),
        };

        let redacted = camera.rtsp_url_redacted();
        assert_eq!(redacted, "rtsp://admin:***@10.0.0.50:554/stream/main");
        assert!(!redacted.contains("super-secret-password"));
    }

    #[test]
    fn test_stream_path_main() {
        let camera = Camera {
            name: "test".to_string(),
            host: "localhost".to_string(),
            port: 554,
            username: None,
            password: None,
            protocol: Protocol::Rtsp,
            transport: Transport::Tcp,
            stream: StreamType::Main,
            custom_path: None,
            audio_enabled: false,
            auth_method: AuthMethod::Auto,
            timeout: Duration::from_secs(10),
        };

        assert_eq!(camera.stream_path(), "stream/main");
    }

    #[test]
    fn test_stream_path_sub() {
        let camera = Camera {
            name: "test".to_string(),
            host: "localhost".to_string(),
            port: 554,
            username: None,
            password: None,
            protocol: Protocol::Rtsp,
            transport: Transport::Tcp,
            stream: StreamType::Sub,
            custom_path: None,
            audio_enabled: false,
            auth_method: AuthMethod::Auto,
            timeout: Duration::from_secs(10),
        };

        assert_eq!(camera.stream_path(), "stream/sub");
    }

    #[test]
    fn test_stream_path_custom() {
        let camera = Camera {
            name: "test".to_string(),
            host: "localhost".to_string(),
            port: 554,
            username: None,
            password: None,
            protocol: Protocol::Rtsp,
            transport: Transport::Tcp,
            stream: StreamType::Custom("live/ch00_1".to_string()),
            custom_path: None,
            audio_enabled: false,
            auth_method: AuthMethod::Auto,
            timeout: Duration::from_secs(10),
        };

        assert_eq!(camera.stream_path(), "live/ch00_1");
    }

    #[test]
    fn test_stream_path_custom_override() {
        let camera = Camera {
            name: "test".to_string(),
            host: "localhost".to_string(),
            port: 554,
            username: None,
            password: None,
            protocol: Protocol::Rtsp,
            transport: Transport::Tcp,
            stream: StreamType::Main,
            custom_path: Some("override/path".to_string()),
            audio_enabled: false,
            auth_method: AuthMethod::Auto,
            timeout: Duration::from_secs(10),
        };

        assert_eq!(camera.stream_path(), "override/path");
    }

    #[test]
    fn test_from_config() {
        let config = CameraConfig {
            name: "front-door".to_string(),
            host: "192.168.1.10".to_string(),
            port: Some(8554),
            username: Some("user".to_string()),
            password: Some("pass".to_string()),
            protocol: Some(Protocol::Rtsps),
            transport: Some(Transport::Udp),
            stream_type: Some(StreamType::Sub),
            custom_path: Some("custom/stream".to_string()),
            audio_enabled: Some(true),
            auth_method: Some(AuthMethod::Digest),
            timeout_secs: Some(20),
        };

        let camera = Camera::from_config(&config);

        assert_eq!(camera.name, "front-door");
        assert_eq!(camera.host, "192.168.1.10");
        assert_eq!(camera.port, 8554);
        assert_eq!(camera.username, Some("user".to_string()));
        assert_eq!(camera.password, Some("pass".to_string()));
        assert_eq!(camera.protocol, Protocol::Rtsps);
        assert_eq!(camera.transport, Transport::Udp);
        assert_eq!(camera.stream, StreamType::Sub);
        assert_eq!(camera.custom_path, Some("custom/stream".to_string()));
        assert!(camera.audio_enabled);
        assert_eq!(camera.auth_method, AuthMethod::Digest);
        assert_eq!(camera.timeout, Duration::from_secs(20));
    }

    #[test]
    fn test_from_config_with_defaults() {
        let config = CameraConfig {
            name: "basic-cam".to_string(),
            host: "camera.local".to_string(),
            port: None,
            username: None,
            password: None,
            protocol: None,
            transport: None,
            stream_type: None,
            custom_path: None,
            audio_enabled: None,
            auth_method: None,
            timeout_secs: None,
        };

        let camera = Camera::from_config(&config);

        assert_eq!(camera.name, "basic-cam");
        assert_eq!(camera.host, "camera.local");
        assert_eq!(camera.port, 554); // default
        assert_eq!(camera.username, None);
        assert_eq!(camera.password, None);
        assert_eq!(camera.protocol, Protocol::Rtsp); // default
        assert_eq!(camera.transport, Transport::Tcp); // default
        assert_eq!(camera.stream, StreamType::Main); // default
        assert_eq!(camera.custom_path, None);
        assert!(!camera.audio_enabled); // default
        assert_eq!(camera.auth_method, AuthMethod::Auto); // default
        assert_eq!(camera.timeout, Duration::from_secs(10)); // default
    }

    #[test]
    fn test_display_name() {
        let camera = Camera {
            name: "my-camera".to_string(),
            host: "localhost".to_string(),
            port: 554,
            username: None,
            password: None,
            protocol: Protocol::Rtsp,
            transport: Transport::Tcp,
            stream: StreamType::Main,
            custom_path: None,
            audio_enabled: false,
            auth_method: AuthMethod::Auto,
            timeout: Duration::from_secs(10),
        };

        assert_eq!(camera.display_name(), "my-camera");
    }

    #[test]
    fn test_camera_status_healthy() {
        let status = CameraStatus::healthy(50);
        assert!(status.reachable);
        assert!(status.rtsp_ok);
        assert_eq!(status.latency_ms, Some(50));
        assert!(status.error.is_none());
    }

    #[test]
    fn test_camera_status_unhealthy() {
        let error = CameraError::ConnectionTimeout(5000);
        let status = CameraStatus::unhealthy(error.clone());
        assert!(!status.reachable);
        assert!(!status.rtsp_ok);
        assert!(status.latency_ms.is_none());
        assert_eq!(status.error, Some(error));
    }

    #[test]
    fn test_camera_error_display() {
        let error = CameraError::InvalidHost("bad-host".to_string());
        assert_eq!(error.to_string(), "Invalid host: bad-host");

        let error = CameraError::ConnectionTimeout(10000);
        assert_eq!(error.to_string(), "Connection timeout after 10000ms");

        let error = CameraError::ConnectionRefused("192.168.1.1".to_string(), 554);
        assert_eq!(
            error.to_string(),
            "Connection refused by camera at 192.168.1.1:554"
        );
    }
}
