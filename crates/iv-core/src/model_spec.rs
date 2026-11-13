//! Typed model specification tables shared by the catalog, display policy and planner.
//!
//! `specs/sources.toml` and `specs/meters.toml` are parsed exactly once each into
//! serde-typed tables held in a [`OnceLock`]. Every consumer reads these tables instead
//! of re-parsing the TOML or walking untyped values. Source range command values are
//! classified by the registered dialect: Yokogawa 7651 ranges carry legacy integer
//! range *codes* (R2..R6), every other model carries SI range values.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use serde::Deserialize;

use crate::catalog::{InstrumentDialect, InstrumentRole, instrument_catalog, instrument_descriptor};
use crate::{MeterRangeResistance, Mode};

/// Role vocabulary accepted in the `roles` arrays.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SpecRole {
    Source,
    Meter,
}

/// Source mode vocabulary accepted in `source_modes`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SourceMode {
    Voltage,
    Current,
}

/// Meter mode vocabulary accepted in `measure_modes`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum MeasureMode {
    DcVoltage,
    DcCurrent,
}

/// Display-precision methods understood by this crate. Other strings are kept as
/// written and treated as "no reviewed display policy".
pub(crate) const SOURCE_RANGE_TABLE: &str = "source_range_table";
pub(crate) const SCPI_RANGE_RESOLUTION_QUERY: &str = "scpi_range_resolution_query";
#[cfg(test)]
const DISPLAY_METHODS: [&str; 4] = [
    SOURCE_RANGE_TABLE,
    SCPI_RANGE_RESOLUTION_QUERY,
    "simulation",
    "unknown",
];

/// A source range command. The two variants must never be compared with each other:
/// a 7651 code of `5` is the 10 V range, not a 5 V or 5 A range.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum RangeCommand {
    /// SI range value sent to SCPI or simulation sources (V or A).
    Si(f64),
    /// Legacy Yokogawa 7651 range code.
    LegacyCode(u8),
}

impl RangeCommand {
    /// The numeric value stored in public profiles (`SourceRangeProfile::command_value`).
    pub(crate) fn value(self) -> f64 {
        match self {
            Self::Si(value) => value,
            Self::LegacyCode(code) => f64::from(code),
        }
    }
}

/// One documented source range in SI units.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SourceRange {
    pub name: String,
    pub command: RangeCommand,
    /// Maximum absolute programmable level in the range's SI unit.
    pub maximum: f64,
    /// Setting increment in the range's SI unit.
    pub resolution: f64,
    /// Fixed hardware current protection of a voltage range (GS210 divider ranges).
    pub fixed_current_limit_a: Option<f64>,
}

/// A source protection setting range in SI units.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct LimitSpec {
    pub minimum: f64,
    pub maximum: f64,
    pub resolution: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) struct SourceDisplayPrecision {
    pub method: String,
    #[serde(default)]
    pub review_status: Option<String>,
    #[serde(default)]
    pub range_query: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub(crate) struct ProtectionStatusSpec {
    #[serde(default)]
    pub query: Option<String>,
    #[serde(default)]
    pub low_limiter_mask: Option<i64>,
    #[serde(default)]
    pub high_limiter_mask: Option<i64>,
    #[serde(default)]
    pub emergency_mask: Option<i64>,
}

/// Typed source model specification.
// `roles`, `source_modes` and the `supports_*` flags are vocabulary declarations
// checked against the catalog descriptors by the integrity tests.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) struct SourceModelSpec {
    pub roles: Vec<SpecRole>,
    pub source_modes: Vec<SourceMode>,
    pub max_abs_v: f64,
    pub max_abs_a: f64,
    pub supports_output: bool,
    pub supports_software_ramp: bool,
    pub voltage_limit: LimitSpec,
    pub current_limit: LimitSpec,
    pub voltage_ranges: Vec<SourceRange>,
    pub current_ranges: Vec<SourceRange>,
    pub display_precision: Option<SourceDisplayPrecision>,
    pub protection_status: Option<ProtectionStatusSpec>,
}

impl SourceModelSpec {
    /// Ranges of the sourced quantity: voltage for IV, current for VI.
    pub(crate) fn ranges(&self, mode: Mode) -> &[SourceRange] {
        match mode {
            Mode::Iv => &self.voltage_ranges,
            Mode::Vi => &self.current_ranges,
        }
    }

