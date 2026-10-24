//! Stable definitions for the IV measurement modes and sweep patterns.

use crate::{InstrumentRole, Mode, Pattern, RunConfig};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlotAxis {
    pub name: &'static str,
    pub unit: &'static str,
}

const REQUIRED_ROLES: &[InstrumentRole] = &[InstrumentRole::Source, InstrumentRole::Meter];

/// Symbolic units resolve against the measurement's source and meter roles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParameterUnit {
    Source,
    Measured,
    Seconds,
    PowerLineCycles,
    Count,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParameterDefinition {
    pub key: &'static str,
    pub label: &'static str,
    pub unit: ParameterUnit,
}

const PARAMETERS: &[ParameterDefinition] = &[
    ParameterDefinition {
        key: "start",
        label: "Start",
        unit: ParameterUnit::Source,
    },
    ParameterDefinition {
        key: "stop",
        label: "End",
        unit: ParameterUnit::Source,
    },
    ParameterDefinition {
        key: "step",
        label: "Step",
        unit: ParameterUnit::Source,
    },
    ParameterDefinition {
        key: "custom_points",
        label: "Custom points",
        unit: ParameterUnit::Source,
    },
    ParameterDefinition {
        key: "source_limit",
        label: "Maximum source target",
        unit: ParameterUnit::Source,
    },
    ParameterDefinition {
        key: "compliance",
        label: "Measurement limit",
        unit: ParameterUnit::Measured,
    },
    ParameterDefinition {
        key: "source_compliance",
        label: "Source compliance",
        unit: ParameterUnit::Measured,
    },
    ParameterDefinition {
        key: "settle_s",
        label: "Wait time (s)",
        unit: ParameterUnit::Seconds,
    },
    ParameterDefinition {
        key: "start_settle_s",
        label: "First wait (s)",
        unit: ParameterUnit::Seconds,
    },
    ParameterDefinition {
        key: "nplc",
        label: "NPLC",
        unit: ParameterUnit::PowerLineCycles,
    },
    ParameterDefinition {
        key: "repeat",
        label: "Readings per point (Repeat)",
        unit: ParameterUnit::Count,
    },
    ParameterDefinition {
        key: "ramp_step",
        label: "Ramp step",
        unit: ParameterUnit::Source,
    },
    ParameterDefinition {
        key: "ramp_wait_s",
        label: "Ramp wait (s)",
        unit: ParameterUnit::Seconds,
    },
];

/// Existing parameter label, resolved from the measurement definition table.
pub fn parameter_definition(key: &str) -> Option<&'static ParameterDefinition> {
    PARAMETERS.iter().find(|parameter| parameter.key == key)
}

/// Describes the source, protection, and meter functions used by a measurement mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MeasurementDefinition {
    pub mode: Mode,
    pub required_roles: &'static [InstrumentRole],
    pub axes: [PlotAxis; 2],
    pub preferred_unit_index: usize,
    pub parameters: &'static [ParameterDefinition],
    pub label: &'static str,
    pub source_function: &'static str,
    pub protection_function: &'static str,
    pub meter_function: &'static str,
    pub source_unit: &'static str,
    pub meter_unit: &'static str,
}

const IV_DEFINITION: MeasurementDefinition = MeasurementDefinition {
    mode: Mode::Iv,
    required_roles: REQUIRED_ROLES,
    axes: [
        PlotAxis {
            name: "Voltage",
            unit: "V",
        },
        PlotAxis {
            name: "Current",
            unit: "A",
        },
    ],
    preferred_unit_index: 1,
    parameters: PARAMETERS,
    label: "V-I (source V, measure I)",
    source_function: "VOLT",
    protection_function: "CURR",
    meter_function: "CURR",
    source_unit: "V",
    meter_unit: "A",
};

const VI_DEFINITION: MeasurementDefinition = MeasurementDefinition {
    mode: Mode::Vi,
    required_roles: REQUIRED_ROLES,
    axes: [
        PlotAxis {
            name: "Current",
            unit: "A",
        },
        PlotAxis {
            name: "Voltage",
            unit: "V",
        },
    ],
    preferred_unit_index: 2,
    parameters: PARAMETERS,
    label: "I-V (source I, measure V)",
    source_function: "CURR",
    protection_function: "VOLT",
    meter_function: "VOLT",
    source_unit: "A",
    meter_unit: "V",
};

impl MeasurementDefinition {
    /// The existing mode-switch defaults, distinct from instrument hard limits.
    pub fn default_config(self) -> RunConfig {
        let mut config = RunConfig {
            mode: self.mode,
            ..RunConfig::default()
        };
        if self.mode == Mode::Vi {
            config.start = -1e-6;
            config.stop = 1e-6;
            config.step = 1e-7;
            config.source_limit = 1e-6;
            config.compliance = 1.0;
            config.ramp_step = 1e-7;
        }
        config
    }
}

