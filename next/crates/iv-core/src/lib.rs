#![forbid(unsafe_code)]

//! Deterministic, hardware-aware planning and CSV persistence for IV/VI runs.

use std::fmt::Write as _;
use std::fs::OpenOptions;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

/// CSV columns kept byte-for-byte compatible with the established IV export.
pub const IV_FIELDS: &[&str] = &[
    "timestamp",
    "measurement",
    "elapsed_s",
    "mode",
    "sweep_pattern",
    "repeat_index",
    "sweep_index",
    "point_index",
    "total_points",
    "direction",
    "source_function",
    "measure_function",
    "target_value",
    "target_unit",
    "source_target_value",
    "source_target_unit",
    "target_V",
    "target_A",
    "source_set_V",
    "source_set_A",
    "source_readback_value",
    "source_readback_unit",
    "source_readback_V",
    "source_readback_A",
    "meter_value",
    "meter_unit",
    "voltage_V",
    "current_A",
    "measured_V",
    "measured_A",
    "voltage_origin",
    "current_origin",
    "resistance_Ohm",
    "conductance_S",
    "compliance",
    "software_limit_hit",
    "status",
    "instrument_status",
    "source_ref",
    "measure_ref",
    "source_model",
    "measure_model",
    "start_settle_s",
    "settle_s",
    "nplc",
    "average_count",
];

const MAX_POINTS: usize = 1_000_000;
const MAX_SECONDS: f64 = 3_600.0;

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
    pub mode: Mode,
    pub pattern: Pattern,
    /// Start target, in SI units (V for IV and A for VI).
    pub start: f64,
    /// Stop target, in SI units (V for IV and A for VI).
    pub stop: f64,
    /// Sweep increment, in SI units. Its sign is ignored; sweep direction is inferred.
    pub step: f64,
    pub repeat: u32,
    pub custom_points: Vec<f64>,
    /// Software maximum absolute source target, in source SI units.
    pub source_limit: f64,
    /// Requested software compliance, A for IV and V for VI.
    pub compliance: f64,
    pub settle_s: f64,
    pub start_settle_s: f64,
    pub nplc: f64,
    pub average_count: u32,
    pub ramp_step: f64,
    pub ramp_wait_s: f64,
}

impl Default for RunConfig {
    fn default() -> Self {
        Self {
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
    #[serde(rename = "YOKOGAWA_GS210")]
    gs210: SourceSpec,
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
    current_limit_resolution_a: f64,
    #[serde(rename = "voltage_limit_min_V")]
    voltage_limit_min_v: f64,
    #[serde(rename = "voltage_limit_max_V")]
    voltage_limit_max_v: f64,
    #[serde(rename = "voltage_limit_resolution_V")]
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
    #[serde(rename = "AGILENT_34411A")]
    dmm34411a: DmmSpec,
}

#[derive(Debug, Deserialize)]
struct DmmSpec {
    nplc_values: Vec<f64>,
}

fn gs210() -> Result<SourceSpec, String> {
    toml::from_str::<Specs>(include_str!(
        "../../../../src/kohdalab_iv/instruments/sources/specs.toml"
    ))
    .map(|specs| specs.gs210)
    .map_err(|error| format!("embedded GS210 specification is invalid: {error}"))
}

fn dmm34411a() -> Result<DmmSpec, String> {
    toml::from_str::<MeterSpecs>(include_str!(
        "../../../../src/kohdalab_iv/instruments/meters/specs.toml"
    ))
    .map(|specs| specs.dmm34411a)
    .map_err(|error| format!("embedded 34411A specification is invalid: {error}"))
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

fn select_range(spec: &SourceSpec, mode: Mode, maximum: f64) -> Result<(f64, f64, f64), String> {
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
        .ok_or_else(|| "source targets exceed the GS210 range.".to_owned())
}

fn aligned(values: &[Decimal], resolution: f64, label: &str) -> Result<(), String> {
    let resolution = decimal(resolution, "source resolution")?;
    for value in values {
        if !value.is_zero() && (*value / resolution).fract() != Decimal::ZERO {
            return Err(format!(
                "{label} {value} is not aligned to source resolution {resolution}."
            ));
        }
    }
    Ok(())
}

fn quantized_hardware_compliance(
    spec: &SourceSpec,
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
            "compliance {requested} exceeds GS210 hardware maximum {maximum}."
        ));
    }
    let requested = requested.max(minimum);
    let ratio = (requested / resolution).ceil();
    Ok((ratio * resolution).min(maximum))
}

