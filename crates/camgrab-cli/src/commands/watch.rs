use clap::Parser;
use colored::Colorize;
use image::GrayImage;
use is_terminal::IsTerminal;
use miette::Result;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Instant;
use tracing::{debug, error, info, warn};

#[derive(Parser)]
pub struct Args {
    /// Camera name from configuration
    camera: String,

    /// Shell command to execute on motion detection (fire-and-forget)
    /// Supports template variables: {camera}, {score}, {time}, {zone}
    /// Environment variables are also set: CAMGRAB_CAMERA, CAMGRAB_SCORE, CAMGRAB_TIME, CAMGRAB_ZONE
    #[arg(long)]
    action: Option<String>,

    /// Motion detection threshold (0.0-1.0, higher = less sensitive)
    #[arg(short, long, default_value = "0.2")]
    threshold: f32,

    /// Cooldown period in seconds between triggered actions
    #[arg(short, long, default_value = "5")]
    cooldown: u64,

    /// Maximum watch duration in seconds (0 = run until Ctrl+C)
    #[arg(long, default_value = "0")]
    duration: u64,

    /// Path to zone configuration file (JSON)
    #[arg(long)]
    zones_from: Option<PathBuf>,

    /// RTSP transport protocol (overrides per-camera default)
    #[arg(long, value_enum)]
    transport: Option<Transport>,

    /// RTSP auth method
    #[arg(long, value_enum)]
    rtsp_auth: Option<RtspAuth>,

    /// Custom RTSP stream path
    #[arg(long)]
    path: Option<String>,

    /// Stream selection
    #[arg(long)]
    stream: Option<String>,

    /// Output events as JSON Lines (one object per line)
    #[arg(long)]
    json: bool,
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
    debug!("Watch command: camera={}", args.camera);

    // Validate
    if args.path.is_some() && args.stream.is_some() {
        return Err(miette::miette!(
            "use --path for custom RTSP token URLs; omit --stream"
        ));
    }

    if args.threshold < 0.0 || args.threshold > 1.0 {
        return Err(miette::miette!("threshold must be between 0.0 and 1.0"));
    }

    let is_tty = io::stdout().is_terminal();
    let start_time = Instant::now();
    let max_duration = if args.duration > 0 {
        Some(std::time::Duration::from_secs(args.duration))
    } else {
        None
    };

    if !args.json && is_tty {
        println!("{}", "▶ Starting motion detection".green().bold());
        println!("  Camera:    {}", args.camera.cyan());
        println!("  Threshold: {}", args.threshold.to_string().yellow());
        println!("  Cooldown:  {}s", args.cooldown);
        if let Some(d) = max_duration {
            println!("  Duration:  {}s", d.as_secs());
        } else {
            println!("  Duration:  indefinite");
        }
        if let Some(ref action) = args.action {
            println!("  Action:    {}", action.yellow());
        }
        if let Some(ref zones_path) = args.zones_from {
            println!("  Zones:     {}", zones_path.display().to_string().cyan());
        }
        println!("\n{}", "Press Ctrl+C to stop...".dimmed());
        println!();
    }

    // Set up Ctrl+C handler
    let ctrl_c = tokio::signal::ctrl_c();
    tokio::pin!(ctrl_c);

    info!("Motion detection started for camera '{}'", args.camera);

    let mut event_count: u64 = 0;

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

    // Create motion detector
    let motion_config = camgrab_core::motion::detector::MotionConfig {
        threshold: args.threshold as f64,
        cooldown: std::time::Duration::from_secs(args.cooldown),
        ..Default::default()
    };
    let mut motion_detector = camgrab_core::motion::detector::MotionDetector::new(motion_config)
        .map_err(|e| miette::miette!("Invalid motion config: {}", e))?;

    // Connect RTSP client and validate
    let mut client = camgrab_core::rtsp::client::RtspClient::new(&camera)
        .map_err(|e| miette::miette!("{}", e))?;
    client
        .connect()
        .await
        .map_err(|e| miette::miette!("Connection failed: {}", e))?;

    info!("RTSP connected, starting motion detection polling loop");

    let mut consecutive_errors: u32 = 0;
    const MAX_CONSECUTIVE_ERRORS: u32 = 10;

