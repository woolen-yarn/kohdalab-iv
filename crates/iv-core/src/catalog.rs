//! Stable instrument catalog and read-only profiles derived from the embedded TOML specs.
//!
//! Descriptors describe protocol-level capabilities. Numeric operating limits remain in
//! `specs/sources.toml` and `specs/meters.toml`; [`instrument_profile`] reads the shared
//! typed tables parsed from those authoritative resources instead of keeping a second
//! numeric table in Rust.

use crate::model_spec::{MeterModelSpec, SourceModelSpec, SourceRange, meter_model, source_model};

/// The role an instrument has in an IV run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InstrumentRole {
    Source,
    Meter,
}

/// Protocol family selected by the transport for an instrument model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InstrumentDialect {
    Scpi,
    Yokogawa7651,
    Agilent34401,
    Adc7461,
    Simulation,
}

/// Source operations made available by an instrument model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SourceCapabilities {
    pub output: bool,
    pub readback: bool,
    pub software_ramp: bool,
    pub voltage: bool,
    pub current: bool,
}

/// Meter operations made available by an instrument model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MeterCapabilities {
    pub voltage: bool,
    pub current: bool,
}

/// Capabilities are role-specific, so consumers cannot treat a meter as a source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InstrumentCapabilities {
    Source(SourceCapabilities),
    Meter(MeterCapabilities),
}

/// A registered instrument model. The catalog order is part of the UI contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct InstrumentDescriptor {
    pub id: &'static str,
    pub role: InstrumentRole,
    pub dialect: InstrumentDialect,
    pub simulation: bool,
    pub capabilities: InstrumentCapabilities,
}

impl InstrumentDescriptor {
    pub fn supports_mode(&self, mode: crate::Mode) -> bool {
        match (self.capabilities, mode) {
            (InstrumentCapabilities::Source(c), crate::Mode::Iv) => c.voltage && c.output,
            (InstrumentCapabilities::Source(c), crate::Mode::Vi) => c.current && c.output,
            (InstrumentCapabilities::Meter(c), crate::Mode::Iv) => c.current,
            (InstrumentCapabilities::Meter(c), crate::Mode::Vi) => c.voltage,
        }
    }
}

const SOURCE_STANDARD: InstrumentCapabilities =
    InstrumentCapabilities::Source(SourceCapabilities {
        output: true,
        readback: true,
        software_ramp: true,
        voltage: true,
        current: true,
    });
const METER_STANDARD: InstrumentCapabilities = InstrumentCapabilities::Meter(MeterCapabilities {
    voltage: true,
    current: true,
});

const SOURCES: &[InstrumentDescriptor] = &[
    InstrumentDescriptor {
        id: "YOKOGAWA_GS210",
        role: InstrumentRole::Source,
        dialect: InstrumentDialect::Scpi,
        simulation: false,
        capabilities: SOURCE_STANDARD,
    },
    InstrumentDescriptor {
        id: "YOKOGAWA_7651",
        role: InstrumentRole::Source,
        dialect: InstrumentDialect::Yokogawa7651,
        simulation: false,
        capabilities: SOURCE_STANDARD,
    },
    InstrumentDescriptor {
        id: "SIMULATED_SOURCE",
        role: InstrumentRole::Source,
        dialect: InstrumentDialect::Simulation,
        simulation: true,
        capabilities: SOURCE_STANDARD,
    },
];

