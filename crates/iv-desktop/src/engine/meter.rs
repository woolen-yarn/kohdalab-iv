//! Shared Auto/fixed policy for normal, balanced and idle acquisition.
use super::*;

pub(super) fn commands(config: &RunConfig) -> Result<Vec<String>, String> {
    iv_core::validate_meter_range(config)?;
    let measure = iv_core::measurement_definition(config.mode).meter_function;
    Ok(match iv_core::selected_meter_range(config) {
        None => vec![format!("SENS:{measure}:DC:RANG:AUTO ON")],
        Some(range) => vec![
            format!("SENS:{measure}:DC:RANG:AUTO OFF"),
            format!("SENS:{measure}:DC:RANG {range:.12e}"),
        ],
    })
}

pub(super) fn verify(
    config: &RunConfig,
    mut query: impl FnMut(&str) -> Result<String, String>,
) -> Result<(), String> {
    if iv_core::selected_meter_range(config).is_none() {
        return Ok(());
    }
    let measure = iv_core::measurement_definition(config.mode).meter_function;
    let auto = query(&format!("SENS:{measure}:DC:RANG:AUTO?"))?;
    if !matches!(
        auto.trim().to_ascii_uppercase().as_str(),
        "0" | "+0" | "OFF"
    ) {
        return Err("Meter fixed range was not confirmed: Auto is still enabled. Check Meter range in the Meter card and reconnect.".into());
    }
    let range = positive_reading(&query(&format!("SENS:{measure}:DC:RANG?"))?);
    check(config, range)
}

pub(super) fn check(config: &RunConfig, actual: Option<f64>) -> Result<(), String> {
    if iv_core::selected_meter_range(config).is_none() {
        return Ok(());
    }
    let actual = actual.ok_or(
        "Meter fixed range could not be read back. Check Meter range in the Meter card and reconnect.",
    )?;
    iv_core::verify_meter_range(iv_core::selected_meter_range(config), actual).map_err(|e| {
        format!(
            "{e} Check Meter range in the Meter card; choose a range covering the entire sweep."
        )
    })
}

/// One guard spans the complete sweep, including live/settling conversions.
/// Electrical-loading tables are loaded once, outside the acquisition loop.
pub(super) struct RangeGuard {
    previous: Option<f64>,
    resistance: Vec<iv_core::MeterRangeResistance>,
    auto_voltage_impedance: Option<bool>,
}

/// `scpi_route` is [`DevicePair::scpi_range_route`] for the meter role. It
/// keeps the unvalidated ADCMT 7461A ADC-command USB route Auto-only, without
/// any range query, while GPIB SCPI reads the active range per conversion.
pub(super) fn monitored(config: &RunConfig, scpi_route: bool) -> bool {
    iv_core::selected_meter_range(config).is_some()
        || iv_core::meter_display_queries(&config.meter_model, config.mode)
            .is_ok_and(|queries| queries.is_some())
        || (scpi_route && iv_core::meter_range_readback(&config.meter_model, config.mode))
}

/// Route for `role`; simulated runs have no transport and no ADC USB route.
pub(super) fn route(pair: Option<&DevicePair>, role: Role) -> bool {
    pair.is_none_or(|pair| pair.scpi_range_route(role))
}

impl RangeGuard {
    pub(super) fn new(config: &RunConfig) -> Self {
        Self {
            previous: None,
            auto_voltage_impedance: None,
            resistance: iv_core::meter_range_resistances(&config.meter_model, config.mode),
        }
    }

    pub(super) fn reset_baseline(&mut self) {
        self.previous = None;
    }

    fn loading(&self, range: f64) -> Option<iv_core::MeterRangeResistance> {
        self.resistance
            .iter()
            .copied()
            .find(|r| same_range(r.range, range))
    }

    pub(super) fn check(&mut self, config: &RunConfig, actual: Option<f64>) -> Result<(), String> {
        check(config, actual)?;
        let actual = actual.ok_or(
            "Meter range readback is unavailable. Stop and reconnect before measuring again.",
        )?;
        if iv_core::selected_meter_range(config).is_none()
            && let Some(previous) = self.previous
            && !same_range(previous, actual)
        {
            let old = self.loading(previous);
            let new = self.loading(actual);
            let unchanged = self.auto_voltage_impedance.is_some_and(|auto| {
                let input = |range| {
                    [0.1, 1.0, 10.0, 100.0, 1000.0]
                        .iter()
                        .position(|&nominal| same_range(nominal, range))
                        .map(|index| auto && index < 3)
                };
                matches!((input(previous), input(actual)), (Some(a), Some(b)) if a == b)
            });
            if !unchanged
                && !matches!((old,new), (Some(a),Some(b)) if a.kind == b.kind && a.ohms == b.ohms)
            {
                let unit = if config.mode == Mode::Iv { "A" } else { "V" };
                let detail = match (old, new) {
                    (Some(a), Some(b)) => {
                        let label = if a.kind == iv_core::MeterResistanceKind::Shunt {
                            "Shunt resistance"
                        } else {
                            "Input resistance"
                        };
                        format!("{label}: {:.6e} ohm → {:.6e} ohm.", a.ohms, b.ohms)
                    }
                    _ => "Unchanged input resistance could not be verified.".to_owned(),
                };
                return Err(format!(
                    "Meter range transition detected.\nRange: {previous:.6e} → {actual:.6e} {unit}. {detail}\nSelect a fixed Meter range covering the whole sweep, review Meter limit separately, and repeat the complete sweep. The transition reading was not accepted."
                ));
            }
        }
        self.previous = Some(actual);
        Ok(())
    }

