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
        return Err("Meter fixed range was not confirmed: Auto is still enabled. Check Meter range in Settings and reconnect.".into());
    }
    let range = positive_reading(&query(&format!("SENS:{measure}:DC:RANG?"))?);
    check(config, range)
}

pub(super) fn check(config: &RunConfig, actual: Option<f64>) -> Result<(), String> {
    if iv_core::selected_meter_range(config).is_none() {
        return Ok(());
    }
    let actual = actual.ok_or(
        "Meter fixed range could not be read back. Check Meter range in Settings and reconnect.",
    )?;
    iv_core::verify_meter_range(iv_core::selected_meter_range(config), actual).map_err(|e| {
        format!("{e} Check Meter range in Settings; choose a range covering the entire sweep.")
    })
}

/// Fixed-range violations shut down the sweep outputs before disk evidence or
/// additional conversions. Returns the same observation used by precision logs.
pub(super) fn observed(
    pair: &mut DevicePair,
    config: &RunConfig,
    state: &mut Snapshot,
    audit: &mut RawWriter,
    point: usize,
    reading: Option<u32>,
    begin: Instant,
) -> Result<RecordedObservation, String> {
    let measure = iv_core::measurement_definition(config.mode).meter_function;
    let command = format!("SENS:{measure}:DC:RANG?");
    let (result, exchanges, timing) = query_without_audit(pair, Role::Meter, &command, begin);
    let validation = result
        .as_ref()
        .map_err(Clone::clone)
        .and_then(|reply| check(config, positive_reading(reply)));
    let stop = if validation.is_err() {
        protection_output_off(Some(pair), state).err()
    } else {
        None
    };
    let evidence = append_transport_exchanges(
        audit, "meter", &command, point, reading, &exchanges, &timing,
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

pub(super) fn conversion(
    pair: &mut DevicePair,
    config: &RunConfig,
    state: &mut Snapshot,
    audit: &mut RawWriter,
    origin: (usize, Option<u32>),
    begin: Instant,
    reading: &DeferredMeterEvidence,
) -> Result<RecordedObservation, String> {
    let checked = observed(pair, config, state, audit, origin.0, origin.1, begin);
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

#[cfg(test)]
mod tests {
    use super::*;
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