const METERS: &[InstrumentDescriptor] = &[
    InstrumentDescriptor {
        id: "AGILENT_34401A",
        role: InstrumentRole::Meter,
        dialect: InstrumentDialect::Agilent34401,
        simulation: false,
        capabilities: METER_STANDARD,
    },
    InstrumentDescriptor {
        id: "AGILENT_34411A",
        role: InstrumentRole::Meter,
        dialect: InstrumentDialect::Scpi,
        simulation: false,
        capabilities: METER_STANDARD,
    },
    InstrumentDescriptor {
        id: "KEYSIGHT_34411A",
        role: InstrumentRole::Meter,
        dialect: InstrumentDialect::Scpi,
        simulation: false,
        capabilities: METER_STANDARD,
    },
    InstrumentDescriptor {
        id: "KEYSIGHT_34465A",
        role: InstrumentRole::Meter,
        dialect: InstrumentDialect::Scpi,
        simulation: false,
        capabilities: METER_STANDARD,
    },
    InstrumentDescriptor {
        id: "ADCMT_7461A",
        role: InstrumentRole::Meter,
        dialect: InstrumentDialect::Adc7461,
        simulation: false,
        capabilities: METER_STANDARD,
    },
    InstrumentDescriptor {
        id: "SIMULATED_METER",
        role: InstrumentRole::Meter,
        dialect: InstrumentDialect::Simulation,
        simulation: true,
        capabilities: METER_STANDARD,
    },
];

/// Registered instruments for a role, in the established selection order.
pub const fn instrument_catalog(role: InstrumentRole) -> &'static [InstrumentDescriptor] {
    match role {
        InstrumentRole::Source => SOURCES,
        InstrumentRole::Meter => METERS,
    }
}

/// Find a registered model by its stable configuration identifier.
pub fn instrument_descriptor(id: &str) -> Option<&'static InstrumentDescriptor> {
    SOURCES
        .iter()
        .chain(METERS)
        .find(|descriptor| descriptor.id == id)
}

/// A selected source range. `command_value` preserves legacy range codes such as 7651's 2..6.
#[derive(Debug, Clone, PartialEq)]
pub struct SourceRangeProfile {
    pub name: String,
    pub command_value: f64,
    pub maximum: f64,
    pub resolution: f64,
}

/// A source protection limit, expressed in the matching SI unit.
#[derive(Debug, Clone, PartialEq)]
pub struct SourceLimitProfile {
    pub minimum: f64,
    pub maximum: f64,
    pub resolution: Option<f64>,
}

/// Read-only source metadata parsed from `specs/sources.toml`.
#[derive(Debug, Clone, PartialEq)]
pub struct SourceProfile {
    pub maximum_voltage: f64,
    pub maximum_current: f64,
    pub voltage_limit: SourceLimitProfile,
    pub current_limit: SourceLimitProfile,
    pub voltage_ranges: Vec<SourceRangeProfile>,
    pub current_ranges: Vec<SourceRangeProfile>,
}

/// The meter integration-time policy specified by the TOML profile.
#[derive(Debug, Clone, PartialEq)]
pub enum NplcProfile {
    Discrete(Vec<f64>),
    Continuous { minimum: f64, maximum: f64 },
}

/// Read-only meter metadata parsed from `specs/meters.toml`.
#[derive(Debug, Clone, PartialEq)]
pub struct MeterProfile {
    pub maximum_input_voltage: f64,
    pub maximum_input_current: f64,
    pub auto_range_required: bool,
    /// Documented manual DC-voltage ranges in V. An empty list means manual
    /// range selection is not supported by this application profile.
    pub voltage_ranges: Vec<f64>,
    /// Documented manual DC-current ranges in A. An empty list means manual
    /// range selection is not supported by this application profile.
    pub current_ranges: Vec<f64>,
    pub nplc: NplcProfile,
}

/// Numeric profile for an instrument. Values come directly from the embedded TOML specs.
#[derive(Debug, Clone, PartialEq)]
pub enum InstrumentProfile {
    Source(SourceProfile),
    Meter(MeterProfile),
}

fn range_profiles(ranges: &[SourceRange]) -> Vec<SourceRangeProfile> {
    ranges
        .iter()
        .map(|range| SourceRangeProfile {
            name: range.name.clone(),
            command_value: range.command.value(),
            maximum: range.maximum,
            resolution: range.resolution,
        })
        .collect()
}