    // Main loop - poll frames via reconnect + capture_raw_frame + motion detect
    loop {
        // Check max duration
        if let Some(max_dur) = max_duration {
            if start_time.elapsed() >= max_dur {
                info!("Maximum watch duration reached");
                break;
            }
        }

        tokio::select! {
            _ = &mut ctrl_c => {
                info!("Received Ctrl+C, shutting down");
                break;
            }
            _ = tokio::time::sleep(tokio::time::Duration::from_millis(500)) => {
                // Reconnect for each capture (capture_raw_frame consumes the session)
                if let Err(e) = client.reconnect().await {
                    consecutive_errors += 1;
                    warn!(
                        "Reconnect failed ({}/{}): {}",
                        consecutive_errors, MAX_CONSECUTIVE_ERRORS, e
                    );
                    if consecutive_errors >= MAX_CONSECUTIVE_ERRORS {
                        return Err(miette::miette!(
                            "Too many consecutive connection failures ({}), aborting",
                            consecutive_errors
                        ));
                    }
                    // Exponential backoff on failure
                    let backoff = std::time::Duration::from_millis(
                        500 * 2u64.saturating_pow(consecutive_errors.min(5))
                    );
                    tokio::time::sleep(backoff).await;
                    continue;
                }
                consecutive_errors = 0;

                // Capture raw frame
                let raw_frame = match client.capture_raw_frame().await {
                    Ok(frame) => frame,
                    Err(e) => {
                        debug!("Frame capture failed: {}", e);
                        continue;
                    }
                };

                // Convert RGB to grayscale for motion detector
                let gray = rgb_to_grayscale(&raw_frame.rgb, raw_frame.width, raw_frame.height);

                // Feed frame to motion detector
                let motion_event = match motion_detector.feed_frame(&gray) {
                    Ok(event) => event,
                    Err(e) => {
                        warn!("Motion detection error: {}", e);
                        continue;
                    }
                };

                // If no motion event triggered, continue polling
                let event = match motion_event {
                    Some(e) => e,
                    None => continue,
                };

                event_count += 1;
                let score = event.score;
                let timestamp = event.timestamp;
                let time_str = timestamp.to_rfc3339();

                // Determine zone name from zone_scores
                let zone_str = event.zone_scores.keys().next()
                    .map(|s| s.as_str())
                    .unwrap_or("default");

                // Output event
                if args.json {
                    let json_event = serde_json::json!({
                        "event": "motion",
                        "camera": args.camera,
                        "score": score,
                        "zone": zone_str,
                        "time": time_str,
                    });
                    println!("{}", json_event);
                } else if is_tty {
                    println!(
                        "{} {} score={:.3} zone={} time={}",
                        "●".red().bold(),
                        "motion".yellow(),
                        score,
                        zone_str,
                        chrono::Local::now().format("%H:%M:%S"),
                    );
                } else {
                    println!(
                        "event=motion camera={} score={:.3} zone={} time={}",
                        args.camera,
                        score,
                        zone_str,
                        time_str,
                    );
                }

                // Execute action (fire-and-forget)
                if let Some(ref action_template) = args.action {
                    fire_and_forget_action(
                        action_template,
                        &args.camera,
                        score as f32,
                        &time_str,
                        zone_str,
                    );
                }
            }
        }
    }

    // Clean disconnect
    client.disconnect().await;

    // Shutdown summary
    let stats = motion_detector.stats();
    if !args.json && is_tty {
        println!();
        println!("{}", "✓ Motion detection stopped".yellow().bold());
        println!("  Events:   {}", event_count.to_string().cyan());
        println!("  Frames:   {}", stats.frames_processed.to_string().cyan());
        println!("  Runtime:  {:.1}s", start_time.elapsed().as_secs_f64());
    }

    Ok(())
}

/// Execute an action command in fire-and-forget mode (non-blocking).
/// The action command receives template variables and environment variables.
fn fire_and_forget_action(action_template: &str, camera: &str, score: f32, time: &str, zone: &str) {
    // Replace template variables (like camsnap's --action-template)
    let action = action_template
        .replace("{camera}", camera)
        .replace("{score}", &format!("{:.3}", score))
        .replace("{time}", time)
        .replace("{zone}", zone);

    let camera = camera.to_string();
    let score_str = format!("{:.3}", score);
    let time = time.to_string();
    let zone = zone.to_string();

    // Fire and forget - spawn without awaiting (like camsnap's cmd.Start())
    tokio::spawn(async move {
        let result = tokio::process::Command::new("sh")
            .arg("-c")
            .arg(&action)
            .env("CAMGRAB_CAMERA", &camera)
            .env("CAMGRAB_SCORE", &score_str)
            .env("CAMGRAB_TIME", &time)
            .env("CAMGRAB_ZONE", &zone)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();

        match result {
            Ok(mut child) => {
                // Wait for it in the background, but don't block
                let _ = child.wait().await;
            }
            Err(e) => {
                error!("Failed to start action command: {}", e);
            }
        }
    });

    debug!("Action fired: {}", action_template);
}

/// Convert RGB pixel data to a grayscale image using luminance formula.
/// Uses the standard BT.601 coefficients: Y = 0.299*R + 0.587*G + 0.114*B
fn rgb_to_grayscale(rgb: &[u8], width: u32, height: u32) -> GrayImage {
    let pixels: Vec<u8> = rgb
        .chunks_exact(3)
        .map(|px| {
            let r = px[0] as f32;
            let g = px[1] as f32;
            let b = px[2] as f32;
            (0.299 * r + 0.587 * g + 0.114 * b) as u8
        })
        .collect();
    GrayImage::from_raw(width, height, pixels).unwrap_or_else(|| GrayImage::new(width, height))
}
