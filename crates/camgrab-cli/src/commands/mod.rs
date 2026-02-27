use clap::Subcommand;
use miette::Result;
use std::path::{Path, PathBuf};

mod add;
mod clip;
mod discover;
mod doctor;
mod list;
mod ptz;
mod snap;
mod watch;

#[derive(Subcommand)]
pub enum Command {
    /// Capture a single snapshot from a camera
    Snap(snap::Args),
    /// Record a video clip from a camera
    Clip(clip::Args),
    /// Watch for motion detection events
    Watch(watch::Args),
    /// Discover ONVIF cameras on the network
    Discover(discover::Args),
    /// Check camera health and connectivity
    Doctor(doctor::Args),
    /// Add a new camera to configuration
    Add(add::Args),
    /// List all configured cameras
    List(list::Args),
    /// Control PTZ (Pan-Tilt-Zoom) cameras
    Ptz(ptz::Args),
}

/// Resolve the config path from a CLI override or the default location.
fn resolve_config_path(override_path: Option<&Path>) -> PathBuf {
    override_path
        .map(|p| p.to_path_buf())
        .unwrap_or_else(camgrab_core::config::default_config_path)
}

pub async fn dispatch(command: Command, config_override: Option<&Path>) -> Result<()> {
    let config_path = resolve_config_path(config_override);
    match command {
        Command::Snap(args) => snap::run(args, &config_path).await,
        Command::Clip(args) => clip::run(args, &config_path).await,
        Command::Watch(args) => watch::run(args, &config_path).await,
        Command::Discover(args) => discover::run(args).await,
        Command::Doctor(args) => doctor::run(args, &config_path).await,
        Command::Add(args) => add::run(args, &config_path).await,
        Command::List(args) => list::run(args, &config_path).await,
        Command::Ptz(args) => ptz::run(args, &config_path).await,
    }
}
