//! Codec detection and stream information
//!
//! This module handles codec identification from RTSP DESCRIBE responses (SDP)
//! and provides structured information about video and audio streams.

use serde::{Deserialize, Serialize};
use std::fmt;

/// Video codec types supported by camgrab
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CodecType {
    /// H.264 / AVC
    H264,
    /// H.265 / HEVC
    H265,
    /// Motion JPEG
    Mjpeg,
    /// Unknown video codec
    Unknown,
}

impl fmt::Display for CodecType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::H264 => write!(f, "H.264"),
            Self::H265 => write!(f, "H.265"),
            Self::Mjpeg => write!(f, "MJPEG"),
            Self::Unknown => write!(f, "Unknown"),
        }
    }
}

impl CodecType {
    /// Detects codec from MIME type or encoding name
    pub fn from_encoding_name(name: &str) -> Self {
        let name_lower = name.to_lowercase();
        match name_lower.as_str() {
            "h264" | "avc" | "h.264" => Self::H264,
            "h265" | "hevc" | "h.265" => Self::H265,
            "jpeg" | "mjpeg" | "motion-jpeg" => Self::Mjpeg,
            _ => Self::Unknown,
        }
    }

    /// Returns the file extension for this codec
    pub fn extension(&self) -> &'static str {
        match self {
            Self::H264 => "h264",
            Self::H265 => "h265",
            Self::Mjpeg => "mjpeg",
            Self::Unknown => "raw",
        }
    }
}

/// Audio codec types supported by camgrab
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AudioCodec {
    /// AAC audio
    Aac,
    /// G.711 A-law
    Pcma,
    /// G.711 μ-law
    Pcmu,
    /// Opus audio
    Opus,
    /// Unknown audio codec
    Unknown,
}

impl fmt::Display for AudioCodec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Aac => write!(f, "AAC"),
            Self::Pcma => write!(f, "PCMA"),
            Self::Pcmu => write!(f, "PCMU"),
            Self::Opus => write!(f, "Opus"),
            Self::Unknown => write!(f, "Unknown"),
        }
    }
}

impl AudioCodec {
    /// Detects audio codec from encoding name
    pub fn from_encoding_name(name: &str) -> Self {
        let name_lower = name.to_lowercase();
        match name_lower.as_str() {
            "aac" | "mpeg4-generic" => Self::Aac,
            "pcma" => Self::Pcma,
            "pcmu" => Self::Pcmu,
            "opus" => Self::Opus,
            _ => Self::Unknown,
        }
    }
}

/// Information about a media stream
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamInfo {
    /// Video codec
    pub video_codec: CodecType,

    /// Audio codec (if present)
    pub audio_codec: Option<AudioCodec>,

    /// Video width in pixels
    pub width: Option<u32>,

    /// Video height in pixels
    pub height: Option<u32>,

    /// Frames per second
    pub fps: Option<f32>,

    /// Audio sample rate (Hz)
    pub sample_rate: Option<u32>,

    /// Audio channels
    pub audio_channels: Option<u8>,
}

impl Default for StreamInfo {
    fn default() -> Self {
        Self {
            video_codec: CodecType::Unknown,
            audio_codec: None,
            width: None,
            height: None,
            fps: None,
            sample_rate: None,
            audio_channels: None,
        }
    }
}

impl StreamInfo {
    /// Creates a new StreamInfo with the given video codec
    pub fn new(video_codec: CodecType) -> Self {
        Self {
            video_codec,
            ..Default::default()
        }
    }

    /// Sets video dimensions
    #[must_use]
    pub fn with_dimensions(mut self, width: u32, height: u32) -> Self {
        self.width = Some(width);
        self.height = Some(height);
        self
    }

    /// Sets frames per second
    #[must_use]
    pub fn with_fps(mut self, fps: f32) -> Self {
        self.fps = Some(fps);
        self
    }

    /// Sets audio codec
    #[must_use]
    pub fn with_audio_codec(mut self, codec: AudioCodec) -> Self {
        self.audio_codec = Some(codec);
        self
    }

    /// Sets audio parameters
    #[must_use]
    pub fn with_audio_params(mut self, sample_rate: u32, channels: u8) -> Self {
        self.sample_rate = Some(sample_rate);
        self.audio_channels = Some(channels);
        self
    }

    /// Returns a human-readable description of the stream
    pub fn description(&self) -> String {
        let mut parts = vec![self.video_codec.to_string()];

        if let (Some(w), Some(h)) = (self.width, self.height) {
            parts.push(format!("{w}x{h}"));
        }

        if let Some(fps) = self.fps {
            parts.push(format!("{fps:.1}fps"));
        }

        if let Some(audio) = self.audio_codec {
            parts.push(format!("audio: {audio}"));
        }

        parts.join(", ")
    }
}

