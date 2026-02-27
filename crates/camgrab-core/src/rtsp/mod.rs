//! RTSP client implementation using pure Rust (retina library)
//!
//! This module provides a complete RTSP client for capturing frames and recording clips
//! from IP cameras. Unlike traditional approaches that rely on ffmpeg, this implementation
//! is 100% pure Rust, offering better performance, smaller binary size, and easier deployment.
//!
//! # Architecture
//!
//! - `client`: Main RTSP client for capturing snapshots and clips
//! - `codec`: Codec detection and stream information parsing
//! - `transport`: Transport layer abstractions (TCP/UDP)
//!
//! # Example
//!
//! ```no_run
//! use camgrab_core::camera::Camera;
//! use camgrab_core::rtsp::client::RtspClient;
//! use std::path::Path;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let camera = Camera {
//!     name: "Front Door".into(),
//!     host: "192.168.1.100".into(),
//!     port: 554,
//!     username: Some("admin".into()),
//!     password: Some("pass".into()),
//!     ..Default::default()
//! };
//!
//! let mut client = RtspClient::new(&camera)?;
//! client.connect().await?;
//!
//! let result = client.snap(Path::new("/tmp/snapshot.jpg")).await?;
//! println!("Captured snapshot: {} bytes", result.size_bytes);
//! # Ok(())
//! # }
//! ```

pub mod client;
pub mod codec;
pub mod transport;
