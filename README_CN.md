# camgrab

[English](./README.md) | **中文**

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

一款现代化的命令行工具，用于抓拍快照、录制视频片段、运动检测及通过 RTSP/ONVIF 管理 IP 摄像头。

## 为什么选择 camgrab？

大多数摄像头工具要么依赖庞大的外部二进制文件（ffmpeg），要么是需要自行部署的完整 NVR 系统。camgrab 是一个零外部依赖的单一二进制文件 -- 连接摄像头后即可立即使用。

| | camgrab | ffmpeg + cron | motion | Frigate | ZoneMinder |
|---|---|---|---|---|---|
| 单文件，零依赖 | **是** | 否 (ffmpeg ~50MB) | 否 (C 库) | 否 (Docker 全家桶) | 否 (LAMP 全家桶) |
| RTSP 抓拍 + 录制 | 内置 | 手写脚本 | 无录制 | 通过 API | 通过 API |
| 原生运动检测 | 内置 (区域 + 过滤器) | 仅场景过滤 | 是 | 基于 ML | 区域 |
| ONVIF 云台控制 | 内置 | 否 | 否 | 通过 MQTT | 有限 |
| 多摄像头并发 | 异步 (tokio) | 串行脚本 | 每进程一路 | 是 | 是 |
| 守护进程 + 定时任务 | 内置 | 外部 cron | 内置 | 常驻运行 | 常驻运行 |
| 通知 | Webhook / MQTT / Email | 自行实现 | 有限 | MQTT | Email |
| 云存储 (S3) | 内置 | 自行实现 | 否 | 否 | 否 |
| 资源占用 | ~20MB 内存 | ~100MB+ | ~50MB+ | ~2GB+ | ~500MB+ |
| 上手难度 | `camgrab add && camgrab snap` | 编写脚本 | 编辑配置 | Docker Compose | LAMP + 配置 |

## 功能特性

- **零外部依赖** -- 基于 retina 的纯 Rust RTSP 客户端，无需安装 ffmpeg
- **快照抓拍** -- 通过 openh264 解码 H.264 关键帧，支持 MJPEG 直通
- **视频录制** -- 按时长录制 RTSP 流，自动处理 AVCC 到 Annex B 转换
- **原生运动检测** -- 帧差分算法，支持可配置区域、灵敏度和过滤器
- **多摄像头并发** -- 基于 tokio 异步运行时，同时抓拍或监控所有摄像头
- **ONVIF 发现 + 云台** -- 自动发现网络上的摄像头；支持水平/垂直/变焦和预置位管理
- **内置通知** -- 开箱即用的 Webhook、MQTT 和邮件告警
- **守护进程模式** -- 基于 cron 表达式的自动化抓拍和持续监控
- **云存储** -- 支持本地文件系统和 S3/MinIO 后端，原子写入
- **健康检查** -- `camgrab doctor` 验证连通性、流信息和系统配置
- **JSON 输出** -- 任意命令添加 `--json` 即可输出机器可读格式
- **跨平台** -- 支持 Linux、macOS、Windows，提供 5 个平台的预编译二进制文件

## 安装

### 预编译二进制文件（推荐）

