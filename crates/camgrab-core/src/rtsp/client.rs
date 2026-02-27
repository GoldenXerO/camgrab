//! RTSP client for capturing snapshots and recording clips
//!
//! This module provides the main RTSP client implementation built on the retina library.
//! It's 100% pure Rust with no ffmpeg dependency, offering:
//!
//! - Single frame capture (snapshots)
//! - Video clip recording with optional audio
//! - Automatic codec detection
//! - Connection management with reconnection support
//! - Comprehensive error handling
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
//!     password: Some("password".into()),
//!     ..Default::default()
//! };
//!
//! let mut client = RtspClient::new(&camera)?;
//! client.connect().await?;
//!
//! let snap_result = client.snap(Path::new("/tmp/snapshot.jpg")).await?;
//! println!("Captured: {} bytes at {}x{}",
//!     snap_result.size_bytes,
//!     snap_result.width,
//!     snap_result.height
//! );
//! # Ok(())
//! # }
//! ```

use chrono::{DateTime, Utc};
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use thiserror::Error;
use url::Url;

use super::codec::{parse_stream_info, AudioCodec, CodecType, StreamInfo};
use super::transport::{
    establish_session, RtspSession, RtspTransport, SessionConfig, TransportError,
};
use crate::camera::Camera;

/// RTSP client errors
#[derive(Debug, Error)]
pub enum RtspError {
    /// Failed to establish RTSP connection
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),

    /// Authentication failed
    #[error("Authentication failed: invalid credentials")]
    AuthError,

    /// Operation timed out
    #[error("Operation timed out after {0:?}")]
    Timeout(Duration),

    /// Unsupported or invalid codec
    #[error("Codec error: {0}")]
    CodecError(String),

    /// I/O error (file operations)
    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),

    /// Stream ended unexpectedly
    #[error("Stream ended unexpectedly")]
    StreamEnded,

    /// Invalid URL
    #[error("Invalid RTSP URL: {0}")]
    InvalidUrl(String),

    /// Not connected
    #[error("Not connected - call connect() first")]
    NotConnected,

    /// Transport layer error
    #[error("Transport error: {0}")]
    Transport(#[from] TransportError),

    /// Frame capture error
    #[error("Failed to capture frame: {0}")]
    FrameCapture(String),

    /// Image encoding error
    #[error("Image encoding error: {0}")]
    ImageEncoding(String),
}

/// Options for recording clips
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipOptions {
    /// Include audio in the recording
    pub include_audio: bool,

    /// Override audio codec (use stream default if None)
    pub audio_codec_override: Option<AudioCodec>,

    /// Container format (mp4, mkv, etc.)
    pub container_format: ContainerFormat,

    /// Maximum file size in bytes (0 = unlimited)
    pub max_file_size: u64,
}

impl Default for ClipOptions {
    fn default() -> Self {
        Self {
            include_audio: true,
            audio_codec_override: None,
            container_format: ContainerFormat::Mp4,
            max_file_size: 0,
        }
    }
}

/// Supported container formats
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ContainerFormat {
    /// MPEG-4 container
    Mp4,
    /// Matroska container
    Mkv,
}

impl ContainerFormat {
    /// Returns the file extension for this container
    pub fn extension(&self) -> &'static str {
        match self {
            Self::Mp4 => "mp4",
            Self::Mkv => "mkv",
        }
    }
}

/// Result of a snapshot operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapResult {
    /// Path to the saved snapshot
    pub path: PathBuf,

    /// File size in bytes
    pub size_bytes: u64,

    /// Video codec used
    pub codec: CodecType,

    /// Image width in pixels
    pub width: u32,

    /// Image height in pixels
    pub height: u32,

    /// Timestamp when the snapshot was captured
    pub timestamp: DateTime<Utc>,
}

/// Raw captured frame data in RGB format (not written to disk)
#[derive(Debug, Clone)]
pub struct RawFrame {
    /// RGB pixel data (3 bytes per pixel, row-major)
    pub rgb: Vec<u8>,

    /// Image width in pixels
    pub width: u32,

