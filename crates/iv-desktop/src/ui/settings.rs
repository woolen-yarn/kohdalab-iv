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
    pub(super) range: Option<f64>,
    pub(super) units: [usize; 3],
}
impl DcSettings {
    /// A unit-dimension change resets only the OFF editor, never hardware.
    pub(super) fn reset_function(&mut self, mode: iv_core::Mode) {
        self.mode = mode;
        self.value = 0.0;
        self.range = Some(if mode == iv_core::Mode::Iv {
            1.0
        } else {
            0.001
        });
        self.compliance = if mode == iv_core::Mode::Iv {
            0.001
        } else {
            1.0
        };
        self.ramp_step = if mode == iv_core::Mode::Iv {
            0.001
        } else {
            1e-6
        };
        self.units = if mode == iv_core::Mode::Iv {
            [0, 1, 1]
        } else {
            [1, 0, 2]
        };
    }
}

impl Default for DcSettings {
    fn default() -> Self {
        Self {
            mode: iv_core::Mode::Iv,
            value: 0.0,
            compliance: 0.001,
            ramp_step: 0.001,
            range: Some(1.0),
            units: [0, 1, 1],
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
    pub(super) application: String,
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
            application: APPLICATION_ID.into(),
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
            output_dir: user_dir().join("iv").join("results").display().to_string(),
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

pub(super) const APPLICATION_ID: &str = "kohdalab-iv";

pub(super) fn settings_path() -> PathBuf {
    settings_path_in(&user_dir())
}

fn settings_path_in(root: &Path) -> PathBuf {
    root.join("iv").join("config.json")
}

/// Read only the application-owned default. Old shared profiles are not
/// discovered or migrated automatically.
pub(super) fn load_startup_settings() -> Result<(Settings, String), String> {
    load_startup_settings_in(&user_dir())
}

fn load_startup_settings_in(root: &Path) -> Result<(Settings, String), String> {
    let destination = settings_path_in(root);
    match std::fs::read(&destination) {
        Ok(bytes) => load_settings(&bytes).map(|settings| (settings, String::new())),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            Ok((Settings::default(), String::new()))
        }
        Err(error) => Err(format!("Read {}: {error}", destination.display())),
    }
}

pub(super) fn save_settings(settings: &Settings, path: &Path) -> Result<(), String> {
    // Reject foreign files and newer schemas before replacing an existing file.
    match std::fs::read(path) {
        Ok(bytes) => {
            load_settings(&bytes)?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(format!("Read settings before Save: {error}")),
    }
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    let mut current = settings.clone();
    current.application = APPLICATION_ID.into();
    current.schema_version = SETTINGS_SCHEMA_VERSION;
    let bytes = serde_json::to_vec_pretty(&current).map_err(|error| error.to_string())?;
    let mut temporary =
        tempfile::NamedTempFile::new_in(parent).map_err(|error| error.to_string())?;
    use std::io::Write;
    temporary
        .write_all(&bytes)
        .map_err(|error| error.to_string())?;
    temporary
        .as_file()
        .sync_all()
        .map_err(|error| error.to_string())?;
    temporary
        .persist(path)
        .map_err(|error| error.error.to_string())?;
    Ok(())
}

/// Load old and current settings, while rejecting settings written by a newer
/// application before any values can be lost by a later save.
pub(super) fn load_settings(bytes: &[u8]) -> Result<Settings, String> {
    let value: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|error| format!("Invalid settings JSON: {error}"))?;
    let object = value
        .as_object()
        .ok_or_else(|| "Invalid settings JSON: expected an object.".to_string())?;
    if let Some(application) = object.get("application")
        && application.as_str() != Some(APPLICATION_ID)
    {
        return Err("This settings file belongs to another application. Choose a KohdaLab IV configuration.".into());
    }
    if object.contains_key("instruments")
        || object.contains_key("measurements")
        || object.contains_key("coordinates")
    {
        return Err("This is another application's configuration or a legacy Python profile. Native KohdaLab IV settings are required.".into());
    }
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
    let recognizable_iv_fields = [
        "config",
        "source",
        "meter",
        "sources",
        "meters",
        "source_model",
        "meter_model",
        "source2",
        "source2_model",
        "filename",
        "simulation",
    ];
    if !object.contains_key("application")
        && !recognizable_iv_fields
            .iter()
            .any(|key| object.contains_key(*key))
    {
        return Err("This JSON file has no KohdaLab IV settings or application identity. Choose an IV configuration file.".into());
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
    fn unidentified_json_is_not_loaded_as_defaults_or_replaced_on_save() {
        let root = tempfile::tempdir().unwrap();
        let destination = settings_path_in(root.path());
        std::fs::create_dir_all(destination.parent().unwrap()).unwrap();
        for bytes in [br#"{}"#.as_slice(), br#"{"foo":"bar"}"#] {
            std::fs::write(&destination, bytes).unwrap();
            assert!(load_startup_settings_in(root.path()).is_err());
            assert!(save_settings(&Settings::default(), &destination).is_err());
            assert_eq!(std::fs::read(&destination).unwrap(), bytes);
        }
        assert!(load_settings(br#"{"application":"kohdalab-iv"}"#).is_ok());
    }

    #[test]
    fn application_marker_separates_iv_from_foreign_and_legacy_profiles() {
        let saved = serde_json::to_value(Settings::default()).unwrap();
        assert_eq!(saved["application"], APPLICATION_ID);
        assert!(load_settings(br#"{"application":"kohdalab-trkr","schema_version":3}"#).is_err());
        assert!(load_settings(br#"{"application":12}"#).is_err());
        assert!(
            load_settings(br#"{"instruments":{},"measurements":{},"coordinates":{}}"#).is_err()
        );
        assert!(
            load_settings(br#"{"config_version":1,"instruments":{},"measurements":{},"roles":{}}"#)
                .is_err()
        );
        let legacy = load_settings(br#"{"filename":"native-legacy","simulation":true}"#).unwrap();
        assert_eq!(legacy.application, APPLICATION_ID);
        assert_eq!(legacy.filename, "native-legacy");
    }

    #[test]
    fn default_is_app_owned_and_shared_legacy_files_are_ignored() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join("config")).unwrap();
        for name in ["iv-next.json", "iv.json", "trkr-next.json", "default.json"] {
            let old = root.path().join("config").join(name);
            let bytes = br#"{"filename":"old-shared-profile"}"#;
            std::fs::write(&old, bytes).unwrap();
            let (settings, notice) = load_startup_settings_in(root.path()).unwrap();
            assert_ne!(settings.filename, "old-shared-profile");
            assert!(notice.is_empty());
            assert!(!settings_path_in(root.path()).exists());
            assert_eq!(std::fs::read(old).unwrap(), bytes);
        }
        assert_eq!(
            settings_path_in(root.path()),
            root.path().join("iv/config.json")
        );
    }

    #[test]
    fn new_iv_settings_win_and_never_fall_back_after_parse_or_version_errors() {
        let root = tempfile::tempdir().unwrap();
        let old = root.path().join("config/iv-next.json");
        std::fs::create_dir_all(old.parent().unwrap()).unwrap();
        std::fs::write(old, br#"{"filename":"old"}"#).unwrap();
        let new = settings_path_in(root.path());
        let settings = Settings {
            filename: "new".into(),
            ..Default::default()
        };
        save_settings(&settings, &new).unwrap();
        assert_eq!(
            load_startup_settings_in(root.path()).unwrap().0.filename,
            "new"
        );
        for bytes in [
            b"broken".as_slice(),
            br#"{"schema_version":4}"#,
            br#"{"application":"kohdalab-trkr"}"#,
        ] {
            std::fs::write(&new, bytes).unwrap();
            assert!(load_startup_settings_in(root.path()).is_err());
            assert_eq!(std::fs::read(&new).unwrap(), bytes);
        }
    }

    #[test]
    fn save_refuses_foreign_unmarked_and_corrupt_files_without_changing_bytes() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("other.json");
        for bytes in [
            br#"{"application":"kohdalab-trkr"}"#.as_slice(),
            br#"{"instruments":{},"measurements":{}}"#,
            b"broken",
        ] {
            std::fs::write(&path, bytes).unwrap();
            assert!(save_settings(&Settings::default(), &path).is_err());
            assert_eq!(std::fs::read(&path).unwrap(), bytes);
        }
        std::fs::remove_file(&path).unwrap();
        save_settings(&Settings::default(), &path).unwrap();
        let value: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(value["application"], APPLICATION_ID);
        let changed = Settings {
            filename: "updated".into(),
            ..Default::default()
        };
        save_settings(&changed, &path).unwrap();
        assert_eq!(
            load_settings(&std::fs::read(path).unwrap())
                .unwrap()
                .filename,
            "updated"
        );
    }

    #[test]
    fn dc_function_changes_reset_editor_dimensions_and_never_enable_output() {
        let mut dc = DcSettings {
            value: 2.0,
            ..Default::default()
        };
        dc.reset_function(iv_core::Mode::Vi);
        assert_eq!(dc.mode, iv_core::Mode::Vi);
        assert_eq!(dc.value, 0.0);
        assert_eq!(dc.compliance, 1.0);
        assert_eq!(dc.units, [1, 0, 2]);
        dc.reset_function(iv_core::Mode::Iv);
        assert_eq!(dc.value, 0.0);
        assert_eq!(dc.compliance, 0.001);
        assert_eq!(dc.units, [0, 1, 1]);
    }

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
