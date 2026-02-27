//! RTSP transport layer abstractions
//!
//! This module provides transport configuration and session management for RTSP connections.

use serde::{Deserialize, Serialize};
use std::time::Duration;
use thiserror::Error;

/// RTSP transport protocol
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum RtspTransport {
    /// TCP transport (interleaved mode)
    ///
    /// More reliable, works through most firewalls and NAT, but slightly higher latency.
    #[default]
    Tcp,

    /// UDP transport
    ///
    /// Lower latency, but may have packet loss and firewall issues.
    Udp,
}

impl RtspTransport {
    /// Converts to retina's transport type
    pub fn to_retina(&self) -> retina::client::Transport {
        match self {
            Self::Tcp => retina::client::Transport::Tcp(Default::default()),
            Self::Udp => {
                retina::client::Transport::Udp(retina::client::UdpTransportOptions::default())
            }
        }
    }
}

/// Configuration for RTSP session establishment
#[derive(Debug, Clone)]
pub struct SessionConfig {
    /// Transport protocol (TCP or UDP)
    pub transport: RtspTransport,

    /// Connection timeout
    pub timeout: Duration,

    /// Keep-alive interval (sends OPTIONS to keep session alive)
    pub keepalive_interval: Duration,

    /// User agent string
    pub user_agent: String,

    /// Whether to ignore TLS certificate errors
    pub insecure_tls: bool,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            transport: RtspTransport::Tcp,
            timeout: Duration::from_secs(10),
            keepalive_interval: Duration::from_secs(30),
            user_agent: format!("camgrab/{}", env!("CARGO_PKG_VERSION")),
            insecure_tls: false,
        }
    }
}

impl SessionConfig {
    /// Creates a new session config with default values
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the transport protocol
    #[must_use]
    pub fn with_transport(mut self, transport: RtspTransport) -> Self {
        self.transport = transport;
        self
    }

    /// Sets the connection timeout
    #[must_use]
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Sets the keep-alive interval
    #[must_use]
    pub fn with_keepalive(mut self, interval: Duration) -> Self {
        self.keepalive_interval = interval;
        self
    }

    /// Sets whether to ignore TLS certificate errors
    #[must_use]
    pub fn with_insecure_tls(mut self, insecure: bool) -> Self {
        self.insecure_tls = insecure;
        self
    }
}

/// Session state wrapper
///
/// This wraps the retina session and provides additional state management.
pub struct RtspSession {
    /// The underlying retina session (in Playing state)
    pub(crate) session: retina::client::Session<retina::client::Playing>,

    /// Configuration used for this session
    pub(crate) config: SessionConfig,
}

impl RtspSession {
    /// Creates a new session wrapper
    pub fn new(
        session: retina::client::Session<retina::client::Playing>,
        config: SessionConfig,
    ) -> Self {
        Self { session, config }
    }

    /// Returns the session configuration
    pub fn config(&self) -> &SessionConfig {
        &self.config
    }

    /// Returns a reference to the underlying retina session
    pub fn inner(&self) -> &retina::client::Session<retina::client::Playing> {
        &self.session
    }

    /// Returns a mutable reference to the underlying retina session
    pub fn inner_mut(&mut self) -> &mut retina::client::Session<retina::client::Playing> {
        &mut self.session
    }
}

/// Errors that can occur during transport operations
#[derive(Debug, Error)]
pub enum TransportError {
    /// Connection timeout
    #[error("Connection timeout after {0:?}")]
    Timeout(Duration),

    /// Network error
    #[error("Network error: {0}")]
    Network(String),

    /// Invalid transport configuration
    #[error("Invalid transport configuration: {0}")]
    InvalidConfig(String),

