//! Configuration module for camgrab
//!
//! Provides config loading/saving (TOML), camera lookup, and upsert operations.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use thiserror::Error;

use crate::camera::{AuthMethod, Protocol, StreamType, Transport};

/// Camera configuration structure
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CameraConfig {
    /// Camera name/identifier
    pub name: String,
    /// Hostname or IP address
    pub host: String,
    /// RTSP port (default: 554)
    #[serde(default)]
    pub port: Option<u16>,
    /// Username for authentication
    #[serde(default)]
    pub username: Option<String>,
    /// Password for authentication
    #[serde(default)]
    pub password: Option<String>,
    /// RTSP protocol (default: rtsp)
    #[serde(default)]
    pub protocol: Option<Protocol>,
    /// Network transport (default: tcp)
    #[serde(default)]
    pub transport: Option<Transport>,
    /// Stream type (default: main)
    #[serde(default)]
    pub stream_type: Option<StreamType>,
    /// Custom stream path override
    #[serde(default)]
    pub custom_path: Option<String>,
    /// Whether to enable audio (default: false)
    #[serde(default)]
    pub audio_enabled: Option<bool>,
    /// Authentication method (default: auto)
    #[serde(default)]
    pub auth_method: Option<AuthMethod>,
    /// Connection timeout in seconds (default: 10)
    #[serde(default)]
    pub timeout_secs: Option<u64>,
}

/// Application configuration wrapping multiple cameras
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct AppConfig {
    /// List of configured cameras
    #[serde(default)]
    pub cameras: Vec<CameraConfig>,
}

impl AppConfig {
    /// Creates a new empty config
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the number of cameras
    pub fn len(&self) -> usize {
        self.cameras.len()
    }

    /// Returns true if no cameras configured
    pub fn is_empty(&self) -> bool {
        self.cameras.is_empty()
    }
}

/// Configuration errors
#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("I/O error accessing {0}: {1}")]
    IoError(PathBuf, #[source] std::io::Error),

    #[error("Failed to parse config from {0}: {1}")]
    ParseError(PathBuf, String),

    #[error("Failed to serialize config: {0}")]
    SerializeError(String),
}

/// Returns the default config path: ~/.config/camgrab/config.toml
pub fn default_config_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("camgrab")
        .join("config.toml")
}

/// Loads configuration from a TOML file.
/// Returns empty AppConfig if file doesn't exist (not an error).
pub fn load(path: &Path) -> Result<AppConfig, ConfigError> {
    if !path.exists() {
        tracing::debug!(
            "Config file not found at {}, returning empty config",
            path.display()
        );
        return Ok(AppConfig::default());
    }

    let contents =
        std::fs::read_to_string(path).map_err(|e| ConfigError::IoError(path.to_path_buf(), e))?;

    let config: AppConfig = toml::from_str(&contents)
        .map_err(|e| ConfigError::ParseError(path.to_path_buf(), e.to_string()))?;

    tracing::debug!(
        "Loaded config with {} camera(s) from {}",
        config.len(),
        path.display()
    );
    Ok(config)
}

/// Saves configuration to a TOML file with 0o600 permissions.
/// Creates parent directories if they don't exist.
pub fn save(path: &Path, config: &AppConfig) -> Result<(), ConfigError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| ConfigError::IoError(parent.to_path_buf(), e))?;
    }

    let contents =
        toml::to_string_pretty(config).map_err(|e| ConfigError::SerializeError(e.to_string()))?;

    std::fs::write(path, &contents).map_err(|e| ConfigError::IoError(path.to_path_buf(), e))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(0o600);
        let _ = std::fs::set_permissions(path, perms);
    }

    tracing::debug!(
        "Saved config with {} camera(s) to {}",
        config.len(),
        path.display()
    );
    Ok(())
}

/// Inserts or updates a camera in the config.
/// Returns true if camera was newly added, false if updated.
pub fn upsert_camera(config: &mut AppConfig, camera: CameraConfig) -> bool {
    if let Some(existing) = config.cameras.iter_mut().find(|c| c.name == camera.name) {
        *existing = camera;
        false
    } else {
        config.cameras.push(camera);
        true
    }
}

