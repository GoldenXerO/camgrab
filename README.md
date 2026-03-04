# CamGrab

**English** | [中文](./README_CN.md)

[![CI](https://github.com/justinhuangcode/camgrab/actions/workflows/ci.yml/badge.svg)](https://github.com/justinhuangcode/camgrab/actions/workflows/ci.yml)
[![Release](https://github.com/justinhuangcode/camgrab/actions/workflows/release.yml/badge.svg)](https://github.com/justinhuangcode/camgrab/actions/workflows/release.yml)
[![Crates.io](https://img.shields.io/crates/v/camgrab-cli?style=flat-square)](https://crates.io/crates/camgrab-cli)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg?style=flat-square)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.88%2B-orange.svg?style=flat-square&logo=rust&logoColor=white)](https://www.rust-lang.org)
[![Docker](https://img.shields.io/badge/ghcr.io-camgrab-blue?style=flat-square&logo=docker&logoColor=white)](https://ghcr.io/justinhuangcode/camgrab)
[![GitHub Stars](https://img.shields.io/github/stars/justinhuangcode/camgrab?style=flat-square&logo=github)](https://github.com/justinhuangcode/camgrab/stargazers)
[![Last Commit](https://img.shields.io/github/last-commit/justinhuangcode/camgrab?style=flat-square)](https://github.com/justinhuangcode/camgrab/commits/main)
[![Issues](https://img.shields.io/github/issues/justinhuangcode/camgrab?style=flat-square)](https://github.com/justinhuangcode/camgrab/issues)
[![Platform](https://img.shields.io/badge/platform-Linux%20%7C%20macOS%20%7C%20Windows-lightgrey?style=flat-square)](https://github.com/justinhuangcode/camgrab)

A modern CLI tool for capturing snapshots, recording clips, detecting motion, and managing IP cameras via RTSP/ONVIF.

## Why CamGrab?

Most camera tools either depend on heavy external binaries (ffmpeg) or are full-blown NVR systems you have to self-host. camgrab is a single binary with zero external dependencies — connect to your cameras and start working in seconds.

| | camgrab | ffmpeg + cron | motion | Frigate | ZoneMinder |
|---|---|---|---|---|---|
| Single binary, zero deps | **Yes** | No (ffmpeg ~50MB) | No (C libs) | No (Docker stack) | No (LAMP stack) |
| RTSP snapshot + clip | Built-in | Manual scripting | No clips | Via API | Via API |
| Native motion detection | Built-in (zones + filters) | Scene filter only | Yes | ML-based | Zones |
| ONVIF PTZ control | Built-in | No | No | Via MQTT | Limited |
| Multi-camera concurrent | Async (tokio) | Sequential scripts | Per-process | Yes | Yes |
| Daemon + cron scheduler | Built-in | External cron | Built-in | Always-on | Always-on |
| Notifications | Webhook / MQTT / Email | DIY | Limited | MQTT | Email |
| Cloud storage (S3) | Built-in | DIY | No | No | No |
| Resource usage | ~20MB RAM | ~100MB+ | ~50MB+ | ~2GB+ | ~500MB+ |
| Setup complexity | `camgrab add && camgrab snap` | Write scripts | Edit config files | Docker compose | LAMP + config |

## Features

- **Zero external dependencies** — Pure Rust RTSP client via retina; no ffmpeg required
- **Snapshot capture** — Single-frame capture with H.264 decode (openh264) and MJPEG passthrough
- **Video clip recording** — Duration-based RTSP recording with AVCC-to-Annex B conversion
- **Native motion detection** — Frame differencing with configurable zones, sensitivity, and filters
- **Multi-camera concurrent** — Snap or watch all cameras simultaneously with tokio async runtime
- **ONVIF discovery + PTZ** — Auto-discover cameras on the network; pan, tilt, zoom, and preset management
- **Built-in notifications** — Webhook, MQTT, and email alerting out of the box
- **Daemon mode with scheduler** — Cron-based automated captures and continuous monitoring
- **Cloud storage** — Local filesystem and S3/MinIO backends with atomic writes
- **Health checks** — `camgrab doctor` verifies connectivity, stream info, and system configuration
- **JSON output** — Pass `--json` on any command for machine-readable output
- **Cross-platform** — Linux, macOS, Windows; pre-built binaries for 5 targets

## Installation

### Pre-built binaries (recommended)

Download the latest release for your platform from [GitHub Releases](https://github.com/justinhuangcode/camgrab/releases):

| Platform | Archive |
| --- | --- |
| Linux x86_64 | `camgrab-v*-linux-x86_64.tar.gz` |
| Linux ARM64 | `camgrab-v*-linux-arm64.tar.gz` |
| macOS Intel | `camgrab-v*-macos-x86_64.tar.gz` |
| macOS Apple Silicon | `camgrab-v*-macos-arm64.tar.gz` |
| Windows x86_64 | `camgrab-v*-windows-x86_64.zip` |

```bash
# Example: Linux x86_64
curl -LO https://github.com/justinhuangcode/camgrab/releases/latest/download/camgrab-v1.0.0-linux-x86_64.tar.gz
tar xzf camgrab-v1.0.0-linux-x86_64.tar.gz
sudo mv camgrab /usr/local/bin/
```

### Homebrew (macOS / Linux)

```bash
brew tap justinhuangcode/tap
brew install camgrab
```

### Docker

```bash
docker pull ghcr.io/justinhuangcode/camgrab:latest
```

### Via Cargo

```bash
cargo install --git https://github.com/justinhuangcode/camgrab -p camgrab-cli
```

### From source

```bash
git clone https://github.com/justinhuangcode/camgrab.git
cd camgrab
cargo build --release
# Binary at target/release/camgrab
```

**Requirements:** Rust 1.88+

## Quick Start

```bash
# 1. Add a camera (interactive)
camgrab add

# Or specify directly
camgrab add front-door \
  --host 192.168.1.100 --port 554 \
  --username admin --password secret123

# 2. Capture a snapshot
camgrab snap front-door
camgrab snap front-door --out snapshot.jpg --format jpeg

# 3. Record a 30-second clip
camgrab clip front-door --duration 30 --out clip.mp4

# 4. Watch for motion
camgrab watch front-door --threshold 0.3 --cooldown 5

# 5. Discover cameras on the network
camgrab discover

# 6. Check camera health
camgrab doctor --all
```

## Commands

| Command | Description |
| --- | --- |
| `add [NAME]` | Add or update a camera (interactive or flags) |
| `list` | List all configured cameras with status |
| `snap <CAMERA>` | Capture a single-frame snapshot |
| `clip <CAMERA>` | Record a video clip for a specified duration |
| `watch <CAMERA>` | Monitor a camera feed for motion events |
| `discover` | Auto-discover ONVIF cameras on the local network |
| `doctor [CAMERA]` | Verify camera connectivity and stream info |
| `ptz <CAMERA>` | Pan-Tilt-Zoom control via ONVIF |
| `daemon start\|stop\|status\|logs` | Manage the background daemon for scheduled operations |

### `camgrab snap`

Capture a single frame from a camera and save as an image.

```bash
camgrab snap <CAMERA> [OPTIONS]
```

| Flag | Default | Description |
| --- | --- | --- |
| `-o, --out <PATH>` | auto | Output path for snapshot |
| `-f, --format <FMT>` | jpeg | Image format: jpeg, png, webp |
| `-t, --timeout <SEC>` | 10 | Connection timeout in seconds |
| `--transport <PROTO>` | tcp | RTSP transport: tcp, udp |
| `--json` | false | Output result as JSON |

```bash
camgrab snap front-door --out /backup/snapshot.png --format png
camgrab snap front-door --json | jq '.path'
```

### `camgrab clip`

Record a video clip for a specified duration from an RTSP stream.

```bash
camgrab clip <CAMERA> [OPTIONS]
```

| Flag | Default | Description |
| --- | --- | --- |
| `-o, --out <PATH>` | auto | Output path for video clip |
| `-d, --duration <SEC>` | 30 | Recording duration in seconds |
| `-f, --format <FMT>` | mp4 | Video format: mp4, mkv, avi |
| `--transport <PROTO>` | tcp | RTSP transport: tcp, udp |
| `--json` | false | Output result as JSON |

```bash
camgrab clip front-door --duration 60
camgrab clip front-door --out evidence.mp4 --duration 120
```

### `camgrab watch`

Monitor a camera feed for motion, trigger actions on detection events.

```bash
camgrab watch <CAMERA> [OPTIONS]
```

| Flag | Default | Description |
| --- | --- | --- |
| `-t, --threshold <FLOAT>` | 0.3 | Motion detection threshold 0.0-1.0 |
| `-c, --cooldown <SEC>` | 5 | Cooldown between events |
| `--sensitivity <LEVEL>` | medium | Detection sensitivity: low, medium, high |
| `--zones-from <PATH>` | — | Path to JSON zone configuration file |
| `--action <CMD>` | — | Command template to execute on detection |
| `--json` | false | Output events as JSON |

**Environment variables in actions:**
- `$CAMGRAB_CAMERA` — Camera name
- `$CAMGRAB_SCORE` — Motion detection score (0.0-1.0)
- `$CAMGRAB_TIME` — Event timestamp (RFC 3339)
- `$CAMGRAB_ZONE` — Zone name (if using zones)

```bash
# Trigger webhook on motion
camgrab watch front-door --action 'curl -X POST https://api.home.com/alert'

# Capture snapshot on motion
camgrab watch front-door \
  --action 'camgrab snap $CAMGRAB_CAMERA --out /alerts/$CAMGRAB_CAMERA-$CAMGRAB_TIME.jpg'

# Stream JSON events for log aggregation
camgrab watch front-door --json | tee -a motion_events.log
```

### `camgrab ptz`

Control Pan-Tilt-Zoom cameras via ONVIF protocol.

```bash
camgrab ptz <CAMERA> [OPTIONS]
```

| Flag | Description |
| --- | --- |
| `--pan <FLOAT>` | Pan position -1.0 to 1.0 |
| `--tilt <FLOAT>` | Tilt position -1.0 to 1.0 |
| `--zoom <FLOAT>` | Zoom level 0.0 to 1.0 |
| `--goto-preset <N>` | Move to saved preset |
| `--save-preset <N>` | Save current position as preset |
| `--preset-name <NAME>` | Name for saved preset |
| `--list-presets` | List all available presets |
| `--home` | Return to home position |
| `--stop` | Stop current movement |

```bash
camgrab ptz front-door --pan 0.5 --tilt 0.2 --zoom 0.8
camgrab ptz front-door --goto-preset 2
camgrab ptz front-door --list-presets
```

### `camgrab daemon`

Start, stop, and manage the background daemon for scheduled operations.

```bash
camgrab daemon <SUBCOMMAND>
```

| Subcommand | Description |
| --- | --- |
| `start` | Start the daemon process |
| `stop` | Stop the daemon process |
| `restart` | Restart the daemon |
| `status` | Check daemon status |
| `logs` | View daemon logs (`--follow` for real-time) |
| `reload` | Reload configuration without restart |

## Docker Usage

```bash
# Pull from GitHub Container Registry
docker pull ghcr.io/justinhuangcode/camgrab:latest

# One-off snapshot
docker run --rm --network host \
  -v ./config.toml:/etc/camgrab/config.toml:ro \
  -v ./output:/var/lib/camgrab \
  ghcr.io/justinhuangcode/camgrab:latest \
  snap front-door -o /var/lib/camgrab/snap.jpg

# Daemon mode for continuous monitoring
docker run -d --name camgrab --restart unless-stopped \
  --network host \
  -v ./config.toml:/etc/camgrab/config.toml:ro \
  -v camgrab-data:/var/lib/camgrab \
  ghcr.io/justinhuangcode/camgrab:latest \
  daemon start
```

The Docker image supports `linux/amd64` and `linux/arm64`.

## Configuration

Configuration files are stored in XDG-compliant locations:

- **Linux/macOS**: `~/.config/camgrab/config.yaml`
- **Windows**: `%APPDATA%\camgrab\config.yaml`

Supports YAML, TOML, and JSON formats.

### Example (YAML)

```yaml
cameras:
  front-door:
    name: "Front Door Camera"
    host: "192.168.1.100"
    port: 554
    username: "admin"
    password: "secret123"
    protocol: rtsp
    transport: tcp
    stream_type: main
    timeout_secs: 10

  back-yard:
    name: "Back Yard Camera"
    host: "192.168.1.101"
    port: 554
    username: "admin"
    password: "secret456"
    custom_path: "/stream1"

storage:
  backends:
    - type: local
      path: "/var/lib/camgrab/storage"
    - type: s3
      bucket: "camgrab-clips"
      region: "us-east-1"
      prefix: "cameras"

notifications:
  webhook:
    enabled: true
    url: "https://api.example.com/webhook"
    method: POST
    headers:
      Authorization: "Bearer token123"
  mqtt:
    enabled: true
    broker: "mqtt.example.com:1883"
    topic: "camgrab/events"

daemon:
  schedules:
    - name: "hourly-snapshots"
      camera: "front-door"
      action: "snap"
      cron: "0 * * * *"
    - name: "night-motion-watch"
      camera: "back-yard"
      action: "watch"
      cron: "0 22 * * *"
      duration_secs: 28800
      threshold: 0.25
```

### Example (TOML)

```toml
[cameras.front-door]
name = "Front Door Camera"
host = "192.168.1.100"
port = 554
username = "admin"
password = "secret123"
protocol = "rtsp"
transport = "tcp"
stream_type = "main"
timeout_secs = 10

[storage]
[[storage.backends]]
type = "local"
path = "/var/lib/camgrab/storage"

[notifications.webhook]
enabled = true
url = "https://api.example.com/webhook"
method = "POST"

[[daemon.schedules]]
name = "hourly-snapshots"
camera = "front-door"
action = "snap"
cron = "0 * * * *"
```

## Architecture

The project is organized as a Cargo workspace with three crates:

```
camgrab/
├── Cargo.toml                    # Workspace manifest
├── Dockerfile                    # Multi-stage Docker build (multi-arch)
├── deny.toml                     # Cargo deny (license + advisory audit)
├── Formula/camgrab.rb            # Homebrew formula
├── .github/workflows/
│   ├── ci.yml                    # Check, Clippy, Fmt, Test, Build, Deny
│   └── release.yml               # Build → Release → Docker → Homebrew
│
├── crates/
│   ├── camgrab-cli/              # CLI binary
│   │   └── src/
│   │       ├── main.rs           # Entry point, clap command dispatch
│   │       └── commands/
│   │           ├── snap.rs       # Snapshot capture
│   │           ├── clip.rs       # Video clip recording
│   │           ├── watch.rs      # Motion detection monitor
│   │           ├── discover.rs   # ONVIF camera discovery
│   │           ├── doctor.rs     # Health check diagnostics
│   │           ├── ptz.rs        # PTZ control
│   │           ├── add.rs        # Camera configuration
│   │           └── list.rs       # Camera listing
│   │
│   ├── camgrab-core/             # Core library
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── camera.rs         # Camera model + management
│   │       ├── error.rs          # Error types
│   │       ├── config/           # YAML/TOML/JSON config (de)serialization
│   │       ├── rtsp/             # Pure Rust RTSP client (retina + openh264)
│   │       ├── motion/           # Frame differencing, zones, filters
│   │       ├── onvif/            # SOAP/WS-Security device + PTZ + discovery
│   │       ├── storage/          # Local + S3/MinIO backends
│   │       └── notify/           # Webhook, MQTT, email
│   │
│   └── camgrab-daemon/           # Background daemon
│       └── src/
│           ├── lib.rs
│           ├── server.rs         # Axum HTTP API
│           └── scheduler.rs      # Cron job scheduling
│
└── examples/
    ├── rtsp_snapshot.rs
    └── rtsp_clip.rs
```

## Performance

- **Memory**: ~20MB base + ~5MB per active RTSP stream
- **CPU**: ~2-5% per 1080p stream (motion detection disabled)
- **Motion detection overhead**: ~8-15% additional CPU per stream
- **Concurrent streams**: Tested with 32+ simultaneous connections
- **Startup time**: <100ms cold start

## Camera Compatibility

Tested with:
- Hikvision DS-2CD2xxx series
- Dahua IPC-HDW/HFW series
- Reolink RLC-xxx series
- Amcrest IP2M/IP4M series
- Ubiquiti UniFi Protect cameras
- Generic ONVIF-compliant cameras

## Security Considerations

- Credentials are stored in plain text in configuration files — ensure proper file permissions (`chmod 600`)
- RTSP streams are typically unencrypted — use VPN or VLAN for sensitive deployments
- RTSPS (RTSP over TLS) support available for encrypted streams
- ONVIF communications support WS-Security with timestamp validation
- Motion detection actions execute shell commands — sanitize inputs carefully

## Troubleshooting

### Connection Issues

```bash
# Verify camera is reachable
ping 192.168.1.100

# Test RTSP port
nc -zv 192.168.1.100 554

# Run doctor command with verbose logging
camgrab -v doctor front-door
```

### Motion Detection False Positives

- Decrease sensitivity: `--sensitivity low`
- Increase threshold: `--threshold 0.5`
- Use motion zones to exclude problematic areas
- Increase cooldown to reduce event spam: `--cooldown 10`

### Performance Issues

- Use UDP transport on local networks: `--transport udp`
- Use sub-stream for lower bandwidth: `stream_type: sub`
- Increase timeout for slow cameras: `timeout_secs: 30`

## Development

```bash
git clone https://github.com/justinhuangcode/camgrab.git
cd camgrab

# Build
cargo build --release

# Test
cargo test --workspace

# Lint
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings

# Run examples
cargo run --example rtsp_snapshot
cargo run --example rtsp_clip
```

### Code Style

- `rustfmt` with default settings
- Clippy pedantic lints enabled (configured in workspace `Cargo.toml`)
- No unsafe code (`#![deny(unsafe_code)]`)

## Contributing

Contributions are welcome! Please see the guidelines:

1. Fork the repository and create a feature branch
2. Write tests for new functionality
3. Run `cargo fmt` and `cargo clippy` before submitting
4. Submit a pull request with a clear description of changes

## Changelog

See [CHANGELOG.md](CHANGELOG.md) for release history.

## Acknowledgments

- [retina](https://github.com/scottlamb/retina) — Pure Rust RTSP client library
- [openh264](https://github.com/ralfbiedert/openh264-rs) — H.264 decoding
- [tokio](https://tokio.rs) — Asynchronous runtime
- [clap](https://clap.rs) — Command-line argument parsing

## License

[MIT](LICENSE)