    /// Retina library error
    #[error("RTSP protocol error: {0}")]
    Retina(#[from] retina::Error),
}

/// Establishes an RTSP session with the given URL and configuration
///
/// This performs the RTSP handshake (OPTIONS, DESCRIBE, SETUP, PLAY) and returns
/// a session in the Playing state.
///
/// # Arguments
///
/// * `url` - The RTSP URL to connect to
/// * `config` - Session configuration
///
/// # Returns
///
/// An `RtspSession` ready to receive media data
///
/// # Errors
///
/// Returns `TransportError` if connection fails, times out, or authentication fails.
pub async fn establish_session(
    url: &url::Url,
    config: SessionConfig,
) -> Result<RtspSession, TransportError> {
    tracing::info!(
        url = %url,
        transport = ?config.transport,
        "Establishing RTSP session"
    );

    // Create session options
    let mut session_options =
        retina::client::SessionOptions::default().user_agent(config.user_agent.clone());

    // Set up credentials if present in URL
    if !url.username().is_empty() {
        let password = url.password().unwrap_or("");
        session_options = session_options.creds(Some(retina::client::Credentials {
            username: url.username().to_string(),
            password: password.to_string(),
        }));
    }

    // Connect with timeout
    let session_result = tokio::time::timeout(
        config.timeout,
        retina::client::Session::describe(url.clone(), session_options),
    )
    .await;

    let session_described = match session_result {
        Ok(Ok(session)) => session,
        Ok(Err(e)) => {
            tracing::error!(error = %e, "Failed to DESCRIBE RTSP stream");
            return Err(TransportError::Retina(e));
        }
        Err(_) => {
            tracing::error!("RTSP connection timeout");
            return Err(TransportError::Timeout(config.timeout));
        }
    };

    tracing::debug!("RTSP DESCRIBE successful");

    // Set up all streams with the configured transport
    let mut session_described = session_described;
    let setup_result = tokio::time::timeout(
        config.timeout,
        session_described.setup(
            0,
            retina::client::SetupOptions::default().transport(config.transport.to_retina()),
        ),
    )
    .await;

    match setup_result {
        Ok(Ok(())) => {}
        Ok(Err(e)) => {
            tracing::error!(error = %e, "Failed to SETUP RTSP stream");
            return Err(TransportError::Retina(e));
        }
        Err(_) => {
            tracing::error!("RTSP SETUP timeout");
            return Err(TransportError::Timeout(config.timeout));
        }
    };

    tracing::debug!("RTSP SETUP successful");

    // Start playing
    let session_play_result = tokio::time::timeout(
        config.timeout,
        session_described.play(retina::client::PlayOptions::default()),
    )
    .await;

    let session_playing = match session_play_result {
        Ok(Ok(session)) => session,
        Ok(Err(e)) => {
            tracing::error!(error = %e, "Failed to PLAY RTSP stream");
            return Err(TransportError::Retina(e));
        }
        Err(_) => {
            tracing::error!("RTSP PLAY timeout");
            return Err(TransportError::Timeout(config.timeout));
        }
    };

    tracing::info!("RTSP session established successfully");

    Ok(RtspSession::new(session_playing, config))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transport_default() {
        let transport = RtspTransport::default();
        assert_eq!(transport, RtspTransport::Tcp);
    }

    #[test]
    fn test_session_config_builder() {
        let config = SessionConfig::new()
            .with_transport(RtspTransport::Udp)
            .with_timeout(Duration::from_secs(5))
            .with_keepalive(Duration::from_secs(60))
            .with_insecure_tls(true);

        assert_eq!(config.transport, RtspTransport::Udp);
        assert_eq!(config.timeout, Duration::from_secs(5));
        assert_eq!(config.keepalive_interval, Duration::from_secs(60));
        assert!(config.insecure_tls);
    }

    #[test]
    fn test_transport_serialization() {
        let tcp = RtspTransport::Tcp;
        let json = serde_json::to_string(&tcp).unwrap();
        assert_eq!(json, r#""tcp""#);

        let udp = RtspTransport::Udp;
        let json = serde_json::to_string(&udp).unwrap();
        assert_eq!(json, r#""udp""#);
    }

    #[test]
    fn test_transport_deserialization() {
        let tcp: RtspTransport = serde_json::from_str(r#""tcp""#).unwrap();
        assert_eq!(tcp, RtspTransport::Tcp);

        let udp: RtspTransport = serde_json::from_str(r#""udp""#).unwrap();
        assert_eq!(udp, RtspTransport::Udp);
    }
}