fn source_profile(spec: &SourceModelSpec) -> SourceProfile {
    SourceProfile {
        maximum_voltage: spec.max_abs_v,
        maximum_current: spec.max_abs_a,
        voltage_limit: SourceLimitProfile {
            minimum: spec.voltage_limit.minimum,
            maximum: spec.voltage_limit.maximum,
            resolution: spec.voltage_limit.resolution,
        },
        current_limit: SourceLimitProfile {
            minimum: spec.current_limit.minimum,
            maximum: spec.current_limit.maximum,
            resolution: spec.current_limit.resolution,
        },
        voltage_ranges: range_profiles(&spec.voltage_ranges),
        current_ranges: range_profiles(&spec.current_ranges),
    }
}

fn meter_profile(id: &str, spec: &MeterModelSpec) -> Result<MeterProfile, String> {
    let required = |value: Option<f64>, key: &str| {
        value.ok_or_else(|| format!("Embedded profile {id} is missing numeric {key}."))
    };
    let nplc = match &spec.nplc_values {
        Some(values) => NplcProfile::Discrete(values.clone()),
        None => NplcProfile::Continuous {
            minimum: required(spec.nplc_min, "nplc_min")?,
            maximum: required(spec.nplc_max, "nplc_max")?,
        },
    };
    Ok(MeterProfile {
        maximum_input_voltage: spec.max_input_v,
        maximum_input_current: spec.max_input_a,
        auto_range_required: spec.auto_range_required,
        voltage_ranges: validated_ranges(&spec.voltage_ranges, id, "voltage_ranges")?,
        current_ranges: validated_ranges(&spec.current_ranges, id, "current_ranges")?,
        nplc,
    })
}

fn validated_ranges(ranges: &[f64], id: &str, key: &str) -> Result<Vec<f64>, String> {
    ranges
        .iter()
        .map(|&value| {
            if !value.is_finite() || value <= 0.0 {
                return Err(format!("Embedded profile {id} has an invalid {key} entry."));
            }
            Ok(value)
        })
        .collect()
}