    /// Hardware protection of the opposite quantity: current for IV, voltage for VI.
    pub(crate) fn protection_limit(&self, mode: Mode) -> &LimitSpec {
        match mode {
            Mode::Iv => &self.current_limit,
            Mode::Vi => &self.voltage_limit,
        }
    }

    /// Absolute maximum programmable level of the sourced quantity.
    pub(crate) fn maximum_level(&self, mode: Mode) -> f64 {
        match mode {
            Mode::Iv => self.max_abs_v,
            Mode::Vi => self.max_abs_a,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) struct MeterDisplayPrecision {
    pub method: String,
    #[serde(default)]
    pub review_status: Option<String>,
    #[serde(default)]
    pub voltage_range_query: Option<String>,
    #[serde(default)]
    pub current_range_query: Option<String>,
    #[serde(default)]
    pub voltage_resolution_query: Option<String>,
    #[serde(default)]
    pub current_resolution_query: Option<String>,
    #[serde(default, rename = "voltage_resolution_V")]
    pub voltage_resolution_v: Option<f64>,
    #[serde(default, rename = "current_resolution_A")]
    pub current_resolution_a: Option<f64>,
}

/// Typed meter model specification. `roles` and `measure_modes` are checked
/// against the catalog descriptors by the integrity tests.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) struct MeterModelSpec {
    pub roles: Vec<SpecRole>,
    pub measure_modes: Vec<MeasureMode>,
    #[serde(rename = "max_input_V")]
    pub max_input_v: f64,
    #[serde(rename = "max_input_A")]
    pub max_input_a: f64,
    pub auto_range_required: bool,
    #[serde(default)]
    pub nplc_values: Option<Vec<f64>>,
    #[serde(default)]
    pub nplc_min: Option<f64>,
    #[serde(default)]
    pub nplc_max: Option<f64>,
    /// Documented manual DC-voltage ranges in V; empty means unsupported.
    #[serde(default)]
    pub voltage_ranges: Vec<f64>,
    /// Documented manual DC-current ranges in A; empty means unsupported.
    #[serde(default)]
    pub current_ranges: Vec<f64>,
    #[serde(default)]
    pub current_input_resistances: Vec<MeterRangeResistance>,
    /// Measured quantities whose active range is read back after every
    /// conversion even without a reviewed display-precision query profile.
    #[serde(default)]
    pub range_readback_modes: Vec<MeasureMode>,
    #[serde(default)]
    pub display_precision: Option<MeterDisplayPrecision>,
}

impl MeterModelSpec {
    /// Run-planning NPLC policy: a non-empty discrete list wins, otherwise the
    /// continuous bounds must both exist and contain the value.
    pub(crate) fn nplc_allowed(&self, nplc: f64) -> bool {
        match self.nplc_values.as_deref() {
            Some(values) if !values.is_empty() => {
                values.iter().any(|value| (*value - nplc).abs() < 1e-12)
            }
            _ => {
                self.nplc_min.is_some_and(|minimum| nplc >= minimum)
                    && self.nplc_max.is_some_and(|maximum| nplc <= maximum)
            }
        }
    }

    /// Maximum meter input of a quantity (`Mode::Iv` → current, `Mode::Vi` → voltage).
    pub(crate) fn maximum_input(&self, measured: Mode) -> f64 {
        match measured {
            Mode::Iv => self.max_input_a,
            Mode::Vi => self.max_input_v,
        }
    }

