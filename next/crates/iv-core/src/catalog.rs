//! Stable instrument catalog and read-only profiles derived from the embedded TOML specs.
//!
//! Descriptors describe protocol-level capabilities. Numeric operating limits remain in
//! `specs/sources.toml` and `specs/meters.toml`; [`instrument_profile`] parses those
//! authoritative resources instead of keeping a second numeric table in Rust.

use std::sync::OnceLock;

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
    pub nplc: NplcProfile,
}

/// Numeric profile for an instrument. Values come directly from the embedded TOML specs.
#[derive(Debug, Clone, PartialEq)]
pub enum InstrumentProfile {
    Source(SourceProfile),
    Meter(MeterProfile),
}

fn sources() -> Result<&'static toml::Value, String> {
    static SPECS: OnceLock<Result<toml::Value, String>> = OnceLock::new();
    SPECS
        .get_or_init(|| {
            toml::from_str(include_str!("../../../specs/sources.toml"))
                .map_err(|error| format!("embedded source specification is invalid: {error}"))
        })
        .as_ref()
        .map_err(Clone::clone)
}

fn meters() -> Result<&'static toml::Value, String> {
    static SPECS: OnceLock<Result<toml::Value, String>> = OnceLock::new();
    SPECS
        .get_or_init(|| {
            toml::from_str(include_str!("../../../specs/meters.toml"))
                .map_err(|error| format!("embedded meter specification is invalid: {error}"))
        })
        .as_ref()
        .map_err(Clone::clone)
}

fn number(value: &toml::Value) -> Option<f64> {
    value
        .as_float()
        .or_else(|| value.as_integer().map(|value| value as f64))
}

fn required_number(table: &toml::Value, key: &str, id: &str) -> Result<f64, String> {
    table
        .get(key)
        .and_then(number)
        .ok_or_else(|| format!("Embedded profile {id} is missing numeric {key}."))
}

fn required_bool(table: &toml::Value, key: &str, id: &str) -> Result<bool, String> {
    table
        .get(key)
        .and_then(toml::Value::as_bool)
        .ok_or_else(|| format!("Embedded profile {id} is missing boolean {key}."))
}

fn source_ranges(
    table: &toml::Value,
    id: &str,
    key: &str,
    maximum_key: &str,
    resolution_key: &str,
) -> Result<Vec<SourceRangeProfile>, String> {
    table
        .get(key)
        .and_then(toml::Value::as_array)
        .ok_or_else(|| format!("Embedded profile {id} is missing {key}."))?
        .iter()
        .map(|row| {
            let name = row
                .get("name")
                .and_then(toml::Value::as_str)
                .ok_or_else(|| format!("Embedded profile {id} has a range without name."))?
                .to_owned();
            Ok(SourceRangeProfile {
                name,
                command_value: required_number(row, "command_value", id)?,
                maximum: required_number(row, maximum_key, id)?,
                resolution: required_number(row, resolution_key, id)?,
            })
        })
        .collect()
}

fn source_profile(id: &str, table: &toml::Value) -> Result<SourceProfile, String> {
    Ok(SourceProfile {
        maximum_voltage: required_number(table, "max_abs_V", id)?,
        maximum_current: required_number(table, "max_abs_A", id)?,
        voltage_limit: SourceLimitProfile {
            minimum: required_number(table, "voltage_limit_min_V", id)?,
            maximum: required_number(table, "voltage_limit_max_V", id)?,
            resolution: table.get("voltage_limit_resolution_V").and_then(number),
        },
        current_limit: SourceLimitProfile {
            minimum: required_number(table, "current_limit_min_A", id)?,
            maximum: required_number(table, "current_limit_max_A", id)?,
            resolution: table.get("current_limit_resolution_A").and_then(number),
        },
        voltage_ranges: source_ranges(table, id, "voltage_ranges", "max_abs_V", "resolution_V")?,
        current_ranges: source_ranges(table, id, "current_ranges", "max_abs_A", "resolution_A")?,
    })
}