/// Validate a GS210 + 34411A run and create its deterministic sweep sequence.
pub fn plan(config: &RunConfig) -> Result<Plan, String> {
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
    let meter = dmm34411a()?;
    if !meter
        .nplc_values
        .iter()
        .any(|value| (*value - config.nplc).abs() < 1e-12)
    {
        return Err(format!("34411A does not support nplc={}.", config.nplc));
    }

    let step = decimal(config.step, "step")?;
    if step.is_zero() {
        return Err("step must be non-zero.".to_owned());
    }
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
                .map(|(index, value)| decimal(*value, &format!("custom_points[{index}]")))
                .collect::<Result<Vec<_>, _>>()?
        }
        Pattern::Linear => forward_points(
            decimal(config.start, "start")?,
            decimal(config.stop, "stop")?,
            step,
        )?,
        Pattern::RoundTrip => {
            let start = decimal(config.start, "start")?;
            let stop = decimal(config.stop, "stop")?;
            let mut points = forward_points(start, stop, step)?;
            append_joined(&mut points, forward_points(stop, start, step)?);
            points
        }
        Pattern::ZeroCentered => {
            let start = decimal(config.start, "start")?;
            let stop = decimal(config.stop, "stop")?;
            let maximum = start.abs().max(stop.abs());
            let mut points = Vec::new();
            append_joined(&mut points, forward_points(Decimal::ZERO, maximum, step)?);
            append_joined(&mut points, forward_points(maximum, -maximum, step)?);
            append_joined(&mut points, forward_points(-maximum, Decimal::ZERO, step)?);
            points
        }
    };
    let base_len = base_points.len();
    let total = base_len
        .checked_mul(config.repeat as usize)
        .ok_or_else(|| "sweep point count overflow.".to_owned())?;
    if total == 0 || total > MAX_POINTS {
        return Err(format!("sweep must contain 1..={MAX_POINTS} points."));
    }

    let gs210 = gs210()?;
    let hard_limit = match config.mode {
        Mode::Iv => gs210.max_abs_v,
        Mode::Vi => gs210.max_abs_a,
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
    let (source_range, range_maximum, resolution) = select_range(&gs210, config.mode, max_target)?;
    aligned(&base_points, resolution, "target")?;
    if config.ramp_step > range_maximum + f64::EPSILON {
        return Err("ramp_step exceeds the selected source range.".to_owned());
    }
    aligned(
        &[decimal(config.ramp_step, "ramp_step")?],
        resolution,
        "ramp_step",
    )?;
    let hardware_compliance =
        quantized_hardware_compliance(&gs210, config.mode, config.compliance)?;

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
        .cycle()
        .take(total)
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
                repeat_index: (index / base_len) as u32 + 1,
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
    pub timestamp: String,
    pub elapsed_s: f64,
    pub point: SweepPoint,
    pub mode: Mode,
    pub pattern: Pattern,
    pub source_set: f64,
    pub source_readback: Option<f64>,
    pub meter_value: f64,
    pub compliance: bool,
    pub status: String,
    pub instrument_status: String,
    pub source_resource: String,
    pub meter_resource: String,
}