    /// Manual ranges of the measured quantity (`Mode::Iv` → current, `Mode::Vi` → voltage).
    pub(crate) fn ranges(&self, measured: Mode) -> &[f64] {
        match measured {
            Mode::Iv => &self.current_ranges,
            Mode::Vi => &self.voltage_ranges,
        }
    }
}

// ---- Raw TOML shapes ---------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct RawVoltageRange {
    name: String,
    command_value: f64,
    #[serde(rename = "max_abs_V")]
    max_abs_v: f64,
    #[serde(rename = "resolution_V")]
    resolution_v: f64,
    #[serde(default)]
    fixed_current_limit_a: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct RawCurrentRange {
    name: String,
    command_value: f64,
    #[serde(rename = "max_abs_A")]
    max_abs_a: f64,
    #[serde(rename = "resolution_A")]
    resolution_a: f64,
}

#[derive(Debug, Deserialize)]
struct RawSource {
    roles: Vec<SpecRole>,
    source_modes: Vec<SourceMode>,
    #[serde(rename = "max_abs_V")]
    max_abs_v: f64,
    #[serde(rename = "max_abs_A")]
    max_abs_a: f64,
    supports_output: bool,
    supports_software_ramp: bool,
    #[serde(rename = "current_limit_min_A")]
    current_limit_min_a: f64,
    #[serde(rename = "current_limit_max_A")]
    current_limit_max_a: f64,
    #[serde(default, rename = "current_limit_resolution_A")]
    current_limit_resolution_a: Option<f64>,
    #[serde(rename = "voltage_limit_min_V")]
    voltage_limit_min_v: f64,
    #[serde(rename = "voltage_limit_max_V")]
    voltage_limit_max_v: f64,
    #[serde(default, rename = "voltage_limit_resolution_V")]
    voltage_limit_resolution_v: Option<f64>,
    voltage_ranges: Vec<RawVoltageRange>,
    current_ranges: Vec<RawCurrentRange>,
    #[serde(default)]
    display_precision: Option<SourceDisplayPrecision>,
    #[serde(default)]
    protection_status: Option<ProtectionStatusSpec>,
}

fn range_command(
    dialect: Option<InstrumentDialect>,
    model: &str,
    name: &str,
    value: f64,
) -> Result<RangeCommand, String> {
    if !value.is_finite() || value <= 0.0 {
        return Err(format!(
            "Embedded profile {model} range {name} has an invalid command_value."
        ));
    }
    if dialect == Some(InstrumentDialect::Yokogawa7651) {
        if value.fract() != 0.0 || value > f64::from(u8::MAX) {
            return Err(format!(
                "Embedded profile {model} range {name} has a non-integer legacy range code."
            ));
        }
        // Checked above: integral and within u8.
        return Ok(RangeCommand::LegacyCode(value as u8));
    }
    Ok(RangeCommand::Si(value))
}

impl RawSource {
    fn into_spec(self, model: &str) -> Result<SourceModelSpec, String> {
        let dialect = instrument_descriptor(model).map(|descriptor| descriptor.dialect);
        let voltage_ranges = self
            .voltage_ranges
            .into_iter()
            .map(|row| {
                Ok(SourceRange {
                    command: range_command(dialect, model, &row.name, row.command_value)?,
                    name: row.name,
                    maximum: row.max_abs_v,
                    resolution: row.resolution_v,
                    fixed_current_limit_a: row.fixed_current_limit_a,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        let current_ranges = self
            .current_ranges
            .into_iter()
            .map(|row| {
                Ok(SourceRange {
                    command: range_command(dialect, model, &row.name, row.command_value)?,
                    name: row.name,
                    maximum: row.max_abs_a,
                    resolution: row.resolution_a,
                    fixed_current_limit_a: None,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        Ok(SourceModelSpec {
            roles: self.roles,
            source_modes: self.source_modes,
            max_abs_v: self.max_abs_v,
            max_abs_a: self.max_abs_a,
            supports_output: self.supports_output,
            supports_software_ramp: self.supports_software_ramp,
            voltage_limit: LimitSpec {
                minimum: self.voltage_limit_min_v,
                maximum: self.voltage_limit_max_v,
                resolution: self.voltage_limit_resolution_v,
            },
            current_limit: LimitSpec {
                minimum: self.current_limit_min_a,
                maximum: self.current_limit_max_a,
                resolution: self.current_limit_resolution_a,
            },
            voltage_ranges,
            current_ranges,
            display_precision: self.display_precision,
            protection_status: self.protection_status,
        })
    }
}

fn parse_sources(text: &str) -> Result<BTreeMap<String, SourceModelSpec>, String> {
    toml::from_str::<BTreeMap<String, RawSource>>(text)
        .map_err(|error| format!("embedded source specification is invalid: {error}"))?
        .into_iter()
        .map(|(model, raw)| {
            let spec = raw
                .into_spec(&model)
                .map_err(|error| format!("embedded source specification is invalid: {error}"))?;
            Ok((model, spec))
        })
        .collect()
}

fn parse_meters(text: &str) -> Result<BTreeMap<String, MeterModelSpec>, String> {
    toml::from_str::<BTreeMap<String, MeterModelSpec>>(text)
        .map_err(|error| format!("embedded meter specification is invalid: {error}"))
}

/// All source tables in `specs/sources.toml`, keyed by model id.
pub(crate) fn source_models() -> Result<&'static BTreeMap<String, SourceModelSpec>, String> {
    static SPECS: OnceLock<Result<BTreeMap<String, SourceModelSpec>, String>> = OnceLock::new();
    SPECS
        .get_or_init(|| parse_sources(include_str!("../../../specs/sources.toml")))
        .as_ref()
        .map_err(Clone::clone)
}

/// All meter tables in `specs/meters.toml`, keyed by model id.
pub(crate) fn meter_models() -> Result<&'static BTreeMap<String, MeterModelSpec>, String> {
    static SPECS: OnceLock<Result<BTreeMap<String, MeterModelSpec>, String>> = OnceLock::new();
    SPECS
        .get_or_init(|| parse_meters(include_str!("../../../specs/meters.toml")))
        .as_ref()
        .map_err(Clone::clone)
}

/// Source table for any model present in `sources.toml` (no catalog gate).
pub(crate) fn source_model(model: &str) -> Result<Option<&'static SourceModelSpec>, String> {
    Ok(source_models()?.get(model))
}

/// Meter table for any model present in `meters.toml` (no catalog gate).
pub(crate) fn meter_model(model: &str) -> Result<Option<&'static MeterModelSpec>, String> {
    Ok(meter_models()?.get(model))
}

/// Source models accepted by run planning and manual output, in catalog order.
/// Simulation sources are deliberately excluded.
fn plannable_source_ids() -> impl Iterator<Item = &'static str> {
    instrument_catalog(InstrumentRole::Source)
        .iter()
        .filter(|descriptor| !descriptor.simulation)
        .map(|descriptor| descriptor.id)
}

/// Meter models accepted by run planning, in catalog order.
fn plannable_meter_ids() -> impl Iterator<Item = &'static str> {
    instrument_catalog(InstrumentRole::Meter)
        .iter()
        .map(|descriptor| descriptor.id)
}

/// Gate a source model for planning, then return its table.
pub(crate) fn plannable_source(model: &str) -> Result<&'static SourceModelSpec, String> {
    if !plannable_source_ids().any(|id| id == model) {
        return Err(format!(
            "Unsupported source model '{model}'. Supported source models: {}.",
            plannable_source_ids().collect::<Vec<_>>().join(", ")
        ));
    }
    source_model(model)?.ok_or_else(|| format!("Embedded source profile for {model} is missing."))
}

/// Gate a meter model for planning, then return its table.
pub(crate) fn plannable_meter(model: &str) -> Result<&'static MeterModelSpec, String> {
    if !plannable_meter_ids().any(|id| id == model) {
        return Err(format!(
            "Unsupported meter model '{model}'. Supported meter models: {}.",
            plannable_meter_ids().collect::<Vec<_>>().join(", ")
        ));
    }
    meter_model(model)?.ok_or_else(|| format!("Embedded meter profile for {model} is missing."))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::InstrumentCapabilities;
    use std::collections::BTreeSet;

