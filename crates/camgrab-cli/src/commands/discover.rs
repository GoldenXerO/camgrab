use clap::Parser;
use colored::Colorize;
use is_terminal::IsTerminal;
use miette::{IntoDiagnostic, Result};
use serde_json::json;
use std::io;
use tracing::{debug, info};

#[derive(Parser)]
pub struct Args {
    /// Discovery timeout in seconds
    #[arg(short, long, default_value = "3")]
    timeout: u64,

    /// Fetch detailed device information for each discovered camera
    #[arg(long)]
    info: bool,

    /// Output results as JSON
    #[arg(long)]
    json: bool,
}

pub async fn run(args: Args) -> Result<()> {
    debug!("Discover command: timeout={}s", args.timeout);

    let is_tty = io::stdout().is_terminal();

    if !args.json && is_tty {
        println!("{}", "▶ Discovering ONVIF cameras...".cyan().bold());
        println!();
    }

    // Run ONVIF WS-Discovery
    let discovery_config = camgrab_core::onvif::discovery::DiscoveryConfig {
        timeout: std::time::Duration::from_secs(args.timeout),
        interface: None,
    };
    let discovered = camgrab_core::onvif::discovery::discover(&discovery_config)
        .map_err(|e| miette::miette!("Discovery failed: {}", e))?;

    if discovered.is_empty() {
        if args.json {
            println!("{}", json!({"devices": []}).to_string());
        } else {
            println!("{}", "✗ No ONVIF devices found".yellow());
        }
        return Ok(());
    }

    info!("Found {} device(s)", discovered.len());

    if args.json {
        let devices_json: Vec<_> = discovered
            .iter()
            .map(|d| {
                json!({
                    "name": d.name.as_deref().unwrap_or("Unknown Device"),
                    "address": d.address,
                    "types": d.types,
                    "xaddrs": d.xaddrs,
                    "scopes": d.scopes,
                })
            })
            .collect();

        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "count": discovered.len(),
                "devices": devices_json
            }))
            .into_diagnostic()?
        );
    } else if is_tty {
        println!(
            "{} Found {} device(s):\n",
            "✓".green().bold(),
            discovered.len().to_string().cyan().bold()
        );

        for (i, device) in discovered.iter().enumerate() {
            let device_name = device.name.as_deref().unwrap_or("Unknown Device");
            println!(
                "{}. {}",
                (i + 1).to_string().yellow().bold(),
                device_name.cyan().bold()
            );

            // Extract address/port from xaddrs if possible, otherwise use device.address
            let address_display = if !device.xaddrs.is_empty() {
                device.xaddrs[0].clone()
            } else {
                device.address.clone()
            };
            println!("   Address:      {}", address_display.yellow());

            if args.info {
                if !device.types.is_empty() {
                    println!("   Types:        {}", device.types.len());
                    for type_str in &device.types {
                        println!("     - {}", type_str.dimmed());
                    }
                }

                if !device.xaddrs.is_empty() {
                    println!("   Endpoints:    {}", device.xaddrs.len());
                    for xaddr in &device.xaddrs {
                        println!("     - {}", xaddr.dimmed());
                    }
                }
            }

            println!("   Scopes:       {}", device.scopes.len());
            for scope in &device.scopes {
                println!("     - {}", scope.dimmed());
            }

            // Print ready-to-use add command (like camsnap)
            let host_for_command = device.address.clone();
            println!(
                "   {}",
                format!(
                    "add: camgrab add cam-{} --host {} --user <user> --pass <pass>",
                    host_for_command.replace('.', "-").replace(':', "-"),
                    host_for_command
                )
                .dimmed()
            );
            println!();
        }
    } else {
        // Non-TTY: one address per line (useful for scripting)
        for device in &discovered {
            if !device.xaddrs.is_empty() {
                for xaddr in &device.xaddrs {
                    println!("{}", xaddr);
                }
            } else {
                println!("{}", device.address);
            }
        }
    }

    Ok(())
}