fn sci(value: f64) -> String {
    // Python's `:.12e`, used by the established export, always has a two-digit
    // exponent. Rust intentionally omits that padding, so normalize it here.
    let raw = format!("{value:.12e}");
    let (mantissa, exponent) = raw
        .split_once('e')
        .expect("scientific formatting includes an exponent");
    let exponent = exponent
        .parse::<i32>()
        .expect("Rust emitted a numeric exponent");
    format!("{mantissa}e{exponent:+03}")
}
fn optional_sci(value: Option<f64>) -> String {
    value.map(sci).unwrap_or_default()
}
fn flag(value: bool) -> String {
    value.to_string()
}

impl Record {
    /// Values in [`IV_FIELDS`] order, using SI values and stable scientific formatting.
    pub fn csv_values(&self, config: &RunConfig) -> Vec<String> {
        let (source_function, measure_function, unit) = match self.mode {
            Mode::Iv => ("voltage", "dc_current", "V"),
            Mode::Vi => ("current", "dc_voltage", "A"),
        };
        let (
            target_v,
            target_a,
            source_set_v,
            source_set_a,
            readback_v,
            readback_a,
            voltage,
            current,
            meter_unit,
            voltage_origin,
            current_origin,
        ) = match self.mode {
            Mode::Iv => (
                Some(self.point.target),
                None,
                Some(self.source_set),
                None,
                self.source_readback,
                None,
                self.source_readback.unwrap_or(self.source_set),
                self.meter_value,
                "A",
                if self.source_readback.is_some() {
                    "source_readback"
                } else {
                    "source_setpoint"
                },
                "meter",
            ),
            Mode::Vi => (
                None,
                Some(self.point.target),
                None,
                Some(self.source_set),
                None,
                self.source_readback,
                self.meter_value,
                self.source_readback.unwrap_or(self.source_set),
                "V",
                "meter",
                if self.source_readback.is_some() {
                    "source_readback"
                } else {
                    "source_setpoint"
                },
            ),
        };
        let resistance = if current == 0.0 {
            None
        } else {
            Some(voltage / current)
        };
        let conductance = if voltage == 0.0 {
            None
        } else {
            Some(current / voltage)
        };
        let source_model = if self
            .source_resource
            .to_ascii_lowercase()
            .contains("simulated")
        {
            "SIMULATED_SOURCE"
        } else {
            "YOKOGAWA_GS210"
        };
        let meter_model = if self
            .meter_resource
            .to_ascii_lowercase()
            .contains("simulated")
        {
            "SIMULATED_METER"
        } else {
            "AGILENT_34411A"
        };
        vec![
            self.timestamp.clone(),
            "iv".into(),
            sci(self.elapsed_s),
            self.mode.as_str().into(),
            self.pattern.as_str().into(),
            self.point.repeat_index.to_string(),
            self.point.sweep_index.to_string(),
            self.point.point_index.to_string(),
            self.point.total_points.to_string(),
            self.point.direction.clone(),
            source_function.into(),
            measure_function.into(),
            sci(self.point.target),
            unit.into(),
            sci(self.point.target),
            unit.into(),
            optional_sci(target_v),
            optional_sci(target_a),
            optional_sci(source_set_v),
            optional_sci(source_set_a),
            optional_sci(self.source_readback),
            unit.into(),
            optional_sci(readback_v),
            optional_sci(readback_a),
            sci(self.meter_value),
            meter_unit.into(),
            sci(voltage),
            sci(current),
            sci(voltage),
            sci(current),
            voltage_origin.into(),
            current_origin.into(),
            optional_sci(resistance),
            optional_sci(conductance),
            flag(self.compliance),
            flag(self.compliance),
            self.status.clone(),
            self.instrument_status.clone(),
            self.source_resource.clone(),
            self.meter_resource.clone(),
            source_model.into(),
            meter_model.into(),
            sci(config.start_settle_s),
            sci(config.settle_s),
            sci(config.nplc),
            config.average_count.to_string(),
        ]
    }
}

pub struct CsvWriter {
    pub path: PathBuf,
    writer: csv::Writer<std::fs::File>,
    row: CsvRow,
}

