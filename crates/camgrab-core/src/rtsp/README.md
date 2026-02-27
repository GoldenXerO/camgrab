# RTSP Client Module

This module provides a complete RTSP client implementation for the camgrab-core crate, built on the [retina](https://github.com/scottlamb/retina) library.

## Key Features

- **100% Pure Rust** - No ffmpeg dependency, smaller binaries, easier deployment
- **Async/Await** - Built on tokio for efficient concurrent operations
- **Comprehensive Codec Support** - H.264, H.265, MJPEG video; AAC, PCMA, PCMU, Opus audio
- **Transport Flexibility** - TCP (reliable) or UDP (low latency)
- **Automatic Codec Detection** - Parses SDP from DESCRIBE response
- **Production-Ready Error Handling** - Detailed error types with thiserror
- **Connection Management** - Timeouts, reconnection, graceful teardown

## Architecture

The module is organized into three submodules:

### 1. `codec` - Codec Detection and Stream Information

- **`CodecType`** - Video codecs: H264, H265, Mjpeg, Unknown
- **`AudioCodec`** - Audio codecs: Aac, Pcma, Pcmu, Opus, Unknown
- **`StreamInfo`** - Structured metadata about the stream
  - Video codec, dimensions, frame rate
  - Audio codec, sample rate, channels
- **`parse_stream_info()`** - Extracts codec information from retina session

### 2. `transport` - Transport Layer Abstractions

- **`RtspTransport`** - TCP or UDP protocol selection
- **`SessionConfig`** - Connection configuration
  - Transport protocol
  - Timeout settings
  - Keep-alive intervals
  - TLS options
- **`RtspSession`** - Wrapper around retina's Playing session
- **`establish_session()`** - Handles RTSP handshake (OPTIONS, DESCRIBE, SETUP, PLAY)

### 3. `client` - Main RTSP Client

- **`RtspClient`** - High-level API for camera operations
  - `new()` - Create client from camera config
  - `connect()` - Establish RTSP session
  - `snap()` - Capture single frame as JPEG/PNG
  - `clip()` - Record video clip with optional audio
  - `disconnect()` - Clean shutdown
  - `reconnect()` - Recover from errors

- **`SnapResult`** - Metadata from snapshot capture
- **`ClipResult`** - Metadata from clip recording
- **`ClipOptions`** - Recording configuration
- **`RtspError`** - Comprehensive error types

## Usage Examples

### Basic Snapshot Capture

```rust
use camgrab_core::camera::Camera;
use camgrab_core::rtsp::client::RtspClient;
use std::path::Path;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let camera = Camera {
        name: "Front Door".into(),
        host: "192.168.1.100".into(),
        port: 554,
        username: Some("admin".into()),
        password: Some("password".into()),
        ..Default::default()
    };

    let mut client = RtspClient::new(&camera)?;
    client.connect().await?;

    let result = client.snap(Path::new("/tmp/snapshot.jpg")).await?;
    println!("Captured: {} bytes", result.size_bytes);

    client.disconnect().await;
    Ok(())
}
```

### Recording a Clip

```rust
use camgrab_core::rtsp::client::{RtspClient, ClipOptions, ContainerFormat};
use std::time::Duration;

let options = ClipOptions {
    include_audio: true,
    audio_codec_override: None,
    container_format: ContainerFormat::Mp4,
    max_file_size: 0,
};

let result = client.clip(
    Path::new("/tmp/clip.mp4"),
    Duration::from_secs(30),
    options
).await?;

println!("Recorded {} bytes in {:.2}s",
    result.size_bytes,
    result.duration.as_secs_f64()
);
```

### Stream Information

```rust
client.connect().await?;

if let Some(info) = client.stream_info() {
    println!("Stream: {}", info.description());
    // Output: "H.264, 1920x1080, 30.0fps, audio: AAC"
}
```

### Error Handling

```rust
use camgrab_core::rtsp::client::RtspError;

match client.connect().await {
    Ok(_) => println!("Connected"),
    Err(RtspError::AuthError) => eprintln!("Invalid credentials"),
    Err(RtspError::Timeout(d)) => eprintln!("Timeout after {:?}", d),
    Err(RtspError::ConnectionFailed(msg)) => eprintln!("Connection failed: {}", msg),
    Err(e) => eprintln!("Other error: {}", e),
}
```

## Implementation Status

### ✅ Completed

- Module structure and organization
- Type definitions for codecs, transports, and results
- Camera configuration integration
- Session establishment (DESCRIBE, SETUP, PLAY)
- Codec detection from SDP
- Error types and handling
- Comprehensive documentation
- Unit tests for codec detection, URL parsing, serialization

### Frame Capture (snap)

Implemented via `RtspClient::snap()` and `RtspClient::capture_raw_frame()`:

1. **Demuxed Stream** - `session.demuxed()` converts the session to an async stream of `CodecItem`
2. **Keyframe Detection** - Waits for `frame.is_random_access_point()` (I-frame)
3. **AVCC-to-Annex B** - Converts 4-byte length-prefixed NALUs to start-code-prefixed format
4. **H.264 Decoding** - openh264 `Decoder::decode()` followed by `write_rgb8()` for YUV-to-RGB
5. **MJPEG Passthrough** - MJPEG frames are already JPEG data, saved directly
6. **Image Encoding** - `image` crate saves RGB pixels as JPEG or PNG based on extension

### Clip Recording (clip)

Implemented via `RtspClient::clip()`:

1. **Raw H.264 Stream** - Writes Annex B format (start code + NALU) directly to file
2. **AVCC Conversion** - Parses 4-byte length prefixes from retina frames, replaces with `00 00 00 01` start codes
3. **Duration Tracking** - Stops recording after the specified duration elapses
4. **Size Limiting** - Respects `max_file_size` option (0 = unlimited)
5. **Async I/O** - Uses `tokio::fs::File` with buffered writes and explicit flush

## Design Decisions

### Why retina over ffmpeg?

1. **Pure Rust** - No C dependencies, easier cross-compilation
2. **Smaller Binaries** - No large ffmpeg libraries
3. **Better Error Handling** - Rust error types instead of C error codes
4. **Type Safety** - Compile-time guarantees
5. **Easier Deployment** - No system dependencies

### Why TCP default over UDP?

1. **Reliability** - No packet loss
2. **Firewall Friendly** - Works through NAT/firewalls
3. **Easier Debugging** - Wireshark can reassemble streams
4. **Camera Support** - Most IP cameras prioritize TCP

UDP is still available for low-latency use cases.

### Error Handling Strategy

- **Specific Errors** - Distinct error types for different failure modes
- **Context Preservation** - Original error messages retained
- **Actionable** - Errors suggest what went wrong
- **Logged** - Important events traced at appropriate levels

## Testing

Run the unit tests:

```bash
cargo test --package camgrab-core --lib rtsp
```

Run the examples:

```bash
cargo run --example rtsp_snapshot
cargo run --example rtsp_clip
```

## Future Enhancements

- [ ] Frame buffer for motion detection integration
- [ ] Streaming to multiple outputs simultaneously
- [ ] Hardware decoder support (VAAPI, NVDEC)
- [ ] Adaptive bitrate handling
- [ ] Multi-stream support (main + sub-stream)
- [ ] RTSP ANNOUNCE support (for publishing)
- [ ] Statistics collection (bitrate, dropped frames)

## Dependencies

- `retina` - RTSP protocol implementation
- `tokio` - Async runtime
- `url` - URL parsing
- `thiserror` - Error handling
- `serde` - Serialization
- `chrono` - Timestamps
- `tracing` - Logging
- `image` - Image encoding (for snapshots)

## References

- [Retina Documentation](https://docs.rs/retina/)
- [RTSP RFC 2326](https://tools.ietf.org/html/rfc2326)
- [RTSP 2.0 RFC 7826](https://tools.ietf.org/html/rfc7826)
- [SDP RFC 4566](https://tools.ietf.org/html/rfc4566)
- [H.264 RTP Payload RFC 6184](https://tools.ietf.org/html/rfc6184)
- [H.265 RTP Payload RFC 7798](https://tools.ietf.org/html/rfc7798)