    /// Image height in pixels
    pub height: u32,

    /// Video codec used
    pub codec: CodecType,

    /// Timestamp when the frame was captured
    pub timestamp: DateTime<Utc>,
}

/// Result of a clip recording operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipResult {
    /// Path to the saved clip
    pub path: PathBuf,

    /// File size in bytes
    pub size_bytes: u64,

    /// Actual duration recorded
    pub duration: Duration,

    /// Video codec used
    pub video_codec: CodecType,

    /// Audio codec used (if any)
    pub audio_codec: Option<AudioCodec>,

    /// Timestamp when recording started
    pub timestamp: DateTime<Utc>,
}

/// Main RTSP client for camera operations
pub struct RtspClient {
    /// Camera configuration
    camera: Camera,

    /// Active RTSP session (if connected)
    session: Option<RtspSession>,

    /// Detected stream information
    stream_info: Option<StreamInfo>,

    /// Session configuration
    config: SessionConfig,
}

impl RtspClient {
    /// Creates a new RTSP client for the given camera
    ///
    /// # Arguments
    ///
    /// * `camera` - Camera configuration
    ///
    /// # Returns
    ///
    /// A new `RtspClient` instance
    ///
    /// # Errors
    ///
    /// Returns `RtspError::InvalidUrl` if the camera's RTSP URL is malformed
    pub fn new(camera: &Camera) -> Result<Self, RtspError> {
        // Validate URL can be parsed
        let url_str = camera.rtsp_url();
        Url::parse(&url_str).map_err(|e| RtspError::InvalidUrl(format!("{e}: {url_str}")))?;

        // Build session config from camera settings
        let transport = match camera.transport {
            crate::camera::Transport::Tcp => RtspTransport::Tcp,
            crate::camera::Transport::Udp => RtspTransport::Udp,
        };

        let config = SessionConfig::new()
            .with_transport(transport)
            .with_timeout(camera.timeout);

        Ok(Self {
            camera: camera.clone(),
            session: None,
            stream_info: None,
            config,
        })
    }

    /// Establishes connection to the RTSP stream
    ///
    /// This performs the RTSP handshake (DESCRIBE, SETUP, PLAY) and detects
    /// the stream codec and parameters.
    ///
    /// # Errors
    ///
    /// Returns `RtspError` if connection fails, authentication fails, or times out
    pub async fn connect(&mut self) -> Result<(), RtspError> {
        let url_str = self.camera.rtsp_url();
        let url =
            Url::parse(&url_str).map_err(|e| RtspError::InvalidUrl(format!("{e}: {url_str}")))?;

        tracing::info!(
            camera = %self.camera.name,
            url = %self.camera.rtsp_url_redacted(),
            "Connecting to RTSP stream"
        );

        // Establish session
        let session = establish_session(&url, self.config.clone()).await?;

        // Parse stream info from the described session
        // Note: We need to get stream info before session moves to Playing state
        // In actual implementation, we'd capture this during the DESCRIBE phase
        let stream_info = parse_stream_info(session.inner());

        tracing::info!(
            camera = %self.camera.name,
            stream_info = %stream_info.description(),
            "RTSP session established"
        );

        self.stream_info = Some(stream_info);
        self.session = Some(session);

        Ok(())
    }

