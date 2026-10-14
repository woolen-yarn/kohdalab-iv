#![forbid(unsafe_code)]

//! Deterministic, hardware-aware planning and CSV persistence for IV/VI runs.

use std::fmt::Write as _;
use std::fs::OpenOptions;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

mod instrument_specs;
pub use instrument_specs::{meter_display_queries, source_display_resolution, source_range_query};

/// Native result schema: acquired values, origins, sweep identity and status.
pub const CSV_FIELDS: &[&str] = &[
    "timestamp",
    "elapsed_s",
    "point_index",
    "repeat_index",
    "sweep_index",
    "direction",
    "target_value",
    "target_unit",
    "voltage_V",
    "current_A",
    "voltage_origin",
    "current_origin",
    "software_limit_hit",
    "status",
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
    toml::from_str::<Specs>(include_str!("../../../specs/sources.toml"))
        .map(|specs| specs.gs210)
        .map_err(|error| format!("embedded GS210 specification is invalid: {error}"))
}

fn dmm34411a() -> Result<DmmSpec, String> {
    toml::from_str::<MeterSpecs>(include_str!("../../../specs/meters.toml"))
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
}

impl Record {
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
        let (voltage, current, voltage_origin, current_origin) = record.electrical_values();
        for (index, value) in [
            (1, record.elapsed_s),
            (6, record.point.target),
            (8, voltage),
            (9, current),
        ] {
            shortest_sci_into(&mut self.fields[index], value);
        }
        for (index, value) in [
            (2, record.point.point_index),
            (3, record.point.repeat_index as usize),
            (4, record.point.sweep_index as usize),
        ] {
            self.fields[index].clear();
            write!(&mut self.fields[index], "{value}").expect("writing to String");
        }
        for (index, value) in [
            (0, record.timestamp.as_str()),
            (5, record.point.direction.as_str()),
            (7, if record.mode == Mode::Iv { "V" } else { "A" }),
            (10, voltage_origin),
            (11, current_origin),
            (12, if record.compliance { "true" } else { "false" }),
            (13, record.status.as_str()),
        ] {
            self.fields[index].clear();
            self.fields[index].push_str(value);
        }
        self.writer
            .write_record(&self.fields)
            .map_err(|e| format!("cannot write CSV row: {e}"))?;
        self.writer
            .flush()
            .map_err(|e| format!("cannot flush CSV row: {e}"))
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
            assert_eq!(row.len(), 14);
            assert_eq!(&row[0], record.timestamp);
            assert_eq!(&row[3], "2");
            assert_eq!(&row[4], "3");
            assert_eq!(&row[6], "2.12e-9");
            assert_eq!(&row[7], if record.mode == Mode::Iv { "V" } else { "A" });
            let (voltage, current) = if record.mode == Mode::Iv {
                (record.source_set, record.meter_value)
            } else {
                (record.meter_value, record.source_set)
            };
            for index in [1, 6, 8, 9] {
                assert!(
                    row[index].contains('e'),
                    "floating CSV fields must use scientific notation"
                );
            }
            for (index, value) in [
                (1, record.elapsed_s),
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
            assert_eq!(&row[12], "true");
            assert_eq!(&row[13], "compliance");
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