fn meter_profile(id: &str, table: &toml::Value) -> Result<MeterProfile, String> {
    let nplc = match table.get("nplc_values").and_then(toml::Value::as_array) {
        Some(values) => NplcProfile::Discrete(
            values
                .iter()
                .map(|value| {
                    number(value).ok_or_else(|| {
                        format!("Embedded profile {id} has a non-numeric nplc_values entry.")
                    })
                })
                .collect::<Result<Vec<_>, _>>()?,
        ),
        None => NplcProfile::Continuous {
            minimum: required_number(table, "nplc_min", id)?,
            maximum: required_number(table, "nplc_max", id)?,
        },
    };
    Ok(MeterProfile {
        maximum_input_voltage: required_number(table, "max_input_V", id)?,
        maximum_input_current: required_number(table, "max_input_A", id)?,
        auto_range_required: required_bool(table, "auto_range_required", id)?,
        nplc,
    })
}

/// Load a registered instrument's numeric profile. Unknown model ids return `Ok(None)`.
pub fn instrument_profile(id: &str) -> Result<Option<InstrumentProfile>, String> {
    let Some(descriptor) = instrument_descriptor(id) else {
        return Ok(None);
    };
    let profile = match descriptor.role {
        InstrumentRole::Source => {
            let table = sources()?
                .get(id)
                .ok_or_else(|| format!("Embedded source profile for {id} is missing."))?;
            InstrumentProfile::Source(source_profile(id, table)?)
        }
        InstrumentRole::Meter => {
            let table = meters()?
                .get(id)
                .ok_or_else(|| format!("Embedded meter profile for {id} is missing."))?;
            InstrumentProfile::Meter(meter_profile(id, table)?)
        }
    };
    Ok(Some(profile))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_registered_model_is_listed_in_the_readme_and_supports_its_dc_modes() {
        let readme = include_str!("../../../../README.md");
        for role in [InstrumentRole::Source, InstrumentRole::Meter] {
            for descriptor in instrument_catalog(role) {
                assert!(
                    readme.contains(descriptor.id),
                    "README is missing {}",
                    descriptor.id
                );
                assert!(descriptor.supports_mode(crate::Mode::Iv));
                assert!(descriptor.supports_mode(crate::Mode::Vi));
            }
        }
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
        for descriptor in SOURCES {
            let table = sources().unwrap().get(descriptor.id).unwrap();
            assert_eq!(
                table["roles"].as_array().unwrap()[0].as_str(),
                Some("source")
            );
            let InstrumentCapabilities::Source(capabilities) = descriptor.capabilities else {
                panic!("source has meter capabilities");
            };
            assert_eq!(
                capabilities.output,
                required_bool(table, "supports_output", descriptor.id).unwrap()
            );
            assert_eq!(
                capabilities.software_ramp,
                required_bool(table, "supports_software_ramp", descriptor.id).unwrap()
            );
            let modes = table["source_modes"].as_array().unwrap();
            assert_eq!(
                capabilities.voltage,
                modes.iter().any(|mode| mode.as_str() == Some("voltage"))
            );
            assert_eq!(
                capabilities.current,
                modes.iter().any(|mode| mode.as_str() == Some("current"))
            );
        }
        for descriptor in METERS {
            let table = meters().unwrap().get(descriptor.id).unwrap();
            assert_eq!(
                table["roles"].as_array().unwrap()[0].as_str(),
                Some("meter")
            );
            let InstrumentCapabilities::Meter(capabilities) = descriptor.capabilities else {
                panic!("meter has source capabilities");
            };
            let modes = table["measure_modes"].as_array().unwrap();
            assert_eq!(
                capabilities.voltage,
                modes.iter().any(|mode| mode.as_str() == Some("dc_voltage"))
            );
            assert_eq!(
                capabilities.current,
                modes.iter().any(|mode| mode.as_str() == Some("dc_current"))
            );
        }
    }
}