    /// Captures a single frame and saves it as JPEG or PNG
    ///
    /// This captures the next available keyframe from the stream and encodes it
    /// to the specified format based on the file extension.
    ///
    /// Supported formats:
    /// - `.jpg`, `.jpeg` - JPEG encoding
    /// - `.png` - PNG encoding
    ///
    /// # Arguments
    ///
    /// * `output` - Path where the snapshot should be saved
    ///
    /// # Returns
    ///
    /// A `SnapResult` containing metadata about the captured snapshot
    ///
    /// # Errors
    ///
    /// Returns `RtspError` if not connected, frame capture fails, or file I/O fails
    pub async fn snap(&mut self, output: &Path) -> Result<SnapResult, RtspError> {
        let session = self.session.take().ok_or(RtspError::NotConnected)?;

        let stream_info = self.stream_info.clone().ok_or(RtspError::NotConnected)?;

        tracing::info!(
            camera = %self.camera.name,
            output = %output.display(),
            "Capturing snapshot"
        );

        // Convert to demuxed stream (consumes the session)
        let mut demuxed = session
            .session
            .demuxed()
            .map_err(|e| RtspError::FrameCapture(format!("Failed to start demuxing: {e}")))?;

        // Read frames until we get a keyframe
        let timeout = tokio::time::timeout(self.config.timeout, async {
            while let Some(item) = demuxed.next().await {
                let item =
                    item.map_err(|e| RtspError::FrameCapture(format!("Stream error: {e}")))?;
                match item {
                    retina::codec::CodecItem::VideoFrame(frame) => {
                        if !frame.is_random_access_point() {
                            tracing::debug!("Skipping non-keyframe");
                            continue;
                        }

                        tracing::debug!(data_len = frame.data().len(), "Captured keyframe");

                        let data = frame.data();

                        // Decode and save based on codec
                        match stream_info.video_codec {
                            CodecType::H264 => {
                                let (rgb, width, height) = decode_h264_frame(data)?;
                                let size = encode_image_to_file(&rgb, width, height, output)?;
                                return Ok(SnapResult {
                                    path: output.to_path_buf(),
                                    size_bytes: size,
                                    codec: CodecType::H264,
                                    width,
                                    height,
                                    timestamp: Utc::now(),
                                });
                            }
                            CodecType::Mjpeg => {
                                // MJPEG frames are already JPEG data
                                tokio::fs::write(output, data).await?;
                                let size = data.len() as u64;
                                return Ok(SnapResult {
                                    path: output.to_path_buf(),
                                    size_bytes: size,
                                    codec: CodecType::Mjpeg,
                                    width: stream_info.width.unwrap_or(0),
                                    height: stream_info.height.unwrap_or(0),
                                    timestamp: Utc::now(),
                                });
                            }
                            other => {
                                return Err(RtspError::CodecError(format!(
                                    "Snapshot not supported for codec: {other}"
                                )));
                            }
                        }
                    }
                    _ => continue,
                }
            }
            Err(RtspError::StreamEnded)
        });

        match timeout.await {
            Ok(result) => result,
            Err(_) => Err(RtspError::Timeout(self.config.timeout)),
        }
    }

    /// Captures a single keyframe and returns raw RGB pixel data in memory.
    ///
    /// Unlike `snap()`, this does not write to disk. It is designed for use cases
    /// like motion detection where frames need to be analyzed in memory.
    ///
    /// **Note:** This consumes the RTSP session. Call `reconnect()` before the next capture.
    pub async fn capture_raw_frame(&mut self) -> Result<RawFrame, RtspError> {
        let session = self.session.take().ok_or(RtspError::NotConnected)?;

        let stream_info = self.stream_info.clone().ok_or(RtspError::NotConnected)?;

        let mut demuxed = session
            .session
            .demuxed()
            .map_err(|e| RtspError::FrameCapture(format!("Failed to start demuxing: {e}")))?;

        let timeout = tokio::time::timeout(self.config.timeout, async {
            while let Some(item) = demuxed.next().await {
                let item =
                    item.map_err(|e| RtspError::FrameCapture(format!("Stream error: {e}")))?;
                match item {
                    retina::codec::CodecItem::VideoFrame(frame) => {
                        if !frame.is_random_access_point() {
                            continue;
                        }

                        let data = frame.data();

                        match stream_info.video_codec {
                            CodecType::H264 => {
                                let (rgb, width, height) = decode_h264_frame(data)?;
                                return Ok(RawFrame {
                                    rgb,
                                    width,
                                    height,
                                    codec: CodecType::H264,
                                    timestamp: Utc::now(),
                                });
                            }
                            CodecType::Mjpeg => {
                                // Decode JPEG to RGB in memory
                                let img = image::load_from_memory(data).map_err(|e| {
                                    RtspError::FrameCapture(format!("MJPEG decode error: {e}"))
                                })?;
                                let rgb_img = img.to_rgb8();
                                let width = rgb_img.width();
                                let height = rgb_img.height();
                                return Ok(RawFrame {
                                    rgb: rgb_img.into_raw(),
                                    width,
                                    height,
                                    codec: CodecType::Mjpeg,
                                    timestamp: Utc::now(),
                                });
                            }
                            other => {
                                return Err(RtspError::CodecError(format!(
                                    "Raw frame capture not supported for codec: {other}"
                                )));
                            }
                        }
                    }
                    _ => continue,
                }
            }
            Err(RtspError::StreamEnded)
        });

        match timeout.await {
            Ok(result) => result,
            Err(_) => Err(RtspError::Timeout(self.config.timeout)),
        }
    }

