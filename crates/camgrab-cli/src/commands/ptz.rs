use clap::{Parser, Subcommand};
use colored::Colorize;
use miette::{IntoDiagnostic, Result};
use serde_json::json;
use std::path::Path;
use tracing::{debug, info};

#[derive(Parser)]
pub struct Args {
    /// Camera name from configuration
    camera: String,

    #[command(subcommand)]
    command: PtzCommand,

    /// Output result as JSON
    #[arg(long)]
    json: bool,
}

#[derive(Subcommand)]
enum PtzCommand {
    /// Move camera relatively (pan/tilt/zoom)
    Move {
        /// Pan speed/direction (-1.0 to 1.0, negative = left, positive = right)
        #[arg(long, default_value = "0.0")]
        pan: f32,

        /// Tilt speed/direction (-1.0 to 1.0, negative = down, positive = up)
        #[arg(long, default_value = "0.0")]
        tilt: f32,

        /// Zoom speed/direction (-1.0 to 1.0, negative = zoom out, positive = zoom in)
        #[arg(long, default_value = "0.0")]
        zoom: f32,

        /// Movement duration in seconds
        #[arg(short, long, default_value = "1.0")]
        duration: f32,
    },

    /// Move to absolute position
    Goto {
        /// Absolute pan position (0.0 to 1.0)
        #[arg(long, required = true)]
        pan: f32,

        /// Absolute tilt position (0.0 to 1.0)
        #[arg(long, required = true)]
        tilt: f32,

        /// Absolute zoom position (0.0 to 1.0)
        #[arg(long)]
        zoom: Option<f32>,
    },

    /// Manage PTZ presets
    Preset {
        #[command(subcommand)]
        action: PresetAction,
    },

    /// Move to home position
    Home,

    /// Stop current PTZ movement
    Stop,

    /// Get current PTZ position
    Position,
}

#[derive(Subcommand)]
enum PresetAction {
    /// List all saved presets
    List,

    /// Save current position as a preset
    Save {
        /// Preset name
        name: String,
    },

    /// Go to a saved preset
    Goto {
        /// Preset name or slot number
        name: String,
    },

    /// Delete a preset
    Delete {
        /// Preset name or slot number
        name: String,
    },
}

