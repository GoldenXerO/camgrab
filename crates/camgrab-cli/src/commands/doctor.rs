use clap::Parser;
use colored::Colorize;
use is_terminal::IsTerminal;
use miette::{IntoDiagnostic, Result};
use serde_json::json;
use std::io;
use std::path::Path;
use std::time::{Duration, Instant};
use tracing::debug;

#[derive(Parser)]
pub struct Args {
    /// Camera name to check (checks all cameras if not specified)
    camera: Option<String>,

    /// Actually try to connect and probe RTSP streams (with 3 retries)
    #[arg(long)]
    probe: bool,

    /// Timeout per check in seconds
    #[arg(long, default_value = "5")]
    timeout: u64,

    /// RTSP transport for probe
    #[arg(long, value_enum, default_value = "tcp")]
    transport: Transport,

    /// RTSP auth method for probe
    #[arg(long, value_enum, default_value = "auto")]
    rtsp_auth: RtspAuth,

    /// Output results as JSON
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

struct HealthCheck {
    name: String,
    status: CheckStatus,
    message: String,
    category: Option<String>,
}

enum CheckStatus {
    Pass,
    Warn,
    Fail,
}

pub async fn run(args: Args, config_path: &Path) -> Result<()> {
    debug!("Doctor command");

    let is_tty = io::stdout().is_terminal();
    let timeout = Duration::from_secs(args.timeout);

    let mut checks = Vec::new();

    // 1. Check configuration file
    let config_exists = config_path.exists();
    checks.push(HealthCheck {
        name: "Configuration file".to_string(),
        status: if config_exists {
            CheckStatus::Pass
        } else {
            CheckStatus::Warn
        },
        message: if config_exists {
            format!("Found at {}", config_path.display())
        } else {
            format!(
                "Not found at {}. Run 'camgrab add' to create one.",
                config_path.display()
            )
        },
        category: None,
    });

    // 2. Check each camera
    // Load real cameras from config
    let app_config =
        camgrab_core::config::load(config_path).map_err(|e| miette::miette!("{}", e))?;

    let cameras_to_check: Vec<(String, String, u16)> = if let Some(ref name) = args.camera {
        // Check specific camera
        match camgrab_core::config::find_camera(&app_config, name) {
            Some(cam) => vec![(cam.name.clone(), cam.host.clone(), cam.port.unwrap_or(554))],
            None => {
                return Err(miette::miette!(
                    "Camera '{}' not found in configuration",
                    name
                ))
            }
        }
    } else {
        // Check all cameras
        app_config
            .cameras
            .iter()
            .map(|cam| (cam.name.clone(), cam.host.clone(), cam.port.unwrap_or(554)))
            .collect()
    };

    if cameras_to_check.is_empty() {
        checks.push(HealthCheck {
            name: "Cameras".to_string(),
            status: CheckStatus::Warn,
            message: "No cameras configured. Add one with: camgrab add <name> --host <ip>"
                .to_string(),
            category: None,
        });
    }

    for (name, host, port) in &cameras_to_check {
        // TCP connectivity check with timing
        let addr = format!("{}:{}", host, port);
        let start = Instant::now();
        let tcp_result = tokio::time::timeout(timeout, tokio::net::TcpStream::connect(&addr)).await;

        let latency = start.elapsed();

        match tcp_result {
            Ok(Ok(_)) => {
                checks.push(HealthCheck {
                    name: format!("{} reachable", name),
                    status: CheckStatus::Pass,
                    message: format!(
                        "{} reachable ({:.0}ms)",
                        addr,
                        latency.as_secs_f64() * 1000.0
                    ),
                    category: None,
                });
            }
            Ok(Err(e)) => {
                let category = classify_network_error(&e.to_string());
                checks.push(HealthCheck {
                    name: format!("{} reachable", name),
                    status: CheckStatus::Fail,
                    message: format!("Cannot connect to {} ({})", addr, category),
                    category: Some(category),
                });
                continue; // Skip probe if TCP fails
            }
            Err(_) => {
                checks.push(HealthCheck {
                    name: format!("{} reachable", name),
                    status: CheckStatus::Fail,
                    message: format!("Connection to {} timed out after {}s", addr, args.timeout),
                    category: Some("network-timeout".to_string()),
                });
                continue;
            }
        }

        // RTSP probe with 3 retries (only if --probe)
        if args.probe {
            let mut probe_ok = false;
            let mut last_error = String::new();
            let max_retries = 3;

            // Build camera from config for real RTSP probe
            let cam_config = camgrab_core::config::find_camera(&app_config, name);

            for attempt in 1..=max_retries {
                debug!(
                    "RTSP probe attempt {}/{} for {}",
                    attempt, max_retries, name
                );

                if let Some(cam_cfg) = cam_config {
                    // Real RTSP probe via camgrab-core
                    let cam = camgrab_core::camera::Camera::from_config(cam_cfg);
                    match camgrab_core::rtsp::client::RtspClient::new(&cam) {
                        Ok(mut client) => {
                            let connect_result =
                                tokio::time::timeout(timeout, client.connect()).await;

                            match connect_result {
                                Ok(Ok(())) => {
                                    client.disconnect().await;
                                    probe_ok = true;
                                    break;
                                }
                                Ok(Err(e)) => {
                                    last_error = e.to_string();
                                }
                                Err(_) => {
                                    last_error = "RTSP connect timed out".to_string();
                                }
                            }
                        }
                        Err(e) => {
                            last_error = format!("RTSP client init: {}", e);
                        }
                    }
                } else {
                    // Fallback to TCP probe if camera config not found
                    let probe_result = tokio::time::timeout(
                        Duration::from_secs(2),
                        tokio::net::TcpStream::connect(&addr),
                    )
                    .await;

                    match probe_result {
                        Ok(Ok(_)) => {
                            probe_ok = true;
                            break;
                        }
                        Ok(Err(e)) => {
                            last_error = e.to_string();
                        }
                        Err(_) => {
                            last_error = "timed out".to_string();
                        }
                    }
                }

                if attempt < max_retries {
                    tokio::time::sleep(Duration::from_millis(500)).await;
                }
            }

            if probe_ok {
                checks.push(HealthCheck {
                    name: format!("{} RTSP probe", name),
                    status: CheckStatus::Pass,
                    message: "Stream accessible".to_string(),
                    category: None,
                });
            } else {
                let category = classify_network_error(&last_error);
                checks.push(HealthCheck {
                    name: format!("{} RTSP probe", name),
                    status: CheckStatus::Fail,
                    message: format!("RTSP probe failed: {} ({})", last_error, category),
                    category: Some(category),
                });
            }
        }
    }

    // Output results
    if args.json {
        let results_json: Vec<_> = checks
            .iter()
            .map(|c| {
                json!({
                    "name": c.name,
                    "status": match c.status {
                        CheckStatus::Pass => "pass",
                        CheckStatus::Warn => "warn",
                        CheckStatus::Fail => "fail",
                    },
                    "message": c.message,
                    "category": c.category,
                })
            })
            .collect();

        let passed = checks
            .iter()
            .filter(|c| matches!(c.status, CheckStatus::Pass))
            .count();
        let warnings = checks
            .iter()
            .filter(|c| matches!(c.status, CheckStatus::Warn))
            .count();
        let failed = checks
            .iter()
            .filter(|c| matches!(c.status, CheckStatus::Fail))
            .count();

        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "total": checks.len(),
                "passed": passed,
                "warnings": warnings,
                "failed": failed,
                "checks": results_json,
            }))
            .into_diagnostic()?
        );
    } else {
        if is_tty {
            println!("{}", "Running health checks...".cyan().bold());
            println!();
        }

        for check in &checks {
            let (icon, label) = match check.status {
                CheckStatus::Pass => ("✓".green(), "PASS".green().bold()),
                CheckStatus::Warn => ("⚠".yellow(), "WARN".yellow().bold()),
                CheckStatus::Fail => ("✗".red(), "FAIL".red().bold()),
            };

            if is_tty {
                println!("{} {} {}", icon, label, check.name.bold());
                println!("  {}", check.message.dimmed());
            } else {
                let status = match check.status {
                    CheckStatus::Pass => "pass",
                    CheckStatus::Warn => "warn",
                    CheckStatus::Fail => "fail",
                };
                println!("{}  {}  {}", status, check.name, check.message);
            }
        }

        if is_tty {
            let passed = checks
                .iter()
                .filter(|c| matches!(c.status, CheckStatus::Pass))
                .count();
            let failed = checks
                .iter()
                .filter(|c| matches!(c.status, CheckStatus::Fail))
                .count();

            println!();
            if failed == 0 {
                println!("{}", "✓ All checks passed!".green().bold());
            } else {
                println!(
                    "{} {}/{} checks passed",
                    "✗".red().bold(),
                    passed,
                    checks.len()
                );
            }
        }
    }

    Ok(())
}

/// Classify a network error message into a category (like camsnap)
fn classify_network_error(err: &str) -> String {
    let lower = err.to_lowercase();
    if lower.contains("401")
        || lower.contains("unauthorized")
        || lower.contains("authentication")
        || lower.contains("forbidden")
        || lower.contains("403")
    {
        "auth".to_string()
    } else if lower.contains("connection refused") || lower.contains("refused") {
        "network-refused".to_string()
    } else if lower.contains("timed out") || lower.contains("timeout") {
        "network-timeout".to_string()
    } else if lower.contains("not found") || lower.contains("404") {
        "not-found".to_string()
    } else {
        "unknown".to_string()
    }
}