    /// OFF precedes both the triggering range/READ evidence and further I/O.
    pub(super) fn observed(
        &mut self,
        pair: &mut DevicePair,
        config: &RunConfig,
        state: &mut Snapshot,
        audit: &mut RawWriter,
        origin: (usize, Option<u32>),
        begin: Instant,
    ) -> Result<RecordedObservation, String> {
        let measure = iv_core::measurement_definition(config.mode).meter_function;
        let command = format!("SENS:{measure}:DC:RANG?");
        let (result, exchanges, timing) = query_without_audit(pair, Role::Meter, &command, begin);
        let validation = result
            .as_ref()
            .map_err(Clone::clone)
            .and_then(|reply| self.check(config, positive_reading(reply)));
        let stop = if validation.is_err() {
            stop::protection_off(Some(pair), state).err()
        } else {
            None
        };
        let evidence = append_transport_exchanges(
            audit, "meter", &command, origin.0, origin.1, &exchanges, &timing,
        );
        if let Err(e) = validation {
            let message = stop.map_or(e.clone(), |s| {
                format!("{e} · Output OFF verification failed: {s}")
            });
            return Err(evidence
                .err()
                .map_or(message.clone(), |a| format!("{message} · {a}")));
        }
        evidence?;
        Ok(RecordedObservation {
            reply: result?,
            timing,
        })
    }

    /// Auto voltage sweeps: CONF resets the meter to its default range (the
    /// 34401A reports 10 V) and only the first READ? autoranges to where the
    /// idle input actually sits. Establish the baseline from that conversion,
    /// with the source verified OFF, so the powered sweep compares against
    /// the real idle range instead of the CONF default. The priming reading
    /// is raw evidence only; it never becomes an analysis CSV point.
    pub(super) fn prime(
        &mut self,
        pair: &mut DevicePair,
        config: &RunConfig,
        state: &mut Snapshot,
        audit: &mut RawWriter,
        begin: Instant,
    ) -> Result<(), String> {
        let output = query_with_audit(pair, Role::Source, ":OUTP?", audit, 0, None, begin)?;
        let off = safety::output_state(&output.reply);
        let verdict = if matches!(off, Ok(OutputState::Off)) {
            "ok"
        } else {
            "rejected"
        };
        audit.append(&serde_json::json!({
            "event":"meter_baseline_output_off_confirmation", "point_index":0,
            "command":":OUTP?", "normalized_response":output.reply,
            "result":verdict, "timing":output.timing.json(),
        }))?;
        if verdict != "ok" {
            return Err(format!(
                "Source output OFF was not confirmed before meter range priming ({:?}). Output was not enabled.",
                output.reply
            ));
        }
        let impedance_query = match config.meter_model.as_str() {
            "AGILENT_34401A" => Some("INP:IMP:AUTO?"),
            // ADC 7461A manual sections 5.2.1 and 6.7.4. The three
            // lowest DCV ranges share the selectable input impedance.
            "ADCMT_7461A" if pair.scpi_range_route(Role::Meter) => Some(":INPUT:IMPEDANCE:AUTO?"),
            _ => None,
        };
        if let Some(command) = impedance_query {
            let impedance = query_with_audit(pair, Role::Meter, command, audit, 0, None, begin)?;
            self.auto_voltage_impedance = match impedance.reply.trim() {
                "0" => Some(false),
                "1" => Some(true),
                _ => None,
            };
        }
        let (result, exchanges, timing) = query_without_audit(pair, Role::Meter, "READ?", begin);
        let parsed = result
            .as_ref()
            .ok()
            .and_then(|reply| reply.trim().parse::<f64>().ok());
        let rejection = match (&result, parsed) {
            (Err(e), _) => Some(format!("Meter range priming READ failed: {e}")),
            (Ok(reply), None) => Some(format!("Meter range priming reading is invalid: {reply}")),
            (Ok(_), Some(value)) if !value.is_finite() || value.abs() > 1e30 => {
                Some("Meter range priming reported overload or an invalid reading.".to_owned())
            }
            _ => None,
        };
        let evidence =
            append_transport_exchanges(audit, "meter", "READ?", 0, None, &exchanges, &timing)
                .and_then(|()| {
                    audit.append(&serde_json::json!({
                        "event":"meter_range_priming_reading", "point_index":0,
                        "reading_index":serde_json::Value::Null, "command":"READ?",
                        "normalized_response":result.as_ref().ok(),
                        "query_error":result.as_ref().err(), "error":rejection,
                        "parsed_value":parsed.filter(|v| v.is_finite()),
                        "source_output":"off_verified", "analysis_csv":false,
                        "timing":timing.json(),
                    }))
                });
        if let Some(rejection) = rejection {
            return Err(evidence
                .err()
                .map_or(rejection.clone(), |a| format!("{rejection} · {a}"))
                + " Output was not enabled.");
        }
        evidence?;
        let range = self.observed(pair, config, state, audit, (0, None), begin)?;
        audit.append(&serde_json::json!({
            "event":"meter_range_baseline", "point_index":0,
            "command":format!("SENS:{}:DC:RANG?", iv_core::measurement_definition(config.mode).meter_function),
            "normalized_response":range.reply, "parsed_range":positive_reading(&range.reply),
            "source_output":"off_verified", "timing":range.timing.json(),
        }))
    }

