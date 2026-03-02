# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [1.0.1] - 2026-03-02

### Fixed

- Add README to crates.io packages (readme path fix for workspace crates)
- Add crates.io version badge to README

## [1.0.0] - 2026-02-27

### Added

- **RTSP snapshot capture** (`camgrab snap`) - Single keyframe capture with H.264 decode via openh264 and MJPEG passthrough
- **RTSP clip recording** (`camgrab clip`) - Duration-based video recording with AVCC-to-Annex B conversion
- **Motion detection** (`camgrab watch`) - Polling-based frame differencing with configurable threshold, cooldown, zones, and fire-and-forget action commands
- **Camera management** (`camgrab add`, `camgrab list`) - TOML-based configuration at `~/.config/camgrab/config.toml` with upsert semantics and 0o600 permissions
- **ONVIF device discovery** (`camgrab discover`) - WS-Discovery via UDP multicast for automatic camera detection
- **ONVIF PTZ control** (`camgrab ptz`) - Pan, tilt, zoom, absolute/relative/continuous moves, preset management via SOAP
- **Health diagnostics** (`camgrab doctor`) - Config validation, TCP connectivity checks, real RTSP connection probing with retry logic
- **Daemon mode** (`camgrab-daemon`) - REST API server with bearer token auth, scheduled jobs (cron-based snap/clip/health-check), and motion event buffering
- **Scheduler** - Cron expression-based job scheduling for automated snap, clip, health check, and custom shell commands
- **Notification system** - Webhook (HTTP POST), MQTT (publish), and email (SMTP via lettre) notification backends with routing
- **Storage backends** - Local filesystem with atomic writes and S3/MinIO cloud storage
- **Error classification** - Automatic error categorization (network, auth, codec, timeout, I/O) with user-friendly suggestions
- **Cross-platform CI/CD** - GitHub Actions for Linux, macOS, Windows testing and multi-architecture release builds
- **Pure Rust** - No ffmpeg or external C library dependencies; RTSP via retina, H.264 decode via openh264

### Architecture

- Three-crate workspace: `camgrab-core` (library), `camgrab-cli` (binary), `camgrab-daemon` (REST server)
- Async throughout via tokio runtime
- 159 unit tests and doc-tests across all crates

[1.0.1]: https://github.com/justinhuangcode/camgrab/releases/tag/v1.0.1
[1.0.0]: https://github.com/justinhuangcode/camgrab/releases/tag/v1.0.0
