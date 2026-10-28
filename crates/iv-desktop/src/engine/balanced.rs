//! Balanced voltage drive: D/2 and -D/2, one owner of all I/O.
//! Gate bias is external to the sweep; no claim of actual channel-midpoint voltage.
use super::*;
use iv_core::{BalancedPlan, SweepPoint, balanced_plan};

/// Stream point details from the durable audit instead of retaining a second
/// million-point buffer in memory. No additional companion files are needed.
#[derive(serde::Serialize)]
struct DualRunMetadata<'a> {
    #[serde(flatten)]
    summary: &'a serde_json::Value,
    dual_source_points: SourcePoints<'a>,
}
struct SourcePoints<'a> {
    path: &'a Path,
    expected_points: usize,
}
impl serde::Serialize for SourcePoints<'_> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::{Error, SerializeSeq};
        let mut reader = csv::Reader::from_path(self.path).map_err(S::Error::custom)?;
        if reader
            .headers()
            .map_err(S::Error::custom)?
            .iter()
            .ne(raw_csv::FIELDS.iter().copied())
        {
            return Err(S::Error::custom(
                "Invalid raw CSV header while saving Dual point metadata",
            ));
        }
        let mut sequence = serializer.serialize_seq(None)?;
        let mut row = csv::StringRecord::new();
        let mut count = 0usize;
        while reader.read_record(&mut row).map_err(S::Error::custom)? {
            if row.get(4) != Some("dual_source_point") {
                continue;
            }
            let details: serde_json::Value =
                serde_json::from_str(row.get(17).unwrap_or("")).map_err(S::Error::custom)?;
            let point = details
                .get("source_values")
                .ok_or_else(|| S::Error::custom("Missing Dual source values"))?;
            count += 1;
            if point.get("point_index").and_then(serde_json::Value::as_u64) != Some(count as u64)
                || row.get(2).and_then(|v| v.parse::<usize>().ok()) != Some(count)
                || count > self.expected_points
            {
                return Err(S::Error::custom(
                    "Dual source point indexes do not match saved CSV points",
                ));
            }
            sequence.serialize_element(point)?;
        }
        if count != self.expected_points {
            return Err(S::Error::custom(
                "Dual source point count does not match saved CSV points",
            ));
        }
        sequence.end()
    }
}

