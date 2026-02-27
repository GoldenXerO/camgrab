//! Example: Capturing a snapshot from an RTSP camera
//!
//! This example demonstrates how to:
//! - Create a camera configuration
//! - Connect to an RTSP stream
//! - Capture a single frame snapshot
//!
//! Run with:
//! ```bash
//! cargo run --example rtsp_snapshot
//! ```

use camgrab_core::camera::{Camera, Protocol, Transport, StreamType, AuthMethod};
use camgrab_core::rtsp::client::RtspClient;
use std::path::Path;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing for log output
    tracing_subscriber::fmt::init();

    // Configure the camera
    let camera = Camera {
        name: "Front Door Camera".to_string(),
        host: "192.168.1.100".to_string(),
        port: 554,
        username: Some("admin".to_string()),
        password: Some("password123".to_string()),
        protocol: Protocol::Rtsp,
        transport: Transport::Tcp,
        stream: StreamType::Main,
        custom_path: None,
        audio_enabled: false,
        auth_method: AuthMethod::Auto,
        timeout: Duration::from_secs(10),
    };

    println!("Connecting to camera: {}", camera.name);
    println!("RTSP URL: {}", camera.rtsp_url_redacted());

    // Create the RTSP client
    let mut client = RtspClient::new(&camera)?;

    // Connect to the stream
    client.connect().await?;

    // Display stream information
    if let Some(info) = client.stream_info() {
        println!("\nStream Information:");
        println!("  {}", info.description());
    }

    // Capture a snapshot
    let output_path = Path::new("/tmp/snapshot.jpg");
    println!("\nCapturing snapshot to: {}", output_path.display());

    match client.snap(output_path).await {
        Ok(result) => {
            println!("\nSnapshot captured successfully!");
            println!("  Path: {}", result.path.display());
            println!("  Size: {} bytes", result.size_bytes);
            println!("  Codec: {}", result.codec);
            println!("  Dimensions: {}x{}", result.width, result.height);
            println!("  Timestamp: {}", result.timestamp);
        }
        Err(e) => {
            eprintln!("\nFailed to capture snapshot: {}", e);
            eprintln!("Note: Frame capture is not yet fully implemented in this example");
        }
    }

    // Clean disconnect
    client.disconnect().await;
    println!("\nDisconnected from camera");

    Ok(())
}