/// Load a registered instrument's numeric profile. Unknown model ids return `Ok(None)`.
pub fn instrument_profile(id: &str) -> Result<Option<InstrumentProfile>, String> {
    let Some(descriptor) = instrument_descriptor(id) else {
        return Ok(None);
    };
    let profile = match descriptor.role {
        InstrumentRole::Source => {
            let spec = source_model(id)?
                .ok_or_else(|| format!("Embedded source profile for {id} is missing."))?;
            InstrumentProfile::Source(source_profile(spec))
        }
        InstrumentRole::Meter => {
            let spec = meter_model(id)?
                .ok_or_else(|| format!("Embedded meter profile for {id} is missing."))?;
            InstrumentProfile::Meter(meter_profile(id, spec)?)
        }
    };
    Ok(Some(profile))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Rows of the README "Hardware / 対応機器" table as (role cell, model IDs, route cell).
    /// Model IDs are the backticked tokens of the "Role · model ID" column.
    fn readme_hardware_rows(readme: &str) -> Vec<(String, Vec<String>, String)> {
        let section = readme
            .split("\n## Hardware / 対応機器")
            .nth(1)
            .expect("README has a Hardware / 対応機器 section");
        let section = section.split("\n## ").next().unwrap();
        section
            .lines()
            .filter(|line| line.starts_with('|'))
            .skip(2) // header and separator
            .map(|line| {
                let cells: Vec<&str> = line.trim_matches('|').split('|').map(str::trim).collect();
                assert_eq!(cells.len(), 3, "unexpected README hardware row: {line}");
                let ids = cells[1]
                    .split('`')
                    .skip(1)
                    .step_by(2)
                    .map(str::to_owned)
                    .collect::<Vec<_>>();
                assert!(
                    !ids.is_empty(),
                    "README hardware row has no model ID: {line}"
                );
                (cells[1].to_owned(), ids, cells[2].to_owned())
            })
            .collect()
    }

    #[test]
    fn every_registered_model_is_listed_in_the_readme_and_supports_its_dc_modes() {
        let rows = readme_hardware_rows(include_str!("../../../README.md"));
        let mut documented = Vec::new();
        for (role_cell, ids, route_cell) in &rows {
            for id in ids {
                let descriptor = instrument_descriptor(id)
                    .unwrap_or_else(|| panic!("README lists unregistered model ID {id}"));
                let role = match descriptor.role {
                    InstrumentRole::Source => "Source",
                    InstrumentRole::Meter => "Meter",
                };
                assert!(
                    role_cell.starts_with(role),
                    "README lists {id} as '{role_cell}', catalog role is {role}"
                );
                assert_eq!(
                    descriptor.simulation,
                    route_cell.starts_with("Simulation only"),
                    "README physical-connection column for {id} disagrees with the catalog simulation flag"
                );
                documented.push(id.as_str());
            }
        }
        let mut registered = Vec::new();
        for role in [InstrumentRole::Source, InstrumentRole::Meter] {
            for descriptor in instrument_catalog(role) {
                assert!(
                    descriptor.supports_mode(crate::Mode::Iv),
                    "{} lacks IV",
                    descriptor.id
                );
                assert!(
                    descriptor.supports_mode(crate::Mode::Vi),
                    "{} lacks VI",
                    descriptor.id
                );
                registered.push(descriptor.id);
            }
        }
        let mut sorted_documented = documented.clone();
        sorted_documented.sort_unstable();
        sorted_documented.dedup();
        assert_eq!(
            documented.len(),
            sorted_documented.len(),
            "README repeats a model ID"
        );
        registered.sort_unstable();
        assert_eq!(
            sorted_documented, registered,
            "README hardware table and instrument catalog differ"
        );
    }

    #[test]
    fn catalog_keeps_the_established_selection_order() {
        assert_eq!(
            instrument_catalog(InstrumentRole::Source)
                .iter()
                .map(|descriptor| descriptor.id)
                .collect::<Vec<_>>(),
            ["YOKOGAWA_GS210", "YOKOGAWA_7651", "SIMULATED_SOURCE"]
        );
        assert_eq!(
            instrument_catalog(InstrumentRole::Meter)
                .iter()
                .map(|descriptor| descriptor.id)
                .collect::<Vec<_>>(),
            [
                "AGILENT_34401A",
                "AGILENT_34411A",
                "KEYSIGHT_34411A",
                "KEYSIGHT_34465A",
                "ADCMT_7461A",
                "SIMULATED_METER",
            ]
        );
    }

    #[test]
    fn every_registered_model_has_a_matching_authoritative_toml_profile() {
        for descriptor in SOURCES.iter().chain(METERS) {
            let profile = instrument_profile(descriptor.id).unwrap();
            assert!(profile.is_some(), "{} has no profile", descriptor.id);
            match (descriptor.role, profile.unwrap()) {
                (InstrumentRole::Source, InstrumentProfile::Source(profile)) => {
                    assert!(!profile.voltage_ranges.is_empty());
                    assert!(!profile.current_ranges.is_empty());
                    assert!(profile.maximum_voltage > 0.0);
                    assert!(profile.maximum_current > 0.0);
                }
                (InstrumentRole::Meter, InstrumentProfile::Meter(profile)) => {
                    assert!(profile.maximum_input_voltage > 0.0);
                    assert!(profile.maximum_input_current > 0.0);
                }
                _ => panic!("profile role differs from catalog"),
            }
        }
        assert!(instrument_profile("UNKNOWN").unwrap().is_none());
    }

    #[test]
    fn catalog_capabilities_match_the_toml_role_and_mode_declarations() {
        use crate::model_spec::{MeasureMode, SourceMode, SpecRole};
        for descriptor in SOURCES {
            let spec = source_model(descriptor.id).unwrap().unwrap();
            assert_eq!(spec.roles.first(), Some(&SpecRole::Source));
            let InstrumentCapabilities::Source(capabilities) = descriptor.capabilities else {
                panic!("source has meter capabilities");
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
        }
        for descriptor in METERS {
            let spec = meter_model(descriptor.id).unwrap().unwrap();
            assert_eq!(spec.roles.first(), Some(&SpecRole::Meter));
            let InstrumentCapabilities::Meter(capabilities) = descriptor.capabilities else {
                panic!("meter has source capabilities");
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
}