从 [GitHub Releases](https://github.com/justinhuangcode/camgrab/releases) 下载适合你平台的最新版本：

| 平台 | 文件名 |
| --- | --- |
| Linux x86_64 | `camgrab-v*-linux-x86_64.tar.gz` |
| Linux ARM64 | `camgrab-v*-linux-arm64.tar.gz` |
| macOS Intel | `camgrab-v*-macos-x86_64.tar.gz` |
| macOS Apple Silicon | `camgrab-v*-macos-arm64.tar.gz` |
| Windows x86_64 | `camgrab-v*-windows-x86_64.zip` |

```bash
# 示例：Linux x86_64
curl -LO https://github.com/justinhuangcode/camgrab/releases/latest/download/camgrab-v1.0.0-linux-x86_64.tar.gz
tar xzf camgrab-v1.0.0-linux-x86_64.tar.gz
sudo mv camgrab /usr/local/bin/
```

### Homebrew（macOS / Linux）

```bash
brew tap justinhuangcode/tap
brew install camgrab
```

### Docker

```bash
docker pull ghcr.io/justinhuangcode/camgrab:latest
```

### 通过 Cargo 安装

```bash
cargo install --git https://github.com/justinhuangcode/camgrab -p camgrab-cli
```

### 从源码编译

```bash
git clone https://github.com/justinhuangcode/camgrab.git
cd camgrab
cargo build --release
# 二进制文件位于 target/release/camgrab
```

**要求：** Rust 1.88+

## 快速上手

```bash
# 1. 添加摄像头（交互式）
camgrab add

# 或直接指定参数
camgrab add front-door \
  --host 192.168.1.100 --port 554 \
  --username admin --password secret123

# 2. 抓拍快照
camgrab snap front-door
camgrab snap front-door --out snapshot.jpg --format jpeg

# 3. 录制 30 秒视频
camgrab clip front-door --duration 30 --out clip.mp4

# 4. 运动检测监控
camgrab watch front-door --threshold 0.3 --cooldown 5

# 5. 发现网络上的摄像头
camgrab discover

# 6. 检查摄像头健康状态
camgrab doctor --all
```

## 命令参考

| 命令 | 说明 |
| --- | --- |
| `add [NAME]` | 添加或更新摄像头（交互式或参数） |
| `list` | 列出所有已配置的摄像头及状态 |
| `snap <CAMERA>` | 抓拍单帧快照 |
| `clip <CAMERA>` | 按指定时长录制视频片段 |
| `watch <CAMERA>` | 监控摄像头画面的运动事件 |
| `discover` | 自动发现局域网上的 ONVIF 摄像头 |
| `doctor [CAMERA]` | 验证摄像头连通性和流信息 |
| `ptz <CAMERA>` | 通过 ONVIF 控制云台 |
| `daemon start\|stop\|status\|logs` | 管理后台守护进程 |

### `camgrab snap`

从摄像头抓拍单帧并保存为图片。

```bash
camgrab snap <CAMERA> [OPTIONS]
```

| 参数 | 默认值 | 说明 |
| --- | --- | --- |
| `-o, --out <PATH>` | 自动 | 输出路径 |
| `-f, --format <FMT>` | jpeg | 图片格式：jpeg, png, webp |
| `-t, --timeout <SEC>` | 10 | 连接超时（秒） |
| `--transport <PROTO>` | tcp | RTSP 传输协议：tcp, udp |
| `--json` | false | 以 JSON 格式输出 |

```bash
camgrab snap front-door --out /backup/snapshot.png --format png
camgrab snap front-door --json | jq '.path'
```

### `camgrab clip`

从 RTSP 流录制指定时长的视频片段。

```bash
camgrab clip <CAMERA> [OPTIONS]
```

| 参数 | 默认值 | 说明 |
| --- | --- | --- |
| `-o, --out <PATH>` | 自动 | 输出路径 |
| `-d, --duration <SEC>` | 30 | 录制时长（秒） |
| `-f, --format <FMT>` | mp4 | 视频格式：mp4, mkv, avi |
| `--transport <PROTO>` | tcp | RTSP 传输协议：tcp, udp |
| `--json` | false | 以 JSON 格式输出 |

```bash
camgrab clip front-door --duration 60
camgrab clip front-door --out evidence.mp4 --duration 120
```

### `camgrab watch`

监控摄像头画面，检测到运动时触发动作。

```bash
camgrab watch <CAMERA> [OPTIONS]
```

| 参数 | 默认值 | 说明 |
| --- | --- | --- |
| `-t, --threshold <FLOAT>` | 0.3 | 运动检测阈值 0.0-1.0 |
| `-c, --cooldown <SEC>` | 5 | 事件间冷却时间 |
| `--sensitivity <LEVEL>` | medium | 检测灵敏度：low, medium, high |
| `--zones-from <PATH>` | -- | 区域配置 JSON 文件路径 |
| `--action <CMD>` | -- | 检测到运动时执行的命令模板 |
| `--json` | false | 以 JSON 格式输出事件 |

**动作中可用的环境变量：**
- `$CAMGRAB_CAMERA` -- 摄像头名称
- `$CAMGRAB_SCORE` -- 运动检测分数 (0.0-1.0)
- `$CAMGRAB_TIME` -- 事件时间戳 (RFC 3339)
- `$CAMGRAB_ZONE` -- 区域名称（如果使用区域）

```bash
# 运动检测时触发 Webhook
camgrab watch front-door --action 'curl -X POST https://api.home.com/alert'

# 运动检测时自动抓拍
camgrab watch front-door \
  --action 'camgrab snap $CAMGRAB_CAMERA --out /alerts/$CAMGRAB_CAMERA-$CAMGRAB_TIME.jpg'

# 以 JSON 流式输出用于日志聚合
camgrab watch front-door --json | tee -a motion_events.log
```

### `camgrab ptz`

通过 ONVIF 协议控制云台摄像头。

```bash
camgrab ptz <CAMERA> [OPTIONS]
```

| 参数 | 说明 |
| --- | --- |
| `--pan <FLOAT>` | 水平位置 -1.0 到 1.0 |
| `--tilt <FLOAT>` | 垂直位置 -1.0 到 1.0 |
| `--zoom <FLOAT>` | 变焦倍数 0.0 到 1.0 |
| `--goto-preset <N>` | 移动到已保存的预置位 |
| `--save-preset <N>` | 将当前位置保存为预置位 |
| `--preset-name <NAME>` | 预置位名称 |
| `--list-presets` | 列出所有可用的预置位 |
| `--home` | 回到初始位置 |
| `--stop` | 停止当前移动 |

```bash
camgrab ptz front-door --pan 0.5 --tilt 0.2 --zoom 0.8
camgrab ptz front-door --goto-preset 2
camgrab ptz front-door --list-presets
```

### `camgrab daemon`

管理后台守护进程，用于定时任务和持续监控。

```bash
camgrab daemon <SUBCOMMAND>
```

| 子命令 | 说明 |
| --- | --- |
| `start` | 启动守护进程 |
| `stop` | 停止守护进程 |
| `restart` | 重启守护进程 |
| `status` | 查看守护进程状态 |
| `logs` | 查看日志（`--follow` 实时跟踪） |
| `reload` | 重新加载配置（无需重启） |

## Docker 用法

```bash
# 从 GitHub Container Registry 拉取
docker pull ghcr.io/justinhuangcode/camgrab:latest

# 一次性抓拍
docker run --rm --network host \
  -v ./config.toml:/etc/camgrab/config.toml:ro \
  -v ./output:/var/lib/camgrab \
  ghcr.io/justinhuangcode/camgrab:latest \
  snap front-door -o /var/lib/camgrab/snap.jpg

# 守护进程模式持续监控
docker run -d --name camgrab --restart unless-stopped \
  --network host \
  -v ./config.toml:/etc/camgrab/config.toml:ro \
  -v camgrab-data:/var/lib/camgrab \
  ghcr.io/justinhuangcode/camgrab:latest \
  daemon start
```

Docker 镜像支持 `linux/amd64` 和 `linux/arm64` 双架构。

## 配置

配置文件存储在 XDG 标准路径：

- **Linux/macOS**：`~/.config/camgrab/config.yaml`
- **Windows**：`%APPDATA%\camgrab\config.yaml`

支持 YAML、TOML 和 JSON 格式。

### 示例（YAML）

```yaml
cameras:
  front-door:
    name: "前门摄像头"
    host: "192.168.1.100"
    port: 554
    username: "admin"
    password: "secret123"
    protocol: rtsp
    transport: tcp
    stream_type: main
    timeout_secs: 10

  back-yard:
    name: "后院摄像头"
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
    - name: "每小时抓拍"
      camera: "front-door"
      action: "snap"
      cron: "0 * * * *"
    - name: "夜间运动监控"
      camera: "back-yard"
      action: "watch"
      cron: "0 22 * * *"
      duration_secs: 28800
      threshold: 0.25
```

## 架构

项目采用 Cargo workspace 组织，包含三个 crate：

```
camgrab/
├── Cargo.toml                    # Workspace 根配置
├── Dockerfile                    # 多阶段 Docker 构建（多架构）
├── deny.toml                     # Cargo deny（许可证 + 安全审计）
├── Formula/camgrab.rb            # Homebrew formula
├── .github/workflows/
│   ├── ci.yml                    # Check, Clippy, Fmt, Test, Build, Deny
│   └── release.yml               # Build → Release → Docker → Homebrew
│
├── crates/
│   ├── camgrab-cli/              # 命令行可执行文件
│   │   └── src/
│   │       ├── main.rs           # 入口，clap 命令分发
│   │       └── commands/
│   │           ├── snap.rs       # 快照抓拍
│   │           ├── clip.rs       # 视频录制
│   │           ├── watch.rs      # 运动检测监控
│   │           ├── discover.rs   # ONVIF 摄像头发现
│   │           ├── doctor.rs     # 健康检查诊断
│   │           ├── ptz.rs        # 云台控制
│   │           ├── add.rs        # 摄像头配置
│   │           └── list.rs       # 摄像头列表
│   │
│   ├── camgrab-core/             # 核心库
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── camera.rs         # 摄像头模型 + 管理
│   │       ├── error.rs          # 错误类型
│   │       ├── config/           # YAML/TOML/JSON 配置序列化
│   │       ├── rtsp/             # 纯 Rust RTSP 客户端 (retina + openh264)
│   │       ├── motion/           # 帧差分、区域、过滤器
│   │       ├── onvif/            # SOAP/WS-Security 设备 + 云台 + 发现
│   │       ├── storage/          # 本地 + S3/MinIO 存储后端
│   │       └── notify/           # Webhook、MQTT、邮件
│   │
│   └── camgrab-daemon/           # 后台守护进程
│       └── src/
│           ├── lib.rs
│           ├── server.rs         # Axum HTTP API
│           └── scheduler.rs      # Cron 定时任务调度
│
└── examples/
    ├── rtsp_snapshot.rs
    └── rtsp_clip.rs
```

## 性能

- **内存占用**：基础 ~20MB + 每路活跃 RTSP 流 ~5MB
- **CPU 占用**：每路 1080p 流 ~2-5%（未启用运动检测）
- **运动检测开销**：每路流额外 ~8-15% CPU
- **并发流数**：已测试 32+ 路同时连接
- **启动时间**：冷启动 <100ms

## 摄像头兼容性

已测试：
- 海康威视 DS-2CD2xxx 系列
- 大华 IPC-HDW/HFW 系列
- Reolink RLC-xxx 系列
- Amcrest IP2M/IP4M 系列
- Ubiquiti UniFi Protect 摄像头
- 通用 ONVIF 兼容摄像头

## 安全注意事项

- 凭据以明文存储在配置文件中 -- 请确保文件权限正确（`chmod 600`）
- RTSP 流通常未加密 -- 敏感场景建议使用 VPN 或 VLAN
- 支持 RTSPS（基于 TLS 的 RTSP）加密传输
- ONVIF 通信支持 WS-Security 时间戳验证
- 运动检测动作会执行 shell 命令 -- 请注意输入过滤

## 故障排除

### 连接问题

```bash
# 验证摄像头可达
ping 192.168.1.100

# 测试 RTSP 端口
nc -zv 192.168.1.100 554

# 详细日志模式运行诊断
camgrab -v doctor front-door
```

### 运动检测误报

- 降低灵敏度：`--sensitivity low`
- 提高阈值：`--threshold 0.5`
- 使用运动区域排除干扰区域
- 增加冷却时间减少事件频率：`--cooldown 10`

### 性能问题

- 局域网使用 UDP 传输：`--transport udp`
- 使用子码流降低带宽：`stream_type: sub`
- 慢速摄像头增加超时：`timeout_secs: 30`

## 开发

```bash
git clone https://github.com/justinhuangcode/camgrab.git
cd camgrab

# 编译
cargo build --release

# 测试
cargo test --workspace

# 代码检查
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings

# 运行示例
cargo run --example rtsp_snapshot
cargo run --example rtsp_clip
```

### 代码规范

- 使用 `rustfmt` 默认配置
- 启用 Clippy pedantic lint（在 workspace `Cargo.toml` 中配置）
- 禁止 unsafe 代码（`#![deny(unsafe_code)]`）

## 贡献

欢迎贡献！请遵循以下指南：

1. Fork 仓库并创建功能分支
2. 为新功能编写测试
3. 提交前运行 `cargo fmt` 和 `cargo clippy`
4. 提交 Pull Request 并清晰描述变更内容

## 更新日志

请查看 [CHANGELOG.md](CHANGELOG.md)。

## 致谢

- [retina](https://github.com/scottlamb/retina) -- 纯 Rust RTSP 客户端库
- [openh264](https://github.com/ralfbiedert/openh264-rs) -- H.264 解码
- [tokio](https://tokio.rs) -- 异步运行时
- [clap](https://clap.rs) -- 命令行参数解析

## 许可证

[MIT](LICENSE)
