use clap::Parser;
use colored::Colorize;
use is_terminal::IsTerminal;
use miette::{IntoDiagnostic, Result};
use serde_json::json;
use std::io;
use std::path::{Path, PathBuf};
use tracing::{debug, info, warn};

#[derive(Parser)]
pub struct Args {
    /// Camera name from configuration
    camera: String,

    /// Output path for the video clip (creates temp file if omitted)
    #[arg(short, long)]
    out: Option<PathBuf>,

    /// Recording duration in seconds
    #[arg(short, long, default_value = "10")]
    duration: u64,

    /// Timeout in seconds (should exceed duration)
    #[arg(short, long, default_value = "20")]
    timeout: u64,

    /// Disable audio recording
    #[arg(long)]
    no_audio: bool,

    /// Audio codec to use (aac, opus, pcma)
    #[arg(long)]
    audio_codec: Option<String>,

    /// Video container format
    #[arg(short, long, value_enum, default_value = "mp4")]
    format: VideoFormat,

    /// RTSP transport protocol (overrides per-camera default)
    #[arg(long, value_enum)]
    transport: Option<Transport>,

    /// RTSP authentication method
    #[arg(long, value_enum)]
    rtsp_auth: Option<RtspAuth>,

    /// Custom RTSP stream path (overrides --stream)
    #[arg(long)]
    path: Option<String>,

    /// Stream selection (stream1/stream2)
    #[arg(long)]
    stream: Option<String>,

    /// Output result as JSON
    #[arg(long)]
    json: bool,
}

#[derive(Clone, clap::ValueEnum)]
enum VideoFormat {
    Mp4,
    Mkv,
}

#[derive(Clone, clap::ValueEnum)]
enum Transport {
    Tcp,
    Udp,
}

#[derive(Clone, clap::ValueEnum)]
enum RtspAuth {
    Auto,
    Basic,
    Digest,
}

pub async fn run(args: Args, config_path: &Path) -> Result<()> {
    debug!(
        "Clip command: camera={}, duration={}s",
        args.camera, args.duration
    );

    // Validate
    if args.path.is_some() && args.stream.is_some() {
        return Err(miette::miette!(
            "use --path for custom RTSP token URLs; omit --stream"
        ));
    }

    if args.timeout <= args.duration {
        warn!(
            "Timeout ({}s) should exceed duration ({}s) for reliable recording",
            args.timeout, args.duration
        );
    }

    // Warn about PCMA + mp4 incompatibility (learned from camsnap)
    if let Some(ref codec) = args.audio_codec {
        if codec == "pcma" && matches!(args.format, VideoFormat::Mp4) {
            warn!("PCMA audio is not compatible with MP4 container; consider --no-audio or --format mkv");
        }
    }

    let is_tty = io::stdout().is_terminal();

    // Generate output path if not provided
    let output_path = match args.out {
        Some(path) => path,
        None => {
            let extension = match args.format {
                VideoFormat::Mp4 => "mp4",
                VideoFormat::Mkv => "mkv",
            };
            let temp_path = std::env::temp_dir().join(format!(
                "camgrab-{}.{}",
                uuid::Uuid::new_v4().as_simple(),
                extension
            ));
            if is_tty && !args.json {
                eprintln!("No --out provided, writing clip to {}", temp_path.display());
            }
            temp_path
        }
    };

    info!(
        "Recording clip for {}s to: {}",
        args.duration,
        output_path.display()
    );

    // Load config and find camera
    let app_config =
        camgrab_core::config::load(config_path).map_err(|e| miette::miette!("{}", e))?;
    let camera_config =
        camgrab_core::config::find_camera(&app_config, &args.camera).ok_or_else(|| {
            miette::miette!(
                "Camera '{}' not found. Run 'camgrab add' first.",
                args.camera
            )
        })?;

    // Build runtime Camera with CLI overrides
    let mut camera = camgrab_core::camera::Camera::from_config(camera_config);
    if let Some(ref t) = args.transport {
        camera.transport = match t {
            Transport::Tcp => camgrab_core::camera::Transport::Tcp,
            Transport::Udp => camgrab_core::camera::Transport::Udp,
        };
    }
    if let Some(ref a) = args.rtsp_auth {
        camera.auth_method = match a {
            RtspAuth::Auto => camgrab_core::camera::AuthMethod::Auto,
            RtspAuth::Basic => camgrab_core::camera::AuthMethod::Basic,
            RtspAuth::Digest => camgrab_core::camera::AuthMethod::Digest,
        };
    }
    if let Some(ref p) = args.path {
        camera.custom_path = Some(p.clone());
    }
    if let Some(ref s) = args.stream {
        camera.stream = match s.as_str() {
            "stream2" | "sub" => camgrab_core::camera::StreamType::Sub,
            _ => camgrab_core::camera::StreamType::Main,
        };
    }
    camera.audio_enabled = !args.no_audio;
    camera.timeout = std::time::Duration::from_secs(args.timeout);

    // Build ClipOptions and record
    use camgrab_core::rtsp::client::{ClipOptions, ContainerFormat};

    let clip_options = ClipOptions {
        include_audio: !args.no_audio,
        audio_codec_override: args
            .audio_codec
            .as_deref()
            .map(camgrab_core::rtsp::codec::AudioCodec::from_encoding_name),
        container_format: match args.format {
            VideoFormat::Mp4 => ContainerFormat::Mp4,
            VideoFormat::Mkv => ContainerFormat::Mkv,
        },
        max_file_size: 0,
    };

    let mut client = camgrab_core::rtsp::client::RtspClient::new(&camera)
        .map_err(|e| miette::miette!("{}", e))?;
    client
        .connect()
        .await
        .map_err(|e| miette::miette!("Connection failed: {}", e))?;
    let clip_result = client
        .clip(
            &output_path,
            std::time::Duration::from_secs(args.duration),
            clip_options,
        )
        .await
        .map_err(|e| miette::miette!("Clip recording failed: {}", e))?;

    let file_size = clip_result.size_bytes;
    let duration_secs = clip_result.duration.as_secs();

    if args.json {
        let result = json!({
            "success": true,
            "camera": args.camera,
            "path": output_path.display().to_string(),
            "duration_seconds": duration_secs,
            "size_bytes": file_size,
            "audio_enabled": !args.no_audio,
            "format": match args.format {
                VideoFormat::Mp4 => "mp4",
                VideoFormat::Mkv => "mkv",
            }
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&result).into_diagnostic()?
        );
    } else if is_tty {
        println!("{}", "✓ Clip recorded successfully".green().bold());
        println!("  Camera:   {}", args.camera.cyan());
        println!("  Path:     {}", output_path.display().to_string().yellow());
        println!("  Duration: {}s", duration_secs.to_string().cyan());
        println!("  Size:     {} bytes", file_size.to_string().cyan());
        println!(
            "  Audio:    {}",
            if args.no_audio { "disabled" } else { "enabled" }
        );
    } else {
        println!("{}", output_path.display());
    }

    Ok(())
}