struct Run<'a> {
    pair: Option<&'a mut DevicePair>,
    state: &'a mut Snapshot,
    audit: &'a mut RawWriter,
    control: &'a Control,
    begin: Instant,
    planned: &'a BalancedPlan,
    latest: &'a StateFeed,
    last_live: Instant,
    levels: [f64; 2],
    meter_precision: MeterResolutionCache,
    point_readbacks: Option<[f64; 2]>,
}
impl Run<'_> {
    fn off(&mut self) -> Result<(), String> {
        shutdown_sweep_sources(self.pair.as_deref_mut(), self.state)
    }
    fn write(&mut self, role: Role, command: &str) -> Result<(), String> {
        let Some(device) = self.pair.as_deref_mut() else {
            return Ok(());
        };
        let (started, elapsed) = HostTiming::start(self.begin);
        let recorded = device.write_recorded(role, command);
        let timing = HostTiming::finish(self.begin, started, elapsed);
        // No disk operation delays OFF after a failed command.
        let stop_error = if recorded.result.is_err() {
            self.off().err()
        } else {
            None
        };
        let evidence = append_transport_exchanges(
            self.audit,
            role_label(role),
            command,
            0,
            None,
            &recorded.exchanges,
            &timing,
        );
        recorded
            .result
            .map(|_| ())
            .and(evidence)
            .map_err(|error| stop_error.map_or(error.clone(), |stop| format!("{error} · {stop}")))
    }
    fn query(&mut self, role: Role, command: &str, point: usize) -> Result<String, String> {
        let device = self
            .pair
            .as_deref_mut()
            .ok_or("Missing dual-source transport")?;
        let (response, exchanges, timing) = query_without_audit(device, role, command, self.begin);
        let stop_error = if response.is_err() {
            self.off().err()
        } else {
            None
        };
        let evidence = append_transport_exchanges(
            self.audit,
            role_label(role),
            command,
            point,
            None,
            &exchanges,
            &timing,
        );
        let reply = response.and_then(|reply| evidence.map(|()| reply));
        reply.map_err(|error| stop_error.map_or(error.clone(), |stop| format!("{error} · {stop}")))
    }
    fn level(&mut self, role: Role, point: usize) -> Result<f64, String> {
        for query in [":SOUR:LEV?", ":SOUR:LEV:FIX?", ":SOUR:LEV:AUTO?"] {
            let text = self.query(role, query, point)?;
            if let Ok(level) = text.trim().parse::<f64>()
                && level.is_finite()
            {
                return Ok(level);
            }
        }
        self.off()?;
        Err(format!(
            "{} did not return a finite source level",
            role_label(role)
        ))
    }
    fn check(&mut self, point: usize) -> Result<bool, String> {
        if self.state.gate_output == OutputState::On
            && let Err(error) = gate::check(self.pair.as_deref_mut(), self.state)
        {
            let off = self.off().err();
            return Err(off.map_or(error.clone(), |other| format!("{error} · {other}")));
        }
        for (index, role) in [Role::Source, Role::Source2].into_iter().enumerate() {
            let Some(device) = self.pair.as_deref_mut() else {
                return Ok(true);
            };
            let model = device.model(role).to_owned();
            let Some(profile) = iv_core::source_protection_profile(&model)? else {
                continue;
            };
            let (reply, exchanges, timing) =
                query_without_audit(device, role, profile.query, self.begin);
            let condition = reply
                .as_ref()
                .map_err(Clone::clone)
                .and_then(|reply| safety::source_condition(reply, profile));
            let unsafe_status = condition
                .as_ref()
                .map_or(true, |condition| condition.stopped());
            let shutdown = if unsafe_status { self.off() } else { Ok(()) };
            if let Ok(condition) = &condition
                && condition.stopped()
            {
                self.state.protection_alert = Some(ProtectionAlert {
                    kind: ProtectionKind::SourceCompliance,
                    limit: if index == 0 {
                        self.planned.plan1.hardware_compliance
                    } else {
                        self.planned.plan2.hardware_compliance
                    },
                    unit: "A".into(),
                    observed: None,
                    indicator: condition.indicator().into(),
                    source_index: Some((index + 1) as u8),
                });
            }
            append_transport_exchanges(
                self.audit,
                role_label(role),
                profile.query,
                point,
                None,
                &exchanges,
                &timing,
            )?;
            self.audit.append(&serde_json::json!({"event":if unsafe_status {"source_protection_stop"}else{"source_condition"},
                "device_role":role_label(role),"point_index":point,"command":profile.query,
                "raw_response":reply.as_ref().ok(),"condition":condition.as_ref().ok(),"error":condition.as_ref().err(),
                "timing":timing.json(),"output_off_verified":self.state.output==OutputState::Off}))?;
            shutdown?;
            let condition = condition?;
            if condition.emergency {
                return Err(format!("{} emergency protection (EMR)", role_label(role)));
            }
            if condition.stopped() {
                return Ok(false);
            }
        }
        Ok(true)
    }
    fn set(&mut self, levels: [f64; 2], point: usize) -> Result<bool, String> {
        if self.control.cancelled() {
            return Ok(false);
        }
        self.write(Role::Source, &format!(":SOUR:LEV:FIX {:.12e}", levels[0]))?;
        if self.control.cancelled() {
            self.off()?;
            return Ok(false);
        }
        self.write(Role::Source2, &format!(":SOUR:LEV:FIX {:.12e}", levels[1]))?;
        self.levels = levels;
        self.state.source2_level = Some(levels[1]);
        self.check(point)
    }
    fn live_conversion(&mut self, point: usize) -> Result<bool, String> {
        let target = self.levels[0] - self.levels[1];
        let live_point = SweepPoint {
            target,
            direction: "monitor".into(),
            repeat_index: 0,
            sweep_index: 0,
            point_index: 0,
            total_points: 0,
        };
        let mut record = simulate_record(
            &self.planned.config1,
            &live_point,
            self.begin.elapsed().as_secs_f64(),
        );
        record.source_readback = None; // Commanded levels, not fabricated readbacks.
        record.source_resolution = Some(self.planned.differential_resolution);
        record.quality_flags = if self.pair.is_some() {
            "balanced_differential"
        } else {
            "simulated|balanced_differential"
        }
        .into();
        record.source_resource = format!(
            "{} | {}",
            self.state.source_resource, self.state.source2_resource
        );
        if let Some(device) = self.pair.as_deref_mut() {
            let (reply, exchanges, timing) =
                query_without_audit(device, Role::Meter, "READ?", self.begin);
            let parsed = reply
                .as_ref()
                .map_err(Clone::clone)
                .and_then(|text| {
                    text.trim()
                        .parse::<f64>()
                        .map_err(|_| "Invalid live meter reading".to_owned())
                })
                .and_then(|value| {
                    if value.is_finite() && value.abs() < 1e30 {
                        Ok(value)
                    } else {
                        Err("Live meter overload".into())
                    }
                });
            let limited = parsed
                .as_ref()
                .is_ok_and(|value| value.abs() >= self.planned.config1.compliance);
            if limited {
                self.state.protection_alert = Some(ProtectionAlert::measurement(
                    &self.planned.config1,
                    *parsed.as_ref().unwrap(),
                ));
            }
            let safe = if limited || parsed.is_err() {
                self.off().map(|()| false)
            } else {
                self.check(point)
            };
            append_meter_reading_audit(
                self.audit,
                point,
                None,
                reply.as_deref().ok(),
                parsed.as_ref().err().map(String::as_str),
                &exchanges,
                &timing,
            )?;
            record.timestamp = timing.completed_at_utc;
            record.elapsed_s = timing.completed_elapsed_s;
            record.meter_value = parsed?;
            if !safe? {
                return Ok(false);
            }
            if let Some((range, resolution)) =
                iv_core::meter_display_queries(&self.planned.config1.meter_model, Mode::Iv)?
            {
                record.meter_range = positive_reading(&self.query(Role::Meter, range, point)?);
                let mut precision = std::mem::take(&mut self.meter_precision);
                let actual = precision.read(record.meter_range, || {
                    self.query(Role::Meter, resolution, point)
                });
                self.meter_precision = precision;
                record.meter_resolution = actual?;
            }
        } else {
            record.timestamp = timestamp();
            record.meter_value = target / 1000.0;
            if record.meter_value.abs() >= self.planned.config1.compliance {
                self.state.protection_alert = Some(ProtectionAlert::measurement(
                    &self.planned.config1,
                    record.meter_value,
                ));
                self.off()?;
                return Ok(false);
            }
        }
        record.status = "live".into();
        self.state.live = Some(record);
        publish(self.latest, self.state);
        Ok(true)
    }
    fn wait(&mut self, seconds: f64, point: usize) -> Result<bool, String> {
        if seconds <= 0.0 {
            return Ok(!self.control.cancelled());
        }
        let budget = if self.pair.is_some() {
            Duration::from_secs_f64((self.planned.config1.nplc * 2.0 / 50.0 + 0.03).max(0.05))
        } else {
            Duration::from_millis(1)
        };
        let end = Instant::now() + Duration::from_secs_f64(seconds);
        loop {
            if self.control.cancelled() {
                return Ok(false);
            }
            if !self.check(point)? {
                return Ok(false);
            }
            if Instant::now() >= end {
                return Ok(true);
            }
            if self.last_live.elapsed() >= Duration::from_millis(50)
                && end.saturating_duration_since(Instant::now()) >= budget
            {
                self.last_live = Instant::now();
                if !self.live_conversion(point)? {
                    return Ok(false);
                }
                continue;
            }
            std::thread::sleep(
                Duration::from_millis(20).min(end.saturating_duration_since(Instant::now())),
            );
        }
    }
    fn source_setup(
        &mut self,
        role: Role,
        config: &RunConfig,
        planned: &iv_core::Plan,
    ) -> Result<(), String> {
        if self.pair.is_none() {
            return Ok(());
        }
        let expected = self
            .pair
            .as_ref()
            .map(|device| match role {
                Role::Source => device.source_identity.clone(),
                _ => device.source2_identity.clone(),
            })
            .unwrap();
        if self.query(role, "*IDN?", 0)?.trim() != expected.trim() {
            return Err("Source identity changed before balanced sweep".into());
        }
        for command in [
            ":OUTP OFF".to_owned(),
            "*CLS".to_owned(),
            ":SOUR:FUNC VOLT".to_owned(),
            format!(":SOUR:RANG {:.12e}", planned.source_range),
            format!(":SOUR:PROT:CURR {:.12e}", planned.hardware_compliance),
            ":SOUR:LEV:FIX 0".into(),
        ] {
            self.write(role, &command)?;
        }
        safety::verify_function(&self.query(role, ":SOUR:FUNC?", 0)?, Mode::Iv)?;
        let query = iv_core::source_range_query(&config.source_model)?.unwrap_or(":SOUR:RANG?");
        let resolution = safety::verify_range(
            &config.source_model,
            Mode::Iv,
            planned.source_range,
            &self.query(role, query, 0)?,
        )?;
        safety::verify_zero(self.level(role, 0)?, resolution)?;
        if role == Role::Source2 {
            self.state.source2_resolution = Some(resolution);
        }
        if iv_core::source_protection_profile(&config.source_model)?.is_some() {
            safety::verify_hardware_limit(
                planned.hardware_compliance,
                &self.query(role, ":SOUR:PROT:CURR?", 0)?,
            )?;
        }
        let error = self.query(role, "SYST:ERR?", 0)?;
        if !error.trim_start().starts_with("0,") && !error.trim_start().starts_with("+0,") {
            return Err(format!("{} configuration: {error}", role_label(role)));
        }
        Ok(())
    }
    fn setup(&mut self, config: &RunConfig) -> Result<(), String> {
        self.off()?;
        self.source_setup(Role::Source, &self.planned.config1, &self.planned.plan1)?;
        self.source_setup(Role::Source2, &self.planned.config2, &self.planned.plan2)?;
        if self.pair.is_some() {
            let identity = self.query(Role::Meter, "*IDN?", 0)?;
            if identity.trim() != self.state.meter_identity.trim() {
                return Err("Meter identity changed before balanced sweep".into());
            }
            for command in [
                "*CLS".to_owned(),
                "CONF:CURR:DC".to_owned(),
                "SENS:CURR:DC:RANG:AUTO ON".into(),
                format!("SENS:CURR:DC:NPLC {:.12e}", config.nplc),
                "SENS:CURR:DC:ZERO:AUTO ON".into(),
                "TRIG:SOUR IMM".into(),
            ] {
                self.write(Role::Meter, &command)?;
            }
            let error = self.query(Role::Meter, "SYST:ERR?", 0)?;
            if !error.trim_start().starts_with("0,") && !error.trim_start().starts_with("+0,") {
                return Err(format!("Meter configuration: {error}"));
            }
        }
        if self.control.cancelled() {
            return Ok(());
        }
        self.state.output = OutputState::Unknown;
        self.state.source2_output = OutputState::Unknown;
        publish(self.latest, self.state);
        for role in [Role::Source, Role::Source2] {
            if self.control.cancelled() {
                return Ok(());
            }
            self.write(role, ":OUTP ON")?;
            if self.pair.is_some()
                && safety::output_state(&self.query(role, ":OUTP?", 0)?)? != OutputState::On
            {
                return Err(format!("{} did not confirm output ON", role_label(role)));
            }
            if role == Role::Source2 {
                self.state.source2_output = OutputState::On;
            }
        }
        self.state.output = OutputState::On;
        Ok(())
    }
    fn reading(
        &mut self,
        config: &RunConfig,
        point: &SweepPoint,
    ) -> Result<Option<Record>, String> {
        self.point_readbacks = None;
        let mut record = simulate_record(config, point, self.begin.elapsed().as_secs_f64());
        record.source_resource = format!(
            "{} | {}",
            self.state.source_resource, self.state.source2_resource
        );
        record.quality_flags = if self.pair.is_some() {
            "balanced_differential"
        } else {
            "simulated|balanced_differential"
        }
        .into();
        record.readings_requested = iv_core::readings_per_point(config);
        let mut stats = SampleStats::default();
        let mut count = 0;
        let mut last_timing = None;
        for reading_index in 1..=record.readings_requested {
            if self.control.cancelled() {
                return Ok(None);
            }
            if !self.check(point.point_index)? {
                return Ok(None);
            }
            let value = if let Some(device) = self.pair.as_deref_mut() {
                let (reply, exchanges, timing) =
                    query_without_audit(device, Role::Meter, "READ?", self.begin);
                let parsed = reply
                    .as_ref()
                    .map_err(Clone::clone)
                    .and_then(|text| {
                        text.trim()
                            .parse::<f64>()
                            .map_err(|_| "Invalid meter reading".to_owned())
                    })
                    .and_then(|v| {
                        if v.is_finite() && v.abs() < 1e30 {
                            Ok(v)
                        } else {
                            Err("Meter overload".into())
                        }
                    });
                let limited = parsed.as_ref().is_ok_and(|v| v.abs() >= config.compliance);
                if limited {
                    self.state.protection_alert = Some(ProtectionAlert::measurement(
                        config,
                        *parsed.as_ref().unwrap(),
                    ));
                }
                let check = if limited || parsed.is_err() {
                    self.off().map(|()| true)
                } else {
                    self.check(point.point_index)
                };
                // Keep the triggering READ even when subsequent status/OFF fails.
                append_meter_reading_audit(
                    self.audit,
                    point.point_index,
                    Some(reading_index),
                    reply.as_deref().ok(),
                    reply.as_ref().err().map(String::as_str),
                    &exchanges,
                    &timing,
                )?;
                last_timing = Some(timing);
                if !check? {
                    return Ok(None);
                }
                parsed?
            } else {
                point.target / 1000.0
            };
            count += 1;
            stats.add(value);
            if value.abs() >= config.compliance {
                self.state.protection_alert = Some(ProtectionAlert::measurement(config, value));
                self.off()?;
                record.compliance = true;
                record.meter_value = value;
                record
                    .quality_flags
                    .push_str("|software_limit_hit|partial_average");
                break;
            }
        }
        if !record.compliance {
            record.meter_value = stats.mean;
        }
        record.readings_completed = Some(count);
        record.meter_stddev = stats.sample_stddev();
        if let Some(timing) = last_timing {
            record.timestamp = timing.completed_at_utc;
            record.elapsed_s = timing.completed_elapsed_s;
        } else {
            record.timestamp = timestamp();
            record.elapsed_s = self.begin.elapsed().as_secs_f64();
        }
        if self.pair.is_some() && !record.compliance {
            let level1 = self.level(Role::Source, point.point_index)?;
            let level2 = self.level(Role::Source2, point.point_index)?;
            let res1 = source_resolution(
                &self.planned.config1.source_model,
                Mode::Iv,
                self.planned.plan1.source_range,
            )
            .unwrap_or(0.0);
            let res2 = source_resolution(
                &self.planned.config2.source_model,
                Mode::Iv,
                self.planned.plan2.source_range,
            )
            .unwrap_or(0.0);
            if (level1 - self.planned.plan1.points[point.point_index - 1].target).abs()
                > res1.max(1e-12) * 0.51
                || (level2 + -self.planned.plan2.points[point.point_index - 1].target).abs()
                    > res2.max(1e-12) * 0.51
            {
                self.off()?;
                return Err(
                    "Balanced source level readback does not match the validated targets".into(),
                );
            }
            self.point_readbacks = Some([level1, level2]);
            record.source_readback = Some(level1 - level2);
            record.source_resolution = Some(self.planned.differential_resolution);
            self.state.source2_level = Some(level2);
            self.audit.append(&serde_json::json!({"event":"balanced_source_readback","point_index":point.point_index,
                "source1_V":level1,"source2_V":level2,"differential_V":level1-level2,"common_mode_V":(level1+level2)/2.0,
                "provenance":"programmed_source_settings_not_independent_output_measurements"}))?;
            if let Some((range, resolution)) =
                iv_core::meter_display_queries(&config.meter_model, Mode::Iv)?
            {
                record.meter_range =
                    positive_reading(&self.query(Role::Meter, range, point.point_index)?);
                record.meter_resolution =
                    positive_reading(&self.query(Role::Meter, resolution, point.point_index)?);
            }
        }
        if record.compliance {
            record.source_readback = None;
            record.source_resolution = None;
            record.status = "compliance".into();
        }
        Ok(Some(record))
    }
}