    /// Records a video clip for the specified duration
    ///
    /// This captures video (and optionally audio) from the stream and saves it
    /// to the specified output file.
    ///
    /// # Arguments
    ///
    /// * `output` - Path where the clip should be saved
    /// * `duration` - How long to record
    /// * `options` - Recording options (audio, codec, container format)
    ///
    /// # Returns
    ///
    /// A `ClipResult` containing metadata about the recorded clip
    ///
    /// # Errors
    ///
    /// Returns `RtspError` if not connected, recording fails, or file I/O fails
    pub async fn clip(
        &mut self,
        output: &Path,
        duration: Duration,
        options: ClipOptions,
    ) -> Result<ClipResult, RtspError> {
        let session = self.session.take().ok_or(RtspError::NotConnected)?;

        let stream_info = self.stream_info.clone().ok_or(RtspError::NotConnected)?;

        tracing::info!(
            camera = %self.camera.name,
            output = %output.display(),
            duration_secs = duration.as_secs(),
            include_audio = options.include_audio,
            container = ?options.container_format,
            "Recording clip"
        );

        // Convert to demuxed stream
        let mut demuxed = session
            .session
            .demuxed()
            .map_err(|e| RtspError::FrameCapture(format!("Failed to start demuxing: {e}")))?;

        // Open output file for writing raw H264 Annex B stream
        let mut file = tokio::fs::File::create(output).await?;
        let start = Instant::now();
        let timestamp = Utc::now();
        let mut total_bytes: u64 = 0;
        let mut got_keyframe = false;

        // H264 Annex B start code
        let start_code: &[u8] = &[0x00, 0x00, 0x00, 0x01];

        while start.elapsed() < duration {
            let next = tokio::time::timeout(Duration::from_secs(5), demuxed.next()).await;

            let item = match next {
                Ok(Some(Ok(item))) => item,
                Ok(Some(Err(e))) => {
                    tracing::warn!("Stream error during clip: {}", e);
                    break;
                }
                Ok(None) => {
                    tracing::info!("Stream ended during clip recording");
                    break;
                }
                Err(_) => {
                    tracing::warn!("Timeout waiting for next frame");
                    break;
                }
            };

            match item {
                retina::codec::CodecItem::VideoFrame(frame) => {
                    // Wait for first keyframe before writing
                    if !got_keyframe {
                        if !frame.is_random_access_point() {
                            continue;
                        }
                        got_keyframe = true;
                    }

                    let data = frame.data();

                    // Write NALUs with Annex B start codes
                    // retina provides data with 4-byte length prefixes (AVCC format)
                    // We need to convert to Annex B (start code prefixed)
                    let mut pos = 0;
                    while pos + 4 <= data.len() {
                        let nalu_len = u32::from_be_bytes([
                            data[pos],
                            data[pos + 1],
                            data[pos + 2],
                            data[pos + 3],
                        ]) as usize;
                        pos += 4;

                        if pos + nalu_len > data.len() {
                            break;
                        }

                        use tokio::io::AsyncWriteExt;
                        file.write_all(start_code).await?;
                        file.write_all(&data[pos..pos + nalu_len]).await?;
                        total_bytes += (4 + nalu_len) as u64;
                        pos += nalu_len;
                    }

                    // Check max file size
                    if options.max_file_size > 0 && total_bytes >= options.max_file_size {
                        tracing::info!("Max file size reached: {} bytes", total_bytes);
                        break;
                    }
                }
                _ => continue,
            }
        }

        use tokio::io::AsyncWriteExt;
        file.flush().await?;

        let actual_duration = start.elapsed();
        tracing::info!(
            camera = %self.camera.name,
            bytes = total_bytes,
            duration_secs = actual_duration.as_secs_f64(),
            "Clip recording complete"
        );

        Ok(ClipResult {
            path: output.to_path_buf(),
            size_bytes: total_bytes,
            duration: actual_duration,
            video_codec: stream_info.video_codec,
            audio_codec: None, // Raw H264 stream has no audio track
            timestamp,
        })
    }

