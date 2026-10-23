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
pub use instrument_specs::{meter_display_queries, source_display_resolution, source_range_query};

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
            mode: Mode::Iv,
            pattern: Pattern::ZeroCentered,
            start: -0.001,
            stop: 0.001,
            step: 0.0001,
            repeat: 1,
            custom_points: Vec::new(),
            source_limit: 0.001,
            compliance: 0.001,
            settle_s: 0.2,
            start_settle_s: 0.5,
            nplc: 1.0,
            average_count: 1,
            ramp_step: 0.0001,
            ramp_wait_s: 0.02,
        }
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
        "AGILENT_34401A" | "AGILENT_34411A" | "KEYSIGHT_34411A" | "KEYSIGHT_34465A" | "ADCMT_7461A"
    ) {
        return Err(format!(
            "Unsupported meter model '{model}'. Supported meter models: AGILENT_34401A, AGILENT_34411A, KEYSIGHT_34411A, KEYSIGHT_34465A, ADCMT_7461A."
        ));
    }
    meter_specs()?
        .remove(model)
        .ok_or_else(|| format!("Embedded meter profile for {model} is missing."))
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
    let tolerance = step.abs() / Decimal::from(1_000_000_000_u64);
    let mut current = start;
    let mut points = Vec::new();
    while if step.is_sign_positive() {
        current <= stop + tolerance
    } else {
        current >= stop - tolerance
    } {
        if points.len() >= MAX_POINTS {
            return Err(format!("sweep exceeds {MAX_POINTS} points."));
        }
        points.push(current);
        current += step;
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
) -> Result<(f64, f64, f64), String> {
    let entries = match mode {
        Mode::Iv => &spec.voltage_ranges,
        Mode::Vi => &spec.current_ranges,
    };
    entries
        .iter()
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
            "compliance {requested} exceeds {model} hardware maximum {maximum}."
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
pub fn prepare_config(config: &RunConfig) -> Result<RunConfig, String> {
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
    let (source_range, range_maximum, resolution) =
        select_range(&source, &config.source_model, config.mode, maximum)?;
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
pub fn plan(config: &RunConfig) -> Result<Plan, String> {
    let prepared = prepare_config(config)?;
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
    finite_positive(config.compliance, "compliance", None)?;
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
            "compliance {} {} exceeds {} meter maximum input {} {}.",
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
    let (source_range, range_maximum, resolution) =
        select_range(&source, &config.source_model, config.mode, maximum)?;
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
    aligned_value(
        config.ramp_step,
        resolution,
        "Ramp step (Timing / Compliance)",
        unit,
    )?;
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
            let maximum = start.abs().max(stop.abs());
            let mut points = Vec::new();
            append_joined(&mut points, forward_points(Decimal::ZERO, maximum, step)?);
            append_joined(&mut points, forward_points(maximum, -maximum, step)?);
            append_joined(&mut points, forward_points(-maximum, Decimal::ZERO, step)?);
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
        config.compliance,
    )?;

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
            compliance: 0.0014,
            ..RunConfig::default()
        };
        assert_eq!(plan(&config).unwrap().hardware_compliance, 0.002);
        // The plan's hardware setting is separate from the caller-owned
        // software limit, which the measurement engine enforces on readings.
        assert_eq!(config.compliance, 0.0014);
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
