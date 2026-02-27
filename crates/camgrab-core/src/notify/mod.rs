use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;
use thiserror::Error;

/// Notification event that occurred in the system
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationEvent {
    pub camera_name: String,
    pub event_type: EventType,
    pub timestamp: DateTime<Utc>,
    pub score: Option<f64>,
    pub image_path: Option<PathBuf>,
    pub message: String,
}

/// Types of events that can trigger notifications
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EventType {
    MotionDetected,
    CameraOffline,
    CameraOnline,
    RecordingStarted,
    RecordingStopped,
    HealthCheckFailed,
}

/// HTTP methods supported by webhook notifier
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "UPPERCASE")]
pub enum HttpMethod {
    POST,
    PUT,
}

/// Errors that can occur during notification
#[derive(Debug, Error)]
pub enum NotifyError {
    #[error("Webhook request failed: {0}")]
    WebhookFailed(String),

    #[error("MQTT publish failed: {0}")]
    MqttFailed(String),

    #[error("Email send failed: {0}")]
    EmailFailed(String),

    #[error("Notification timeout")]
    Timeout,

    #[error("Configuration error: {0}")]
    ConfigError(String),
}

/// Trait for notification backends
#[async_trait::async_trait]
pub trait Notifier: Send + Sync {
    /// Send a notification event
    async fn send(&self, event: &NotificationEvent) -> Result<(), NotifyError>;

    /// Get the name of this notifier
    fn name(&self) -> &str;
}

/// Webhook notifier that sends HTTP requests
pub struct WebhookNotifier {
    url: String,
    headers: HashMap<String, String>,
    method: HttpMethod,
    client: reqwest::Client,
}

impl WebhookNotifier {
    pub fn new(
        url: String,
        headers: HashMap<String, String>,
        method: HttpMethod,
    ) -> Result<Self, NotifyError> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .map_err(|e| NotifyError::ConfigError(format!("Failed to create HTTP client: {e}")))?;

        Ok(Self {
            url,
            headers,
            method,
            client,
        })
    }

    async fn send_with_retry(
        &self,
        event: &NotificationEvent,
        attempts: u32,
    ) -> Result<(), NotifyError> {
        let mut last_error = None;

        for attempt in 0..attempts {
            if attempt > 0 {
                // Exponential backoff: 1s, 2s, 4s
                let delay = Duration::from_secs(2_u64.pow(attempt - 1));
                tokio::time::sleep(delay).await;
            }

            let mut request = match self.method {
                HttpMethod::POST => self.client.post(&self.url),
                HttpMethod::PUT => self.client.put(&self.url),
            };

            // Add custom headers
            for (key, value) in &self.headers {
                request = request.header(key, value);
            }

            match request.json(event).send().await {
                Ok(response) => {
                    if response.status().is_success() {
                        return Ok(());
                    }
                    last_error = Some(format!(
                        "HTTP {}: {}",
                        response.status(),
                        response.text().await.unwrap_or_default()
                    ));
                }
                Err(e) => {
                    last_error = Some(e.to_string());
                }
            }
        }

        Err(NotifyError::WebhookFailed(
            last_error.unwrap_or_else(|| "Unknown error".to_string()),
        ))
    }
}

#[async_trait::async_trait]
impl Notifier for WebhookNotifier {
    async fn send(&self, event: &NotificationEvent) -> Result<(), NotifyError> {
        self.send_with_retry(event, 3).await
    }

    fn name(&self) -> &str {
        "webhook"
    }
}

/// MQTT notifier that publishes messages to a broker
pub struct MqttNotifier {
    broker: String,
    port: u16,
    topic: String,
    client_id: String,
    username: Option<String>,
    password: Option<String>,
    qos: rumqttc::QoS,
}