    fn positive(value: f64, what: &str) {
        assert!(
            value.is_finite() && value > 0.0,
            "{what} must be finite and positive, got {value}"
        );
    }

    fn strictly_increasing(values: &[f64], what: &str) {
        for pair in values.windows(2) {
            assert!(pair[0] < pair[1], "{what} is not strictly increasing: {values:?}");
        }
    }

    fn close(left: f64, right: f64) -> bool {
        (left - right).abs() <= 1e-12 * left.abs().max(right.abs()).max(1.0)
    }

    #[test]
    fn plannable_gates_keep_the_established_wording_and_order() {
        assert_eq!(
            plannable_source("SIMULATED_SOURCE").unwrap_err(),
            "Unsupported source model 'SIMULATED_SOURCE'. Supported source models: YOKOGAWA_GS210, YOKOGAWA_7651."
        );
        assert_eq!(
            plannable_meter("YOKOGAWA_GS210").unwrap_err(),
            "Unsupported meter model 'YOKOGAWA_GS210'. Supported meter models: AGILENT_34401A, AGILENT_34411A, KEYSIGHT_34411A, KEYSIGHT_34465A, ADCMT_7461A, SIMULATED_METER."
        );
        assert!(plannable_source("YOKOGAWA_7651").is_ok());
        assert!(plannable_meter("SIMULATED_METER").is_ok());
    }

