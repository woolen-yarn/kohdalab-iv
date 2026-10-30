//! Optional DMM measuring the source quantity, never substituted with a setpoint.
use super::*;

pub(super) fn clear_reading(record: &mut Record) {
    record.reference_meter_value = None;
    record.reference_meter_range = None;
    record.reference_meter_resolution = None;
}

pub(super) fn validate_connections(
    config: &RunConfig,
    state: &Snapshot,
    pair: Option<&DevicePair>,
) -> Result<(), String> {
    match (&config.reference_meter, state.reference_meter_connected) {
        (None, false) => Ok(()),
        (None, true) => Err("A source-quantity Meter is connected but not assigned. Review Meter (V) / Meter (I) in Settings before Start.".into()),
        (Some(_), false) => Err("Connect the assigned source-quantity Meter before Start, or select None in Settings.".into()),
        (Some(reference), true) => {
            if state.reference_meter_simulation != state.source_simulation {
                return Err("Both Meters and Source must use real instruments together or simulation together.".into());
            }
            if !state.source_simulation && pair.is_none_or(|p| !p.is_connected(Role::ReferenceMeter) || p.model(Role::ReferenceMeter) != reference.model) {
                return Err("The connected source-quantity Meter does not match Settings. Disconnect and reconnect it.".into());
            }
            Ok(())
        }
    }
}

pub(super) fn meter_config(config: &RunConfig) -> Option<RunConfig> {
    let reference = config.reference_meter.as_ref()?;
    let mut result = config.clone();
    result.mode = if config.mode == Mode::Iv {
        Mode::Vi
    } else {
        Mode::Iv
    };
    result.meter_model.clone_from(&reference.model);
    result.reference_meter = None;
    result.balanced_source = None;
    Some(result)
}

pub(super) struct Observer {
    config: RunConfig,
    guard: meter::RangeGuard,
    resolution: MeterResolutionCache,
}

/// All setup happens while the sweep outputs are OFF.
pub(super) fn prepare(
    pair: &mut DevicePair,
    config: &RunConfig,
    audit: &mut RawWriter,
    begin: Instant,
) -> Result<Option<Observer>, String> {
    let Some(config) = meter_config(config) else {
        return Ok(None);
    };
    let function = iv_core::measurement_definition(config.mode).meter_function;
    let mut commands = vec![format!("CONF:{function}:DC")];
    commands.extend(meter::commands(&config)?);
    commands.extend([
        format!("SENS:{function}:DC:NPLC {:.12e}", config.nplc),
        format!("SENS:{function}:DC:ZERO:AUTO ON"),
        "TRIG:SOUR IMM".into(),
    ]);
    for command in commands {
        write_with_audit(pair, Role::ReferenceMeter, &command, audit, begin)?;
    }
    let error = query_with_audit(
        pair,
        Role::ReferenceMeter,
        "SYST:ERR?",
        audit,
        0,
        None,
        begin,
    )?
    .reply;
    if !error.trim_start().starts_with("0,") && !error.trim_start().starts_with("+0,") {
        return Err(format!(
            "Source-quantity Meter configuration failed: {error}"
        ));
    }
    meter::verify(&config, |cmd| {
        Ok(query_with_audit(pair, Role::ReferenceMeter, cmd, audit, 0, None, begin)?.reply)
    })?;
    let mut guard = meter::RangeGuard::new(&config);
    if meter::monitored(&config) {
        let range = query_with_audit(
            pair,
            Role::ReferenceMeter,
            &format!("SENS:{function}:DC:RANG?"),
            audit,
            0,
            None,
            begin,
        )?;
        guard.check(&config, positive_reading(&range.reply))?;
    }
    Ok(Some(Observer {
        config,
        guard,
        resolution: MeterResolutionCache::default(),
    }))
}

impl Observer {
    /// Validate before persisting: a failed/overload/range-transition reply
    /// cannot postpone the common protection output-off request.
    #[allow(clippy::too_many_arguments)]
    fn query(
        pair: &mut DevicePair,
        state: &mut Snapshot,
        audit: &mut RawWriter,
        begin: Instant,
        point: usize,
        index: u32,
        command: &str,
        validate: impl FnOnce(&str) -> Result<(), String>,
    ) -> Result<RecordedObservation, String> {
        let (result, exchanges, timing) =
            query_without_audit(pair, Role::ReferenceMeter, command, begin);
        let validation = result
            .as_ref()
            .map_err(Clone::clone)
            .and_then(|r| validate(r));
        let off = if validation.is_err() {
            protection_output_off(Some(pair), state).err()
        } else {
            None
        };
        let evidence = append_transport_exchanges(
            audit,
            "reference_meter",
            command,
            point,
            Some(index),
            &exchanges,
            &timing,
        );
        if let Err(error) = validation {
            let error = format!("Source-quantity Meter: {error}");
            let error = off.map_or(error.clone(), |e| {
                format!("{error} · Output OFF verification failed: {e}")
            });
            return Err(evidence
                .err()
                .map_or(error.clone(), |e| format!("{error} · {e}")));
        }
        evidence?;
        Ok(RecordedObservation {
            reply: result?,
            timing,
        })
    }