/// Parses stream information from retina's session description
///
/// This function extracts codec information, dimensions, and other metadata
/// from the RTSP session after DESCRIBE.
///
/// # Arguments
///
/// * `session` - The retina session containing stream metadata
///
/// # Returns
///
/// A `StreamInfo` struct with detected codec and stream parameters
pub fn parse_stream_info<S: retina::client::State>(
    session: &retina::client::Session<S>,
) -> StreamInfo {
    let mut info = StreamInfo::default();

    // Iterate through the streams in the session
    for (stream_id, stream) in session.streams().iter().enumerate() {
        // Video stream detection
        if let Some(params) = stream.parameters() {
            // Match on the ParametersRef enum to detect codec types
            match params {
                retina::codec::ParametersRef::Video(video_params) => {
                    // Extract codec information from the RFC 6381 codec string
                    let codec_str = video_params.rfc6381_codec();

                    if codec_str.starts_with("avc1") || codec_str.starts_with("avc3") {
                        info.video_codec = CodecType::H264;
                        tracing::debug!(
                            stream = stream_id,
                            codec = codec_str,
                            "Detected H.264 stream"
                        );
                    } else if codec_str.starts_with("hvc1") || codec_str.starts_with("hev1") {
                        info.video_codec = CodecType::H265;
                        tracing::debug!(
                            stream = stream_id,
                            codec = codec_str,
                            "Detected H.265 stream"
                        );
                    } else if codec_str.starts_with("mp4v") {
                        info.video_codec = CodecType::Mjpeg;
                        tracing::debug!(
                            stream = stream_id,
                            codec = codec_str,
                            "Detected MJPEG stream"
                        );
                    }

                    // Extract dimensions
                    let (width, height) = video_params.pixel_dimensions();
                    info.width = Some(width);
                    info.height = Some(height);

                    // Extract frame rate if available
                    if let Some((num, denom)) = video_params.frame_rate() {
                        if denom > 0 {
                            info.fps = Some(num as f32 / denom as f32);
                        }
                    }
                }
                retina::codec::ParametersRef::Audio(audio_params) => {
                    // Detect audio codec from RFC 6381 codec string
                    if let Some(codec_str) = audio_params.rfc6381_codec() {
                        if codec_str.starts_with("mp4a") {
                            info.audio_codec = Some(AudioCodec::Aac);
                        }
                    }

                    info.sample_rate = Some(audio_params.clock_rate());

                    tracing::debug!(
                        stream = stream_id,
                        sample_rate = audio_params.clock_rate(),
                        "Detected audio stream"
                    );
                }
                retina::codec::ParametersRef::Message(_) => {
                    // Message streams (like ONVIF metadata) - not currently handled
                    tracing::debug!(stream = stream_id, "Detected message stream");
                }
            }
        }
    }

    info
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_codec_from_encoding_name() {
        assert_eq!(CodecType::from_encoding_name("H264"), CodecType::H264);
        assert_eq!(CodecType::from_encoding_name("h264"), CodecType::H264);
        assert_eq!(CodecType::from_encoding_name("AVC"), CodecType::H264);
        assert_eq!(CodecType::from_encoding_name("H265"), CodecType::H265);
        assert_eq!(CodecType::from_encoding_name("HEVC"), CodecType::H265);
        assert_eq!(CodecType::from_encoding_name("MJPEG"), CodecType::Mjpeg);
        assert_eq!(CodecType::from_encoding_name("jpeg"), CodecType::Mjpeg);
        assert_eq!(CodecType::from_encoding_name("unknown"), CodecType::Unknown);
    }

    #[test]
    fn test_audio_codec_from_encoding_name() {
        assert_eq!(AudioCodec::from_encoding_name("AAC"), AudioCodec::Aac);
        assert_eq!(AudioCodec::from_encoding_name("aac"), AudioCodec::Aac);
        assert_eq!(
            AudioCodec::from_encoding_name("MPEG4-GENERIC"),
            AudioCodec::Aac
        );
        assert_eq!(AudioCodec::from_encoding_name("PCMA"), AudioCodec::Pcma);
        assert_eq!(AudioCodec::from_encoding_name("PCMU"), AudioCodec::Pcmu);
        assert_eq!(AudioCodec::from_encoding_name("Opus"), AudioCodec::Opus);
        assert_eq!(
            AudioCodec::from_encoding_name("unknown"),
            AudioCodec::Unknown
        );
    }

    #[test]
    fn test_codec_extension() {
        assert_eq!(CodecType::H264.extension(), "h264");
        assert_eq!(CodecType::H265.extension(), "h265");
        assert_eq!(CodecType::Mjpeg.extension(), "mjpeg");
        assert_eq!(CodecType::Unknown.extension(), "raw");
    }

    #[test]
    fn test_stream_info_builder() {
        let info = StreamInfo::new(CodecType::H264)
            .with_dimensions(1920, 1080)
            .with_fps(30.0)
            .with_audio_codec(AudioCodec::Aac)
            .with_audio_params(48000, 2);

        assert_eq!(info.video_codec, CodecType::H264);
        assert_eq!(info.width, Some(1920));
        assert_eq!(info.height, Some(1080));
        assert_eq!(info.fps, Some(30.0));
        assert_eq!(info.audio_codec, Some(AudioCodec::Aac));
        assert_eq!(info.sample_rate, Some(48000));
        assert_eq!(info.audio_channels, Some(2));
    }

    #[test]
    fn test_stream_info_description() {
        let info = StreamInfo::new(CodecType::H264)
            .with_dimensions(1920, 1080)
            .with_fps(30.0)
            .with_audio_codec(AudioCodec::Aac);

        let desc = info.description();
        assert!(desc.contains("H.264"));
        assert!(desc.contains("1920x1080"));
        assert!(desc.contains("30.0fps"));
        assert!(desc.contains("AAC"));
    }

    #[test]
    fn test_codec_display() {
        assert_eq!(format!("{}", CodecType::H264), "H.264");
        assert_eq!(format!("{}", CodecType::H265), "H.265");
        assert_eq!(format!("{}", CodecType::Mjpeg), "MJPEG");
        assert_eq!(format!("{}", AudioCodec::Aac), "AAC");
    }
}