/// Reusable storage for the allocation-sensitive persistence path.  The public
/// `Record::csv_values` intentionally remains the convenient owned API.
struct CsvRow {
    fields: [String; IV_FIELDS.len()],
    elapsed: String,
    target: String,
    source_set: String,
    readback: String,
    meter: String,
    voltage: String,
    current: String,
    resistance: String,
    conductance: String,
    config: Option<CachedConfig>,
}

struct CachedConfig {
    start_settle_bits: u64,
    settle_bits: u64,
    nplc_bits: u64,
    average_count: u32,
    start_settle: String,
    settle: String,
    nplc: String,
    average: String,
}

impl CsvRow {
    fn new() -> Self {
        Self {
            fields: std::array::from_fn(|_| String::new()),
            elapsed: String::new(),
            target: String::new(),
            source_set: String::new(),
            readback: String::new(),
            meter: String::new(),
            voltage: String::new(),
            current: String::new(),
            resistance: String::new(),
            conductance: String::new(),
            config: None,
        }
    }

    fn refresh_config(&mut self, config: &RunConfig) {
        let changed = self.config.as_ref().is_none_or(|cached| {
            (
                cached.start_settle_bits,
                cached.settle_bits,
                cached.nplc_bits,
                cached.average_count,
            ) != (
                config.start_settle_s.to_bits(),
                config.settle_s.to_bits(),
                config.nplc.to_bits(),
                config.average_count,
            )
        });
        if changed {
            let mut cached = CachedConfig {
                start_settle_bits: config.start_settle_s.to_bits(),
                settle_bits: config.settle_s.to_bits(),
                nplc_bits: config.nplc.to_bits(),
                average_count: config.average_count,
                start_settle: String::new(),
                settle: String::new(),
                nplc: String::new(),
                average: String::new(),
            };
            sci_into(&mut cached.start_settle, config.start_settle_s);
            sci_into(&mut cached.settle, config.settle_s);
            sci_into(&mut cached.nplc, config.nplc);
            write!(&mut cached.average, "{}", config.average_count).expect("writing to String");
            self.config = Some(cached);
        }
    }

