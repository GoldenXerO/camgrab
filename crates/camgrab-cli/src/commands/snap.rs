use clap::Parser;
use colored::Colorize;
use is_terminal::IsTerminal;
use miette::{IntoDiagnostic, Result};
use serde_json::json;
use std::io;
use std::path::{Path, PathBuf};
use tracing::{debug, info};

#[derive(Parser)]
pub struct Args {
    /// Camera name from configuration
    camera: String,

    /// Output path for the snapshot (creates temp file if omitted)
    #[arg(short, long)]
    out: Option<PathBuf>,

    /// Image format
    #[arg(short, long, value_enum, default_value = "jpeg")]
    format: ImageFormat,

    /// Timeout in seconds
    #[arg(short, long, default_value = "10")]
    timeout: u64,

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
enum ImageFormat {
    Jpeg,
    Png,
    Webp,
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
    debug!("Snapshot command: camera={}", args.camera);

    // Validate mutually exclusive flags
    if args.path.is_some() && args.stream.is_some() {
        return Err(miette::miette!(
            "use --path for custom RTSP token URLs; omit --stream"
        ));
    }

    let is_tty = io::stdout().is_terminal();

    // Generate output path if not provided (create temp file)
    let (output_path, _is_temp) = match args.out {
        Some(path) => (path, false),
        None => {
            let extension = match args.format {
                ImageFormat::Jpeg => "jpg",
                ImageFormat::Png => "png",
                ImageFormat::Webp => "webp",
            };
            let temp_path = std::env::temp_dir().join(format!(
                "camgrab-{}.{}",
                uuid::Uuid::new_v4().as_simple(),
                extension
            ));
            if is_tty && !args.json {
                eprintln!(
                    "No --out provided, writing snapshot to {}",
                    temp_path.display()
                );
            }
            (temp_path, true)
        }
    };

    info!("Capturing snapshot to: {}", output_path.display());

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

    // Apply CLI overrides
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
    camera.timeout = std::time::Duration::from_secs(args.timeout);

    // Connect and snap
    let mut client = camgrab_core::rtsp::client::RtspClient::new(&camera)
        .map_err(|e| miette::miette!("{}", e))?;
    client
        .connect()
        .await
        .map_err(|e| miette::miette!("Connection failed: {}", e))?;
    let snap_result = client
        .snap(&output_path)
        .await
        .map_err(|e| miette::miette!("Snapshot failed: {}", e))?;

    let file_size = snap_result.size_bytes;
    let dimensions = (snap_result.width, snap_result.height);

    if args.json {
        let result = json!({
            "success": true,
            "camera": args.camera,
            "path": output_path.display().to_string(),
            "size_bytes": file_size,
            "width": dimensions.0,
            "height": dimensions.1,
            "format": match args.format {
                ImageFormat::Jpeg => "jpeg",
                ImageFormat::Png => "png",
                ImageFormat::Webp => "webp",
            }
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&result).into_diagnostic()?
        );
    } else if is_tty {
        println!("{}", "✓ Snapshot captured successfully".green().bold());
        println!("  Camera:     {}", args.camera.cyan());
        println!(
            "  Path:       {}",
            output_path.display().to_string().yellow()
        );
        println!("  Size:       {} bytes", file_size.to_string().cyan());
        println!("  Dimensions: {}x{}", dimensions.0, dimensions.1);
    } else {
        // Non-TTY: plain output, just print the path (useful for piping)
        println!("{}", output_path.display());
    }

    Ok(())
}
