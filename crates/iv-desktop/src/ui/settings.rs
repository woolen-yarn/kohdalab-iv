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
    /// Requested source-side hardware protection.  `None` keeps the legacy
    /// core behaviour, where the source protection follows `compliance`.
    #[serde(default)]
    pub(super) source_current_protection: Option<f64>,
    #[serde(default)]
    pub(super) source_voltage_protection: Option<f64>,
    pub(super) source_protection_units: [usize; 2],
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
            source_current_protection: None,
            source_voltage_protection: None,
            source_protection_units: [1, 0],
        }
    }
}

impl Settings {
    /// Store the values belonging to the mode currently held by `config`.
    /// This is called before switching modes or opening the Settings dialog.
    pub(super) fn store_active_protection(&mut self) {
        match self.config.mode {
            iv_core::Mode::Iv => {
                self.current_compliance = self.config.compliance;
                self.source_current_protection = self.config.source_compliance;
            }
            iv_core::Mode::Vi => {
                self.voltage_compliance = self.config.compliance;
                self.source_voltage_protection = self.config.source_compliance;
            }
        }
    }

    /// Restore the values cached for the selected mode into the run config.
    pub(super) fn activate_mode_protection(&mut self) {
        match self.config.mode {
            iv_core::Mode::Iv => {
                self.config.compliance = self.current_compliance;
                self.config.source_compliance = self.source_current_protection;
            }
            iv_core::Mode::Vi => {
                self.config.compliance = self.voltage_compliance;
                self.config.source_compliance = self.source_voltage_protection;
            }
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
    // Preserve legacy coupling (None), including its range and resolution.
    // Merely loading/opening settings must not opt into a different range.
    settings.store_active_protection();
    settings.activate_mode_protection();
    settings.schema_version = SETTINGS_SCHEMA_VERSION;
    Ok(settings)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn active_explicit_source_protection_cannot_be_overwritten_by_stale_gui_cache() {
        let mut original = Settings::default();
        original.config.mode = iv_core::Mode::Vi;
        original.config.compliance = 0.2;
        original.config.source_compliance = Some(5.0);
        original.source_voltage_protection = Some(20.0);
        let loaded = load_settings(&serde_json::to_vec(&original).unwrap()).unwrap();
        assert_eq!(loaded.config.source_compliance, Some(5.0));
        assert_eq!(loaded.source_voltage_protection, Some(5.0));
        assert_eq!(loaded.config.compliance, 0.2);
    }

    #[test]
    fn autosave_cannot_overwrite_a_future_versioned_file() {
        let path =
            std::env::temp_dir().join(format!("iv-future-settings-{}.json", std::process::id()));
        let original = br#"{"schema_version":3,"filename":"future"}"#;
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
    fn legacy_protection_preserves_range_and_resolution_until_explicitly_edited() {
        let legacy = Settings {
            schema_version: 1,
            config: iv_core::RunConfig {
                start: -0.001,
                stop: 0.001,
                step: 0.0001,
                ..Default::default()
            },
            ..Default::default()
        };
        let before = iv_core::plan(&legacy.config).unwrap();
        let settings = load_settings(&serde_json::to_vec(&legacy).unwrap()).unwrap();
        assert_eq!(settings.config.source_compliance, None);
        assert_eq!(settings.source_current_protection, None);
        let after = iv_core::plan(&settings.config).unwrap();
        assert_eq!(before.source_range, after.source_range);
        assert_eq!(before.points, after.points);
        assert_eq!(before.hardware_compliance, after.hardware_compliance);
        assert_eq!(
            iv_core::prepare_config(&legacy.config).unwrap().step,
            iv_core::prepare_config(&settings.config).unwrap().step
        );
    }

    #[test]
    fn mode_switch_restores_independent_source_and_meter_limits() {
        let mut settings = Settings {
            current_compliance: 0.002,
            voltage_compliance: 1.5,
            source_current_protection: Some(0.003),
            source_voltage_protection: Some(2.0),
            ..Default::default()
        };

        settings.config.mode = iv_core::Mode::Iv;
        settings.activate_mode_protection();
        assert_eq!(settings.config.compliance, 0.002);
        assert_eq!(settings.config.source_compliance, Some(0.003));

        settings.config.mode = iv_core::Mode::Vi;
        settings.activate_mode_protection();
        assert_eq!(settings.config.compliance, 1.5);
        assert_eq!(settings.config.source_compliance, Some(2.0));
    }

    #[test]
    fn current_marker_is_accepted_and_future_marker_is_explained() {
        assert_eq!(
            load_settings(br#"{"schema_version":2,"filename":"current"}"#)
                .unwrap()
                .filename,
            "current"
        );
        let error = load_settings(br#"{"schema_version":3}"#).err().unwrap();
        assert!(error.contains("schema version 3"));
        assert!(error.contains("newer KohdaLab IV"));
    }

    #[test]
    fn malformed_marker_is_rejected_before_deserialization() {
        let error = load_settings(br#"{"schema_version":"one"}"#).err().unwrap();
        assert!(error.contains("non-negative integer"));
    }
}