    pub(super) fn read(
        &mut self,
        pair: &mut DevicePair,
        state: &mut Snapshot,
        audit: &mut RawWriter,
        control: &Control,
        begin: Instant,
        record: &mut Record,
    ) -> Result<bool, String> {
        let mut stats = SampleStats::default();
        let mut range = None;
        let mut resolution = None::<f64>;
        let count = self.config.repeat.max(self.config.average_count).max(1);
        for index in 1..=count {
            if control.cancelled() {
                return Ok(false);
            }
            let reading = Self::query(
                pair,
                state,
                audit,
                begin,
                record.point.point_index,
                index,
                "READ?",
                |reply| {
                    let value = reply
                        .trim()
                        .parse::<f64>()
                        .map_err(|_| "Invalid reading".to_string())?;
                    if !value.is_finite() || value.abs() > 1e30 {
                        return Err(
                            "Overload or non-finite reading; choose a suitable fixed range.".into(),
                        );
                    }
                    Ok(())
                },
            )?;
            let value = reading
                .reply
                .trim()
                .parse::<f64>()
                .map_err(|e| e.to_string())?;
            // RangeGuard owns previous loading; move it temporarily to avoid
            // borrowing Observer twice across its protected query operation.
            if meter::monitored(&self.config) {
                let command = format!(
                    "SENS:{}:DC:RANG?",
                    iv_core::measurement_definition(self.config.mode).meter_function
                );
                let observed = Self::query(
                    pair,
                    state,
                    audit,
                    begin,
                    record.point.point_index,
                    index,
                    &command,
                    |reply| self.guard.check(&self.config, positive_reading(reply)),
                )?;
                range = positive_reading(&observed.reply);
            }
            if let Some((_, query)) =
                iv_core::meter_display_queries(&self.config.meter_model, self.config.mode)?
            {
                let r = self.resolution.read(range, || {
                    Ok(Self::query(
                        pair,
                        state,
                        audit,
                        begin,
                        record.point.point_index,
                        index,
                        query,
                        |_| Ok(()),
                    )?
                    .reply)
                })?;
                if let Some(r) = r {
                    resolution = Some(resolution.unwrap_or(0.0).max(r));
                }
            }
            stats.add(value);
            audit.append(&serde_json::json!({"event":"reference_meter_reading","point_index":record.point.point_index,"reading_index":index,"parsed_value":value,"quantity":if record.mode==Mode::Iv{"V"}else{"A"},"range":range,"resolution":resolution,"timing":reading.timing.json()}))?;
        }
        record.reference_meter_value = Some(stats.mean);
        record.reference_meter_range = range;
        record.reference_meter_resolution = resolution;
        Ok(true)
    }
}

pub(super) fn prepare_live(
    pair: &mut DevicePair,
    primary: &RunConfig,
    control: &Control,
) -> Result<Option<Observer>, String> {
    if !pair.is_connected(Role::ReferenceMeter) {
        return Ok(None);
    }
    let mut primary = primary.clone();
    primary.reference_meter = Some(iv_core::ReferenceMeterConfig {
        model: pair.reference_meter_model.clone(),
    });
    let config = meter_config(&primary).ok_or("Reference configuration missing")?;
    let function = iv_core::measurement_definition(config.mode).meter_function;
    pair.write(Role::ReferenceMeter, &format!("CONF:{function}:DC"))?;
    for command in meter::commands(&config)? {
        pair.write(Role::ReferenceMeter, &command)?;
    }
    pair.write(Role::ReferenceMeter, &format!("SENS:{function}:DC:NPLC 1"))?;
    pair.write(Role::ReferenceMeter, "TRIG:SOUR IMM")?;
    meter::verify(&config, |cmd| {
        monitor_query(pair, Role::ReferenceMeter, cmd, control)
    })?;
    let mut guard = meter::RangeGuard::new(&config);
    if meter::monitored(&config) {
        let range = monitor_query(
            pair,
            Role::ReferenceMeter,
            &format!("SENS:{function}:DC:RANG?"),
            control,
        )?;
        guard.check(&config, positive_reading(&range))?;
    }
    Ok(Some(Observer {
        config,
        guard,
        resolution: MeterResolutionCache::default(),
    }))
}
impl Observer {
    pub(super) fn read_live(
        &mut self,
        pair: &mut DevicePair,
        state: &mut Snapshot,
        control: &Control,
        record: &mut Record,
    ) -> Result<(), String> {
        record.reference_meter_value = None;
        let result: Result<(), String> = (|| {
            let reply = monitor_query(pair, Role::ReferenceMeter, "READ?", control)?;
            let value = reply
                .trim()
                .parse::<f64>()
                .map_err(|_| "Invalid source-quantity Meter reading".to_string())?;
            if !value.is_finite() || value.abs() > 1e30 {
                return Err("Source-quantity Meter overload".into());
            }
            let mut range = None;
            let mut resolution = None;
            if let Some((range_query, resolution_query)) =
                iv_core::meter_display_queries(&self.config.meter_model, self.config.mode)?
            {
                range = positive_reading(&monitor_query(
                    pair,
                    Role::ReferenceMeter,
                    range_query,
                    control,
                )?);
                if sweep_outputs_off(state) {
                    self.guard.reset_baseline();
                }
                self.guard.check(&self.config, range)?;
                resolution = self.resolution.read(range, || {
                    monitor_query(pair, Role::ReferenceMeter, resolution_query, control)
                })?;
            }
            record.reference_meter_value = Some(value);
            record.reference_meter_range = range;
            record.reference_meter_resolution = resolution;
            Ok(())
        })();
        if let Err(error) = result {
            let off = protection_output_off(Some(pair), state).err();
            return Err(off.map_or(error.clone(), |e| format!("{error} · {e}")));
        }
        Ok(())
    }
}