    fn fill(&mut self, record: &Record, config: &RunConfig) {
        macro_rules! set {
            ($index:expr, $value:expr) => {{
                let value = $value;
                let field = &mut self.fields[$index];
                field.clear();
                field.push_str(value);
            }};
        }
        macro_rules! number {
            ($index:expr, $value:expr) => {{
                self.fields[$index].clear();
                write!(&mut self.fields[$index], "{}", $value).expect("writing to String");
            }};
        }
        self.refresh_config(config);
        let (source_function, measure_function, unit) = match record.mode {
            Mode::Iv => ("voltage", "dc_current", "V"),
            Mode::Vi => ("current", "dc_voltage", "A"),
        };
        let source_model = model_name(
            &record.source_resource,
            "SIMULATED_SOURCE",
            "YOKOGAWA_GS210",
        );
        let meter_model = model_name(&record.meter_resource, "SIMULATED_METER", "AGILENT_34411A");
        sci_into(&mut self.elapsed, record.elapsed_s);
        sci_into(&mut self.target, record.point.target);
        if record.source_set.to_bits() == record.point.target.to_bits() {
            self.source_set.clone_from(&self.target);
        } else {
            sci_into(&mut self.source_set, record.source_set);
        }
        if record.source_readback.map(f64::to_bits) == Some(record.point.target.to_bits()) {
            self.readback.clone_from(&self.target);
        } else {
            optional_sci_into(&mut self.readback, record.source_readback);
        }
        sci_into(&mut self.meter, record.meter_value);

        set!(0, &record.timestamp);
        set!(1, "iv");
        set!(2, &self.elapsed);
        set!(3, record.mode.as_str());
        set!(4, record.pattern.as_str());
        number!(5, record.point.repeat_index);
        number!(6, record.point.sweep_index);
        number!(7, record.point.point_index);
        number!(8, record.point.total_points);
        set!(9, &record.point.direction);
        set!(10, source_function);
        set!(11, measure_function);
        set!(12, &self.target);
        set!(13, unit);
        set!(14, &self.target);
        set!(15, unit);
        set!(20, &self.readback);
        set!(21, unit);
        set!(24, &self.meter);
        set!(34, if record.compliance { "true" } else { "false" });
        set!(35, if record.compliance { "true" } else { "false" });
        set!(36, &record.status);
        set!(37, &record.instrument_status);
        set!(38, &record.source_resource);
        set!(39, &record.meter_resource);
        set!(40, source_model);
        set!(41, meter_model);
        let cached = self.config.as_ref().expect("config refreshed");
        set!(42, &cached.start_settle);
        set!(43, &cached.settle);
        set!(44, &cached.nplc);
        set!(45, &cached.average);

        let (voltage, current) = match record.mode {
            Mode::Iv => {
                set!(16, &self.target);
                set!(17, "");
                set!(18, &self.source_set);
                set!(19, "");
                set!(22, &self.readback);
                set!(23, "");
                set!(25, "A");
                set!(
                    30,
                    if record.source_readback.is_some() {
                        "source_readback"
                    } else {
                        "source_setpoint"
                    }
                );
                set!(31, "meter");
                (
                    record.source_readback.unwrap_or(record.source_set),
                    record.meter_value,
                )
            }
            Mode::Vi => {
                set!(16, "");
                set!(17, &self.target);
                set!(18, "");
                set!(19, &self.source_set);
                set!(22, "");
                set!(23, &self.readback);
                set!(25, "V");
                set!(30, "meter");
                set!(
                    31,
                    if record.source_readback.is_some() {
                        "source_readback"
                    } else {
                        "source_setpoint"
                    }
                );
                (
                    record.meter_value,
                    record.source_readback.unwrap_or(record.source_set),
                )
            }
        };
        let source = if record.source_readback.is_some() {
            &self.readback
        } else {
            &self.source_set
        };
        let (voltage_text, current_text) = if record.mode == Mode::Iv {
            (source, &self.meter)
        } else {
            (&self.meter, source)
        };
        self.voltage.clone_from(voltage_text);
        self.current.clone_from(current_text);
        set!(26, &self.voltage);
        set!(27, &self.current);
        set!(28, &self.voltage);
        set!(29, &self.current);
        optional_sci_into(
            &mut self.resistance,
            (current != 0.0).then(|| voltage / current),
        );
        optional_sci_into(
            &mut self.conductance,
            (voltage != 0.0).then(|| current / voltage),
        );
        set!(32, &self.resistance);
        set!(33, &self.conductance);
    }
}

fn sci_into(buffer: &mut String, value: f64) {
    buffer.clear();
    write!(buffer, "{value:.12e}").expect("writing to String");
    let exponent_start = buffer
        .find('e')
        .expect("scientific formatting includes exponent");
    let exponent = buffer[exponent_start + 1..]
        .parse::<i32>()
        .expect("numeric exponent");
    buffer.truncate(exponent_start + 1);
    write!(buffer, "{exponent:+03}").expect("writing to String");
}

fn optional_sci_into(buffer: &mut String, value: Option<f64>) {
    match value {
        Some(value) => sci_into(buffer, value),
        None => buffer.clear(),
    }
}

fn model_name<'a>(resource: &str, simulated: &'a str, hardware: &'a str) -> &'a str {
    if resource
        .as_bytes()
        .windows(9)
        .any(|part| part.eq_ignore_ascii_case(b"simulated"))
    {
        simulated
    } else {
        hardware
    }
}