/// Return the existing mode definition without changing any planning behavior.
pub const fn measurement_definition(mode: Mode) -> &'static MeasurementDefinition {
    match mode {
        Mode::Iv => &IV_DEFINITION,
        Mode::Vi => &VI_DEFINITION,
    }
}

/// A displayable sweep pattern in the original selector order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SweepDefinition {
    pub pattern: Pattern,
    pub label: &'static str,
}

impl SweepDefinition {
    pub const fn description(&self) -> &'static str {
        match self.pattern {
            Pattern::Linear => {
                "Start -> End. Direction follows the endpoints; Step sets the magnitude. The last interval may be shorter to reach End exactly."
            }
            Pattern::RoundTrip => {
                "Start -> End -> Start. Each endpoint is reached exactly; turning points are not duplicated. The last interval of each segment may be shorter than Step."
            }
            Pattern::ZeroCentered => {
                "0 -> End -> Start -> 0, within the specified endpoints. The interval must include zero; either endpoint may be zero. The last interval of each segment may be shorter than Step."
            }
            Pattern::CustomList => {
                "Follow Custom points in their entered order, including repeated values. Start, End and Step do not generate the list."
            }
        }
    }
}

const SWEEP_DEFINITIONS: &[SweepDefinition] = &[
    SweepDefinition {
        pattern: Pattern::Linear,
        label: "linear",
    },
    SweepDefinition {
        pattern: Pattern::RoundTrip,
        label: "round_trip",
    },
    SweepDefinition {
        pattern: Pattern::ZeroCentered,
        label: "zero_centered",
    },
    SweepDefinition {
        pattern: Pattern::CustomList,
        label: "custom_list",
    },
];

/// Registered sweep patterns in the established selector order.
pub const fn sweep_definitions() -> &'static [SweepDefinition] {
    SWEEP_DEFINITIONS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parameter_keys_axes_and_mode_defaults_match_the_existing_configuration() {
        let serialized = serde_json::to_value(RunConfig::default()).unwrap();
        for mode in [Mode::Iv, Mode::Vi] {
            let definition = measurement_definition(mode);
            assert_eq!(
                definition.required_roles,
                &[InstrumentRole::Source, InstrumentRole::Meter]
            );
            assert_eq!(definition.axes[0].unit, definition.source_unit);
            assert_eq!(definition.axes[1].unit, definition.meter_unit);
            for parameter in definition.parameters {
                assert!(serialized.get(parameter.key).is_some(), "{}", parameter.key);
                assert_eq!(parameter_definition(parameter.key), Some(parameter));
            }
        }
        assert_eq!(
            measurement_definition(Mode::Iv).default_config(),
            RunConfig::default()
        );
        let vi = measurement_definition(Mode::Vi).default_config();
        assert_eq!((vi.start, vi.stop, vi.step), (-1e-6, 1e-6, 1e-7));
        assert_eq!(
            (vi.source_limit, vi.compliance, vi.ramp_step),
            (1e-6, 1.0, 1e-7)
        );
        assert_eq!(parameter_definition("stop").unwrap().label, "End");
        assert!(parameter_definition("not-a-config-field").is_none());
    }

    #[test]
    fn mode_definitions_preserve_existing_ui_strings_and_scpi_functions() {
        let iv = measurement_definition(Mode::Iv);
        assert_eq!(iv.label, "V-I (source V, measure I)");
        assert_eq!(iv.source_function, "VOLT");
        assert_eq!(iv.protection_function, "CURR");
        assert_eq!(iv.meter_function, "CURR");
        assert_eq!((iv.source_unit, iv.meter_unit), ("V", "A"));

        let vi = measurement_definition(Mode::Vi);
        assert_eq!(vi.label, "I-V (source I, measure V)");
        assert_eq!(vi.source_function, "CURR");
        assert_eq!(vi.protection_function, "VOLT");
        assert_eq!(vi.meter_function, "VOLT");
        assert_eq!((vi.source_unit, vi.meter_unit), ("A", "V"));
    }

    #[test]
    fn sweep_definitions_keep_the_existing_order_and_labels() {
        assert_eq!(
            sweep_definitions()
                .iter()
                .map(|definition| (definition.pattern, definition.label))
                .collect::<Vec<_>>(),
            [
                (Pattern::Linear, "linear"),
                (Pattern::RoundTrip, "round_trip"),
                (Pattern::ZeroCentered, "zero_centered"),
                (Pattern::CustomList, "custom_list"),
            ]
        );
    }
}