    #[test]
    fn spec_tables_and_catalog_are_one_to_one_by_role() {
        let sources = source_models().unwrap();
        let meters = meter_models().unwrap();
        let source_ids: BTreeSet<_> = instrument_catalog(InstrumentRole::Source)
            .iter()
            .map(|descriptor| descriptor.id)
            .collect();
        let meter_ids: BTreeSet<_> = instrument_catalog(InstrumentRole::Meter)
            .iter()
            .map(|descriptor| descriptor.id)
            .collect();
        assert_eq!(
            source_ids.len(),
            instrument_catalog(InstrumentRole::Source).len(),
            "duplicate source descriptor id"
        );
        assert_eq!(
            meter_ids.len(),
            instrument_catalog(InstrumentRole::Meter).len(),
            "duplicate meter descriptor id"
        );
        assert!(source_ids.is_disjoint(&meter_ids));
        assert_eq!(sources.keys().map(String::as_str).collect::<BTreeSet<_>>(), source_ids);
        assert_eq!(meters.keys().map(String::as_str).collect::<BTreeSet<_>>(), meter_ids);
        for spec in sources.values() {
            assert_eq!(spec.roles, [SpecRole::Source]);
        }
        for spec in meters.values() {
            assert_eq!(spec.roles, [SpecRole::Meter]);
        }
    }

    #[test]
    fn declared_modes_match_descriptor_capabilities() {
        for descriptor in instrument_catalog(InstrumentRole::Source) {
            let spec = source_model(descriptor.id).unwrap().unwrap();
            let InstrumentCapabilities::Source(capabilities) = descriptor.capabilities else {
                panic!("{} has meter capabilities", descriptor.id);
            };
            assert_eq!(capabilities.output, spec.supports_output);
            assert_eq!(capabilities.software_ramp, spec.supports_software_ramp);
            assert_eq!(
                capabilities.voltage,
                spec.source_modes.contains(&SourceMode::Voltage)
            );
            assert_eq!(
                capabilities.current,
                spec.source_modes.contains(&SourceMode::Current)
            );
            assert_eq!(
                spec.source_modes.iter().collect::<BTreeSet<_>>().len(),
                spec.source_modes.len()
            );
        }
        for descriptor in instrument_catalog(InstrumentRole::Meter) {
            let spec = meter_model(descriptor.id).unwrap().unwrap();
            let InstrumentCapabilities::Meter(capabilities) = descriptor.capabilities else {
                panic!("{} has source capabilities", descriptor.id);
            };
            assert_eq!(
                capabilities.voltage,
                spec.measure_modes.contains(&MeasureMode::DcVoltage)
            );
            assert_eq!(
                capabilities.current,
                spec.measure_modes.contains(&MeasureMode::DcCurrent)
            );
        }
    }

    #[test]
    fn range_commands_are_typed_by_dialect() {
        for (model, spec) in source_models().unwrap() {
            let legacy = instrument_descriptor(model).unwrap().dialect
                == InstrumentDialect::Yokogawa7651;
            for range in spec.voltage_ranges.iter().chain(&spec.current_ranges) {
                match range.command {
                    RangeCommand::LegacyCode(code) => {
                        assert!(legacy, "{model} {} has a legacy code", range.name);
                        assert!((2..=6).contains(&code), "{model} code {code}");
                    }
                    RangeCommand::Si(value) => {
                        assert!(!legacy, "{model} {} has an SI command", range.name);
                        positive(value, "SI range command");
                        // SI command values are nominal ranges within the programmable maximum.
                        assert!(
                            value <= range.maximum * (1.0 + 1e-12),
                            "{model} {} nominal {value} exceeds maximum {}",
                            range.name,
                            range.maximum
                        );
                    }
                }
            }
        }
        let legacy = source_model("YOKOGAWA_7651").unwrap().unwrap();
        assert_eq!(legacy.voltage_ranges[0].command, RangeCommand::LegacyCode(2));
        assert_eq!(legacy.current_ranges[0].command, RangeCommand::LegacyCode(4));
        let scpi = source_model("YOKOGAWA_GS210").unwrap().unwrap();
        assert_eq!(scpi.voltage_ranges[0].command, RangeCommand::Si(0.01));
        assert!(range_command(Some(InstrumentDialect::Yokogawa7651), "m", "r", 2.5).is_err());
        assert!(range_command(None, "m", "r", f64::NAN).is_err());
        assert!(range_command(None, "m", "r", 0.0).is_err());
    }