    pub(super) fn conversion(
        &mut self,
        pair: &mut DevicePair,
        config: &RunConfig,
        state: &mut Snapshot,
        audit: &mut RawWriter,
        origin: (usize, Option<u32>),
        acquisition: (Instant, &DeferredMeterEvidence),
    ) -> Result<RecordedObservation, String> {
        let (begin, reading) = acquisition;
        let checked = self.observed(pair, config, state, audit, origin, begin);
        let evidence = append_meter_reading_audit(
            audit,
            origin.0,
            origin.1,
            reading.reply.as_deref(),
            reading.query_error.as_deref(),
            &reading.exchanges,
            &reading.timing,
        );
        match (checked, evidence) {
            (Ok(range), Ok(())) => Ok(range),
            (Err(e), Ok(())) | (Ok(_), Err(e)) => Err(e),
            (Err(e), Err(a)) => Err(format!("{e} · {a}")),
        }
    }
}

fn same_range(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-10 * a.abs().max(b.abs())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn auto_only_allows_known_equal_loading_ranges() {
        let config = RunConfig::default();
        let mut guard = RangeGuard::new(&config);
        guard.check(&config, Some(0.0001)).unwrap();
        guard.check(&config, Some(0.001)).unwrap(); // both 200 ohm
        let error = guard.check(&config, Some(0.01)).unwrap_err();
        assert!(error.contains("Shunt resistance"));
        assert!(error.contains("fixed Meter range"));
        let mut c = config.clone();
        c.meter_model = "AGILENT_34401A".into();
        let mut guard = RangeGuard::new(&c);
        guard.check(&c, Some(0.01)).unwrap();
        guard.check(&c, Some(0.1)).unwrap(); // both 5 ohm
        assert!(guard.check(&c, Some(1.)).is_err());
        // A new run starts with a new baseline, not the prior run's range.
        assert!(RangeGuard::new(&c).check(&c, Some(1.)).is_ok());
    }

    #[test]
    fn unknown_loading_and_voltage_input_changes_fail_closed() {
        for (model, mode, start, end) in [
            ("KEYSIGHT_34465A", Mode::Iv, 1e-6, 1e-5),
            ("AGILENT_34411A", Mode::Vi, 0.1, 1.),
        ] {
            let c = RunConfig {
                meter_model: model.into(),
                mode,
                ..Default::default()
            };
            let mut guard = RangeGuard::new(&c);
            guard.check(&c, Some(start)).unwrap();
            assert!(
                guard
                    .check(&c, Some(end))
                    .unwrap_err()
                    .contains("could not be verified")
            );
        }
    }

    #[test]
    fn voltage_transition_requires_known_equal_34401a_input_resistance() {
        let config = RunConfig {
            meter_model: "AGILENT_34401A".into(),
            mode: Mode::Vi,
            ..Default::default()
        };
        for (setting, from, to, accepted) in [
            (None, 0.1, 1.0, false),
            (Some(false), 0.1, 1.0, true),
            (Some(false), 10.0, 100.0, true),
            (Some(true), 0.1, 1.0, true),
            (Some(true), 10.0, 100.0, false),
            (Some(true), 100.0, 10.0, false),
            (Some(true), 100.0, 1000.0, true),
            (Some(false), 1.0, 2.0, false),
        ] {
            let mut guard = RangeGuard::new(&config);
            guard.auto_voltage_impedance = setting;
            guard.check(&config, Some(from)).unwrap();
            assert_eq!(guard.check(&config, Some(to)).is_ok(), accepted);
        }
    }

    #[test]
    fn adcmt_voltage_is_monitored_only_on_the_scpi_route_and_any_transition_stops() {
        let mut config = RunConfig {
            meter_model: "ADCMT_7461A".into(),
            mode: Mode::Vi,
            ..Default::default()
        };
        assert!(monitored(&config, true));
        assert!(!monitored(&config, false), "ADC USB Auto stays unmonitored");
        config.mode = Mode::Iv;
        assert!(!monitored(&config, true), "DC current stays unmonitored");
        config.mode = Mode::Vi;
        // DCV input resistance is unverified: every Auto change stops.
        for (from, to) in [(0.1, 1.0), (1.0, 0.1), (1.0, 10.0)] {
            let mut guard = RangeGuard::new(&config);
            guard.check(&config, Some(from)).unwrap();
            guard.check(&config, Some(from)).unwrap();
            let error = guard.check(&config, Some(to)).unwrap_err();
            assert!(error.contains("could not be verified"), "{error}");
        }
        assert!(RangeGuard::new(&config).check(&config, None).is_err());
        // Fixed: a mismatched or missing per-sample readback fails closed.
        config.meter_voltage_range = Some(0.1);
        assert!(monitored(&config, false));
        let mut guard = RangeGuard::new(&config);
        guard.check(&config, Some(0.1)).unwrap();
        assert!(guard.check(&config, Some(1.0)).is_err());
        assert!(guard.check(&config, None).is_err());
        assert!(
            verify(&config, |cmd| Ok(if cmd.ends_with("AUTO?") {
                "1"
            } else {
                "0.1"
            }
            .into()))
            .is_err()
        );
        assert!(
            verify(&config, |cmd| Ok(if cmd.ends_with("AUTO?") {
                "0"
            } else {
                "+1.00000E+00"
            }
            .into()))
            .is_err()
        );
        assert!(
            verify(&config, |cmd| Ok(if cmd.ends_with("AUTO?") {
                "0"
            } else {
                "+1.00000E-01"
            }
            .into()))
            .is_ok()
        );
    }

    #[test]
    fn adcmt_voltage_auto_checks_the_read_back_impedance_setting() {
        let config = RunConfig {
            meter_model: "ADCMT_7461A".into(),
            mode: Mode::Vi,
            ..Default::default()
        };
        for (setting, allows_high_range) in [(true, false), (false, true)] {
            let mut guard = RangeGuard::new(&config);
            guard.auto_voltage_impedance = Some(setting);
            guard.check(&config, Some(0.1)).unwrap();
            guard.check(&config, Some(1.0)).unwrap();
            assert_eq!(guard.check(&config, Some(100.0)).is_ok(), allows_high_range);
        }
    }

    #[test]
    fn fixed_ranges_reject_even_same_shunt_range_changes() {
        let c = RunConfig {
            meter_current_range: Some(0.0001),
            ..Default::default()
        };
        assert!(RangeGuard::new(&c).check(&c, Some(0.001)).is_err());
    }

    #[test]
    fn fixed_configuration_is_explicit_and_auto_stays_compatible() {
        let mut config = RunConfig::default();
        assert_eq!(commands(&config).unwrap(), ["SENS:CURR:DC:RANG:AUTO ON"]);
        config.meter_current_range = Some(0.001);
        assert_eq!(
            commands(&config).unwrap(),
            [
                "SENS:CURR:DC:RANG:AUTO OFF",
                "SENS:CURR:DC:RANG 1.000000000000e-3"
            ]
        );
        config.mode = Mode::Vi;
        config.meter_voltage_range = Some(1.0);
        assert_eq!(
            commands(&config).unwrap(),
            [
                "SENS:VOLT:DC:RANG:AUTO OFF",
                "SENS:VOLT:DC:RANG 1.000000000000e0"
            ]
        );
    }

    #[test]
    fn fixed_setup_requires_auto_off_and_the_chosen_actual_range() {
        let config = RunConfig {
            meter_current_range: Some(0.001),
            ..Default::default()
        };
        assert!(
            verify(&config, |cmd| Ok(if cmd.ends_with("AUTO?") {
                "0"
            } else {
                "0.001"
            }
            .into()))
            .is_ok()
        );
        assert!(verify(&config, |_| Ok("1".into())).is_err());
        assert!(
            verify(&config, |cmd| Ok(if cmd.ends_with("AUTO?") {
                "0"
            } else {
                "0.01"
            }
            .into()))
            .is_err()
        );
    }
}
