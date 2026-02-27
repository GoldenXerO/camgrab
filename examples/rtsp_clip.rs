//! Example: Recording a clip from an RTSP camera
//!
//! This example demonstrates how to:
//! - Create a camera configuration
//! - Connect to an RTSP stream
//! - Record a video clip with audio
//!
//! Run with:
//! ```bash
//! cargo run --example rtsp_clip
//! ```

use camgrab_core::camera::{Camera, Protocol, Transport, StreamType, AuthMethod};
use camgrab_core::rtsp::client::{RtspClient, ClipOptions, ContainerFormat};
use std::path::Path;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing for log output
    tracing_subscriber::fmt::init();

    // Configure the camera
    let camera = Camera {
        name: "Backyard Camera".to_string(),
        host: "192.168.1.101".to_string(),
        port: 554,
        username: Some("admin".to_string()),
        password: Some("password123".to_string()),
        protocol: Protocol::Rtsp,
        transport: Transport::Tcp,
        stream: StreamType::Main,
        custom_path: None,
        audio_enabled: true,
        auth_method: AuthMethod::Digest,
        timeout: Duration::from_secs(15),
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

    // Configure recording options
    let options = ClipOptions {
        include_audio: true,
        audio_codec_override: None,
        container_format: ContainerFormat::Mp4,
        max_file_size: 0, // Unlimited
    };

    // Record a 30-second clip
    let output_path = Path::new("/tmp/clip.mp4");
    let duration = Duration::from_secs(30);

    println!("\nRecording {} second clip to: {}", duration.as_secs(), output_path.display());
    println!("  Include audio: {}", options.include_audio);
    println!("  Container: {:?}", options.container_format);

    match client.clip(output_path, duration, options).await {
        Ok(result) => {
            println!("\nClip recorded successfully!");
            println!("  Path: {}", result.path.display());
            println!("  Size: {} bytes", result.size_bytes);
            println!("  Duration: {:.2}s", result.duration.as_secs_f64());
            println!("  Video codec: {}", result.video_codec);
            if let Some(audio) = result.audio_codec {
                println!("  Audio codec: {}", audio);
            }
            println!("  Timestamp: {}", result.timestamp);
        }
        Err(e) => {
            eprintln!("\nFailed to record clip: {}", e);
            eprintln!("Note: Clip recording is not yet fully implemented in this example");
        }
    }

    // Clean disconnect
    client.disconnect().await;
    println!("\nDisconnected from camera");

    Ok(())
}