    #[test]
    fn source_numeric_ranges_are_finite_positive_ordered_and_bounded() {
        for (model, spec) in source_models().unwrap() {
            positive(spec.max_abs_v, "max_abs_V");
            positive(spec.max_abs_a, "max_abs_A");
            for mode in [Mode::Iv, Mode::Vi] {
                let ranges = spec.ranges(mode);
                assert!(!ranges.is_empty(), "{model} {mode:?} has no ranges");
                let mut names = BTreeSet::new();
                for range in ranges {
                    assert!(names.insert(range.name.as_str()), "{model} duplicate {}", range.name);
                    positive(range.maximum, "range maximum");
                    positive(range.resolution, "range resolution");
                    assert!(range.resolution < range.maximum, "{model} {}", range.name);
                    assert!(
                        range.maximum <= spec.maximum_level(mode) * (1.0 + 1e-12),
                        "{model} {} exceeds the model maximum",
                        range.name
                    );
                }
                strictly_increasing(
                    &ranges.iter().map(|range| range.maximum).collect::<Vec<_>>(),
                    "range maxima",
                );
                strictly_increasing(
                    &ranges.iter().map(|range| range.command.value()).collect::<Vec<_>>(),
                    "range commands",
                );
                for pair in ranges.windows(2) {
                    assert!(pair[0].resolution <= pair[1].resolution, "{model} resolution order");
                }
                // The largest range reaches exactly the declared model maximum.
                assert!(close(
                    ranges.last().unwrap().maximum,
                    spec.maximum_level(mode)
                ));
            }
        }
    }

    #[test]
    fn protection_limits_are_bounded_by_the_source_maxima() {
        for (model, spec) in source_models().unwrap() {
            for (limit, maximum) in [
                (&spec.current_limit, spec.max_abs_a),
                (&spec.voltage_limit, spec.max_abs_v),
            ] {
                positive(limit.minimum, "protection minimum");
                positive(limit.maximum, "protection maximum");
                assert!(limit.minimum < limit.maximum, "{model} protection order");
                assert!(
                    limit.maximum <= maximum * (1.0 + 1e-12),
                    "{model} protection maximum {} exceeds {maximum}",
                    limit.maximum
                );
                if let Some(resolution) = limit.resolution {
                    positive(resolution, "protection resolution");
                    assert!(resolution <= limit.minimum * (1.0 + 1e-12), "{model}");
                }
            }
            // Planning requires a hardware protection increment for real sources.
            if !instrument_descriptor(model).unwrap().simulation {
                assert!(spec.current_limit.resolution.is_some(), "{model}");
                assert!(spec.voltage_limit.resolution.is_some(), "{model}");
            }
            for range in &spec.voltage_ranges {
                if let Some(fixed) = range.fixed_current_limit_a {
                    positive(fixed, "fixed current protection");
                    assert!(fixed <= spec.current_limit.maximum * (1.0 + 1e-12), "{model}");
                }
            }
            assert!(spec.current_ranges.iter().all(|r| r.fixed_current_limit_a.is_none()));
        }
        // Only the GS210 divider ranges declare a fixed protection.
        let gs210 = source_model("YOKOGAWA_GS210").unwrap().unwrap();
        assert_eq!(
            gs210
                .voltage_ranges
                .iter()
                .filter(|range| range.fixed_current_limit_a == Some(0.2))
                .map(|range| range.name.as_str())
                .collect::<Vec<_>>(),
            ["10mV", "100mV"]
        );
    }

    #[test]
    fn protection_status_masks_are_distinct_nonzero_bits() {
        for (model, spec) in source_models().unwrap() {
            let Some(status) = &spec.protection_status else {
                continue;
            };
            assert!(status.query.as_deref().is_some_and(|q| !q.is_empty()), "{model}");
            let masks = [
                status.low_limiter_mask,
                status.high_limiter_mask,
                status.emergency_mask,
            ]
            .map(|mask| u16::try_from(mask.unwrap()).unwrap());
            for mask in masks {
                assert!(mask != 0 && mask.is_power_of_two(), "{model} mask {mask}");
            }
            assert_eq!(
                (masks[0] & masks[1]) | (masks[0] & masks[2]) | (masks[1] & masks[2]),
                0
            );
        }
    }

