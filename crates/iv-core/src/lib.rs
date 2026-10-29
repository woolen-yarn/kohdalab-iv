#![forbid(unsafe_code)]

//! Deterministic, hardware-aware planning and CSV persistence for IV/VI runs.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs::OpenOptions;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

mod instrument_specs;
pub use instrument_specs::{
    SourceProtectionProfile, meter_display_queries, source_display_resolution,
    source_protection_profile, source_range_query,
};

mod catalog;
mod measurements;
pub use catalog::{
    InstrumentCapabilities, InstrumentDescriptor, InstrumentDialect, InstrumentProfile,
    InstrumentRole, MeterCapabilities, MeterProfile, NplcProfile, SourceCapabilities,
    SourceLimitProfile, SourceProfile, SourceRangeProfile, instrument_catalog,
    instrument_descriptor, instrument_profile,
};
pub use measurements::{
    MeasurementDefinition, ParameterDefinition, ParameterUnit, PlotAxis, SweepDefinition,
    measurement_definition, parameter_definition, sweep_definitions,
};

/// Analysis result schema: one row per saved sweep point.
pub const CSV_FIELDS: &[&str] = &[
    "run_id",
    "point_index",
    "timestamp",
    "elapsed_s",
    "sweep_index",
    "direction",
    "target_value",
    "target_unit",
    "voltage_V",
    "current_A",
    "voltage_origin",
    "current_origin",
    "readings_requested",
    "readings_completed",
    "meter_stddev",
    "meter_unit",
    "software_limit_hit",
    "quality_flags",
];

const MAX_POINTS: usize = 1_000_000;
pub const MAX_SECONDS: f64 = 3_600.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Mode {
    #[serde(rename = "dc_iv")]
    Iv,
    #[serde(rename = "dc_vi")]
    Vi,
}

impl Mode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Iv => "dc_iv",
            Self::Vi => "dc_vi",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Pattern {
    #[serde(rename = "linear")]
    Linear,
    #[serde(rename = "round_trip")]
    RoundTrip,
    #[serde(rename = "zero_centered")]
    ZeroCentered,
    #[serde(rename = "custom_list")]
    CustomList,
}

impl Pattern {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Linear => "linear",
            Self::RoundTrip => "round_trip",
            Self::ZeroCentered => "zero_centered",
            Self::CustomList => "custom_list",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunConfig {
    /// Optional user supplied sample and experiment context retained in run metadata.
    #[serde(default, skip_serializing_if = "serde_json::Map::is_empty")]
    pub experiment: serde_json::Map<String, serde_json::Value>,
    /// TOML source profile used to validate and quantize this run.
    ///
    /// Older saved configurations omit this field and therefore retain the
    /// original GS210 behaviour.
    #[serde(default = "default_source_model")]
    pub source_model: String,
    /// TOML meter profile used to validate NPLC and input compliance.
    /// Older saved configurations omit this field and therefore retain the
    /// original 34411A behaviour.
    #[serde(default = "default_meter_model")]
    pub meter_model: String,
    /// Fixed DC-voltage meter range in V. `None` leaves voltage measurements
    /// in the instrument's automatic range mode.
    #[serde(default)]
    pub meter_voltage_range: Option<f64>,
    /// Fixed DC-current meter range in A. `None` leaves current measurements
    /// in the instrument's automatic range mode.
    #[serde(default)]
    pub meter_current_range: Option<f64>,
    pub mode: Mode,
    pub pattern: Pattern,
    /// Start target, in SI units (V for IV and A for VI).
    pub start: f64,
    /// Stop target, in SI units (V for IV and A for VI).
    pub stop: f64,
    /// Sweep increment, in SI units. Its sign is ignored; sweep direction is inferred.
    pub step: f64,
    /// Meter readings averaged at each fixed source target; not sweep repetitions.
    pub repeat: u32,
    pub custom_points: Vec<f64>,
    /// Software maximum absolute source target, in source SI units.
    pub source_limit: f64,
    /// Requested software compliance, A for IV and V for VI.
    pub compliance: f64,
    /// Independent source hardware protection, A for IV or V for VI.
    /// None preserves old configs: use the meter software stop value.
    #[serde(default)]
    pub source_compliance: Option<f64>,
    /// Optional second voltage source for a balanced differential IV sweep.
    /// The differential target remains `start..stop`; the two source targets
    /// are `ratio * D` and `-(1 - ratio) * D`.  Older configurations omit this field.
    #[serde(default)]
    pub balanced_source: Option<BalancedSourceConfig>,
    pub settle_s: f64,
    pub start_settle_s: f64,
    pub nplc: f64,
    /// Legacy alias for per-point averaging. `repeat` takes precedence when > 1.
    pub average_count: u32,
    pub ramp_step: f64,
    pub ramp_wait_s: f64,
}

fn default_source_model() -> String {
    "YOKOGAWA_GS210".into()
}

fn default_meter_model() -> String {
    "AGILENT_34411A".into()
}

impl Default for RunConfig {
    fn default() -> Self {
        Self {
            experiment: serde_json::Map::new(),
            source_model: default_source_model(),
            meter_model: default_meter_model(),
            meter_voltage_range: None,
            meter_current_range: None,
            mode: Mode::Iv,
            pattern: Pattern::ZeroCentered,
            start: -0.001,
            stop: 0.001,
            step: 0.0001,
            repeat: 1,
            custom_points: Vec::new(),
            source_limit: 0.001,
            compliance: 0.001,
            source_compliance: None,
            balanced_source: None,
            settle_s: 0.2,
            start_settle_s: 0.5,
            nplc: 1.0,
            average_count: 1,
            ramp_step: 0.0001,
            ramp_wait_s: 0.02,
        }
    }
}

/// Configuration for the second source in a balanced differential IV sweep.
/// The gate remains external to this run and is deliberately not represented
/// as a source target here.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BalancedSourceConfig {
    #[serde(default = "default_source_model")]
    pub source_model: String,
    /// Integer voltage weights [a, b]. Both sources
    /// have opposite polarities: V1 = a/(a+b) * D; V2 = -b/(a+b) * D.
    #[serde(default = "default_balance_ratio")]
    pub ratio: [u32; 2],
    /// Hardware current compliance for the second voltage source, in A.
    pub source_compliance: f64,
}

fn default_balance_ratio() -> [u32; 2] {
    [1, 1]
}

impl Default for BalancedSourceConfig {
    fn default() -> Self {
        Self {
            source_model: default_source_model(),
            source_compliance: 0.001,
            ratio: default_balance_ratio(),
        }
    }
}