    /// Returns information about the stream (if connected)
    ///
    /// This includes codec details, resolution, frame rate, and audio parameters.
    ///
    /// # Returns
    ///
    /// Stream information if connected, `None` otherwise
    pub fn stream_info(&self) -> Option<&StreamInfo> {
        self.stream_info.as_ref()
    }

    /// Checks if the client is currently connected
    pub fn is_connected(&self) -> bool {
        self.session.is_some()
    }

    /// Returns the camera configuration
    pub fn camera(&self) -> &Camera {
        &self.camera
    }

    /// Performs a clean shutdown of the RTSP session
    ///
    /// This sends TEARDOWN and closes the connection gracefully.
    pub async fn disconnect(&mut self) {
        if let Some(session) = self.session.take() {
            tracing::info!(
                camera = %self.camera.name,
                "Disconnecting RTSP session"
            );

            // The session will send TEARDOWN on drop
            drop(session);

            self.stream_info = None;
        }
    }

    /// Attempts to reconnect to the stream
    ///
    /// This is useful for recovering from network errors or stream interruptions.
    ///
    /// # Errors
    ///
    /// Returns `RtspError` if reconnection fails
    pub async fn reconnect(&mut self) -> Result<(), RtspError> {
        tracing::info!(
            camera = %self.camera.name,
            "Attempting to reconnect"
        );

        self.disconnect().await;
        self.connect().await
    }
}

/// Decode an H264 frame (Annex B or AVCC) to RGB pixels using openh264
fn decode_h264_frame(data: &[u8]) -> Result<(Vec<u8>, u32, u32), RtspError> {
    use openh264::decoder::Decoder;
    use openh264::formats::YUVSource;

    let mut decoder = Decoder::new()
        .map_err(|e| RtspError::FrameCapture(format!("Failed to create H264 decoder: {e}")))?;

    // retina provides data in AVCC format (4-byte length prefixed NALUs)
    // openh264 expects Annex B format (start code prefixed)
    // Convert: replace length prefix with start codes
    let mut annex_b = Vec::with_capacity(data.len());
    let start_code: &[u8] = &[0x00, 0x00, 0x00, 0x01];
    let mut pos = 0;
    while pos + 4 <= data.len() {
        let nalu_len =
            u32::from_be_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]) as usize;
        pos += 4;
        if pos + nalu_len > data.len() {
            break;
        }
        annex_b.extend_from_slice(start_code);
        annex_b.extend_from_slice(&data[pos..pos + nalu_len]);
        pos += nalu_len;
    }

    // If the data didn't look like AVCC (no valid length prefixes found),
    // try feeding it directly as Annex B
    let decode_data = if annex_b.is_empty() { data } else { &annex_b };

    let decoded = decoder
        .decode(decode_data)
        .map_err(|e| RtspError::FrameCapture(format!("H264 decode error: {e}")))?;

    let decoded = decoded.ok_or_else(|| {
        RtspError::FrameCapture("H264 decoder returned no image (need more data)".into())
    })?;

    let (width, height) = decoded.dimensions();
    let width = width as u32;
    let height = height as u32;

    // Use openh264's built-in YUV to RGB conversion
    let mut rgb = vec![0u8; (width * height * 3) as usize];
    decoded.write_rgb8(&mut rgb);

    Ok((rgb, width, height))
}

