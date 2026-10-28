//! Saved GUI settings and their existing on-disk representation.

use crate::compatibility::SETTINGS_SCHEMA_VERSION;
use iv_core::RunConfig;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// A profile may not replace the binding of any connected instrument,
/// including a fixed-bias source that is outside the IV sweep roles.
pub(super) fn profile_load_allowed(state: &crate::protocol::Snapshot) -> bool {
    !state.source_connected
        && !state.source2_connected
        && !state.meter_connected
        && !state.gate_connected
}

/// A saved instrument connection.  The desktop engine still has one source,
/// one optional second source and one meter role; the inventory lets users
/// keep several prepared connections and bind those roles in Settings.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(super) struct InstrumentEntry {
    /// Stable within a configuration file.  IDs deliberately do not depend on
    /// the device model or resource, which users are expected to edit.
    pub(super) id: u64,
    pub(super) model: String,
    pub(super) resource: String,
    pub(super) dc: DcSettings,
}

/// Saved setpoints only: loading settings never connects or enables output.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(super) struct DcSettings {
    pub(super) mode: iv_core::Mode,
    pub(super) value: f64,
    pub(super) compliance: f64,
    pub(super) ramp_step: f64,
    pub(super) units: [usize; 3],
}
impl Default for DcSettings {
    fn default() -> Self {
        Self {
            mode: iv_core::Mode::Iv,
            value: 0.0,
            compliance: 0.001,
            ramp_step: 0.001,
            units: [1, 1, 1],
        }
    }
}

impl InstrumentEntry {
    fn source(id: u64, model: impl Into<String>, resource: impl Into<String>) -> Self {
        Self {
            id,
            model: model.into(),
            resource: resource.into(),
            dc: DcSettings::default(),
        }
    }

    fn meter(id: u64, model: impl Into<String>, resource: impl Into<String>) -> Self {
        Self::source(id, model, resource)
    }
}

