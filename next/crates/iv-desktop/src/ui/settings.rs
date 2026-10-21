//! Saved GUI settings and their existing on-disk representation.

use crate::compatibility::SETTINGS_SCHEMA_VERSION;
use iv_core::RunConfig;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub(super) struct Settings {
    /// This stays optional on read so pre-versioned config files remain valid.
    pub(super) schema_version: u32,
    pub(super) config: RunConfig,
    pub(super) source: String,
    pub(super) meter: String,
    pub(super) output_dir: String,
    pub(super) filename: String,
    pub(super) simulation: bool,
    pub(super) source_model: String,
    pub(super) meter_model: String,
    pub(super) auto_suffix: bool,
    pub(super) units: [usize; 3],
    pub(super) current_compliance: f64,
    pub(super) voltage_compliance: f64,
    pub(super) compliance_units: [usize; 2],
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            schema_version: SETTINGS_SCHEMA_VERSION,
            config: RunConfig::default(),
            source: String::new(),
            meter: String::new(),
            output_dir: user_dir().join("results").display().to_string(),
            filename: "iv_run".into(),
            simulation: false,
            source_model: "YOKOGAWA_GS210".into(),
            meter_model: "AGILENT_34411A".into(),
            auto_suffix: true,
            units: [1; 3],
            current_compliance: 0.001,
            voltage_compliance: 1.0,
            compliance_units: [1, 0],
        }
    }
}

fn user_dir() -> PathBuf {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join(".kohdalab")
}

pub(super) fn settings_path() -> PathBuf {
    user_dir().join("config").join("iv-next.json")
}

pub(super) fn save_settings(settings: &Settings, path: &Path) -> Result<(), String> {
    // Never replace a declared future schema with fallback defaults on auto-save.
    if let Ok(bytes) = std::fs::read(path)
        && let Ok(value) = serde_json::from_slice::<serde_json::Value>(&bytes)
        && value.get("schema_version").is_some()
    {
        load_settings(&bytes)?;
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let mut current = settings.clone();
    current.schema_version = SETTINGS_SCHEMA_VERSION;
    let bytes = serde_json::to_vec_pretty(&current).map_err(|e| e.to_string())?;
    std::fs::write(path, bytes).map_err(|e| e.to_string())
}

/// Load old and current settings, while rejecting settings written by a newer
/// application before any values can be lost by a later save.
pub(super) fn load_settings(bytes: &[u8]) -> Result<Settings, String> {
    let value: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|error| format!("Invalid settings JSON: {error}"))?;
    let object = value
        .as_object()
        .ok_or_else(|| "Invalid settings JSON: expected an object.".to_string())?;
    let version = match object.get("schema_version") {
        None => 0,
        Some(serde_json::Value::Number(number)) => {
            u32::try_from(number.as_u64().ok_or_else(|| {
                "Invalid settings schema_version: expected a non-negative integer.".to_string()
            })?)
            .map_err(|_| {
                "Invalid settings schema_version: expected a non-negative integer.".to_string()
            })?
        }
        Some(_) => {
            return Err("Invalid settings schema_version: expected a non-negative integer.".into());
        }
    };
    if version > SETTINGS_SCHEMA_VERSION {
        return Err(format!(
            "This settings file uses schema version {version}, but this KohdaLab IV version supports up to {SETTINGS_SCHEMA_VERSION}. Open it with a newer KohdaLab IV release."
        ));
    }
    let mut settings: Settings = serde_json::from_value(value)
        .map_err(|error| format!("Invalid settings values: {error}"))?;
    // v0 had the same field representation.  Upgrade only the bookkeeping
    // marker; defaults and unknown-field tolerance are left untouched.
    settings.schema_version = SETTINGS_SCHEMA_VERSION;
    Ok(settings)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn autosave_cannot_overwrite_a_future_versioned_file() {
        let path =
            std::env::temp_dir().join(format!("iv-future-settings-{}.json", std::process::id()));
        let original = br#"{"schema_version":2,"filename":"future"}"#;
        std::fs::write(&path, original).unwrap();
        assert!(save_settings(&Settings::default(), &path).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), original);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn earlier_preview_config_keeps_safe_defaults() {
        let settings = load_settings(br#"{"filename":"experiment","simulation":true}"#).unwrap();
        assert!(settings.auto_suffix);
        assert_eq!(settings.units, [1; 3]);
        assert_eq!(settings.config.compliance, 0.001);
        assert_eq!(settings.schema_version, SETTINGS_SCHEMA_VERSION);
    }

    #[test]
    fn explicit_legacy_marker_is_migrated_without_changing_values() {
        let settings = load_settings(
            br#"{"schema_version":0,"filename":"legacy","unknown_future_field":true}"#,
        )
        .unwrap();
        assert_eq!(settings.filename, "legacy");
        assert_eq!(settings.schema_version, SETTINGS_SCHEMA_VERSION);
    }

    #[test]
    fn current_marker_is_accepted_and_future_marker_is_explained() {
        assert_eq!(
            load_settings(br#"{"schema_version":1,"filename":"current"}"#)
                .unwrap()
                .filename,
            "current"
        );
        let error = load_settings(br#"{"schema_version":2}"#).err().unwrap();
        assert!(error.contains("schema version 2"));
        assert!(error.contains("newer KohdaLab IV"));
    }

    #[test]
    fn malformed_marker_is_rejected_before_deserialization() {
        let error = load_settings(br#"{"schema_version":"one"}"#).err().unwrap();
        assert!(error.contains("non-negative integer"));
    }
}