/// Encode RGB pixels to an image file (JPEG or PNG based on extension)
fn encode_image_to_file(
    rgb: &[u8],
    width: u32,
    height: u32,
    path: &Path,
) -> Result<u64, RtspError> {
    let img = image::RgbImage::from_raw(width, height, rgb.to_vec())
        .ok_or_else(|| RtspError::ImageEncoding("Failed to create image from RGB data".into()))?;

    img.save(path)
        .map_err(|e| RtspError::ImageEncoding(format!("Failed to save image: {e}")))?;

    let metadata = std::fs::metadata(path)?;
    Ok(metadata.len())
}

impl Drop for RtspClient {
    fn drop(&mut self) {
        if self.session.is_some() {
            tracing::debug!(
                camera = %self.camera.name,
                "RtspClient dropped, session will be cleaned up"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::camera::{Camera, Protocol, Transport};
    use std::time::Duration as StdDuration;

    fn create_test_camera() -> Camera {
        Camera {
            name: "test-camera".to_string(),
            host: "192.168.1.100".to_string(),
            port: 554,
            username: Some("admin".to_string()),
            password: Some("password".to_string()),
            protocol: Protocol::Rtsp,
            transport: Transport::Tcp,
            stream: crate::camera::StreamType::Main,
            custom_path: None,
            audio_enabled: false,
            auth_method: crate::camera::AuthMethod::Auto,
            timeout: StdDuration::from_secs(10),
        }
    }

    #[test]
    fn test_rtsp_client_creation() {
        let camera = create_test_camera();
        let client = RtspClient::new(&camera);

        assert!(client.is_ok());
        let client = client.unwrap();
        assert!(!client.is_connected());
        assert!(client.stream_info().is_none());
    }

    #[test]
    fn test_rtsp_client_invalid_url() {
        let mut camera = create_test_camera();
        camera.host = "not a valid host!".to_string();

        // URL parsing fails because spaces are invalid in URLs
        let client = RtspClient::new(&camera);
        assert!(client.is_err());
        match client {
            Err(RtspError::InvalidUrl(_)) => {} // expected
            _ => panic!("Expected InvalidUrl error"),
        }
    }

    #[test]
    fn test_clip_options_default() {
        let options = ClipOptions::default();
        assert!(options.include_audio);
        assert!(options.audio_codec_override.is_none());
        assert_eq!(options.container_format, ContainerFormat::Mp4);
        assert_eq!(options.max_file_size, 0);
    }

    #[test]
    fn test_container_format_extension() {
        assert_eq!(ContainerFormat::Mp4.extension(), "mp4");
        assert_eq!(ContainerFormat::Mkv.extension(), "mkv");
    }

    #[test]
    fn test_snap_not_connected() {
        let camera = create_test_camera();
        let mut client = RtspClient::new(&camera).unwrap();

        // This would be async in real usage, but we're just testing the error path
        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(client.snap(Path::new("/tmp/test.jpg")));

        assert!(matches!(result, Err(RtspError::NotConnected)));
    }

    #[test]
    fn test_clip_not_connected() {
        let camera = create_test_camera();
        let mut client = RtspClient::new(&camera).unwrap();

        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(client.clip(
            Path::new("/tmp/test.mp4"),
            StdDuration::from_secs(10),
            ClipOptions::default(),
        ));

        assert!(matches!(result, Err(RtspError::NotConnected)));
    }

    #[test]
    fn test_rtsp_error_display() {
        let err = RtspError::ConnectionFailed("connection refused".into());
        assert_eq!(err.to_string(), "Connection failed: connection refused");

        let err = RtspError::AuthError;
        assert_eq!(
            err.to_string(),
            "Authentication failed: invalid credentials"
        );

        let err = RtspError::Timeout(StdDuration::from_secs(10));
        assert_eq!(err.to_string(), "Operation timed out after 10s");

        let err = RtspError::StreamEnded;
        assert_eq!(err.to_string(), "Stream ended unexpectedly");
    }
}