impl Default for InstrumentEntry {
    fn default() -> Self {
        Self::source(0, "YOKOGAWA_GS210", String::new())
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub(super) struct Settings {
    /// This stays optional on read so pre-versioned config files remain valid.
    pub(super) schema_version: u32,
    pub(super) config: RunConfig,
    pub(super) source: String,
    pub(super) meter: String,
    pub(super) source2: String,
    pub(super) source2_model: String,
    pub(super) source2_compliance_unit: usize,
    /// Saved source and meter choices.  The original scalar fields above are
    /// retained as the active-role binding for backwards-compatible config
    /// files and the existing device engine protocol.
    #[serde(default)]
    pub(super) sources: Vec<InstrumentEntry>,
    #[serde(default)]
    pub(super) meters: Vec<InstrumentEntry>,
    #[serde(default)]
    pub(super) source1_id: u64,
    #[serde(default)]
    pub(super) source2_id: u64,
    #[serde(default)]
    pub(super) meter_id: u64,
    /// Inventory card associated with the independent voltage-bias connection.
    pub(super) dc_gate_id: u64,
    #[serde(default = "default_inventory_id")]
    pub(super) next_inventory_id: u64,
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
            source2: String::new(),
            source2_model: "YOKOGAWA_GS210".into(),
            source2_compliance_unit: 1,
            sources: vec![
                InstrumentEntry::source(1, "YOKOGAWA_GS210", String::new()),
                InstrumentEntry::source(2, "YOKOGAWA_GS210", String::new()),
            ],
            meters: vec![InstrumentEntry::meter(3, "AGILENT_34411A", String::new())],
            source1_id: 1,
            source2_id: 2,
            meter_id: 3,
            dc_gate_id: 0,
            next_inventory_id: 4,
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
    /// Populate the inventory from legacy scalar settings and make the active
    /// entries available through those scalar bindings.  This runs after every
    /// JSON load and at app startup, so old settings files remain valid.
    pub(super) fn migrate_inventory(&mut self) {
        if self.sources.is_empty() {
            self.sources.push(InstrumentEntry::source(
                1,
                self.source_model.clone(),
                self.source.clone(),
            ));
            self.sources.push(InstrumentEntry::source(
                2,
                self.source2_model.clone(),
                self.source2.clone(),
            ));
            self.source1_id = 1;
            self.source2_id = 2;
        }
        if self.meters.is_empty() {
            self.meters.push(InstrumentEntry::meter(
                3,
                self.meter_model.clone(),
                self.meter.clone(),
            ));
            self.meter_id = 3;
        }

        let mut next = self.next_inventory_id.max(1);
        let mut used = HashSet::new();
        for entry in self.sources.iter_mut().chain(self.meters.iter_mut()) {
            if entry.id == 0 || !used.insert(entry.id) {
                while used.contains(&next) {
                    next += 1;
                }
                entry.id = next;
                used.insert(entry.id);
                next += 1;
            }
            next = next.max(entry.id.saturating_add(1));
        }
        self.next_inventory_id = next;
        if !self.sources.iter().any(|entry| entry.id == self.source1_id) {
            self.source1_id = self.sources[0].id;
        }
        if !self.sources.iter().any(|entry| entry.id == self.source2_id)
            || self.source2_id == self.source1_id
        {
            self.source2_id = self
                .sources
                .iter()
                .find(|entry| entry.id != self.source1_id)
                .map(|entry| entry.id)
                .unwrap_or(self.source1_id);
        }
        if !self.meters.iter().any(|entry| entry.id == self.meter_id) {
            self.meter_id = self.meters[0].id;
        }
        self.bind_active_entries();
    }

    /// Copy the active inventory entries into the legacy fields used by the
    /// current engine commands.  Call only after role selections change.
    pub(super) fn bind_active_entries(&mut self) {
        if let Some(entry) = self
            .sources
            .iter()
            .find(|entry| entry.id == self.source1_id)
        {
            self.source_model.clone_from(&entry.model);
            self.source.clone_from(&entry.resource);
        }
        if let Some(entry) = self
            .sources
            .iter()
            .find(|entry| entry.id == self.source2_id)
        {
            self.source2_model.clone_from(&entry.model);
            self.source2.clone_from(&entry.resource);
        }
        if let Some(entry) = self.meters.iter().find(|entry| entry.id == self.meter_id) {
            self.meter_model.clone_from(&entry.model);
            self.meter.clone_from(&entry.resource);
        }
    }

    /// Persist edits made through the existing active-device cards back to
    /// their selected inventory entries.
    pub(super) fn store_active_entries(&mut self) {
        if let Some(entry) = self
            .sources
            .iter_mut()
            .find(|entry| entry.id == self.source1_id)
        {
            entry.model.clone_from(&self.source_model);
            entry.resource.clone_from(&self.source);
        }
        if self.config.balanced_source.is_some()
            && let Some(entry) = self
                .sources
                .iter_mut()
                .find(|entry| entry.id == self.source2_id)
        {
            entry.model.clone_from(&self.source2_model);
            entry.resource.clone_from(&self.source2);
        }
        if let Some(entry) = self
            .meters
            .iter_mut()
            .find(|entry| entry.id == self.meter_id)
        {
            entry.model.clone_from(&self.meter_model);
            entry.resource.clone_from(&self.meter);
        }
    }

    pub(super) fn add_source(&mut self) {
        let id = self.allocate_inventory_id();
        self.sources
            .push(InstrumentEntry::source(id, "YOKOGAWA_GS210", String::new()));
    }

    pub(super) fn add_meter(&mut self) {
        let id = self.allocate_inventory_id();
        self.meters
            .push(InstrumentEntry::meter(id, "AGILENT_34411A", String::new()));
    }

    pub(super) fn remove_source(&mut self, id: u64) -> bool {
        if id == self.source1_id || id == self.source2_id || self.sources.len() <= 2 {
            return false;
        }
        self.sources.retain(|entry| entry.id != id);
        true
    }

    pub(super) fn remove_meter(&mut self, id: u64) -> bool {
        if id == self.meter_id || self.meters.len() <= 1 {
            return false;
        }
        self.meters.retain(|entry| entry.id != id);
        true
    }

    fn allocate_inventory_id(&mut self) -> u64 {
        let id = self.next_inventory_id.max(1);
        self.next_inventory_id = id.saturating_add(1);
        id
    }

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
    settings.migrate_inventory();
    settings.schema_version = SETTINGS_SCHEMA_VERSION;
    Ok(settings)
}

const fn default_inventory_id() -> u64 {
    1
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
        let original = br#"{"schema_version":4,"filename":"future"}"#;
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
        let error = load_settings(br#"{"schema_version":4}"#).err().unwrap();
        assert!(error.contains("schema version 4"));
        assert!(error.contains("newer KohdaLab IV"));
    }

    #[test]
    fn malformed_marker_is_rejected_before_deserialization() {
        let error = load_settings(br#"{"schema_version":"one"}"#).err().unwrap();
        assert!(error.contains("non-negative integer"));
    }

    #[test]
    fn legacy_scalar_devices_migrate_to_two_sources_and_one_meter() {
        let settings = load_settings(
            br#"{
                "source_model":"YOKOGAWA_7651",
                "source":"GPIB0::3::INSTR",
                "source2_model":"YOKOGAWA_GS210",
                "source2":"GPIB0::4::INSTR",
                "meter_model":"AGILENT_34411A",
                "meter":"GPIB0::7::INSTR"
            }"#,
        )
        .unwrap();
        assert_eq!(settings.sources.len(), 2);
        assert_eq!(settings.meters.len(), 1);
        assert_ne!(settings.source1_id, settings.source2_id);
        assert_eq!(settings.sources[0].model, "YOKOGAWA_7651");
        assert_eq!(settings.sources[1].resource, "GPIB0::4::INSTR");
        assert_eq!(settings.meters[0].model, "AGILENT_34411A");
    }

    #[test]
    fn editing_inactive_second_source_survives_active_card_storage() {
        let mut settings = Settings::default();
        settings.migrate_inventory();
        let second = settings.source2_id;
        settings
            .sources
            .iter_mut()
            .find(|entry| entry.id == second)
            .unwrap()
            .resource = "GPIB0::9::INSTR".into();
        settings.store_active_entries();
        assert_eq!(
            settings
                .sources
                .iter()
                .find(|entry| entry.id == second)
                .unwrap()
                .resource,
            "GPIB0::9::INSTR"
        );
        settings.config.balanced_source = Some(iv_core::BalancedSourceConfig::default());
        settings.bind_active_entries();
        assert_eq!(settings.source2, "GPIB0::9::INSTR");
    }

    #[test]
    fn active_inventory_binding_preserves_inactive_devices() {
        let mut settings = Settings::default();
        settings.add_source();
        let inactive = settings.sources[2].clone();
        settings.source_model = "YOKOGAWA_7651".into();
        settings.source = "GPIB0::3::INSTR".into();
        settings.store_active_entries();
        settings.bind_active_entries();
        assert_eq!(settings.source_model, "YOKOGAWA_7651");
        assert_eq!(settings.sources[2], inactive);
        assert!(settings.remove_source(settings.sources[2].id));
        assert_eq!(settings.sources.len(), 2);
    }
}

#[cfg(test)]
mod dc_persistence_tests {
    use super::*;
    #[test]
    fn a_connected_bias_always_blocks_profile_replacement() {
        for output in [
            crate::state::OutputState::On,
            crate::state::OutputState::Off,
            crate::state::OutputState::Unknown,
        ] {
            let state = crate::protocol::Snapshot {
                gate_connected: true,
                gate_output: output,
                ..Default::default()
            };
            assert!(!profile_load_allowed(&state));
        }
        assert!(profile_load_allowed(&crate::protocol::Snapshot::default()));
    }
    #[test]
    fn old_device_entries_get_off_by_default_dc_settings_and_saved_values_roundtrip() {
        let old = br#"{"sources":[{"id":1,"model":"YOKOGAWA_GS210","resource":"GPIB0::3::INSTR"},{"id":2,"model":"YOKOGAWA_7651","resource":"GPIB0::5::INSTR"}]}"#;
        let mut settings = load_settings(old).unwrap();
        assert_eq!(settings.sources[0].dc, DcSettings::default());
        settings.sources[1].dc.value = -0.25;
        settings.dc_gate_id = settings.sources[1].id;
        settings.store_active_entries();
        let json = serde_json::to_value(&settings).unwrap();
        assert!(json["sources"][1]["dc"].get("enabled").is_none());
        assert!(json["sources"][1]["dc"].get("output").is_none());
        let loaded = load_settings(&serde_json::to_vec(&settings).unwrap()).unwrap();
        assert_eq!(loaded.sources[1].dc.value, -0.25);
        assert_eq!(loaded.dc_gate_id, settings.sources[1].id);
    }
}
