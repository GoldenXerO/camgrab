use clap::Parser;
use colored::Colorize;
use is_terminal::IsTerminal;
use miette::{IntoDiagnostic, Result};
use serde_json::json;
use std::io;
use std::path::Path;
use tracing::{debug, info};

#[derive(Parser)]
pub struct Args {
    /// Unique name for the camera
    name: String,

    /// Camera hostname or IP address
    #[arg(long, required = true)]
    host: String,

    /// RTSP port
    #[arg(long, default_value = "554")]
    port: u16,

    /// Username for authentication
    #[arg(long, short)]
    user: Option<String>,

    /// Password for authentication
    #[arg(long, short = 'P')]
    pass: Option<String>,

    /// RTSP protocol (rtsp or rtsps)
    #[arg(long, value_enum, default_value = "rtsp")]
    protocol: Protocol,

    /// Default transport protocol for this camera
    #[arg(long, value_enum)]
    rtsp_transport: Option<Transport>,

    /// Default stream for this camera (stream1/stream2)
    #[arg(long)]
    stream: Option<String>,

    /// Custom RTSP path (e.g., /Bfy47SNWz9n2WRrw for UniFi Protect)
    #[arg(long)]
    path: Option<String>,

    /// Disable audio for this camera
    #[arg(long)]
    no_audio: bool,

    /// Default audio codec for this camera
    #[arg(long)]
    audio_codec: Option<String>,

    /// Authentication method
    #[arg(long, value_enum, default_value = "auto")]
    auth_method: AuthMethod,

    /// Output result as JSON
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Clone, clap::ValueEnum)]
enum Protocol {
    Rtsp,
    Rtsps,
}

#[derive(Debug, Clone, clap::ValueEnum)]
enum Transport {
    Tcp,
    Udp,
}

#[derive(Debug, Clone, clap::ValueEnum)]
enum AuthMethod {
    Auto,
    Basic,
    Digest,
}

pub async fn run(args: Args, config_path: &Path) -> Result<()> {
    debug!("Add command: name={}, host={}", args.name, args.host);

    // Validate camera name
    if !args
        .name
        .chars()
        .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
    {
        return Err(miette::miette!(
            "Camera name must contain only alphanumeric characters, underscores, and hyphens"
        ));
    }

    // Validate mutually exclusive flags
    if args.path.is_some() && args.stream.is_some() {
        return Err(miette::miette!(
            "use --path for custom RTSP token URLs; omit --stream"
        ));
    }

    let is_tty = io::stdout().is_terminal();

    // Build display URL with redacted password
    let protocol_str = match args.protocol {
        Protocol::Rtsp => "rtsp",
        Protocol::Rtsps => "rtsps",
    };

    let auth_part = match (&args.user, &args.pass) {
        (Some(u), Some(_)) => format!("{}:***@", u),
        (Some(u), None) => format!("{}@", u),
        _ => String::new(),
    };

    let path_part = args
        .path
        .as_deref()
        .or(args.stream.as_deref())
        .unwrap_or("stream1");

    let display_url = format!(
        "{}://{}{}:{}/{}",
        protocol_str, auth_part, args.host, args.port, path_part
    );

    // Load config, upsert camera, save
    let mut app_config =
        camgrab_core::config::load(config_path).map_err(|e| miette::miette!("{}", e))?;

    // Map CLI enums to core enums and build CameraConfig
    let core_protocol = match args.protocol {
        Protocol::Rtsp => camgrab_core::camera::Protocol::Rtsp,
        Protocol::Rtsps => camgrab_core::camera::Protocol::Rtsps,
    };

    let core_transport = args.rtsp_transport.as_ref().map(|t| match t {
        Transport::Tcp => camgrab_core::camera::Transport::Tcp,
        Transport::Udp => camgrab_core::camera::Transport::Udp,
    });

    let core_auth_method = match args.auth_method {
        AuthMethod::Auto => camgrab_core::camera::AuthMethod::Auto,
        AuthMethod::Basic => camgrab_core::camera::AuthMethod::Basic,
        AuthMethod::Digest => camgrab_core::camera::AuthMethod::Digest,
    };

    let stream_type = args.stream.as_ref().map(|s| match s.as_str() {
        "stream1" => camgrab_core::camera::StreamType::Main,
        "stream2" => camgrab_core::camera::StreamType::Sub,
        custom => camgrab_core::camera::StreamType::Custom(custom.to_string()),
    });

    let camera_config = camgrab_core::config::CameraConfig {
        name: args.name.clone(),
        host: args.host.clone(),
        port: Some(args.port),
        username: args.user.clone(),
        password: args.pass.clone(),
        protocol: Some(core_protocol),
        transport: core_transport,
        stream_type,
        custom_path: args.path.clone(),
        audio_enabled: Some(!args.no_audio),
        auth_method: Some(core_auth_method),
        timeout_secs: None,
    };

    let is_update = camgrab_core::config::upsert_camera(&mut app_config, camera_config);

    camgrab_core::config::save(config_path, &app_config).map_err(|e| miette::miette!("{}", e))?;

    let action = if is_update { "updated" } else { "added" };
    info!("Camera '{}' {} in configuration", args.name, action);

    if args.json {
        let result = json!({
            "success": true,
            "camera": args.name,
            "host": args.host,
            "port": args.port,
            "rtsp_url": display_url,
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&result).into_diagnostic()?
        );
    } else if is_tty {
        println!("{}", "✓ Camera added successfully!".green().bold());
        println!("  Name:     {}", args.name.yellow().bold());
        println!("  Host:     {}", args.host.cyan());
        println!("  Port:     {}", args.port);
        println!("  Protocol: {:?}", args.protocol);
        if let Some(ref t) = args.rtsp_transport {
            println!("  Transport: {:?}", t);
        }
        println!("  Auth:     {:?}", args.auth_method);
        println!(
            "  Audio:    {}",
            if args.no_audio { "disabled" } else { "enabled" }
        );
        println!("  URL:      {}", display_url.dimmed());
        println!();
        println!("You can now use this camera with:");
        println!(
            "  {} - Capture a snapshot",
            format!("camgrab snap {}", args.name).cyan()
        );
        println!(
            "  {} - Record a clip",
            format!("camgrab clip {}", args.name).cyan()
        );
        println!(
            "  {} - Watch for motion",
            format!("camgrab watch {}", args.name).cyan()
        );
    } else {
        println!("{}", args.name);
    }

    Ok(())
}
