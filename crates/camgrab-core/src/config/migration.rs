//! Configuration migration module.
//!
//! This module provides functionality to migrate configuration files from older
//! versions to the current version. The migration system is designed to be
//! extensible and support multiple configuration versions.
//!
//! # Migration Process
//!
//! When loading a configuration file, the system:
//! 1. Parses the raw YAML/TOML into a `serde_yaml::Value`
//! 2. Detects the configuration version
//! 3. Applies necessary migrations in sequence
//! 4. Deserializes the migrated value into the current `AppConfig` structure
//!
//! # Example
//!
//! ```no_run
//! use camgrab_core::config::migration::migrate;
//! use serde_yaml::Value;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let raw_config: Value = serde_yaml::from_str(r#"
//!   version: v1
//!   cameras:
//!     - name: Camera 1
//!       host: 192.168.1.100
//! "#)?;
//!
//! let migrated = migrate(raw_config)?;
//! # Ok(())
//! # }
//! ```

use crate::config::{ConfigError, ConfigVersion};
use serde_yaml::Value;

/// Migrate configuration from any version to the current version.
///
/// This function detects the configuration version and applies all necessary
/// migrations in sequence to bring it up to the current version.
///
/// # Arguments
///
/// * `value` - Raw configuration as a `serde_yaml::Value`
///
/// # Returns
///
/// Returns the migrated configuration value, or an error if migration fails.
///
/// # Errors
///
/// Returns `ConfigError::Migration` if:
/// - The version field is missing or invalid
/// - An unsupported version is encountered
/// - Migration logic fails
pub fn migrate(mut value: Value) -> Result<Value, ConfigError> {
    // Detect current version
    let version = detect_version(&value)?;

    match version {
        ConfigVersion::V1 => {
            // V1 is the current version, no migration needed
            Ok(value)
        } // Future versions would be handled here:
          // ConfigVersion::V2 => {
          //     value = migrate_v1_to_v2(value)?;
          //     Ok(value)
          // }
    }
}

/// Detect the configuration version from a raw value.
///
/// If the version field is missing, defaults to V1 for backward compatibility.
fn detect_version(value: &Value) -> Result<ConfigVersion, ConfigError> {
    match value.get("version") {
        Some(Value::String(v)) => match v.as_str() {
            "v1" => Ok(ConfigVersion::V1),
            other => Err(ConfigError::Migration(format!(
                "Unsupported version: {}",
                other
            ))),
        },
        None => {
            // Default to V1 if version is not specified (backward compatibility)
            Ok(ConfigVersion::V1)
        }
        Some(_) => Err(ConfigError::Migration(
            "Version field must be a string".to_string(),
        )),
    }
}

// Future migration functions would be implemented here:
//
// /// Migrate from V1 to V2.
// fn migrate_v1_to_v2(mut value: Value) -> Result<Value, ConfigError> {
//     // Example: Add a new field with a default value
//     if let Some(mapping) = value.as_mapping_mut() {
//         // Add new fields or transform existing ones
//         mapping.insert(
//             Value::String("new_field".to_string()),
//             Value::String("default_value".to_string()),
//         );
//
//         // Update version
//         mapping.insert(
//             Value::String("version".to_string()),
//             Value::String("v2".to_string()),
//         );
//     }
//     Ok(value)
// }

#[cfg(test)]
mod tests {
    use super::*;
    use serde_yaml::Value;

    #[test]
    fn test_detect_version_v1() {
        let yaml = r#"
version: v1
cameras: []
"#;
        let value: Value = serde_yaml::from_str(yaml).unwrap();
        let version = detect_version(&value).unwrap();
        assert_eq!(version, ConfigVersion::V1);
    }

    #[test]
    fn test_detect_version_missing() {
        let yaml = r#"
cameras: []
"#;
        let value: Value = serde_yaml::from_str(yaml).unwrap();
        let version = detect_version(&value).unwrap();
        assert_eq!(version, ConfigVersion::V1); // Should default to V1
    }

    #[test]
    fn test_detect_version_invalid() {
        let yaml = r#"
version: v99
cameras: []
"#;
        let value: Value = serde_yaml::from_str(yaml).unwrap();
        assert!(detect_version(&value).is_err());
    }

    #[test]
    fn test_detect_version_invalid_type() {
        let yaml = r#"
version: 123
cameras: []
"#;
        let value: Value = serde_yaml::from_str(yaml).unwrap();
        assert!(detect_version(&value).is_err());
    }

    #[test]
    fn test_migrate_v1() {
        let yaml = r#"
version: v1
cameras:
  - name: Test Camera
    host: 192.168.1.100
    port: 554
    username: admin
    password: secret
"#;
        let value: Value = serde_yaml::from_str(yaml).unwrap();
        let migrated = migrate(value.clone()).unwrap();
        assert_eq!(value, migrated); // V1 should remain unchanged
    }

    #[test]
    fn test_migrate_no_version() {
        let yaml = r#"
cameras:
  - name: Test Camera
    host: 192.168.1.100
    port: 554
    username: admin
"#;
        let value: Value = serde_yaml::from_str(yaml).unwrap();
        let migrated = migrate(value.clone()).unwrap();

        // Should migrate successfully and keep the data
        assert_eq!(value, migrated);
    }

    #[test]
    fn test_migrate_unsupported_version() {
        let yaml = r#"
version: v999
cameras: []
"#;
        let value: Value = serde_yaml::from_str(yaml).unwrap();
        assert!(migrate(value).is_err());
    }

    #[test]
    fn test_migrate_complex_config() {
        let yaml = r#"
version: v1
cameras:
  - name: Front Door
    host: 192.168.1.100
    port: 554
    username: admin
    password: secret123
    protocol: rtsp
    transport: tcp
    stream: main
    audio: true
    timeout_seconds: 10
    connect_timeout_seconds: 5
    ptz_enabled: true
    motion_detection_zones:
      - name: entrance
        x: 0.2
        y: 0.3
        width: 0.5
        height: 0.4
        sensitivity: 0.7
notifications:
  webhook:
    url: https://example.com/webhook
  mqtt:
    broker: mqtt.example.com
    port: 1883
    topic: camgrab/events
storage:
  local_path: /var/lib/camgrab
  s3:
    bucket: my-bucket
    region: us-east-1
    prefix: cameras/
"#;
        let value: Value = serde_yaml::from_str(yaml).unwrap();
        let migrated = migrate(value.clone()).unwrap();
        assert_eq!(value, migrated); // V1 should remain unchanged
    }

    #[test]
    fn test_migrate_preserves_structure() {
        let yaml = r#"
version: v1
cameras:
  - name: Camera 1
    host: 192.168.1.100
    port: 554
    username: admin
notifications:
  email:
    smtp_server: smtp.example.com
    smtp_port: 587
    from: camgrab@example.com
    to:
      - admin@example.com
      - security@example.com
    use_tls: true
"#;
        let value: Value = serde_yaml::from_str(yaml).unwrap();
        let migrated = migrate(value.clone()).unwrap();

        // Verify structure is preserved
        assert_eq!(
            migrated.get("cameras").unwrap().as_sequence().unwrap().len(),
            1
        );
        assert_eq!(
            migrated
                .get("notifications")
                .unwrap()
                .get("email")
                .unwrap()
                .get("to")
                .unwrap()
                .as_sequence()
                .unwrap()
                .len(),
            2
        );
    }
}