pub async fn run(args: Args, config_path: &Path) -> Result<()> {
    debug!("PTZ command: camera={}", args.camera);

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

    // Build PTZ controller
    // PTZ requires ONVIF endpoint - construct from camera host
    let onvif_port = 80; // Standard ONVIF port
    let endpoint = format!(
        "http://{}:{}/onvif/ptz_service",
        camera_config.host, onvif_port
    );
    let profile_token = "Profile_1".to_string(); // Default ONVIF profile token
    let auth = match (&camera_config.username, &camera_config.password) {
        (Some(u), Some(p)) => Some((u.as_str(), p.as_str())),
        _ => None,
    };
    let ptz = camgrab_core::onvif::ptz::PtzController::new(&endpoint, &profile_token, auth);

    match args.command {
        PtzCommand::Move {
            pan,
            tilt,
            zoom,
            duration,
        } => {
            // Validate ranges
            if !(-1.0..=1.0).contains(&pan)
                || !(-1.0..=1.0).contains(&tilt)
                || !(-1.0..=1.0).contains(&zoom)
            {
                return Err(miette::miette!(
                    "Pan, tilt, and zoom values must be between -1.0 and 1.0"
                ));
            }

            if duration <= 0.0 {
                return Err(miette::miette!("Duration must be positive"));
            }

            if !args.json {
                println!("{}", "▶ Moving camera".cyan().bold());
                println!("  Pan:      {}", format_direction(pan, "left", "right"));
                println!("  Tilt:     {}", format_direction(tilt, "down", "up"));
                println!("  Zoom:     {}", format_direction(zoom, "out", "in"));
                println!("  Duration: {}s", duration);
            }

            ptz.execute(camgrab_core::onvif::ptz::PtzCommand::ContinuousMove(
                camgrab_core::onvif::ptz::PtzPosition::new(pan as f64, tilt as f64, zoom as f64),
            ))
            .await
            .map_err(|e| miette::miette!("PTZ move failed: {}", e))?;
            // Stop after duration
            tokio::time::sleep(std::time::Duration::from_secs_f32(duration)).await;
            ptz.execute(camgrab_core::onvif::ptz::PtzCommand::Stop)
                .await
                .map_err(|e| miette::miette!("PTZ stop failed: {}", e))?;

            info!("Camera moved");

            if args.json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&json!({
                        "success": true,
                        "action": "move",
                        "pan": pan,
                        "tilt": tilt,
                        "zoom": zoom,
                        "duration": duration,
                    }))
                    .into_diagnostic()?
                );
            } else {
                println!("\n{}", "✓ Movement complete".green().bold());
            }
        }

        PtzCommand::Goto { pan, tilt, zoom } => {
            // Validate ranges
            if !(0.0..=1.0).contains(&pan) || !(0.0..=1.0).contains(&tilt) {
                return Err(miette::miette!(
                    "Pan and tilt values must be between 0.0 and 1.0"
                ));
            }
            if let Some(z) = zoom {
                if !(0.0..=1.0).contains(&z) {
                    return Err(miette::miette!("Zoom value must be between 0.0 and 1.0"));
                }
            }

            if !args.json {
                println!("{}", "▶ Moving to absolute position".cyan().bold());
                println!("  Pan:  {}", pan);
                println!("  Tilt: {}", tilt);
                if let Some(z) = zoom {
                    println!("  Zoom: {}", z);
                }
            }

            let zoom_val = zoom.unwrap_or(0.0);
            ptz.execute(camgrab_core::onvif::ptz::PtzCommand::AbsoluteMove(
                camgrab_core::onvif::ptz::PtzPosition::new(
                    pan as f64,
                    tilt as f64,
                    zoom_val as f64,
                ),
            ))
            .await
            .map_err(|e| miette::miette!("PTZ goto failed: {}", e))?;

            info!("Camera moved to absolute position");

            if args.json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&json!({
                        "success": true,
                        "action": "goto",
                        "pan": pan,
                        "tilt": tilt,
                        "zoom": zoom,
                    }))
                    .into_diagnostic()?
                );
            } else {
                println!("\n{}", "✓ Position reached".green().bold());
            }
        }

        PtzCommand::Preset { action } => match action {
            PresetAction::List => {
                let presets = ptz
                    .get_presets()
                    .await
                    .map_err(|e| miette::miette!("Failed to get presets: {}", e))?;

                if args.json {
                    let presets_json: Vec<_> = presets
                        .iter()
                        .map(|p| {
                            json!({
                                "token": p.token,
                                "name": p.name,
                                "pan": p.position.as_ref().map(|pos| pos.pan).unwrap_or(0.0),
                                "tilt": p.position.as_ref().map(|pos| pos.tilt).unwrap_or(0.0),
                                "zoom": p.position.as_ref().map(|pos| pos.zoom).unwrap_or(0.0),
                            })
                        })
                        .collect();

                    println!(
                        "{}",
                        serde_json::to_string_pretty(&json!({
                            "count": presets.len(),
                            "presets": presets_json
                        }))
                        .into_diagnostic()?
                    );
                } else {
                    if presets.is_empty() {
                        println!("{}", "No presets configured.".yellow());
                    } else {
                        println!(
                            "{} {} preset(s):\n",
                            "✓".green().bold(),
                            presets.len().to_string().cyan().bold()
                        );

                        for preset in presets {
                            println!(
                                "  {} {}",
                                preset.token.yellow().bold(),
                                if !preset.name.is_empty() {
                                    format!("({})", preset.name)
                                } else {
                                    String::new()
                                }
                                .dimmed()
                            );
                            if let Some(pos) = &preset.position {
                                println!(
                                    "    Position: pan={:.2}, tilt={:.2}, zoom={:.2}",
                                    pos.pan, pos.tilt, pos.zoom
                                );
                            }
                        }
                    }
                }
            }

            PresetAction::Save { name } => {
                if !args.json {
                    println!(
                        "{}",
                        format!("▶ Saving current position as '{}'", name)
                            .cyan()
                            .bold()
                    );
                }

                ptz.execute(camgrab_core::onvif::ptz::PtzCommand::SetPreset(
                    name.clone(),
                ))
                .await
                .map_err(|e| miette::miette!("Failed to save preset: {}", e))?;

                info!("Preset '{}' saved", name);

                if args.json {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&json!({
                            "success": true,
                            "action": "preset_save",
                            "name": name,
                        }))
                        .into_diagnostic()?
                    );
                } else {
                    println!("\n{}", format!("✓ Preset '{}' saved", name).green().bold());
                }
            }

            PresetAction::Goto { name } => {
                if !args.json {
                    println!("{}", format!("▶ Moving to preset '{}'", name).cyan().bold());
                }

                ptz.execute(camgrab_core::onvif::ptz::PtzCommand::GotoPreset(
                    name.clone(),
                ))
                .await
                .map_err(|e| miette::miette!("Failed to go to preset: {}", e))?;

                info!("Moved to preset '{}'", name);

                if args.json {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&json!({
                            "success": true,
                            "action": "preset_goto",
                            "name": name,
                        }))
                        .into_diagnostic()?
                    );
                } else {
                    println!(
                        "\n{}",
                        format!("✓ Moved to preset '{}'", name).green().bold()
                    );
                }
            }

            PresetAction::Delete { name } => {
                if !args.json {
                    println!("{}", format!("▶ Deleting preset '{}'", name).cyan().bold());
                }

                ptz.execute(camgrab_core::onvif::ptz::PtzCommand::RemovePreset(
                    name.clone(),
                ))
                .await
                .map_err(|e| miette::miette!("Failed to delete preset: {}", e))?;

                info!("Preset '{}' deleted", name);

                if args.json {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&json!({
                            "success": true,
                            "action": "preset_delete",
                            "name": name,
                        }))
                        .into_diagnostic()?
                    );
                } else {
                    println!(
                        "\n{}",
                        format!("✓ Preset '{}' deleted", name).green().bold()
                    );
                }
            }
        },

        PtzCommand::Home => {
            if !args.json {
                println!("{}", "▶ Moving to home position".cyan().bold());
            }

            ptz.execute(camgrab_core::onvif::ptz::PtzCommand::GotoHome)
                .await
                .map_err(|e| miette::miette!("Failed to go to home: {}", e))?;

            info!("Moved to home position");

            if args.json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&json!({
                        "success": true,
                        "action": "home",
                    }))
                    .into_diagnostic()?
                );
            } else {
                println!("\n{}", "✓ Home position reached".green().bold());
            }
        }

        PtzCommand::Stop => {
            if !args.json {
                println!("{}", "▶ Stopping PTZ movement".cyan().bold());
            }

            ptz.execute(camgrab_core::onvif::ptz::PtzCommand::Stop)
                .await
                .map_err(|e| miette::miette!("PTZ stop failed: {}", e))?;

            info!("PTZ movement stopped");

            if args.json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&json!({
                        "success": true,
                        "action": "stop",
                    }))
                    .into_diagnostic()?
                );
            } else {
                println!("\n{}", "✓ Movement stopped".green().bold());
            }
        }

        PtzCommand::Position => {
            let pos = ptz
                .get_position()
                .await
                .map_err(|e| miette::miette!("Failed to get position: {}", e))?;

            if args.json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&json!({
                        "success": true,
                        "position": {
                            "pan": pos.pan,
                            "tilt": pos.tilt,
                            "zoom": pos.zoom,
                        }
                    }))
                    .into_diagnostic()?
                );
            } else {
                println!("{}", "Current PTZ Position:".cyan().bold());
                println!("  Pan:  {:.2}", pos.pan);
                println!("  Tilt: {:.2}", pos.tilt);
                println!("  Zoom: {:.2}", pos.zoom);
            }
        }
    }

    Ok(())
}

fn format_direction(value: f32, negative: &str, positive: &str) -> String {
    if value < -0.01 {
        format!("{} ({:.2})", negative, value.abs())
            .red()
            .to_string()
    } else if value > 0.01 {
        format!("{} ({:.2})", positive, value).green().to_string()
    } else {
        "stationary".dimmed().to_string()
    }
}