    #[test]
    fn meter_numeric_ranges_are_finite_positive_ordered_and_bounded() {
        for (model, spec) in meter_models().unwrap() {
            positive(spec.max_input_v, "max_input_V");
            positive(spec.max_input_a, "max_input_A");
            for mode in [Mode::Iv, Mode::Vi] {
                let ranges = spec.ranges(mode);
                for range in ranges {
                    positive(*range, "meter range");
                    assert!(
                        *range <= spec.maximum_input(mode) * (1.0 + 1e-12),
                        "{model} range {range} exceeds the maximum input"
                    );
                }
                strictly_increasing(ranges, "meter ranges");
            }
            if spec.auto_range_required {
                assert!(spec.voltage_ranges.is_empty() && spec.current_ranges.is_empty());
            } else {
                // A profile may verify fixed ranges for one quantity only
                // (ADCMT 7461A: DC voltage over GPIB SCPI).
                assert!(!spec.voltage_ranges.is_empty() || !spec.current_ranges.is_empty());
            }
            for mode in &spec.range_readback_modes {
                assert!(spec.measure_modes.contains(mode), "{model} readback mode");
            }
            match &spec.nplc_values {
                Some(values) => {
                    assert!(!values.is_empty(), "{model} has an empty nplc list");
                    values.iter().for_each(|value| positive(*value, "nplc"));
                    strictly_increasing(values, "nplc_values");
                }
                None => {
                    let (minimum, maximum) = (spec.nplc_min.unwrap(), spec.nplc_max.unwrap());
                    positive(minimum, "nplc_min");
                    positive(maximum, "nplc_max");
                    assert!(minimum < maximum, "{model} nplc order");
                }
            }
            let resistance_ranges = spec
                .current_input_resistances
                .iter()
                .map(|row| {
                    positive(row.range, "resistance range");
                    positive(row.ohms, "input resistance");
                    row.range
                })
                .collect::<Vec<_>>();
            strictly_increasing(&resistance_ranges, "input resistance ranges");
            // Where manual current ranges are documented, loading rows refer to them.
            if !spec.current_ranges.is_empty() {
                for range in &resistance_ranges {
                    assert!(
                        spec.current_ranges.iter().any(|r| close(*r, *range)),
                        "{model} resistance range {range} is not a current range"
                    );
                }
            }
        }
    }

    #[test]
    fn display_precision_uses_the_known_vocabulary() {
        for (model, spec) in source_models().unwrap() {
            let precision = spec.display_precision.as_ref().unwrap();
            assert!(DISPLAY_METHODS.contains(&precision.method.as_str()), "{model}");
            assert_eq!(precision.method, SOURCE_RANGE_TABLE, "{model}");
            assert!(precision.review_status.is_some(), "{model}");
        }
        for (model, spec) in meter_models().unwrap() {
            let precision = spec.display_precision.as_ref().unwrap();
            assert!(DISPLAY_METHODS.contains(&precision.method.as_str()), "{model}");
            assert!(precision.review_status.is_some(), "{model}");
            if precision.method == SCPI_RANGE_RESOLUTION_QUERY {
                for query in [
                    &precision.voltage_range_query,
                    &precision.current_range_query,
                    &precision.voltage_resolution_query,
                    &precision.current_resolution_query,
                ] {
                    assert!(query.as_deref().is_some_and(|q| q.ends_with('?')), "{model}");
                }
            }
            for resolution in [precision.voltage_resolution_v, precision.current_resolution_a]
                .into_iter()
                .flatten()
            {
                positive(resolution, "display resolution");
            }
        }
    }

    #[test]
    fn nplc_policy_matches_discrete_and_continuous_profiles() {
        let discrete = meter_model("AGILENT_34401A").unwrap().unwrap();
        assert!(discrete.nplc_allowed(1.0));
        assert!(!discrete.nplc_allowed(0.5));
        let continuous = meter_model("ADCMT_7461A").unwrap().unwrap();
        assert!(continuous.nplc_allowed(0.5));
        assert!(!continuous.nplc_allowed(0.01));
        assert!(!continuous.nplc_allowed(f64::NAN));
    }
}