impl BalancedSourceConfig {
    pub fn source1_fraction(&self) -> f64 {
        self.ratio[0] as f64 / (self.ratio[0] as f64 + self.ratio[1] as f64)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SweepPoint {
    pub target: f64,
    pub direction: String,
    pub repeat_index: u32,
    pub sweep_index: u32,
    pub point_index: usize,
    pub total_points: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Plan {
    pub points: Vec<SweepPoint>,
    /// Selected GS210 source range in source SI units.
    pub source_range: f64,
    /// GS210-compatible hardware compliance. The requested value remains a software limit.
    pub hardware_compliance: f64,
}

/// Fully validated source-specific plans for a balanced differential IV run.
/// `points` retain the requested differential targets; `config1` and
/// `config2` contain the corresponding individual source targets.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BalancedPlan {
    pub config1: RunConfig,
    pub config2: RunConfig,
    pub plan1: Plan,
    pub plan2: Plan,
    pub points: Vec<SweepPoint>,
    pub ratio: [u32; 2],
    pub differential_resolution: f64,
}

impl BalancedPlan {
    /// Exact decimal ramp, excluding the final target (already in plan1/plan2).
    pub fn ramp_targets(
        &self,
        from: f64,
        to: f64,
        step: f64,
    ) -> Result<impl Iterator<Item = Result<[f64; 2], String>>, String> {
        let mut level = decimal(from, "ramp start")?;
        let target = decimal(to, "ramp target")?;
        let step = decimal(step, "ramp step")?;
        if step <= Decimal::ZERO {
            return Err("Ramp step must be positive".into());
        }
        if (target - level).abs() / step > Decimal::from(MAX_POINTS as u32) {
            return Err("Dual ramp exceeds one million steps".into());
        }
        let part1 = Decimal::from(self.ratio[0]);
        let part2 = Decimal::from(self.ratio[1]);
        let total = part1 + part2;
        Ok(std::iter::from_fn(move || {
            if (target - level).abs() <= step {
                return None;
            }
            level += if target > level { step } else { -step };
            Some((|| {
                Ok([
                    decimal_to_f64(
                        scaled_decimal(level, part1, total, "first ramp target")?,
                        "first ramp target",
                    )?,
                    decimal_to_f64(
                        scaled_decimal(-level, part2, total, "second ramp target")?,
                        "second ramp target",
                    )?,
                ])
            })())
        }))
    }
}

#[derive(Debug, Deserialize)]
struct Specs {
    #[serde(flatten)]
    sources: BTreeMap<String, SourceSpec>,
}

#[derive(Debug, Deserialize)]
struct SourceSpec {
    #[serde(rename = "max_abs_V")]
    max_abs_v: f64,
    #[serde(rename = "max_abs_A")]
    max_abs_a: f64,
    #[serde(rename = "current_limit_min_A")]
    current_limit_min_a: f64,
    #[serde(rename = "current_limit_max_A")]
    current_limit_max_a: f64,
    #[serde(rename = "current_limit_resolution_A")]
    #[serde(default)]
    current_limit_resolution_a: f64,
    #[serde(rename = "voltage_limit_min_V")]
    voltage_limit_min_v: f64,
    #[serde(rename = "voltage_limit_max_V")]
    voltage_limit_max_v: f64,
    #[serde(rename = "voltage_limit_resolution_V")]
    #[serde(default)]
    voltage_limit_resolution_v: f64,
    voltage_ranges: Vec<RangeSpec>,
    current_ranges: Vec<RangeSpec>,
}

#[derive(Debug, Deserialize)]
struct RangeSpec {
    command_value: f64,
    #[serde(rename = "max_abs_V")]
    max_abs_v: Option<f64>,
    #[serde(rename = "resolution_V")]
    resolution_v: Option<f64>,
    #[serde(rename = "max_abs_A")]
    max_abs_a: Option<f64>,
    #[serde(rename = "resolution_A")]
    resolution_a: Option<f64>,
    #[serde(default)]
    fixed_current_limit_a: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct MeterSpecs {
    #[serde(flatten)]
    meters: BTreeMap<String, DmmSpec>,
}

#[derive(Debug, Deserialize)]
struct DmmSpec {
    #[serde(default)]
    nplc_values: Vec<f64>,
    #[serde(rename = "max_input_V")]
    max_input_v: f64,
    #[serde(rename = "max_input_A")]
    max_input_a: f64,
    nplc_min: Option<f64>,
    nplc_max: Option<f64>,
    #[serde(default)]
    voltage_ranges: Vec<f64>,
    #[serde(default)]
    current_ranges: Vec<f64>,
}

fn source_specs() -> Result<BTreeMap<String, SourceSpec>, String> {
    toml::from_str::<Specs>(include_str!("../../../specs/sources.toml"))
        .map(|specs| specs.sources)
        .map_err(|error| format!("embedded source specification is invalid: {error}"))
}

fn meter_specs() -> Result<BTreeMap<String, DmmSpec>, String> {
    toml::from_str::<MeterSpecs>(include_str!("../../../specs/meters.toml"))
        .map(|specs| specs.meters)
        .map_err(|error| format!("embedded meter specification is invalid: {error}"))
}

fn source_spec(model: &str) -> Result<SourceSpec, String> {
    if !matches!(model, "YOKOGAWA_GS210" | "YOKOGAWA_7651") {
        return Err(format!(
            "Unsupported source model '{model}'. Supported source models: YOKOGAWA_GS210, YOKOGAWA_7651."
        ));
    }
    source_specs()?
        .remove(model)
        .ok_or_else(|| format!("Embedded source profile for {model} is missing."))
}

fn meter_spec(model: &str) -> Result<DmmSpec, String> {
    if !matches!(
        model,
        "AGILENT_34401A"
            | "AGILENT_34411A"
            | "KEYSIGHT_34411A"
            | "KEYSIGHT_34465A"
            | "ADCMT_7461A"
            | "SIMULATED_METER"
    ) {
        return Err(format!(
            "Unsupported meter model '{model}'. Supported meter models: AGILENT_34401A, AGILENT_34411A, KEYSIGHT_34411A, KEYSIGHT_34465A, ADCMT_7461A, SIMULATED_METER."
        ));
    }
    meter_specs()?
        .remove(model)
        .ok_or_else(|| format!("Embedded meter profile for {model} is missing."))
}

/// Fixed ranges supported by a meter profile for the selected measurement
/// mode. Values are SI amperes with a voltage source (`Mode::Iv`) and SI volts
/// with a current source (`Mode::Vi`).
pub fn meter_ranges(model: &str, mode: Mode) -> Result<Vec<f64>, String> {
    let meter = meter_spec(model)?;
    Ok(match mode {
        Mode::Iv => meter.current_ranges,
        Mode::Vi => meter.voltage_ranges,
    })
}

/// The only fixed range relevant to a run. Voltage-source mode measures current,
/// while current-source mode measures voltage. The inactive range is retained so a
/// user can switch modes without losing the previous selection.
pub const fn selected_meter_range(config: &RunConfig) -> Option<f64> {
    match config.mode {
        Mode::Iv => config.meter_current_range,
        Mode::Vi => config.meter_voltage_range,
    }
}

fn ranges_match(expected: f64, actual: f64) -> bool {
    (expected - actual).abs() <= 1e-12 * expected.abs().max(actual.abs()).max(1.0)
}

/// Validate a requested fixed meter range without issuing any instrument I/O.
/// `None` is the explicit Auto mode. A model with no documented manual range
/// list rejects fixed values instead of silently falling back to Auto.
pub fn validate_meter_range(config: &RunConfig) -> Result<(), String> {
    let Some(requested) = selected_meter_range(config) else {
        return Ok(());
    };
    if !requested.is_finite() || requested <= 0.0 {
        return Err("Fixed meter range must be finite and positive.".into());
    }
    let ranges = meter_ranges(&config.meter_model, config.mode)?;
    let quantity = match config.mode {
        Mode::Iv => "current",
        Mode::Vi => "voltage",
    };
    if ranges.is_empty() {
        return Err(format!(
            "{} has no verified fixed {} range support. Select Auto.",
            config.meter_model, quantity
        ));
    }
    if !ranges.iter().any(|range| ranges_match(*range, requested)) {
        return Err(format!(
            "{} does not support fixed {} range {}. Choose one of: {}.",
            config.meter_model,
            quantity,
            requested,
            ranges
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    Ok(())
}

/// Confirm that a meter stayed on the requested manual range after a range
/// query. Auto mode intentionally performs no comparison.
pub fn verify_meter_range(requested: Option<f64>, actual: f64) -> Result<(), String> {
    let Some(requested) = requested else {
        return Ok(());
    };
    if !actual.is_finite() || actual <= 0.0 {
        return Err("Meter returned an invalid fixed-range query result.".into());
    }
    if !ranges_match(requested, actual) {
        return Err(format!(
            "Meter range changed during a fixed-range run (requested {requested}, actual {actual})."
        ));
    }
    Ok(())
}

fn decimal(value: f64, name: &str) -> Result<Decimal, String> {
    if !value.is_finite() {
        return Err(format!("{name} must be finite."));
    }
    Decimal::from_str(&value.to_string()).map_err(|_| format!("{name} is not representable."))
}

fn finite_positive(value: f64, name: &str, maximum: Option<f64>) -> Result<(), String> {
    if !value.is_finite() || value <= 0.0 {
        return Err(format!("{name} must be finite and positive."));
    }
    if let Some(maximum) = maximum
        && value > maximum
    {
        return Err(format!("{name} must be <= {maximum}."));
    }
    Ok(())
}

/// Increment an SI source level on an exact decimal hardware grid.
/// Rejects range/precision violations; it never clamps or changes range.
pub fn nudge_source_value(
    value: f64,
    delta: f64,
    resolution: f64,
    maximum: f64,
) -> Result<f64, String> {
    finite_positive(resolution, "Source resolution", None)?;
    finite_positive(maximum, "Source range", None)?;
    let current = aligned_value(value, resolution, "DC level", "SI")?;
    let increment = aligned_value(delta, resolution, "DC increment", "SI")?;
    let next = current
        .checked_add(increment)
        .ok_or("DC increment is outside the supported numeric range.")?;
    if next.abs() > decimal(maximum, "Source range")? {
        return Err(format!(
            "DC level exceeds the enabled ±{maximum:e} source range."
        ));
    }
    decimal_to_f64(next, "DC level")
}

fn decimal_range(start: Decimal, stop: Decimal, step: Decimal) -> Result<Vec<Decimal>, String> {
    if step.is_zero() {
        return Err("step must be non-zero.".to_owned());
    }
    if step.is_sign_positive() && start > stop {
        return Err("positive step cannot reach stop from start.".to_owned());
    }
    if step.is_sign_negative() && start < stop {
        return Err("negative step cannot reach stop from start.".to_owned());
    }
    let mut current = start;
    let mut points = Vec::new();
    loop {
        if points.len() >= MAX_POINTS {
            return Err(format!("sweep exceeds {MAX_POINTS} points."));
        }
        points.push(current);
        if current == stop {
            break;
        }
        // Inputs are already aligned decimal hardware increments. Preserve the
        // exact endpoint with a shorter final interval, without overshooting it
        // or adding a potentially enormous step beyond the source range.
        current = if (stop - current).abs() <= step.abs() {
            stop
        } else {
            current + step
        };
    }
    Ok(points)
}

fn forward_points(start: Decimal, stop: Decimal, step: Decimal) -> Result<Vec<Decimal>, String> {
    let signed_step = if stop >= start {
        step.abs()
    } else {
        -step.abs()
    };
    decimal_range(start, stop, signed_step)
}

fn append_joined(points: &mut Vec<Decimal>, segment: Vec<Decimal>) {
    let start = usize::from(
        points
            .last()
            .is_some_and(|last| segment.first() == Some(last)),
    );
    points.extend(segment.into_iter().skip(start));
}

#[cfg(test)]
fn directions(points: &[Decimal]) -> Vec<String> {
    if points.len() <= 1 {
        return vec!["forward".to_owned(); points.len()];
    }
    let mut last = "forward";
    points
        .iter()
        .enumerate()
        .map(|(index, point)| {
            let delta = if index == 0 {
                points[1] - *point
            } else {
                *point - points[index - 1]
            };
            if delta > Decimal::ZERO {
                last = "forward";
            }
            if delta < Decimal::ZERO {
                last = "backward";
            }
            last.to_owned()
        })
        .collect()
}

fn select_range(
    spec: &SourceSpec,
    model: &str,
    mode: Mode,
    maximum: f64,
    requested_protection: Option<f64>,
) -> Result<(f64, f64, f64), String> {
    let effective_protection = requested_protection
        .map(|value| {
            finite_positive(value, "Source protection limit", None)?;
            quantized_hardware_compliance(spec, model, mode, value)
        })
        .transpose()?;
    let entries = match mode {
        Mode::Iv => &spec.voltage_ranges,
        Mode::Vi => &spec.current_ranges,
    };
    entries
        .iter()
        // Honor an explicit Source hardware limit without silently weakening
        // it: GS210 divider ranges are ineligible below their fixed 200 mA.
        // Legacy None preserves the previous range/physical limit behaviour.
        .filter(|entry| {
            mode != Mode::Iv
                || effective_protection.is_none_or(|limit| {
                    entry
                        .fixed_current_limit_a
                        .is_none_or(|fixed| fixed <= limit + f64::EPSILON)
                })
        })
        .filter_map(|entry| match mode {
            Mode::Iv => entry
                .max_abs_v
                .zip(entry.resolution_v)
                .map(|(maximum, resolution)| (entry.command_value, maximum, resolution)),
            Mode::Vi => entry
                .max_abs_a
                .zip(entry.resolution_a)
                .map(|(maximum, resolution)| (entry.command_value, maximum, resolution)),
        })
        .filter(|(_, supported_maximum, _)| maximum <= *supported_maximum + f64::EPSILON)
        .min_by(|left, right| left.1.total_cmp(&right.1))
        .ok_or_else(|| format!("source targets exceed the {model} range."))
}

// Accept only floating-point conversion noise, never a meaningful fraction of
// a hardware increment. Snap before building the sweep to avoid accumulating it.
fn aligned_value(value: f64, resolution: f64, label: &str, unit: &str) -> Result<Decimal, String> {
    let value_decimal = decimal(value, label)?;
    let increment = decimal(resolution, "source resolution")?;
    let nearest = value_decimal
        .checked_div(increment)
        .and_then(|ratio| ratio.round().checked_mul(increment))
        .ok_or_else(|| format!("{label} is too large for the selected source resolution."))?;
    let residual = (value_decimal - nearest).abs();
    let tolerance = decimal(value.abs() * f64::EPSILON * 4.0, "alignment tolerance")?;
    if residual > tolerance {
        return Err(format!(
            "{label} ({value:e} {unit}) must be a multiple of {resolution:e} {unit} for the selected source range."
        ));
    }
    Ok(nearest)
}

fn quantized_hardware_compliance(
    spec: &SourceSpec,
    model: &str,
    mode: Mode,
    requested: f64,
) -> Result<f64, String> {
    let (minimum, maximum, resolution) = match mode {
        Mode::Iv => (
            spec.current_limit_min_a,
            spec.current_limit_max_a,
            spec.current_limit_resolution_a,
        ),
        Mode::Vi => (
            spec.voltage_limit_min_v,
            spec.voltage_limit_max_v,
            spec.voltage_limit_resolution_v,
        ),
    };
    if requested > maximum {
        return Err(format!(
            "Source protection limit (source_compliance) {requested} exceeds {model} hardware maximum {maximum}."
        ));
    }
    if !resolution.is_finite() || resolution <= 0.0 {
        return Err(format!(
            "{model} has no valid hardware compliance increment for this mode."
        ));
    }
    let requested = requested.max(minimum);
    let ratio = (requested / resolution).ceil();
    Ok((ratio * resolution).min(maximum))
}

fn gcd_i128(mut left: i128, mut right: i128) -> i128 {
    left = left.abs();
    right = right.abs();
    while right != 0 {
        let remainder = left % right;
        left = right;
        right = remainder;
    }
    left
}

fn ceil_to_increment(value: Decimal, increment: Decimal, label: &str) -> Result<Decimal, String> {
    if value <= Decimal::ZERO || increment <= Decimal::ZERO {
        return Err(format!("{label} must be positive."));
    }
    value
        .checked_div(increment)
        .and_then(|ratio| ratio.ceil().checked_mul(increment))
        .ok_or_else(|| format!("{label} is too large for the combined source resolution."))
}

fn decimal_to_f64(value: Decimal, label: &str) -> Result<f64, String> {
    value
        .to_string()
        .parse::<f64>()
        .map_err(|_| format!("{label} is outside f64 range."))
}

fn scaled_decimal(
    value: Decimal,
    part: Decimal,
    total: Decimal,
    label: &str,
) -> Result<Decimal, String> {
    let product = value
        .checked_mul(part)
        .ok_or_else(|| format!("{label} is too large"))?;
    let divided = product
        .checked_div(total)
        .ok_or_else(|| format!("{label} is too large"))?;
    if divided.checked_mul(total) != Some(product) {
        return Err(format!(
            "{label} cannot be represented exactly with this voltage ratio. Choose a target on the combined source grid."
        ));
    }
    Ok(divided)
}
fn scaled_value(value: f64, part: Decimal, total: Decimal, label: &str) -> Result<f64, String> {
    decimal_to_f64(
        scaled_decimal(decimal(value, label)?, part, total, label)?,
        label,
    )
}

/// Exact common lattice for two decimal DACs and a rational voltage split.
/// Reduce the fractions before taking their rational LCM; approximating
/// resolution / ratio would introduce rounding when e.g. ratio = 0.3.
fn split_increment(first: Decimal, second: Decimal, ratio: [u32; 2]) -> Result<Decimal, String> {
    let scale = first.scale().max(second.scale());
    let denominator = 10_i128
        .checked_pow(scale)
        .ok_or("source resolution scale is too large")?;
    let ratio_denominator = i128::from(ratio[0]) + i128::from(ratio[1]);
    let numerator = i128::from(ratio[0]);
    let fraction = |resolution: Decimal, part: i128| -> Result<(i128, i128), String> {
        let n = resolution
            .mantissa()
            .checked_mul(
                10_i128
                    .checked_pow(scale - resolution.scale())
                    .ok_or("source scale overflow")?,
            )
            .and_then(|v| v.checked_mul(ratio_denominator))
            .ok_or("combined source resolution overflow")?;
        let d = denominator
            .checked_mul(part)
            .ok_or("voltage ratio precision is too large")?;
        let divisor = gcd_i128(n, d);
        Ok((n / divisor, d / divisor))
    };
    let (a, b) = fraction(first, numerator)?;
    let (c, d) = fraction(second, ratio_denominator - numerator)?;
    let n = a
        .checked_div(gcd_i128(a, c))
        .and_then(|v| v.checked_mul(c))
        .ok_or("combined source resolution overflow")?;
    let d = gcd_i128(b, d);
    let n = Decimal::try_from_i128_with_scale(n, 0)
        .map_err(|_| "combined source resolution overflow")?;
    let d = Decimal::try_from_i128_with_scale(d, 0)
        .map_err(|_| "combined source resolution overflow")?;
    let increment = n
        .checked_div(d)
        .ok_or("combined source resolution is not representable")?;
    if increment.checked_mul(d) != Some(n) {
        return Err(
            "Voltage ratio cannot be represented exactly by both source resolutions".into(),
        );
    }
    Ok(increment.normalize())
}

fn require_exact_protection(spec: &SourceSpec, model: &str, requested: f64) -> Result<(), String> {
    let actual = quantized_hardware_compliance(spec, model, Mode::Iv, requested)?;
    if actual > requested + f64::EPSILON {
        return Err(format!(
            "Source compliance {requested:e} A cannot be represented exactly by {model}; use {actual:e} A or a lower supported value."
        ));
    }
    Ok(())
}

/// Backward-compatible per-point accumulation count. Never multiply the old
/// average and repeat fields: both describe the same operation now. The legacy
/// alias is used only when repeat is 1; repeat = 0 remains an invalid count.
pub fn readings_per_point(config: &RunConfig) -> u32 {
    if config.repeat != 1 {
        config.repeat
    } else {
        config.average_count
    }
}

/// Resolve hardware-dependent source settings without increasing either limit.
/// Endpoints remain explicit: only representation noise is snapped. The step
/// magnitude rounds up to the next hardware increment; ramping uses one
/// percent of the nominal source range or the sweep step, whichever is larger,
/// capped to the range. No instrument I/O occurs here.
fn prepare_config_single(config: &RunConfig) -> Result<RunConfig, String> {
    let mut prepared = config.clone();
    let readings = readings_per_point(config);
    if readings == 0 || readings > 10_000 {
        return Err("Readings per point (Repeat) must be between 1 and 10000.".into());
    }
    prepared.repeat = readings;
    prepared.average_count = readings;
    let maximum = if config.pattern == Pattern::CustomList {
        for value in &config.custom_points {
            decimal(*value, "custom target")?;
        }
        config
            .custom_points
            .iter()
            .map(|v| v.abs())
            .fold(0.0_f64, f64::max)
    } else {
        decimal(config.start, "start")?;
        decimal(config.stop, "stop")?;
        config.start.abs().max(config.stop.abs())
    };
    let source = source_spec(&config.source_model)?;
    // Validate the meter here as well so UI previews report a bad model before
    // a run is started. `plan` validates its mode-dependent limits below.
    meter_spec(&config.meter_model)?;
    validate_meter_range(config)?;
    let (source_range, range_maximum, resolution) = select_range(
        &source,
        &config.source_model,
        config.mode,
        maximum,
        config.source_compliance,
    )?;
    let increment = decimal(resolution, "source resolution")?;
    let requested = decimal(config.step, "step")?;
    if requested.is_zero() {
        return Err("step must be non-zero.".into());
    }
    let magnitude = match aligned_value(config.step.abs(), resolution, "Step", "SI") {
        Ok(value) => value,
        Err(_) => requested
            .abs()
            .checked_div(increment)
            .and_then(|ratio| ratio.ceil().checked_mul(increment))
            .ok_or("Step is too large for the selected source resolution.")?,
    };
    let step: f64 = magnitude
        .to_string()
        .parse()
        .map_err(|_| "Invalid effective step")?;
    prepared.step = step.copysign(config.step);
    // The 7651 stores a range command *code* (2..6), not an SI range. Use the
    // selected range maximum for its automatic ramp calculation. GS210 keeps
    // its established nominal-range calculation unchanged.
    let automatic_ramp_range = if config.source_model == "YOKOGAWA_7651" {
        range_maximum
    } else {
        source_range
    };
    let ramp =
        (decimal(automatic_ramp_range, "source range")? / Decimal::from(100_u32) / increment)
            .ceil()
            * increment;
    prepared.ramp_step = ramp
        .to_string()
        .parse::<f64>()
        .map_err(|_| "Invalid automatic ramp step")?
        .max(step)
        .min(range_maximum);
    Ok(prepared)
}

/// Validate a profiled source + meter run and create its deterministic sweep sequence.
fn plan_single(config: &RunConfig) -> Result<Plan, String> {
    let prepared = prepare_config_single(config)?;
    let config = &prepared;
    for (value, name) in [
        (config.start, "start"),
        (config.stop, "stop"),
        (config.step, "step"),
        (config.source_limit, "source_limit"),
        (config.compliance, "compliance"),
        (config.settle_s, "settle_s"),
        (config.start_settle_s, "start_settle_s"),
        (config.nplc, "nplc"),
        (config.ramp_step, "ramp_step"),
        (config.ramp_wait_s, "ramp_wait_s"),
    ] {
        decimal(value, name)?;
    }
    if config.repeat == 0 {
        return Err("repeat must be >= 1.".to_owned());
    }
    finite_positive(config.source_limit, "source_limit", None)?;
    finite_positive(config.compliance, "Meter stop limit (compliance)", None)?;
    let source_protection = config.source_compliance.unwrap_or(config.compliance);
    finite_positive(
        source_protection,
        "Source protection limit (source_compliance)",
        None,
    )?;
    finite_positive(config.ramp_step, "ramp_step", None)?;
    finite_positive(config.nplc, "nplc", None)?;
    if config.average_count == 0 || config.average_count > 10_000 {
        return Err("average_count must be between 1 and 10000.".to_owned());
    }
    for (value, name) in [
        (config.settle_s, "settle_s"),
        (config.start_settle_s, "start_settle_s"),
        (config.ramp_wait_s, "ramp_wait_s"),
    ] {
        if !value.is_finite() || value < 0.0 || value > MAX_SECONDS {
            return Err(format!(
                "{name} must be finite and between 0 and {MAX_SECONDS}."
            ));
        }
    }
    let meter = meter_spec(&config.meter_model)?;
    let nplc_allowed = if !meter.nplc_values.is_empty() {
        meter
            .nplc_values
            .iter()
            .any(|value| (*value - config.nplc).abs() < 1e-12)
    } else {
        meter.nplc_min.is_some_and(|minimum| config.nplc >= minimum)
            && meter.nplc_max.is_some_and(|maximum| config.nplc <= maximum)
    };
    if !nplc_allowed {
        return Err(format!(
            "{} does not support nplc={}.",
            config.meter_model, config.nplc
        ));
    }
    let meter_input_limit = match config.mode {
        Mode::Iv => meter.max_input_a,
        Mode::Vi => meter.max_input_v,
    };
    if config.compliance > meter_input_limit + f64::EPSILON {
        let unit = if config.mode == Mode::Iv { "A" } else { "V" };
        return Err(format!(
            "Meter stop limit (compliance) {} {} exceeds {} meter maximum input {} {}.",
            config.compliance, unit, config.meter_model, meter_input_limit, unit
        ));
    }

    // Check range and increments before allocating a potentially large sweep.
    let source = source_spec(&config.source_model)?;
    let maximum = if config.pattern == Pattern::CustomList {
        config
            .custom_points
            .iter()
            .map(|v| v.abs())
            .fold(0.0_f64, f64::max)
    } else {
        config.start.abs().max(config.stop.abs())
    };
    let (source_range, range_maximum, resolution) = select_range(
        &source,
        &config.source_model,
        config.mode,
        maximum,
        config.source_compliance,
    )?;
    let unit = if config.mode == Mode::Vi { "A" } else { "V" };
    let start = if config.pattern != Pattern::CustomList {
        aligned_value(config.start, resolution, "Start", unit)?
    } else {
        Decimal::ZERO
    };
    let stop = if config.pattern != Pattern::CustomList {
        aligned_value(config.stop, resolution, "End", unit)?
    } else {
        Decimal::ZERO
    };
    let step = if config.pattern != Pattern::CustomList {
        aligned_value(config.step, resolution, "Step", unit)?
    } else {
        decimal(config.step, "step")?
    };
    if step.is_zero() {
        return Err("step must be non-zero.".to_owned());
    }
    if config.ramp_step > range_maximum + f64::EPSILON {
        return Err("ramp_step exceeds the selected source range.".to_owned());
    }
    aligned_value(config.ramp_step, resolution, "Ramp step (Settings)", unit)?;
    let base_points = match config.pattern {
        Pattern::CustomList => {
            if config.custom_points.is_empty() {
                return Err("custom_list requires custom_points.".to_owned());
            }
            if config.custom_points.len() > MAX_POINTS {
                return Err(format!("custom_list exceeds {MAX_POINTS} points."));
            }
            config
                .custom_points
                .iter()
                .enumerate()
                .map(|(index, value)| {
                    aligned_value(*value, resolution, &format!("custom_points[{index}]"), unit)
                })
                .collect::<Result<Vec<_>, _>>()?
        }
        Pattern::Linear => forward_points(start, stop, step)?,
        Pattern::RoundTrip => {
            let mut points = forward_points(start, stop, step)?;
            append_joined(&mut points, forward_points(stop, start, step)?);
            points
        }
        Pattern::ZeroCentered => {
            if start.min(stop) > Decimal::ZERO || start.max(stop) < Decimal::ZERO {
                return Err("zero_centered: the interval between Start and End must include zero (an endpoint may be zero). Use linear or round_trip for an interval entirely above or below zero.".into());
            }
            let mut points = Vec::new();
            append_joined(&mut points, forward_points(Decimal::ZERO, stop, step)?);
            append_joined(&mut points, forward_points(stop, start, step)?);
            append_joined(&mut points, forward_points(start, Decimal::ZERO, step)?);
            points
        }
    };
    let base_len = base_points.len();
    let total = base_len;
    if total == 0 || total > MAX_POINTS {
        return Err(format!("sweep must contain 1..={MAX_POINTS} points."));
    }

    let hard_limit = match config.mode {
        Mode::Iv => source.max_abs_v,
        Mode::Vi => source.max_abs_a,
    };
    let maximum_target = base_points
        .iter()
        .map(|point| point.abs())
        .max()
        .unwrap_or(Decimal::ZERO);
    let max_target = maximum_target
        .to_string()
        .parse::<f64>()
        .map_err(|_| "target is outside f64 range.".to_owned())?;
    let effective_limit = config.source_limit.min(hard_limit);
    if max_target > effective_limit + f64::EPSILON {
        return Err(format!(
            "source target {max_target} exceeds effective limit {effective_limit}."
        ));
    }
    let hardware_compliance = quantized_hardware_compliance(
        &source,
        &config.source_model,
        config.mode,
        source_protection,
    )?;
    // GS210 low-voltage ranges use a divider and have a fixed 200 mA
    // current limiter (manual §5.4). Report/program the physical setting,
    // rather than promising an unavailable lower hardware threshold.
    let hardware_compliance = if config.mode == Mode::Iv {
        source
            .voltage_ranges
            .iter()
            .find(|range| range.command_value == source_range)
            .and_then(|range| range.fixed_current_limit_a)
            .unwrap_or(hardware_compliance)
    } else {
        hardware_compliance
    };

    let targets = base_points
        .iter()
        .map(|target| {
            target
                .to_string()
                .parse::<f64>()
                .expect("validated decimal fits f64")
        })
        .collect::<Vec<_>>();
    let mut previous = base_points[0];
    let mut direction = "forward";
    let points = base_points
        .iter()
        .copied()
        .enumerate()
        .map(|(index, target)| {
            let delta = if index == 0 && total > 1 {
                base_points[1 % base_len] - target
            } else {
                target - previous
            };
            if delta > Decimal::ZERO {
                direction = "forward";
            }
            if delta < Decimal::ZERO {
                direction = "backward";
            }
            previous = target;
            SweepPoint {
                target: targets[index % base_len],
                direction: direction.into(),
                repeat_index: 1,
                sweep_index: 1,
                point_index: index + 1,
                total_points: total,
            }
        })
        .collect();
    Ok(Plan {
        points,
        source_range,
        hardware_compliance,
    })
}

fn balanced_base_points(config: &RunConfig, increment: Decimal) -> Result<Vec<Decimal>, String> {
    let unit = "V";
    let points = match config.pattern {
        Pattern::CustomList => {
            if config.custom_points.is_empty() {
                return Err("custom_list requires custom_points.".into());
            }
            if config.custom_points.len() > MAX_POINTS {
                return Err(format!("custom_list exceeds {MAX_POINTS} points."));
            }
            config
                .custom_points
                .iter()
                .enumerate()
                .map(|(index, value)| {
                    aligned_value(
                        *value,
                        decimal_to_f64(increment, "combined source resolution")?,
                        &format!("custom_points[{index}]"),
                        unit,
                    )
                })
                .collect::<Result<Vec<_>, _>>()?
        }
        _ => {
            let resolution = decimal_to_f64(increment, "combined source resolution")?;
            let start = aligned_value(config.start, resolution, "Start", unit)?;
            let stop = aligned_value(config.stop, resolution, "End", unit)?;
            let step = aligned_value(config.step, resolution, "Step", unit)?;
            if step.is_zero() {
                return Err("step must be non-zero.".into());
            }
            match config.pattern {
                Pattern::Linear => forward_points(start, stop, step)?,
                Pattern::RoundTrip => {
                    let mut points = forward_points(start, stop, step)?;
                    append_joined(&mut points, forward_points(stop, start, step)?);
                    points
                }
                Pattern::ZeroCentered => {
                    if start.min(stop) > Decimal::ZERO || start.max(stop) < Decimal::ZERO {
                        return Err("zero_centered: the interval between Start and End must include zero (an endpoint may be zero). Use linear or round_trip for an interval entirely above or below zero.".into());
                    }
                    let mut points = Vec::new();
                    append_joined(&mut points, forward_points(Decimal::ZERO, stop, step)?);
                    append_joined(&mut points, forward_points(stop, start, step)?);
                    append_joined(&mut points, forward_points(start, Decimal::ZERO, step)?);
                    points
                }
                Pattern::CustomList => unreachable!(),
            }
        }
    };
    if points.is_empty() || points.len() > MAX_POINTS {
        return Err(format!("sweep must contain 1..={MAX_POINTS} points."));
    }
    Ok(points)
}

fn sweep_points(base_points: &[Decimal]) -> Result<Vec<SweepPoint>, String> {
    let total = base_points.len();
    let targets = base_points
        .iter()
        .map(|target| decimal_to_f64(*target, "target"))
        .collect::<Result<Vec<_>, _>>()?;
    let mut previous = base_points[0];
    let mut direction = "forward";
    Ok(base_points
        .iter()
        .copied()
        .enumerate()
        .map(|(index, target)| {
            let delta = if index == 0 && total > 1 {
                base_points[1] - target
            } else {
                target - previous
            };
            if delta > Decimal::ZERO {
                direction = "forward";
            }
            if delta < Decimal::ZERO {
                direction = "backward";
            }
            previous = target;
            SweepPoint {
                target: targets[index],
                direction: direction.into(),
                repeat_index: 1,
                sweep_index: 1,
                point_index: index + 1,
                total_points: total,
            }
        })
        .collect())
}

fn prepare_balanced_configs(
    config: &RunConfig,
) -> Result<(RunConfig, RunConfig, RunConfig, Decimal), String> {
    let second = config
        .balanced_source
        .as_ref()
        .ok_or("balanced source configuration is missing.")?;
    if config.mode != Mode::Iv {
        return Err("Balanced source operation supports V-I (voltage source) mode only.".into());
    }
    if second.ratio.iter().any(|part| *part == 0 || *part > 1000) {
        return Err(
            "Voltage ratio requires two integers from 1 to 1000, such as 1:1 or 2:1.".into(),
        );
    }
    let part1 = Decimal::from(second.ratio[0]);
    let part2 = Decimal::from(second.ratio[1]);
    let total = part1 + part2;
    let fraction1 = second.ratio[0] as f64 / (second.ratio[0] as f64 + second.ratio[1] as f64);
    let fraction2 = second.ratio[1] as f64 / (second.ratio[0] as f64 + second.ratio[1] as f64);
    finite_positive(second.source_compliance, "Second source compliance", None)?;
    let primary_protection = config.source_compliance.unwrap_or(config.compliance);
    finite_positive(primary_protection, "Source compliance", None)?;

    // Select both ranges from the individual weighted differential targets before
    // accepting any endpoint. This is what makes a differential target exact
    // for *both* instruments rather than merely for source 1.
    let maximum = if config.pattern == Pattern::CustomList {
        for value in &config.custom_points {
            decimal(*value, "custom target")?;
        }
        config
            .custom_points
            .iter()
            .map(|value| value.abs())
            .fold(0.0_f64, f64::max)
    } else {
        decimal(config.start, "start")?;
        decimal(config.stop, "stop")?;
        config.start.abs().max(config.stop.abs())
    };
    let first_spec = source_spec(&config.source_model)?;
    let second_spec = source_spec(&second.source_model)?;
    require_exact_protection(&first_spec, &config.source_model, primary_protection)?;
    require_exact_protection(&second_spec, &second.source_model, second.source_compliance)?;
    let (_, first_range_max, first_resolution) = select_range(
        &first_spec,
        &config.source_model,
        Mode::Iv,
        maximum * fraction1,
        Some(primary_protection),
    )?;
    let (_, second_range_max, second_resolution) = select_range(
        &second_spec,
        &second.source_model,
        Mode::Iv,
        maximum * fraction2,
        Some(second.source_compliance),
    )?;
    let common_increment = split_increment(
        decimal(first_resolution, "first source resolution")?,
        decimal(second_resolution, "second source resolution")?,
        second.ratio,
    )?;
    let requested_step = decimal(config.step, "step")?;
    if requested_step.is_zero() {
        return Err("step must be non-zero.".into());
    }
    let step_magnitude = ceil_to_increment(requested_step.abs(), common_increment, "Step")?;
    let step = if requested_step.is_sign_negative() {
        -step_magnitude
    } else {
        step_magnitude
    };

    let mut first = config.clone();
    let mut second_config = config.clone();
    first.balanced_source = None;
    second_config.balanced_source = None;
    first.source_compliance = Some(primary_protection);
    second_config.source_model = second.source_model.clone();
    second_config.source_compliance = Some(second.source_compliance);
    for (name, first_value, second_value) in [
        ("start", config.start, -config.start),
        ("stop", config.stop, -config.stop),
    ] {
        let first_half = scaled_value(first_value, part1, total, name)?;
        let second_half = scaled_value(second_value, part2, total, name)?;
        if name == "start" {
            first.start = first_half;
            second_config.start = second_half;
        } else {
            first.stop = first_half;
            second_config.stop = second_half;
        }
    }
    first.step = decimal_to_f64(
        scaled_decimal(step, part1, total, "individual step")?,
        "individual step",
    )?;
    second_config.step = decimal_to_f64(
        scaled_decimal(step, part2, total, "individual step")?,
        "individual step",
    )?;
    first.source_limit = config.source_limit * fraction1;
    second_config.source_limit = config.source_limit * fraction2;
    first.custom_points = config
        .custom_points
        .iter()
        .map(|value| scaled_value(*value, part1, total, "custom target"))
        .collect::<Result<Vec<_>, _>>()?;
    second_config.custom_points = config
        .custom_points
        .iter()
        .map(|value| scaled_value(-*value, part2, total, "custom target"))
        .collect::<Result<Vec<_>, _>>()?;

    let prepared_first = prepare_config_single(&first)?;
    let prepared_second = prepare_config_single(&second_config)?;
    let candidate_ramp = decimal(prepared_first.ramp_step, "first ramp step")?
        .checked_mul(total)
        .and_then(|v| v.checked_div(part1))
        .ok_or("First source ramp overflow")?
        .min(
            decimal(prepared_second.ramp_step, "second ramp step")?
                .checked_mul(total)
                .and_then(|v| v.checked_div(part2))
                .ok_or("Second source ramp overflow")?,
        );
    let ramp = ceil_to_increment(candidate_ramp, common_increment, "Ramp step")?;
    let first_ramp = decimal_to_f64(
        scaled_decimal(ramp, part1, total, "first ramp step")?,
        "first ramp step",
    )?;
    let second_ramp = decimal_to_f64(
        scaled_decimal(ramp, part2, total, "second ramp step")?,
        "second ramp step",
    )?;
    if first_ramp > first_range_max + f64::EPSILON || second_ramp > second_range_max + f64::EPSILON
    {
        return Err("The combined source resolution cannot represent a safe ramp step for both selected ranges.".into());
    }

    let mut differential = config.clone();
    differential.step = decimal_to_f64(step, "combined step")?;
    differential.ramp_step = decimal_to_f64(ramp, "combined ramp step")?;
    differential.repeat = prepared_first.repeat;
    differential.average_count = prepared_first.average_count;

    let mut first = prepared_first;
    let mut second_config = prepared_second;
    first.ramp_step = first_ramp;
    second_config.ramp_step = second_ramp;
    Ok((differential, first, second_config, common_increment))
}

/// Resolve a configuration for display without discarding its differential
/// semantics. Existing single-source configurations are unchanged.
pub fn prepare_config(config: &RunConfig) -> Result<RunConfig, String> {
    if config.balanced_source.is_some() {
        prepare_balanced_configs(config).map(|(differential, _, _, _)| differential)
    } else {
        prepare_config_single(config)
    }
}

/// Build the two exact source plans for a balanced differential V-I sweep.
pub fn balanced_plan(config: &RunConfig) -> Result<BalancedPlan, String> {
    if config.balanced_source.is_none() {
        return Err("balanced source configuration is not enabled.".into());
    }
    let (differential, config1, config2, common_increment) = prepare_balanced_configs(config)?;
    let plan1 = plan_single(&config1)?;
    let plan2 = plan_single(&config2)?;
    let base_points = balanced_base_points(&differential, common_increment)?;
    let maximum = base_points
        .iter()
        .map(|value| value.abs())
        .max()
        .unwrap_or(Decimal::ZERO);
    let source_limit = decimal(differential.source_limit, "source_limit")?;
    if maximum > source_limit {
        return Err(format!(
            "source target {} exceeds effective limit {}.",
            maximum, differential.source_limit
        ));
    }
    let points = sweep_points(&base_points)?;
    if plan1.points.len() != points.len() || plan2.points.len() != points.len() {
        return Err("Balanced source plans do not have matching point counts.".into());
    }
    Ok(BalancedPlan {
        ratio: config.balanced_source.as_ref().unwrap().ratio,
        differential_resolution: decimal_to_f64(common_increment, "differential resolution")?,
        config1,
        config2,
        plan1,
        plan2,
        points,
    })
}

/// Validate a profiled source + meter run and create its deterministic sweep sequence.
pub fn plan(config: &RunConfig) -> Result<Plan, String> {
    if config.balanced_source.is_some() {
        let balanced = balanced_plan(config)?;
        return Ok(Plan {
            points: balanced.points,
            source_range: balanced.plan1.source_range,
            hardware_compliance: balanced.plan1.hardware_compliance,
        });
    }
    plan_single(config)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Record {
    /// UUID shared by the analysis CSV, raw CSV and run manifest.
    #[serde(default)]
    pub run_id: String,
    pub timestamp: String,
    pub elapsed_s: f64,
    pub point: SweepPoint,
    pub mode: Mode,
    pub pattern: Pattern,
    pub source_set: f64,
    pub source_readback: Option<f64>,
    pub meter_value: f64,
    /// Display metadata only; CSV always preserves the unrounded SI values.
    #[serde(default)]
    pub source_resolution: Option<f64>,
    #[serde(default)]
    pub meter_resolution: Option<f64>,
    #[serde(default)]
    pub meter_range: Option<f64>,
    pub compliance: bool,
    pub status: String,
    pub instrument_status: String,
    pub source_resource: String,
    pub meter_resource: String,
    /// Requested and accepted individual meter readings for this point.
    #[serde(default)]
    pub readings_requested: u32,
    #[serde(default)]
    pub readings_completed: Option<u32>,
    /// Sample standard deviation (ddof=1) of accepted physical meter readings.
    #[serde(default)]
    pub meter_stddev: Option<f64>,
    /// Explicit flags for analysis.  `status` remains available to IPC clients.
    #[serde(default)]
    pub quality_flags: String,
}

impl Record {
    /// Exact native CSV field values for export and the saved-data inspector.
    pub fn native_csv_values(&self) -> [String; CSV_FIELDS.len()] {
        let mut fields = std::array::from_fn(|_| String::new());
        fill_native_csv_values(self, &mut fields);
        fields
    }

    pub fn electrical_values(&self) -> (f64, f64, &'static str, &'static str) {
        let source = self.source_readback.unwrap_or(self.source_set);
        let origin = if self.source_readback.is_some() {
            "source_readback"
        } else {
            "source_setpoint"
        };
        if self.mode == Mode::Iv {
            (source, self.meter_value, origin, "meter")
        } else {
            (self.meter_value, source, "meter", origin)
        }
    }
}

pub struct CsvWriter {
    pub path: PathBuf,
    writer: csv::Writer<std::fs::File>,
    fields: [String; CSV_FIELDS.len()],
}

impl CsvWriter {
    pub fn create(path: &Path) -> Result<Self, String> {
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .map_err(|e| format!("cannot create {}: {e}", path.display()))?;
        let mut writer = csv::Writer::from_writer(file);
        writer
            .write_record(CSV_FIELDS)
            .map_err(|e| format!("cannot write CSV header: {e}"))?;
        writer
            .flush()
            .map_err(|e| format!("cannot flush CSV header: {e}"))?;
        Ok(Self {
            path: path.to_path_buf(),
            writer,
            fields: std::array::from_fn(|_| String::new()),
        })
    }
    pub fn append(&mut self, record: &Record) -> Result<(), String> {
        fill_native_csv_values(record, &mut self.fields);
        self.writer
            .write_record(&self.fields)
            .map_err(|e| format!("cannot write CSV row: {e}"))?;
        self.writer
            .flush()
            .map_err(|e| format!("cannot flush CSV row: {e}"))
    }
}

fn fill_native_csv_values(record: &Record, fields: &mut [String; CSV_FIELDS.len()]) {
    let (voltage, current, voltage_origin, current_origin) = record.electrical_values();
    for (index, value) in [
        (3, record.elapsed_s),
        (6, record.point.target),
        (8, voltage),
        (9, current),
    ] {
        shortest_sci_into(&mut fields[index], value);
    }
    for (index, value) in [
        (1, record.point.point_index),
        (4, record.point.sweep_index as usize),
        (12, record.readings_requested as usize),
    ] {
        fields[index].clear();
        write!(&mut fields[index], "{value}").expect("writing to String");
    }
    for (index, value) in [
        (0, record.run_id.as_str()),
        (2, record.timestamp.as_str()),
        (5, record.point.direction.as_str()),
        (7, if record.mode == Mode::Iv { "V" } else { "A" }),
        (10, voltage_origin),
        (11, current_origin),
        (
            13,
            record
                .readings_completed
                .map(|value| value.to_string())
                .as_deref()
                .unwrap_or(""),
        ),
        (
            14,
            record
                .meter_stddev
                .map(|value| format!("{value:e}"))
                .as_deref()
                .unwrap_or(""),
        ),
        (15, if record.mode == Mode::Iv { "A" } else { "V" }),
        (16, if record.compliance { "true" } else { "false" }),
        (17, record.quality_flags.as_str()),
    ] {
        fields[index].clear();
        fields[index].push_str(value);
    }
}

fn shortest_sci_into(buffer: &mut String, value: f64) {
    buffer.clear();
    write!(buffer, "{value:e}").expect("writing to String");
}

/// A deterministic 1 kΩ open-load simulation for GUI and persistence verification.
pub fn simulate_record(config: &RunConfig, point: &SweepPoint, elapsed_s: f64) -> Record {
    let (source_set, meter_value, source_readback) = match config.mode {
        Mode::Iv => (point.target, point.target / 1_000.0, Some(point.target)),
        Mode::Vi => (point.target, point.target * 1_000.0, Some(point.target)),
    };
    Record {
        run_id: String::new(),
        timestamp: "1970-01-01T00:00:00Z".into(),
        elapsed_s,
        point: point.clone(),
        mode: config.mode,
        pattern: config.pattern,
        source_set,
        source_readback,
        meter_value,
        source_resolution: instrument_specs::simulation_display_precision(config.mode)
            .ok()
            .and_then(|p| p.0),
        meter_resolution: instrument_specs::simulation_display_precision(config.mode)
            .ok()
            .and_then(|p| p.1),
        meter_range: None,
        compliance: false,
        status: "ok".into(),
        instrument_status: String::new(),
        source_resource: "source.simulated".into(),
        meter_resource: "meter.simulated".into(),
        readings_requested: config.average_count,
        readings_completed: None,
        meter_stddev: None,
        quality_flags: "simulated".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn documented_meter_ranges_are_available_for_every_supported_profile() {
        let current_cases = [
            ("AGILENT_34401A", vec![0.01, 0.1, 1.0, 3.0]),
            ("AGILENT_34411A", vec![0.0001, 0.001, 0.01, 0.1, 1.0, 3.0]),
            ("KEYSIGHT_34411A", vec![0.0001, 0.001, 0.01, 0.1, 1.0, 3.0]),
            (
                "KEYSIGHT_34465A",
                vec![0.000001, 0.00001, 0.0001, 0.001, 0.01, 0.1, 1.0, 3.0, 10.0],
            ),
            (
                "SIMULATED_METER",
                vec![0.000001, 0.00001, 0.0001, 0.001, 0.01, 0.1, 1.0],
            ),
        ];
        for (model, expected) in current_cases {
            assert_eq!(meter_ranges(model, Mode::Iv).unwrap(), expected, "{model}");
        }
        assert_eq!(
            meter_ranges("KEYSIGHT_34465A", Mode::Vi).unwrap(),
            vec![0.1, 1.0, 10.0, 100.0, 1000.0]
        );
        for model in [
            "AGILENT_34401A",
            "AGILENT_34411A",
            "KEYSIGHT_34411A",
            "SIMULATED_METER",
        ] {
            assert!(
                !meter_ranges(model, Mode::Vi).unwrap().is_empty(),
                "{model}"
            );
        }
        assert!(meter_ranges("ADCMT_7461A", Mode::Iv).unwrap().is_empty());
        assert!(meter_ranges("ADCMT_7461A", Mode::Vi).unwrap().is_empty());
    }

    #[test]
    fn selected_fixed_range_is_independent_for_each_measurement_mode() {
        let config = RunConfig {
            meter_current_range: Some(0.001),
            meter_voltage_range: Some(10.0),
            ..RunConfig::default()
        };
        assert_eq!(selected_meter_range(&config), Some(0.001));
        let voltage_config = RunConfig {
            mode: Mode::Vi,
            ..config
        };
        assert_eq!(selected_meter_range(&voltage_config), Some(10.0));
    }

    #[test]
    fn fixed_meter_range_validation_requires_a_documented_positive_preset() {
        let mut config = RunConfig {
            meter_current_range: Some(0.001),
            ..RunConfig::default()
        };
        assert!(validate_meter_range(&config).is_ok());
        config.meter_current_range = Some(0.0002);
        assert!(
            validate_meter_range(&config)
                .unwrap_err()
                .contains("does not support")
        );
        config.meter_current_range = Some(0.0);
        assert!(
            validate_meter_range(&config)
                .unwrap_err()
                .contains("positive")
        );
        config.meter_current_range = Some(f64::NAN);
        assert!(
            validate_meter_range(&config)
                .unwrap_err()
                .contains("finite")
        );
        config.meter_current_range = Some(0.001);
        config.meter_model = "ADCMT_7461A".into();
        assert!(
            validate_meter_range(&config)
                .unwrap_err()
                .contains("no verified fixed current range support")
        );
    }

    #[test]
    fn inactive_fixed_range_does_not_change_the_active_mode_validation() {
        let config = RunConfig {
            meter_current_range: Some(0.001),
            meter_voltage_range: Some(0.123),
            ..RunConfig::default()
        };
        assert!(validate_meter_range(&config).is_ok());
        let voltage_config = RunConfig {
            mode: Mode::Vi,
            ..config
        };
        assert!(validate_meter_range(&voltage_config).is_err());
    }

    #[test]
    fn range_verification_only_enforces_fixed_requests() {
        assert!(verify_meter_range(None, f64::NAN).is_ok());
        assert!(verify_meter_range(Some(0.001), 0.001).is_ok());
        assert!(verify_meter_range(Some(0.001), 0.01).is_err());
        assert!(verify_meter_range(Some(0.001), 0.0).is_err());
    }

    #[test]
    fn old_configs_deserialize_with_auto_meter_ranges() {
        let config = RunConfig::default();
        let mut encoded = serde_json::to_value(config).unwrap();
        let object = encoded.as_object_mut().unwrap();
        object.remove("meter_voltage_range");
        object.remove("meter_current_range");
        let decoded: RunConfig = serde_json::from_value(encoded).unwrap();
        assert_eq!(decoded.meter_voltage_range, None);
        assert_eq!(decoded.meter_current_range, None);
    }

    #[test]
    fn run_config_experiment_roundtrips_and_empty_default_stays_omitted() {
        let config = RunConfig::default();
        let encoded = serde_json::to_value(&config).unwrap();
        assert!(encoded.get("experiment").is_none());
        let mut config = config;
        config
            .experiment
            .insert("sample_id".into(), serde_json::json!("S-42"));
        config
            .experiment
            .insert("units".into(), serde_json::json!({"length": "um"}));
        let decoded: RunConfig =
            serde_json::from_value(serde_json::to_value(&config).unwrap()).unwrap();
        assert_eq!(decoded.experiment, config.experiment);
    }

    #[test]
    fn balanced_voltage_plans_keep_the_requested_differential_targets_for_all_sources() {
        for first_model in ["YOKOGAWA_GS210", "YOKOGAWA_7651"] {
            for second_model in ["YOKOGAWA_GS210", "YOKOGAWA_7651"] {
                let first_compliance = if first_model == "YOKOGAWA_7651" {
                    5e-3
                } else {
                    1e-3
                };
                let config = RunConfig {
                    source_model: first_model.into(),
                    pattern: Pattern::Linear,
                    start: -200e-6,
                    stop: 200e-6,
                    // This is deliberately not on the 20 uV combined grid.
                    step: 30e-6,
                    ramp_step: 30e-6,
                    source_limit: 500e-6,
                    compliance: first_compliance,
                    source_compliance: Some(first_compliance),
                    balanced_source: Some(BalancedSourceConfig {
                        source_model: second_model.into(),
                        source_compliance: 5e-3,
                        ..Default::default()
                    }),
                    ..RunConfig::default()
                };
                let prepared = prepare_config(&config).unwrap();
                assert!(prepared.balanced_source.is_some());
                let expected_step =
                    if first_model == "YOKOGAWA_7651" && second_model == "YOKOGAWA_7651" {
                        30e-6
                    } else {
                        40e-6
                    };
                assert_eq!(prepared.step, expected_step);
                let balanced = balanced_plan(&config).unwrap();
                let differential: Vec<_> =
                    balanced.points.iter().map(|point| point.target).collect();
                assert_eq!(differential.first(), Some(&-200e-6));
                assert_eq!(differential.last(), Some(&200e-6));
                assert!(
                    differential
                        .windows(2)
                        .all(|pair| pair[1] - pair[0] <= expected_step + 1e-15)
                );
                for ((target, first), second) in differential
                    .iter()
                    .zip(balanced.plan1.points.iter())
                    .zip(balanced.plan2.points.iter())
                {
                    assert_eq!(first.target, *target / 2.0);
                    assert_eq!(second.target, -*target / 2.0);
                }
                assert!(balanced.config1.balanced_source.is_none());
                assert!(balanced.config2.balanced_source.is_none());
                assert_eq!(balanced.config1.source_compliance, Some(first_compliance));
                assert_eq!(balanced.config2.source_compliance, Some(5e-3));
                assert_eq!(balanced.config1.source_limit, 250e-6);
                assert_eq!(balanced.config2.source_limit, 250e-6);
            }
        }
    }

    #[test]
    fn balanced_integer_ratios_are_exact_and_preserve_differential_voltage() {
        for weights in [[1, 1], [2, 1], [1, 2], [2, 2], [3, 7]] {
            let config = RunConfig {
                pattern: Pattern::Linear,
                start: -0.003,
                stop: 0.003,
                step: 0.0015,
                source_limit: 0.01,
                source_compliance: Some(0.001),
                balanced_source: Some(BalancedSourceConfig {
                    ratio: weights,
                    ..Default::default()
                }),
                ..Default::default()
            };
            let planned = balanced_plan(&config).unwrap();
            let a = Decimal::from(weights[0]);
            let b = Decimal::from(weights[1]);
            for ((target, first), second) in planned
                .points
                .iter()
                .zip(&planned.plan1.points)
                .zip(&planned.plan2.points)
            {
                let differential = decimal(target.target, "target").unwrap();
                let first = decimal(first.target, "first").unwrap();
                let second = decimal(second.target, "second").unwrap();
                assert_eq!(first - second, differential);
                assert_eq!(first * b, -second * a);
            }
        }
        let mut config = RunConfig {
            balanced_source: Some(BalancedSourceConfig {
                ratio: [2, 1],
                ..Default::default()
            }),
            ..Default::default()
        };
        assert!(
            balanced_plan(&config).is_err(),
            "unrepresentable +/-1mV endpoints must not be silently rounded for2:1"
        );
        for invalid in [[0, 1], [1, 0], [1001, 1]] {
            config.balanced_source.as_mut().unwrap().ratio = invalid;
            assert!(
                balanced_plan(&config)
                    .unwrap_err()
                    .contains("Voltage ratio")
            );
        }
    }

    #[test]
    fn balanced_sources_reject_unrepresentable_endpoints_and_custom_targets() {
        let balanced = BalancedSourceConfig::default();
        let endpoints = RunConfig {
            pattern: Pattern::Linear,
            start: -15e-6,
            stop: 20e-6,
            step: 20e-6,
            source_limit: 100e-6,
            source_compliance: Some(1e-3),
            balanced_source: Some(balanced.clone()),
            ..RunConfig::default()
        };
        assert!(balanced_plan(&endpoints).unwrap_err().contains("Start"));
        let custom = RunConfig {
            pattern: Pattern::CustomList,
            custom_points: vec![0.0, 15e-6],
            step: 20e-6,
            source_limit: 100e-6,
            source_compliance: Some(1e-3),
            balanced_source: Some(balanced),
            ..RunConfig::default()
        };
        assert!(
            balanced_plan(&custom)
                .unwrap_err()
                .contains("custom_points[1]")
        );
    }

    #[test]
    fn balanced_sources_validate_mode_limits_and_second_protection_before_planning() {
        let mut config = RunConfig {
            mode: Mode::Vi,
            balanced_source: Some(BalancedSourceConfig::default()),
            ..RunConfig::default()
        };
        assert!(balanced_plan(&config).unwrap_err().contains("V-I"));

        config.mode = Mode::Iv;
        config.balanced_source.as_mut().unwrap().source_compliance = 31.0;
        assert!(
            balanced_plan(&config)
                .unwrap_err()
                .contains("hardware maximum")
        );

        config.balanced_source.as_mut().unwrap().source_compliance = 1e-3;
        config.source_compliance = Some(1.5e-3);
        assert!(
            balanced_plan(&config)
                .unwrap_err()
                .contains("cannot be represented exactly")
        );

        config.source_compliance = Some(1e-3);
        config.start = -200e-6;
        config.stop = 200e-6;
        config.step = 20e-6;
        config.source_limit = 100e-6;
        config.source_compliance = Some(1e-3);
        assert!(
            balanced_plan(&config)
                .unwrap_err()
                .contains("effective limit")
        );
    }

    #[test]
    fn unit_conversion_noise_is_snapped_before_sweep_generation() {
        let config = RunConfig {
            mode: Mode::Vi,
            pattern: Pattern::Linear,
            start: -100.0 * 1e-6,
            stop: 1e-6,
            step: 0.1 * 1e-6,
            ramp_step: 0.1 * 1e-6,
            source_limit: 0.1,
            compliance: 1.0,
            ..RunConfig::default()
        };
        let result = plan(&config).unwrap();
        assert_eq!(result.points.len(), 1011);
        assert_eq!(result.points.first().unwrap().target, -0.0001);
        assert_eq!(result.points.last().unwrap().target, 1e-6);
        assert_eq!(result.points[1000].target, 0.0);
    }

    #[test]
    fn high_current_settings_resolve_step_and_ramp_without_raising_limits() {
        let mut config = RunConfig {
            mode: Mode::Vi,
            start: -0.1,
            stop: 0.1,
            step: 1e-7,
            ramp_step: 1e-7,
            source_limit: 0.1,
            compliance: 1.0,
            ..RunConfig::default()
        };
        let prepared = prepare_config(&config).unwrap();
        assert_eq!(prepared.step, 1e-6);
        assert_eq!(prepared.ramp_step, 0.001);
        assert_eq!(prepared.source_limit, config.source_limit);
        assert_eq!(prepared.compliance, config.compliance);
        assert_eq!(plan(&config).unwrap().points.len(), 400001);
        config.step = 1.5e-6;
        assert_eq!(prepare_config(&config).unwrap().step, 2e-6);
        config.step = -100.0 * 1e-6;
        assert_eq!(prepare_config(&config).unwrap().step, -0.0001);
        config.step = 1.0;
        assert_eq!(prepare_config(&config).unwrap().ramp_step, 0.12);
        config.source_limit = 0.001;
        assert!(
            plan(&config)
                .unwrap_err()
                .contains("exceeds effective limit")
        );
        config.start = -0.1000005;
        assert!(plan(&config).unwrap_err().starts_with("Start "));
    }

    #[test]
    fn alignment_tolerance_rejects_real_sub_resolution_values() {
        for value in [0.1e-6, 1.5e-6, 1e-20, 1e-6 + 1e-15, 1e28] {
            assert!(aligned_value(value, 1e-6, "Step", "A").is_err());
        }
        assert_eq!(
            aligned_value(0.0, 1e-6, "Start", "A").unwrap(),
            Decimal::ZERO
        );
        assert_eq!(
            aligned_value(100.0 * 1e-6, 1e-6, "Start", "A").unwrap(),
            decimal(1e-4, "expected").unwrap()
        );
    }

    #[test]
    fn defaults_create_decimal_zero_centered_order() {
        let plan = plan(&RunConfig::default()).unwrap();
        let targets = plan
            .points
            .iter()
            .map(|point| point.target)
            .collect::<Vec<_>>();
        assert_eq!(targets.first(), Some(&0.0));
        assert_eq!(targets[10], 0.001);
        assert_eq!(targets[11], 0.0009);
        assert_eq!(targets.last(), Some(&0.0));
        assert_eq!(plan.points.len(), 41);
    }

    #[test]
    fn zero_centered_respects_both_endpoints_instead_of_mirroring_the_amplitude() {
        for (start, stop, expected) in [
            (
                -1.6,
                0.0,
                vec![0.0, -0.4, -0.8, -1.2, -1.6, -1.2, -0.8, -0.4, 0.0],
            ),
            (0.0, -0.8, vec![0.0, -0.4, -0.8, -0.4, 0.0]),
            (0.8, 0.0, vec![0.0, 0.4, 0.8, 0.4, 0.0]),
            (-0.4, 0.8, vec![0.0, 0.4, 0.8, 0.4, 0.0, -0.4, 0.0]),
            (0.8, -0.4, vec![0.0, -0.4, 0.0, 0.4, 0.8, 0.4, 0.0]),
            (0.0, 0.0, vec![0.0]),
        ] {
            let config = RunConfig {
                start,
                stop,
                step: 0.4,
                source_limit: 2.0,
                ..RunConfig::default()
            };
            let targets: Vec<_> = plan(&config)
                .unwrap()
                .points
                .iter()
                .map(|p| p.target)
                .collect();
            assert_eq!(targets, expected, "Start={start}, End={stop}");
            assert!(
                targets
                    .iter()
                    .all(|v| *v >= start.min(stop) && *v <= start.max(stop))
            );
        }
    }

    #[test]
    fn zero_centered_requires_zero_within_the_requested_interval() {
        for (start, stop) in [(0.4, 0.8), (-0.8, -0.4), (0.8, 0.4), (-0.4, -0.8)] {
            let config = RunConfig {
                start,
                stop,
                step: 0.1,
                source_limit: 2.0,
                ..RunConfig::default()
            };
            assert!(plan(&config).unwrap_err().contains("must include zero"));
        }
    }

    #[test]
    fn sweep_bounds_and_endpoints_hold_for_all_profiled_sources_and_modes() {
        for source_model in ["YOKOGAWA_GS210", "YOKOGAWA_7651"] {
            for mode in [Mode::Iv, Mode::Vi] {
                for pattern in [Pattern::Linear, Pattern::RoundTrip, Pattern::ZeroCentered] {
                    for start in -3..=3 {
                        for stop in -3..=3 {
                            for step in [1, 2, 5] {
                                let config = RunConfig {
                                    source_model: source_model.into(),
                                    mode,
                                    pattern,
                                    start: format!("{start}e-4").parse().unwrap(),
                                    stop: format!("{stop}e-4").parse().unwrap(),
                                    step: format!("{step}e-4").parse().unwrap(),
                                    source_limit: 0.001,
                                    compliance: 0.001,
                                    ..RunConfig::default()
                                };
                                let result = plan(&config);
                                if pattern == Pattern::ZeroCentered
                                    && (start.min(stop) > 0 || start.max(stop) < 0)
                                {
                                    assert!(result.unwrap_err().contains("must include zero"));
                                    continue;
                                }
                                let result = result.unwrap();
                                let targets: Vec<_> =
                                    result.points.iter().map(|p| p.target).collect();
                                assert!(
                                    targets.iter().all(|v| *v >= config.start.min(config.stop)
                                        && *v <= config.start.max(config.stop)),
                                    "{config:?}: {targets:?}"
                                );
                                let first = if pattern == Pattern::ZeroCentered {
                                    0.0
                                } else {
                                    config.start
                                };
                                let last = match pattern {
                                    Pattern::Linear => config.stop,
                                    Pattern::RoundTrip => config.start,
                                    _ => 0.0,
                                };
                                assert_eq!(targets.first(), Some(&first));
                                assert_eq!(targets.last(), Some(&last));
                                assert!(
                                    targets.windows(2).all(|w| w[0] != w[1]
                                        && (w[1] - w[0]).abs() <= config.step + 1e-12)
                                );
                                assert!(
                                    targets.contains(&config.start)
                                        && targets.contains(&config.stop)
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn nondivisible_steps_include_exact_turning_and_final_endpoints() {
        for (pattern, expected) in [
            (Pattern::Linear, vec![-0.8, -0.2, 0.4, 0.8]),
            (
                Pattern::RoundTrip,
                vec![-0.8, -0.2, 0.4, 0.8, 0.2, -0.4, -0.8],
            ),
            (
                Pattern::ZeroCentered,
                vec![0.0, 0.6, 0.8, 0.2, -0.4, -0.8, -0.2, 0.0],
            ),
        ] {
            let config = RunConfig {
                pattern,
                start: -0.8,
                stop: 0.8,
                step: 0.6,
                source_limit: 1.0,
                ..RunConfig::default()
            };
            let targets: Vec<_> = plan(&config)
                .unwrap()
                .points
                .iter()
                .map(|p| p.target)
                .collect();
            assert_eq!(targets, expected, "{pattern:?}");
        }
    }

    #[test]
    fn repeats_average_each_target_without_repeating_the_sweep() {
        let mut config = RunConfig {
            start: 0.0,
            stop: 0.0002,
            step: 0.0001,
            pattern: Pattern::RoundTrip,
            repeat: 2,
            ..RunConfig::default()
        };
        let sweep_plan = plan(&config).unwrap();
        assert_eq!(
            sweep_plan
                .points
                .iter()
                .map(|point| point.target)
                .collect::<Vec<_>>(),
            vec![0.0, 0.0001, 0.0002, 0.0001, 0.0]
        );
        assert!(sweep_plan.points.iter().all(|p| p.repeat_index == 1));
        assert_eq!(prepare_config(&config).unwrap().average_count, 2);
        config.average_count = 5;
        assert_eq!(prepare_config(&config).unwrap().average_count, 2);
        config.pattern = Pattern::CustomList;
        config.custom_points = vec![0.0, 0.0001, -0.0001];
        config.repeat = 1;
        assert_eq!(plan(&config).unwrap().points.len(), 3);
        assert_eq!(prepare_config(&config).unwrap().average_count, 5);
        config.repeat = 0;
        assert!(plan(&config).is_err());
        config.repeat = 10001;
        assert!(plan(&config).is_err());
    }

    #[test]
    fn streaming_directions_match_original_decimal_algorithm() {
        for values in [
            vec![0.0],
            vec![0.0001, 0.0],
            vec![0.0, 0.0, 0.0001, 0.0001, -0.0001, 0.0],
        ] {
            for repeat in [1, 3] {
                let config = RunConfig {
                    pattern: Pattern::CustomList,
                    custom_points: values.clone(),
                    repeat,
                    ..RunConfig::default()
                };
                let result = plan(&config).unwrap();
                let decimals = values
                    .iter()
                    .map(|value| decimal(*value, "test target").unwrap())
                    .collect::<Vec<_>>();
                assert_eq!(
                    result
                        .points
                        .iter()
                        .map(|point| point.direction.clone())
                        .collect::<Vec<_>>(),
                    directions(&decimals)
                );
                for (index, point) in result.points.iter().enumerate() {
                    assert_eq!(point.target, values[index % values.len()]);
                    assert_eq!(point.repeat_index, 1);
                    assert_eq!(point.point_index, index + 1);
                    assert_eq!(point.total_points, decimals.len());
                }
            }
        }
    }

    #[test]
    fn rejects_invalid_bounds_and_unavailable_nplc() {
        assert!(
            plan(&RunConfig {
                source_limit: f64::NAN,
                ..RunConfig::default()
            })
            .is_err()
        );
        assert!(
            plan(&RunConfig {
                source_limit: 0.0009,
                ..RunConfig::default()
            })
            .is_err()
        );
        assert!(
            plan(&RunConfig {
                nplc: 0.5,
                ..RunConfig::default()
            })
            .is_err()
        );
        assert!(
            plan(&RunConfig {
                settle_s: 3_601.0,
                ..RunConfig::default()
            })
            .is_err()
        );
        assert!(
            plan(&RunConfig {
                average_count: 10_001,
                ..RunConfig::default()
            })
            .is_err()
        );
    }

    #[test]
    fn model_profiles_preserve_legacy_defaults_and_accept_reviewed_nplc_rules() {
        let mut legacy = serde_json::to_value(RunConfig::default()).unwrap();
        legacy.as_object_mut().unwrap().remove("source_model");
        legacy.as_object_mut().unwrap().remove("meter_model");
        let restored: RunConfig = serde_json::from_value(legacy).unwrap();
        assert_eq!(restored.source_model, "YOKOGAWA_GS210");
        assert_eq!(restored.meter_model, "AGILENT_34411A");
        assert!(plan(&restored).is_ok());

        let config = RunConfig {
            meter_model: "AGILENT_34401A".into(),
            nplc: 0.02,
            ..RunConfig::default()
        };
        assert!(plan(&config).is_ok());

        let config = RunConfig {
            meter_model: "ADCMT_7461A".into(),
            nplc: 2.5,
            ..RunConfig::default()
        };
        assert!(plan(&config).is_ok());
    }

    #[test]
    fn model_profiles_reject_unknown_models_invalid_nplc_and_meter_input_compliance() {
        let config = RunConfig {
            source_model: "UNKNOWN_SOURCE".into(),
            ..RunConfig::default()
        };
        assert!(
            prepare_config(&config)
                .unwrap_err()
                .contains("Unsupported source model")
        );

        let config = RunConfig {
            meter_model: "UNKNOWN_METER".into(),
            ..RunConfig::default()
        };
        assert!(
            prepare_config(&config)
                .unwrap_err()
                .contains("Unsupported meter model")
        );

        let config = RunConfig {
            meter_model: "KEYSIGHT_34465A".into(),
            nplc: 2.0,
            ..RunConfig::default()
        };
        assert!(plan(&config).unwrap_err().contains("does not support nplc"));

        let config = RunConfig {
            meter_model: "AGILENT_34401A".into(),
            compliance: 3.1,
            ..RunConfig::default()
        };
        assert!(plan(&config).unwrap_err().contains("meter maximum input"));
    }

    #[test]
    fn legacy_7651_range_codes_do_not_inflate_automatic_ramp_steps() {
        let config = RunConfig {
            source_model: "YOKOGAWA_7651".into(),
            pattern: Pattern::Linear,
            start: 0.0,
            stop: 0.05,
            step: 0.001,
            source_limit: 0.1,
            ..RunConfig::default()
        };
        let prepared = prepare_config(&config).unwrap();
        // 100 mV range has code 3 but supports 120 mV. Its automatic ramp is
        // 1% of 120 mV (1.2 mV), rather than 3% (=30 mV).
        assert_eq!(prepared.ramp_step, 0.0012);
        assert_eq!(plan(&config).unwrap().source_range, 3.0);
    }

    #[test]
    fn all_wait_fields_reject_negative_configs_and_allow_zero() {
        for field in ["settle_s", "start_settle_s", "ramp_wait_s"] {
            let mut config = RunConfig::default();
            let value = match field {
                "settle_s" => &mut config.settle_s,
                "start_settle_s" => &mut config.start_settle_s,
                _ => &mut config.ramp_wait_s,
            };
            *value = -0.001;
            assert!(plan(&config).unwrap_err().contains(field));
            match field {
                "settle_s" => config.settle_s = 0.0,
                "start_settle_s" => config.start_settle_s = 0.0,
                _ => config.ramp_wait_s = 0.0,
            }
            assert!(plan(&config).is_ok());
        }
    }

    #[test]
    fn compliance_is_quantized_up_and_software_value_is_retained() {
        let config = RunConfig {
            start: -0.2,
            stop: 0.2,
            step: 0.1,
            source_limit: 1.0,
            compliance: 0.0014,
            ..RunConfig::default()
        };
        assert_eq!(plan(&config).unwrap().hardware_compliance, 0.002);
        // The plan's hardware setting is separate from the caller-owned
        // software limit, which the measurement engine enforces on readings.
        assert_eq!(config.compliance, 0.0014);
    }

    #[test]
    fn source_hardware_and_meter_stop_limits_are_independent_and_legacy_is_preserved() {
        let mut config = RunConfig {
            mode: Mode::Vi,
            pattern: Pattern::Linear,
            start: -0.0004,
            stop: 0.0004,
            step: 0.00004,
            source_limit: 0.001,
            compliance: 0.2,
            source_compliance: Some(5.0),
            ..Default::default()
        };
        assert_eq!(plan(&config).unwrap().hardware_compliance, 5.0);
        assert_eq!(prepare_config(&config).unwrap().compliance, 0.2);
        config.source_compliance = None;
        assert_eq!(plan(&config).unwrap().hardware_compliance, 1.0);
        for invalid in [0.0, -1.0, f64::NAN, f64::INFINITY, 31.0] {
            config.source_compliance = Some(invalid);
            assert!(
                plan(&config).is_err(),
                "invalid source protection {invalid}"
            );
        }
        config.source_compliance = Some(1.1);
        assert_eq!(plan(&config).unwrap().hardware_compliance, 2.0);
    }

    #[test]
    fn low_voltage_gs210_range_reports_the_fixed_hardware_current_limit() {
        let mut config = RunConfig {
            compliance: 0.0001,
            ..Default::default()
        };
        let planned = plan(&config).unwrap();
        assert_eq!(planned.source_range, 0.01);
        assert_eq!(planned.hardware_compliance, 0.2);
        config.source_compliance = Some(0.002);
        let protected = plan(&config).unwrap();
        assert_eq!(protected.source_range, 1.0);
        assert_eq!(protected.hardware_compliance, 0.002);
        assert_eq!(config.compliance, 0.0001);
        config.source_compliance = Some(0.2);
        assert_eq!(plan(&config).unwrap().source_range, 0.01);
        let legacy = RunConfig {
            source_model: "YOKOGAWA_7651".into(),
            source_compliance: Some(0.002),
            ..config
        };
        assert_eq!(plan(&legacy).unwrap().hardware_compliance, 0.005);
    }

    #[test]
    fn csv_numbers_round_trip_without_losing_f64_bits() {
        let mut values = vec![
            0.0,
            -0.0,
            0.1,
            -0.2,
            1e-12,
            -1e-15,
            f64::MIN_POSITIVE,
            f64::MAX,
            f64::MIN,
            f64::from_bits(1),
            f64::from_bits(0x000f_ffff_ffff_ffff),
            f64::from_bits(0x3ff0_0000_0000_0001),
        ];
        // Exercise varied signs, exponents and mantissas, including subnormals.
        let mut bits = 0x0123_4567_89ab_cdef_u64;
        for _ in 0..10_000 {
            bits ^= bits << 13;
            bits ^= bits >> 7;
            bits ^= bits << 17;
            let value = f64::from_bits(bits);
            if value.is_finite() {
                values.push(value);
            }
        }
        let mut buffer = String::new();
        for value in values {
            shortest_sci_into(&mut buffer, value);
            assert_eq!(buffer.parse::<f64>().unwrap().to_bits(), value.to_bits());
        }
    }

    #[test]
    fn compact_csv_keeps_targets_sweep_identity_and_exact_readings() {
        let path = std::env::temp_dir().join(format!(
            "iv-compact-{}-{}.csv",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let config = RunConfig::default();
        let point = plan(&config).unwrap().points[0].clone();
        let mut writer = CsvWriter::create(&path).unwrap();
        let mut expected = Vec::new();
        for mode in [Mode::Iv, Mode::Vi] {
            let mut record = simulate_record(&config, &point, 0.1);
            record.mode = mode;
            record.point.target = 2.12e-9;
            record.point.repeat_index = 2;
            record.point.sweep_index = 3;
            record.source_set = -0.0;
            record.source_readback = None;
            record.meter_value = 1.2345678901234567e-12;
            record.compliance = true;
            record.status = "compliance".into();
            record.run_id = "test-run".into();
            record.readings_requested = 3;
            record.readings_completed = Some(3);
            record.meter_stddev = Some(1e-13);
            record.quality_flags = "software_limit_hit".into();
            writer.append(&record).unwrap();
            expected.push(record);
        }
        drop(writer);
        let mut reader = csv::Reader::from_path(&path).unwrap();
        assert_eq!(
            reader.headers().unwrap().iter().collect::<Vec<_>>(),
            CSV_FIELDS
        );
        let rows = reader.records().collect::<Result<Vec<_>, _>>().unwrap();
        assert_eq!(rows.len(), 2);
        for (row, record) in rows.iter().zip(expected) {
            assert_eq!(row.len(), 18);
            let inspector = record.native_csv_values();
            assert_eq!(
                row.iter().collect::<Vec<_>>(),
                inspector.iter().map(String::as_str).collect::<Vec<_>>()
            );
            assert_eq!(&row[0], record.run_id);
            assert_eq!(&row[2], record.timestamp);
            assert_eq!(&row[4], "3");
            assert_eq!(&row[6], "2.12e-9");
            assert_eq!(&row[7], if record.mode == Mode::Iv { "V" } else { "A" });
            let (voltage, current) = if record.mode == Mode::Iv {
                (record.source_set, record.meter_value)
            } else {
                (record.meter_value, record.source_set)
            };
            for index in [3, 6, 8, 9] {
                assert!(
                    row[index].contains('e'),
                    "floating CSV fields must use scientific notation"
                );
            }
            for (index, value) in [
                (3, record.elapsed_s),
                (6, record.point.target),
                (8, voltage),
                (9, current),
            ] {
                assert_eq!(
                    row[index].parse::<f64>().unwrap().to_bits(),
                    value.to_bits()
                );
            }
            assert_eq!(
                &row[10],
                if record.mode == Mode::Iv {
                    "source_setpoint"
                } else {
                    "meter"
                }
            );
            assert_eq!(
                &row[11],
                if record.mode == Mode::Iv {
                    "meter"
                } else {
                    "source_setpoint"
                }
            );
            assert_eq!(&row[16], "true");
            assert_eq!(&row[17], "software_limit_hit");
        }
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn csv_header_is_flushed_before_any_point_and_never_overwrites() {
        let path = std::env::temp_dir().join(format!(
            "iv-header-{}-{}.csv",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let writer = CsvWriter::create(&path).unwrap();
        assert!(CsvWriter::create(&path).is_err());
        assert_eq!(
            std::fs::read_to_string(&path).unwrap().trim(),
            CSV_FIELDS.join(",")
        );
        drop(writer);
        std::fs::remove_file(path).unwrap();
    }
}

#[cfg(test)]
mod dc_nudge_tests {
    use super::*;
    #[test]
    fn repeated_digit_steps_do_not_accumulate_binary_decimal_drift() {
        let mut value = 0.0;
        for _ in 0..1000 {
            value = nudge_source_value(value, 0.0001, 1e-6, 1.0).unwrap();
        }
        assert_eq!(value, 0.1);
        assert_eq!(nudge_source_value(-0.001, 0.001, 1e-6, 1.0).unwrap(), 0.0);
        assert_eq!(nudge_source_value(2e-6, 1e-6, 1e-9, 1e-3).unwrap(), 3e-6);
    }
    #[test]
    fn source_increment_rejects_range_and_resolution_errors_without_clamping() {
        assert!(nudge_source_value(0.1, 0.0001, 1e-6, 0.1).is_err());
        assert!(nudge_source_value(-0.1, -0.0001, 1e-6, 0.1).is_err());
        assert!(nudge_source_value(0.0, 1e-9, 1e-6, 0.1).is_err());
        assert!(nudge_source_value(0.0000005, 0.0, 1e-6, 0.1).is_err());
        for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert!(nudge_source_value(bad, 0.0, 1e-6, 0.1).is_err());
        }
    }
}