impl MqttNotifier {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        broker: String,
        port: u16,
        topic: String,
        client_id: String,
        username: Option<String>,
        password: Option<String>,
        qos: u8,
    ) -> Result<Self, NotifyError> {
        let qos = match qos {
            0 => rumqttc::QoS::AtMostOnce,
            1 => rumqttc::QoS::AtLeastOnce,
            2 => rumqttc::QoS::ExactlyOnce,
            _ => {
                return Err(NotifyError::ConfigError(format!(
                    "Invalid QoS level: {qos}"
                )))
            }
        };

        Ok(Self {
            broker,
            port,
            topic,
            client_id,
            username,
            password,
            qos,
        })
    }
}

#[async_trait::async_trait]
impl Notifier for MqttNotifier {
    async fn send(&self, event: &NotificationEvent) -> Result<(), NotifyError> {
        let mut options = rumqttc::MqttOptions::new(&self.client_id, &self.broker, self.port);

        if let (Some(username), Some(password)) = (&self.username, &self.password) {
            options.set_credentials(username, password);
        }

        options.set_keep_alive(Duration::from_secs(30));

        let (client, mut eventloop) = rumqttc::AsyncClient::new(options, 10);

        // Serialize event to JSON
        let payload = serde_json::to_vec(event)
            .map_err(|e| NotifyError::MqttFailed(format!("Failed to serialize event: {e}")))?;

        // Publish message
        client
            .publish(&self.topic, self.qos, false, payload)
            .await
            .map_err(|e| NotifyError::MqttFailed(format!("Failed to publish: {e}")))?;

        // Wait for publish to complete
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                match eventloop.poll().await {
                    Ok(rumqttc::Event::Outgoing(rumqttc::Outgoing::Publish(_))) => {
                        return Ok(());
                    }
                    Ok(_) => continue,
                    Err(e) => {
                        return Err(NotifyError::MqttFailed(format!("Event loop error: {e}")))
                    }
                }
            }
        })
        .await
        .map_err(|_| NotifyError::Timeout)?
    }

    fn name(&self) -> &str {
        "mqtt"
    }
}

/// Email notifier that sends SMTP emails
pub struct EmailNotifier {
    smtp_host: String,
    smtp_port: u16,
    from: String,
    to: Vec<String>,
    username: String,
    password: String,
}