/// Finds a camera by name.
pub fn find_camera<'a>(config: &'a AppConfig, name: &str) -> Option<&'a CameraConfig> {
    config.cameras.iter().find(|c| c.name == name)
}

/// Removes a camera by name.
/// Returns true if camera was found and removed.
pub fn remove_camera(config: &mut AppConfig, name: &str) -> bool {
    let len = config.cameras.len();
    config.cameras.retain(|c| c.name != name);
    config.cameras.len() < len
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn sample_camera(name: &str) -> CameraConfig {
        CameraConfig {
            name: name.to_string(),
            host: "192.168.1.100".to_string(),
            port: Some(554),
            username: Some("admin".to_string()),
            password: Some("secret".to_string()),
            protocol: Some(Protocol::Rtsp),
            transport: Some(Transport::Tcp),
            stream_type: None,
            custom_path: None,
            audio_enabled: Some(false),
            auth_method: Some(AuthMethod::Auto),
            timeout_secs: Some(10),
        }
    }

    #[test]
    fn test_default_config_path() {
        let path = default_config_path();
        assert!(path.ends_with("camgrab/config.toml"));
    }

    #[test]
    fn test_load_nonexistent() {
        let path = Path::new("/tmp/nonexistent-camgrab-test/config.toml");
        let config = load(path).unwrap();
        assert!(config.is_empty());
    }

    #[test]
    fn test_save_and_load_roundtrip() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("config.toml");

        let mut config = AppConfig::new();
        config.cameras.push(sample_camera("front_door"));
        config.cameras.push(sample_camera("backyard"));

        save(&path, &config).unwrap();
        let loaded = load(&path).unwrap();

        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded.cameras[0].name, "front_door");
        assert_eq!(loaded.cameras[1].name, "backyard");
        assert_eq!(loaded.cameras[0].host, "192.168.1.100");
        assert_eq!(loaded.cameras[0].password, Some("secret".to_string()));
    }

    #[test]
    fn test_upsert_new_camera() {
        let mut config = AppConfig::new();
        let is_new = upsert_camera(&mut config, sample_camera("test"));
        assert!(is_new);
        assert_eq!(config.len(), 1);
    }

    #[test]
    fn test_upsert_existing_camera() {
        let mut config = AppConfig::new();
        upsert_camera(&mut config, sample_camera("test"));

        let mut updated = sample_camera("test");
        updated.host = "10.0.0.1".to_string();
        let is_new = upsert_camera(&mut config, updated);

        assert!(!is_new);
        assert_eq!(config.len(), 1);
        assert_eq!(config.cameras[0].host, "10.0.0.1");
    }

    #[test]
    fn test_find_camera() {
        let mut config = AppConfig::new();
        config.cameras.push(sample_camera("front_door"));
        config.cameras.push(sample_camera("backyard"));

        assert!(find_camera(&config, "front_door").is_some());
        assert!(find_camera(&config, "backyard").is_some());
        assert!(find_camera(&config, "nonexistent").is_none());
    }

    #[test]
    fn test_remove_camera() {
        let mut config = AppConfig::new();
        config.cameras.push(sample_camera("front_door"));
        config.cameras.push(sample_camera("backyard"));

        assert!(remove_camera(&mut config, "front_door"));
        assert_eq!(config.len(), 1);
        assert_eq!(config.cameras[0].name, "backyard");

        assert!(!remove_camera(&mut config, "nonexistent"));
        assert_eq!(config.len(), 1);
    }

    #[test]
    fn test_save_creates_parent_dirs() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("nested").join("deep").join("config.toml");

        let config = AppConfig::new();
        save(&path, &config).unwrap();
        assert!(path.exists());
    }

    #[test]
    fn test_toml_serialization_readable() {
        let mut config = AppConfig::new();
        config.cameras.push(sample_camera("test_cam"));

        let toml_str = toml::to_string_pretty(&config).unwrap();
        assert!(toml_str.contains("name = \"test_cam\""));
        assert!(toml_str.contains("host = \"192.168.1.100\""));
        assert!(toml_str.contains("[[cameras]]"));
    }

    #[test]
    fn test_app_config_default() {
        let config = AppConfig::default();
        assert!(config.is_empty());
        assert_eq!(config.len(), 0);
    }
}