impl CsvWriter {
    /// Create a new result file and flush its header before source output can start.
    pub fn create(path: &Path) -> Result<Self, String> {
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .map_err(|error| format!("cannot create {}: {error}", path.display()))?;
        let mut writer = csv::Writer::from_writer(file);
        writer
            .write_record(IV_FIELDS)
            .map_err(|error| format!("cannot write CSV header: {error}"))?;
        writer
            .flush()
            .map_err(|error| format!("cannot flush CSV header: {error}"))?;
        Ok(Self {
            path: path.to_path_buf(),
            writer,
            row: CsvRow::new(),
        })
    }

    /// Append and flush one acquired point so partial runs remain recoverable.
    pub fn append(&mut self, record: &Record, config: &RunConfig) -> Result<(), String> {
        self.row.fill(record, config);
        self.writer
            .write_record(&self.row.fields)
            .map_err(|error| format!("cannot write CSV row: {error}"))?;
        self.writer
            .flush()
            .map_err(|error| format!("cannot flush CSV row: {error}"))
    }
}

/// A deterministic 1 kΩ open-load simulation for GUI and persistence verification.
pub fn simulate_record(config: &RunConfig, point: &SweepPoint, elapsed_s: f64) -> Record {
    let (source_set, meter_value, source_readback) = match config.mode {
        Mode::Iv => (point.target, point.target / 1_000.0, Some(point.target)),
        Mode::Vi => (point.target, point.target * 1_000.0, Some(point.target)),
    };
    Record {
        timestamp: "1970-01-01T00:00:00Z".into(),
        elapsed_s,
        point: point.clone(),
        mode: config.mode,
        pattern: config.pattern,
        source_set,
        source_readback,
        meter_value,
        compliance: false,
        status: "ok".into(),
        instrument_status: String::new(),
        source_resource: "source.simulated".into(),
        meter_resource: "meter.simulated".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

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
    fn patterns_join_and_repeat_like_python_plan() {
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
            vec![
                0.0, 0.0001, 0.0002, 0.0001, 0.0, 0.0, 0.0001, 0.0002, 0.0001, 0.0
            ]
        );
        assert_eq!(sweep_plan.points[5].repeat_index, 2);
        // A repeated round trip joins at zero, so the zero delta retains the
        // preceding direction exactly as the established Python planner does.
        assert_eq!(sweep_plan.points[5].direction, "backward");
        config.pattern = Pattern::CustomList;
        config.custom_points = vec![0.0, 0.0001, -0.0001];
        config.repeat = 1;
        assert_eq!(plan(&config).unwrap().points.len(), 3);
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
                    .cycle()
                    .take(values.len() * repeat as usize)
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
                    assert_eq!(point.repeat_index as usize, index / values.len() + 1);
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
    fn record_columns_and_simulation_are_consistent() {
        let config = RunConfig::default();
        let point = plan(&config).unwrap().points[10].clone();
        let record = simulate_record(&config, &point, 0.25);
        let row = record.csv_values(&config);
        assert_eq!(row.len(), IV_FIELDS.len());
        assert_eq!(
            row[IV_FIELDS
                .iter()
                .position(|field| *field == "resistance_Ohm")
                .unwrap()],
            "1.000000000000e+03"
        );
        assert_eq!(
            row[IV_FIELDS
                .iter()
                .position(|field| *field == "conductance_S")
                .unwrap()],
            "1.000000000000e-03"
        );
        let source = include_str!("../../../../src/kohdalab_iv/api/measurement_rows.py");
        let fields_start = source.find("IV_FIELDS = [").unwrap();
        let fields_end = source[fields_start..].find("]\n\n").unwrap() + fields_start;
        let python_fields = source[fields_start..fields_end]
            .lines()
            .filter_map(|line| {
                let line = line.trim().trim_end_matches(',');
                line.strip_prefix('"')
                    .and_then(|value| value.strip_suffix('"'))
            })
            .collect::<Vec<_>>();
        assert_eq!(IV_FIELDS, python_fields);
    }

    #[test]
    fn writer_is_exclusive_and_persists_header_before_rows() {
        let path = std::env::temp_dir().join(format!(
            "iv-core-{}-{}.csv",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let config = RunConfig::default();
        let mut writer = CsvWriter::create(&path).unwrap();
        assert!(CsvWriter::create(&path).is_err());
        let initial = fs::read_to_string(&path).unwrap();
        assert_eq!(initial.trim(), IV_FIELDS.join(","));
        let record = simulate_record(&config, &plan(&config).unwrap().points[0], 0.0);
        writer.append(&record, &config).unwrap();
        drop(writer);
        assert_eq!(fs::read_to_string(&path).unwrap().lines().count(), 2);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn writer_matches_legacy_rows_for_escaped_values_and_changed_config() {
        let path = std::env::temp_dir().join(format!(
            "iv-core-row-{}-{}.csv",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut writer = CsvWriter::create(&path).unwrap();
        let mut config = RunConfig::default();
        let mut iv = Record {
            timestamp: "a,\"quoted\"\nvalue".into(),
            elapsed_s: -0.0,
            point: SweepPoint {
                target: -0.0,
                direction: "forward,\"quoted\"\nvalue".into(),
                repeat_index: 1,
                sweep_index: 2,
                point_index: 3,
                total_points: 4,
            },
            mode: Mode::Iv,
            pattern: Pattern::CustomList,
            source_set: -0.0,
            source_readback: None,
            meter_value: -0.0,
            compliance: true,
            status: "ok,\"quoted\"\nvalue".into(),
            instrument_status: "status,\"quoted\"\nvalue".into(),
            source_resource: "source.SIMULATED,\"quoted\"\nvalue".into(),
            meter_resource: "meter.simulated,\"quoted\"\nvalue".into(),
        };
        let legacy_iv = iv.csv_values(&config);
        let expected_iv = vec![
            "a,\"quoted\"\nvalue",
            "iv",
            "-0.000000000000e+00",
            "dc_iv",
            "custom_list",
            "1",
            "2",
            "3",
            "4",
            "forward,\"quoted\"\nvalue",
            "voltage",
            "dc_current",
            "-0.000000000000e+00",
            "V",
            "-0.000000000000e+00",
            "V",
            "-0.000000000000e+00",
            "",
            "-0.000000000000e+00",
            "",
            "",
            "V",
            "",
            "",
            "-0.000000000000e+00",
            "A",
            "-0.000000000000e+00",
            "-0.000000000000e+00",
            "-0.000000000000e+00",
            "-0.000000000000e+00",
            "source_setpoint",
            "meter",
            "",
            "",
            "true",
            "true",
            "ok,\"quoted\"\nvalue",
            "status,\"quoted\"\nvalue",
            "source.SIMULATED,\"quoted\"\nvalue",
            "meter.simulated,\"quoted\"\nvalue",
            "SIMULATED_SOURCE",
            "SIMULATED_METER",
            "5.000000000000e-01",
            "2.000000000000e-01",
            "1.000000000000e+00",
            "1",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect::<Vec<_>>();
        assert_eq!(legacy_iv, expected_iv);
        writer.append(&iv, &config).unwrap();

        config.start_settle_s = 1.25;
        config.settle_s = 2.5;
        config.nplc = 10.0;
        config.average_count = 7;
        iv.mode = Mode::Vi;
        iv.source_readback = Some(-0.0);
        let legacy_vi = iv.csv_values(&config);
        writer.append(&iv, &config).unwrap();
        drop(writer);

        let mut reader = csv::Reader::from_path(&path).unwrap();
        let rows = reader
            .records()
            .map(|row| row.unwrap().iter().map(str::to_owned).collect::<Vec<_>>())
            .collect::<Vec<_>>();
        assert_eq!(rows, vec![legacy_iv, legacy_vi]);
        assert_eq!(rows[0][20], "");
        assert_eq!(rows[0][2], "-0.000000000000e+00");
        assert_eq!(rows[1][42], "1.250000000000e+00");
        assert_eq!(rows[1][45], "7");
        fs::remove_file(path).unwrap();
    }
}
