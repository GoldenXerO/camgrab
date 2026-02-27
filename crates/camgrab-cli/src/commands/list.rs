use clap::Parser;
use colored::Colorize;
use is_terminal::IsTerminal;
use miette::{IntoDiagnostic, Result};
use serde_json::json;
use std::io;
use std::path::Path;
use tracing::debug;

#[derive(Parser)]
pub struct Args {
    /// Output results as JSON
    #[arg(long)]
    json: bool,
}

pub async fn run(args: Args, config_path: &Path) -> Result<()> {
    debug!("List command");

    let is_tty = io::stdout().is_terminal();

    // Load config from camgrab-core
    let app_config =
        camgrab_core::config::load(config_path).map_err(|e| miette::miette!("{}", e))?;

    // Extract camera data from config
    let mut cameras = app_config.cameras;

    // Sort alphabetically by name (like camsnap)
    cameras.sort_by(|a, b| a.name.cmp(&b.name));

    if cameras.is_empty() {
        if args.json {
            println!("{}", json!({"cameras": []}).to_string());
        } else if is_tty {
            println!("{}", "No cameras configured.".yellow());
            println!(
                "\nAdd one with: {}",
                "camgrab add <name> --host <ip> --user <user> --pass <pass>".green()
            );
        }
        return Ok(());
    }

    if args.json {
        let cameras_json: Vec<_> = cameras
            .iter()
            .map(|c| {
                json!({
                    "name": c.name,
                    "host": c.host,
                    "port": c.port.unwrap_or(554),
                    "username": c.username,
                    "protocol": c.protocol.as_ref().map(|p| p.to_string()).unwrap_or_else(|| "rtsp".to_string()),
                    "transport": c.transport.as_ref().map(|t| t.to_string()).unwrap_or_else(|| "tcp".to_string()),
                })
            })
            .collect();

        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "count": cameras.len(),
                "cameras": cameras_json
            }))
            .into_diagnostic()?
        );
    } else if is_tty {
        println!(
            "{} {} camera(s) configured:\n",
            "✓".green().bold(),
            cameras.len().to_string().cyan().bold()
        );

        for camera in &cameras {
            let password_display = if camera.password.is_some() {
                "***"
            } else {
                "-"
            };
            let user_display = camera.username.as_deref().unwrap_or("-");

            println!(
                "  {:<16} {:<20} :{:<6} {:<8} user={}  pass={}",
                camera.name.cyan(),
                camera.host,
                camera.port.unwrap_or(554),
                camera
                    .transport
                    .as_ref()
                    .map(|t| t.to_string())
                    .unwrap_or_else(|| "tcp".to_string()),
                user_display,
                password_display
            );
        }

        println!(
            "\nUse {} to check connectivity",
            "camgrab doctor --probe".cyan()
        );
    } else {
        // Non-TTY: one camera name per line
        for camera in &cameras {
            println!("{}", camera.name);
        }
    }

    Ok(())
}