pub(super) fn execute(
    requested: &RunConfig,
    path: &Path,
    mut pair: Option<&mut DevicePair>,
    control: &Control,
    state: &mut Snapshot,
    latest: &StateFeed,
    events: &mpsc::SyncSender<(u64, Record)>,
) -> Result<(), String> {
    if !state.connected || !state.source2_connected {
        return Err("Connect Source 1, Source 2 and Meter before balanced Start.".into());
    }
    if state.source_simulation != state.meter_simulation
        || state.source2_simulation != state.source_simulation
    {
        return Err(
            "Use three real instruments or three simulated instruments for balanced measurement."
                .into(),
        );
    }
    let config = iv_core::prepare_config(requested)?;
    let planned = balanced_plan(&config)?;
    if !state.simulation {
        let device = pair.as_deref().ok_or("Dual-source transport missing")?;
        if device.source_model != config.source_model
            || device.source2_model != config.balanced_source.as_ref().unwrap().source_model
            || device.meter_model != config.meter_model
        {
            return Err(
                "Connected dual-source models do not match the selected run models.".into(),
            );
        }
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let mut writer = CsvWriter::create(path)?;
    let run_id = raw_csv::new_run_id();
    let raw_path = path.with_extension("raw.csv");
    let mut audit = RawWriter::create(&raw_path, &run_id)?;
    let manifest = path.with_extension("run.json");
    let instruments = serde_json::json!({"source1":{"identity":state.source_identity,"resource":state.source_resource},"source2":{"identity":state.source2_identity,"resource":state.source2_resource},"meter":{"identity":state.meter_identity,"resource":state.meter_resource},"gate":{"identity":state.gate_identity,"resource":state.gate_resource,"output":state.gate_output,"setting_V":state.gate_voltage}});
    let settings = serde_json::json!({"formula":"source1 = a/(a+b) * differential; source2 = -b/(a+b) * differential","ratio":config.balanced_source.as_ref().unwrap().ratio,"source1":planned.config1,"source2":planned.config2,
        "ranges":[planned.plan1.source_range,planned.plan2.source_range],"hardware_compliances":[planned.plan1.hardware_compliance,planned.plan2.hardware_compliance],
        "update_policy":"sequential_commands_then_measure; no hardware simultaneity or actual channel-midpoint guarantee"});
    state.run_id += 1;
    state.points = 0;
    state.total = planned.points.len();
    state.last = None;
    state.resistance = None;
    state.phase = Phase::Preparing;
    state.error.clear();
    state.balanced_active = true;
    state.source2_resolution = source_resolution(
        &planned.config2.source_model,
        Mode::Iv,
        planned.plan2.source_range,
    );
    state.path = path.display().to_string();
    write_json_new(&manifest,&serde_json::json!({"schema_version":RUN_METADATA_SCHEMA_VERSION,"csv_schema":CSV_SCHEMA,"csv":path.display().to_string(),"in_progress":true,"requested_config":requested,"config":config,"balanced_drive":settings,"instruments":instruments,"run_id":run_id})).map_err(|e|e.to_string())?;
    publish(latest, state);
    let begin = Instant::now();
    let mut fit = ResistanceFit::default();
    let mut stopped = false;
    let mut runner = Run {
        pair: pair.take(),
        state,
        audit: &mut audit,
        control,
        begin,
        planned: &planned,
        latest,
        last_live: Instant::now(),
        levels: [0.0; 2],
        meter_precision: MeterResolutionCache::default(),
        point_readbacks: None,
    };
    let work = (|| -> Result<(), String> {
        runner.setup(&config)?;
        runner.state.phase = Phase::Running;
        let mut level = 0.0;
        for point in &planned.points {
            if control.cancelled() {
                stopped = true;
                break;
            }
            let mut reached = true;
            for levels in planned.ramp_targets(level, point.target, config.ramp_step)? {
                if control.cancelled()
                    || !runner.set(levels?, point.point_index)?
                    || !runner.wait(config.ramp_wait_s, point.point_index)?
                {
                    reached = false;
                    break;
                }
            }
            if !reached
                || !runner.set(
                    [
                        planned.plan1.points[point.point_index - 1].target,
                        planned.plan2.points[point.point_index - 1].target,
                    ],
                    point.point_index,
                )?
            {
                stopped = true;
                break;
            }
            level = point.target;
            let wait = config.settle_s
                + if point.point_index == 1 {
                    config.start_settle_s
                } else {
                    0.0
                };
            if !runner.wait(wait, point.point_index)? {
                stopped = true;
                break;
            }
            let Some(mut record) = runner.reading(&config, point)? else {
                stopped = true;
                break;
            };
            record.run_id = run_id.clone();
            writer.append(&record)?;
            runner.state.points += 1;
            // Keep the analysis CSV fixed. Detailed source values are a
            // structured audit event, streamed into the companion run JSON.
            let source_values = serde_json::json!({
                "point_index":point.point_index,"timestamp":record.timestamp,
                "differential_setpoint_V":record.source_set,
                "source1":{"setpoint_V":planned.plan1.points[point.point_index-1].target,
                    "level_readback_V":runner.point_readbacks.map(|values|values[0]),"output_voltage_V":null},
                "source2":{"setpoint_V":planned.plan2.points[point.point_index-1].target,
                    "level_readback_V":runner.point_readbacks.map(|values|values[1]),"output_voltage_V":null},
                "differential_level_readback_V":runner.point_readbacks.map(|values|values[0]-values[1]),
                "common_mode_level_readback_V":runner.point_readbacks.map(|values|(values[0]+values[1])/2.0),
                "readback_kind":"programmed_level_not_independent_terminal_voltage",
                "simulation":runner.state.simulation,"software_limit_hit":record.compliance,
            });
            runner.audit.append(&serde_json::json!({"event":"dual_source_point","point_index":point.point_index,"source_values":source_values}))?;
            if !record.compliance {
                runner.state.resistance = fit.add(
                    record.source_readback.unwrap_or(record.source_set),
                    record.meter_value,
                );
            }
            runner.state.elapsed = begin.elapsed().as_secs_f64();
            runner.state.last = Some(record.clone());
            runner.state.live = Some(record.clone());
            runner.state.message = format!(
                "Balanced point {} of {} saved.",
                point.point_index,
                planned.points.len()
            );
            let _ = events.try_send((runner.state.run_id, record.clone()));
            publish(latest, runner.state);
            if record.compliance {
                break;
            }
        }
        Ok(())
    })();
    let off = runner.off();
    let gate_off = if work.is_err() || off.is_err() || runner.state.protection_alert.is_some() {
        shutdown_gate(runner.pair.as_deref_mut(), runner.state)
    } else {
        Ok(())
    };
    for error in [work.err(), off.err(), gate_off.err()]
        .into_iter()
        .flatten()
    {
        if !runner.state.error.is_empty() {
            runner.state.error.push_str(" · ");
        }
        runner.state.error.push_str(&error);
    }
    runner.state.phase = if !runner.state.error.is_empty() {
        Phase::Failed
    } else if let Some(alert) = &runner.state.protection_alert {
        if alert.kind == ProtectionKind::SourceCompliance {
            Phase::SourceLimitStop
        } else {
            Phase::ComplianceStop
        }
    } else if stopped {
        Phase::Stopped
    } else {
        Phase::Completed
    };
    runner.state.activity = Activity::Idle;
    runner.state.elapsed = begin.elapsed().as_secs_f64();
    runner.state.message = format!(
        "{} · {} balanced points saved · sweep outputs {}",
        runner.state.phase,
        runner.state.points,
        runner.state.output.to_lowercase()
    );
    let mut recording_error = None;
    if let Err(error)=runner.audit.append(&serde_json::json!({"event":"run_finished","outcome":runner.state.phase,"output":runner.state.output,"points_saved":runner.state.points})).and_then(|()|runner.audit.finish()) {recording_error=Some(error);}
    let hashes = recording_hashes(path, &raw_path);
    if let Err(error) = &hashes {
        recording_error = Some(error.clone());
    }
    if recording_error.is_some()
        && let Err(error) = shutdown_gate(runner.pair.as_deref_mut(), runner.state)
    {
        runner
            .state
            .error
            .push_str(&format!(" Gate OFF failed: {error}"));
    }
    if let Some(error) = &recording_error {
        runner.state.phase = Phase::Failed;
        runner
            .state
            .error
            .push_str(&format!(" Recording incomplete: {error}"));
    }
    let metadata = serde_json::json!({"schema_version":RUN_METADATA_SCHEMA_VERSION,"application_version":env!("CARGO_PKG_VERSION"),"csv_schema":CSV_SCHEMA,"csv_fields":iv_core::CSV_FIELDS,
        "csv_numeric_format":"shortest_roundtrip_scientific","csv":path.display().to_string(),"run_id":run_id,"in_progress":false,"requested_config":requested,"config":config,
        "balanced_drive":settings,"instruments":instruments,"outcome":runner.state,"simulation":runner.state.simulation,"experiment":config.experiment,
        "raw_csv":{"schema":"raw_csv_v1","filename":raw_path.file_name().map(|n|n.to_string_lossy())},"file_hashes":hashes.ok(),"file_hash_error":recording_error});
    let complete_metadata = DualRunMetadata {
        summary: &metadata,
        dual_source_points: SourcePoints {
            path: &raw_path,
            expected_points: runner.state.points,
        },
    };
    if let Err(error) = replace_json(&manifest, &complete_metadata) {
        if let Err(off_error) = shutdown_gate(runner.pair.as_deref_mut(), runner.state) {
            runner
                .state
                .error
                .push_str(&format!(" Gate OFF failed: {off_error}"));
        }
        runner.state.phase = Phase::Failed;
        runner
            .state
            .error
            .push_str(&format!(" Metadata save failed: {error}"));
    }
    publish(latest, runner.state);
    Ok(())
}

#[cfg(test)]
mod metadata_tests {
    use super::*;

    #[test]
    fn incomplete_or_disordered_audit_cannot_finalize_metadata() {
        for indexes in [vec![], vec![1], vec![2, 1], vec![1, 1], vec![1, 2, 3]] {
            let directory = tempfile::tempdir().unwrap();
            let raw = directory.path().join("run.raw.csv");
            let manifest = directory.path().join("run.run.json");
            let initial = serde_json::json!({"in_progress":true});
            write_json_new(&manifest, &initial).unwrap();
            let mut audit = RawWriter::create(&raw, "test").unwrap();
            for index in indexes {
                audit
                    .append(
                        &serde_json::json!({"event":"dual_source_point", "point_index":index,
                    "source_values":{"point_index":index}}),
                    )
                    .unwrap();
            }
            audit.finish().unwrap();
            let summary = serde_json::json!({"in_progress":false});
            let metadata = DualRunMetadata {
                summary: &summary,
                dual_source_points: SourcePoints {
                    path: &raw,
                    expected_points: 2,
                },
            };
            assert!(replace_json(&manifest, &metadata).is_err());
            assert_eq!(
                serde_json::from_slice::<serde_json::Value>(&std::fs::read(&manifest).unwrap())
                    .unwrap(),
                initial
            );
        }
    }

    #[test]
    fn complete_audit_serializes_in_point_order() {
        let directory = tempfile::tempdir().unwrap();
        let raw = directory.path().join("run.raw.csv");
        let mut audit = RawWriter::create(&raw, "test").unwrap();
        for index in 1..=2 {
            audit
                .append(
                    &serde_json::json!({"event":"dual_source_point", "point_index":index,
                "source_values":{"point_index":index}}),
                )
                .unwrap();
        }
        audit.finish().unwrap();
        let points = serde_json::to_value(SourcePoints {
            path: &raw,
            expected_points: 2,
        })
        .unwrap();
        assert_eq!(
            points,
            serde_json::json!([{"point_index":1},{"point_index":2}])
        );
    }
}