impl EmailNotifier {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        smtp_host: String,
        smtp_port: u16,
        from: String,
        to: Vec<String>,
        username: String,
        password: String,
    ) -> Self {
        Self {
            smtp_host,
            smtp_port,
            from,
            to,
            username,
            password,
        }
    }

    fn format_html_email(event: &NotificationEvent) -> String {
        let event_type_str = match event.event_type {
            EventType::MotionDetected => "Motion Detected",
            EventType::CameraOffline => "Camera Offline",
            EventType::CameraOnline => "Camera Online",
            EventType::RecordingStarted => "Recording Started",
            EventType::RecordingStopped => "Recording Stopped",
            EventType::HealthCheckFailed => "Health Check Failed",
        };

        let score_html = event
            .score
            .map(|s| format!("<p><strong>Score:</strong> {s:.2}</p>"))
            .unwrap_or_default();

        let image_html = event
            .image_path
            .as_ref()
            .map(|path| format!("<p><strong>Image:</strong> {}</p>", path.display()))
            .unwrap_or_default();

        format!(
            r#"<!DOCTYPE html>
<html>
<head>
    <style>
        body {{ font-family: Arial, sans-serif; margin: 20px; }}
        .header {{ background-color: #f0f0f0; padding: 10px; border-radius: 5px; }}
        .content {{ margin-top: 20px; }}
        .footer {{ margin-top: 20px; color: #666; font-size: 12px; }}
    </style>
</head>
<body>
    <div class="header">
        <h2>Camera Alert: {}</h2>
    </div>
    <div class="content">
        <p><strong>Camera:</strong> {}</p>
        <p><strong>Event:</strong> {}</p>
        <p><strong>Time:</strong> {}</p>
        {}
        {}
        <p><strong>Message:</strong> {}</p>
    </div>
    <div class="footer">
        <p>This is an automated notification from camgrab</p>
    </div>
</body>
</html>"#,
            event_type_str,
            event.camera_name,
            event_type_str,
            event.timestamp.format("%Y-%m-%d %H:%M:%S UTC"),
            score_html,
            image_html,
            event.message
        )
    }
}

#[async_trait::async_trait]
impl Notifier for EmailNotifier {
    async fn send(&self, event: &NotificationEvent) -> Result<(), NotifyError> {
        use lettre::message::header::ContentType;
        use lettre::transport::smtp::authentication::Credentials;
        use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};

        let subject = format!(
            "[camgrab] {} - {}",
            event.camera_name,
            match event.event_type {
                EventType::MotionDetected => "Motion Detected",
                EventType::CameraOffline => "Camera Offline",
                EventType::CameraOnline => "Camera Online",
                EventType::RecordingStarted => "Recording Started",
                EventType::RecordingStopped => "Recording Stopped",
                EventType::HealthCheckFailed => "Health Check Failed",
            }
        );

        let html_body = Self::format_html_email(event);

        // Build email for each recipient
        for recipient in &self.to {
            let email =
                Message::builder()
                    .from(self.from.parse().map_err(|e| {
                        NotifyError::EmailFailed(format!("Invalid from address: {e}"))
                    })?)
                    .to(recipient.parse().map_err(|e| {
                        NotifyError::EmailFailed(format!("Invalid to address: {e}"))
                    })?)
                    .subject(&subject)
                    .header(ContentType::TEXT_HTML)
                    .body(html_body.clone())
                    .map_err(|e| NotifyError::EmailFailed(format!("Failed to build email: {e}")))?;

            let creds = Credentials::new(self.username.clone(), self.password.clone());

            let mailer = AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&self.smtp_host)
                .map_err(|e| NotifyError::EmailFailed(format!("Failed to create transport: {e}")))?
                .port(self.smtp_port)
                .credentials(creds)
                .build();

            mailer
                .send(email)
                .await
                .map_err(|e| NotifyError::EmailFailed(format!("Failed to send email: {e}")))?;
        }

        Ok(())
    }

    fn name(&self) -> &str {
        "email"
    }
}

/// Router that manages multiple notifiers
pub struct NotificationRouter {
    notifiers: Vec<Box<dyn Notifier + Send + Sync>>,
}

impl NotificationRouter {
    pub fn new() -> Self {
        Self {
            notifiers: Vec::new(),
        }
    }

    /// Add a notifier to the router
    pub fn add(&mut self, notifier: Box<dyn Notifier + Send + Sync>) {
        self.notifiers.push(notifier);
    }

    /// Broadcast an event to all notifiers
    pub async fn broadcast(&self, event: &NotificationEvent) -> Vec<Result<(), NotifyError>> {
        let mut results = Vec::new();

        for notifier in &self.notifiers {
            results.push(notifier.send(event).await);
        }

        results
    }

    /// Send an event to a specific notifier by name
    pub async fn send_to(&self, name: &str, event: &NotificationEvent) -> Result<(), NotifyError> {
        for notifier in &self.notifiers {
            if notifier.name() == name {
                return notifier.send(event).await;
            }
        }

        Err(NotifyError::ConfigError(format!(
            "Notifier not found: {name}"
        )))
    }
}

impl Default for NotificationRouter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    // Mock notifier for testing
    struct MockNotifier {
        name: String,
        calls: Arc<Mutex<Vec<NotificationEvent>>>,
        should_fail: bool,
    }

    impl MockNotifier {
        fn new(name: &str) -> Self {
            Self {
                name: name.to_string(),
                calls: Arc::new(Mutex::new(Vec::new())),
                should_fail: false,
            }
        }

        fn new_failing(name: &str) -> Self {
            Self {
                name: name.to_string(),
                calls: Arc::new(Mutex::new(Vec::new())),
                should_fail: true,
            }
        }

        #[allow(dead_code)]
        fn get_calls(&self) -> Vec<NotificationEvent> {
            self.calls.lock().unwrap().clone()
        }
    }

    #[async_trait::async_trait]
    impl Notifier for MockNotifier {
        async fn send(&self, event: &NotificationEvent) -> Result<(), NotifyError> {
            self.calls.lock().unwrap().push(event.clone());

            if self.should_fail {
                Err(NotifyError::WebhookFailed("Mock failure".to_string()))
            } else {
                Ok(())
            }
        }

        fn name(&self) -> &str {
            &self.name
        }
    }

    #[test]
    fn test_event_type_serialization() {
        let event_type = EventType::MotionDetected;
        let json = serde_json::to_string(&event_type).unwrap();
        assert_eq!(json, r#""motion_detected""#);

        let deserialized: EventType = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, EventType::MotionDetected);
    }

    #[tokio::test]
    async fn test_notification_router_broadcast() {
        let mut router = NotificationRouter::new();

        let notifier1 = MockNotifier::new("test1");
        let notifier2 = MockNotifier::new("test2");

        let calls1 = notifier1.calls.clone();
        let calls2 = notifier2.calls.clone();

        router.add(Box::new(notifier1));
        router.add(Box::new(notifier2));

        let event = NotificationEvent {
            camera_name: "test-camera".to_string(),
            event_type: EventType::MotionDetected,
            timestamp: Utc::now(),
            score: Some(0.95),
            image_path: Some(PathBuf::from("/tmp/test.jpg")),
            message: "Motion detected".to_string(),
        };

        let results = router.broadcast(&event).await;

        assert_eq!(results.len(), 2);
        assert!(results[0].is_ok());
        assert!(results[1].is_ok());

        assert_eq!(calls1.lock().unwrap().len(), 1);
        assert_eq!(calls2.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn test_notification_router_send_to() {
        let mut router = NotificationRouter::new();

        let notifier1 = MockNotifier::new("test1");
        let notifier2 = MockNotifier::new("test2");

        let calls1 = notifier1.calls.clone();
        let calls2 = notifier2.calls.clone();

        router.add(Box::new(notifier1));
        router.add(Box::new(notifier2));

        let event = NotificationEvent {
            camera_name: "test-camera".to_string(),
            event_type: EventType::CameraOffline,
            timestamp: Utc::now(),
            score: None,
            image_path: None,
            message: "Camera went offline".to_string(),
        };

        let result = router.send_to("test2", &event).await;
        assert!(result.is_ok());

        assert_eq!(calls1.lock().unwrap().len(), 0);
        assert_eq!(calls2.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn test_notification_router_not_found() {
        let router = NotificationRouter::new();

        let event = NotificationEvent {
            camera_name: "test-camera".to_string(),
            event_type: EventType::CameraOnline,
            timestamp: Utc::now(),
            score: None,
            image_path: None,
            message: "Camera came online".to_string(),
        };

        let result = router.send_to("nonexistent", &event).await;
        assert!(result.is_err());
        assert!(matches!(result, Err(NotifyError::ConfigError(_))));
    }

    #[tokio::test]
    async fn test_notification_router_partial_failure() {
        let mut router = NotificationRouter::new();

        router.add(Box::new(MockNotifier::new("success")));
        router.add(Box::new(MockNotifier::new_failing("failure")));

        let event = NotificationEvent {
            camera_name: "test-camera".to_string(),
            event_type: EventType::RecordingStarted,
            timestamp: Utc::now(),
            score: None,
            image_path: None,
            message: "Recording started".to_string(),
        };

        let results = router.broadcast(&event).await;

        assert_eq!(results.len(), 2);
        assert!(results[0].is_ok());
        assert!(results[1].is_err());
    }
}
