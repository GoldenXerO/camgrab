use clap::Parser;
use miette::{IntoDiagnostic, WrapErr};
use std::path::PathBuf;
use tracing_subscriber::{fmt, EnvFilter};

mod commands;

#[derive(Parser)]
#[command(name = "camgrab")]
#[command(version, about = "Capture, record, and monitor RTSP/ONVIF cameras", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: commands::Command,

    /// Enable verbose logging
    #[arg(short, long, global = true)]
    verbose: bool,

    /// Override configuration file path
    #[arg(long, global = true, env = "CAMGRAB_CONFIG")]
    config: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> miette::Result<()> {
    let cli = Cli::parse();

    // Initialize tracing subscriber
    let env_log = std::env::var("CAMGRAB_LOG").ok();
    let log_level = if cli.verbose {
        "camgrab=debug,camgrab_cli=debug,camgrab_core=debug"
    } else {
        env_log.as_deref().unwrap_or("camgrab=info")
    };

    fmt()
        .with_env_filter(EnvFilter::try_new(log_level).into_diagnostic()?)
        .with_target(false)
        .init();

    // Dispatch to command handler
    commands::dispatch(cli.command, cli.config.as_deref())
        .await
        .wrap_err("Command execution failed")
}
