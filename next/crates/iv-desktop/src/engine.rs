use crate::protocol::{Command, Snapshot};
use iv_core::{CsvWriter, Mode, Record, RunConfig, plan, simulate_record};
use iv_transport::{DevicePair, HelperPaths, Role};
use std::io::{BufRead, Read, Write};
use std::path::Path;
use std::sync::{
    Arc, Condvar, Mutex,
    atomic::{AtomicBool, Ordering},
    mpsc,
};
use std::time::{Duration, Instant};

struct Control {
    stop: AtomicBool,
    off: AtomicBool,
    closed: AtomicBool,
    heartbeat: Mutex<Instant>,
}

// Coalesce display state, but wake immediately for connection and safety changes.
// Acquired records are persisted separately and are never coalesced in the CSV.
#[derive(Default)]
struct StateFeed {
    pending: Mutex<PendingState>,
    changed: Condvar,
}

#[derive(Default)]
struct PendingState {
    snapshot: Option<Snapshot>,
    closed: bool,
}

#[derive(serde::Serialize)]
#[serde(tag = "event", content = "data", rename_all = "snake_case")]
enum BorrowedEvent<'a> {
    State(&'a Snapshot),
    Point { run_id: u64, record: &'a Record },
}

fn write_events<W: Write>(
    output: &mut W,
    buffer: &mut Vec<u8>,
    points: &mpsc::Receiver<(u64, Record)>,
    snapshot: Option<&Snapshot>,
) -> std::io::Result<()> {
    buffer.clear();
    for (run_id, record) in points.try_iter().take(128) {
        serde_json::to_writer(
            &mut *buffer,
            &BorrowedEvent::Point {
                run_id,
                record: &record,
            },
        )?;
        buffer.push(b'\n');
    }
    if let Some(snapshot) = snapshot {
        serde_json::to_writer(&mut *buffer, &BorrowedEvent::State(snapshot))?;
        buffer.push(b'\n');
    }
    if !buffer.is_empty() {
        output.write_all(buffer)?;
        output.flush()?;
    }
    Ok(())
}

impl Control {
    fn cancelled(&self) -> bool {
        self.stop.load(Ordering::Acquire)
            || self.closed.load(Ordering::Acquire)
            || self
                .heartbeat
                .lock()
                .map_or(true, |t| t.elapsed() > Duration::from_secs(2))
    }
}

pub fn serve() -> Result<(), String> {
    let control = Arc::new(Control {
        stop: AtomicBool::new(false),
        off: AtomicBool::new(false),
        closed: AtomicBool::new(false),
        heartbeat: Mutex::new(Instant::now()),
    });
    let (tx, rx) = mpsc::sync_channel(8);
    let reader_control = control.clone();
    std::thread::spawn(move || {
        let stdin = std::io::stdin();
        let mut input = stdin.lock();
        let mut line = Vec::with_capacity(1024);
        loop {
            line.clear();
            match input.by_ref().take(65537).read_until(b'\n', &mut line) {
                Ok(0) => break,
                Ok(_) if line.len() <= 65536 && line.last() == Some(&b'\n') => {}
                _ => break,
            }
            let Ok(command) = serde_json::from_slice::<Command>(&line) else {
                break;
            };
            if let Ok(mut heartbeat) = reader_control.heartbeat.lock() {
                *heartbeat = Instant::now();
            }
            match command {
                Command::Heartbeat => {}
                Command::Stop => {
                    reader_control.stop.store(true, Ordering::Release);
                }
                Command::OutputOff => {
                    reader_control.off.store(true, Ordering::Release);
                    reader_control.stop.store(true, Ordering::Release);
                    let _ = tx.try_send(Command::OutputOff);
                }
                Command::Shutdown => break,
                other => {
                    if matches!(other, Command::Run { .. }) {
                        reader_control.stop.store(false, Ordering::Release);
                        reader_control.off.store(false, Ordering::Release);
                    }
                    if tx.try_send(other).is_err() {
                        break;
                    }
                }
            }
        }
        reader_control.closed.store(true, Ordering::Release);
        reader_control.stop.store(true, Ordering::Release);
    });
    let latest = Arc::new(StateFeed::default());
    let writer_state = latest.clone();
    let (points_tx, points_rx) = mpsc::sync_channel::<(u64, Record)>(128);
    let (writer_finished, writer_ack) = mpsc::sync_channel(1);
    let output_control = control.clone();
    std::thread::spawn(move || {
        let stdout = std::io::stdout();
        let mut output = stdout.lock();
        let mut buffer = Vec::with_capacity(65536);
        loop {
            let Ok(mut pending) = writer_state.pending.lock() else {
                break;
            };
            while pending.snapshot.is_none() && !pending.closed {
                let Ok(next) = writer_state.changed.wait(pending) else {
                    return;
                };
                pending = next;
            }
            let snapshot = pending.snapshot.take();
            let closed = pending.closed;
            drop(pending);
            if write_events(&mut output, &mut buffer, &points_rx, snapshot.as_ref()).is_err() {
                // A lost consumer cancels acquisition; source cleanup stays on
                // the engine thread, independent of stdout progress.
                output_control.closed.store(true, Ordering::Release);
                output_control.stop.store(true, Ordering::Release);
                break;
            }
            if closed {
                break;
            }
        }
        let _ = writer_finished.send(());
    });
    let mut pair: Option<DevicePair> = None;
    let mut state = Snapshot::default();
    publish(&latest, &state);
    while !control.closed.load(Ordering::Acquire) {
        if (state.source_connected || state.meter_connected)
            && control
                .heartbeat
                .lock()
                .map_or(true, |t| t.elapsed() > Duration::from_secs(2))
        {
            if state.source_connected {
                clean_output(pair.as_mut(), &mut state, None, &control);
            }
            if let Some(mut device) = pair.take() {
                let _ = device.close();
            }
            clear_connections(&mut state);
            publish(&latest, &state);
        }
        let command = match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(command) => command,
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(_) => break,
        };
        let result = match command {
            Command::Discover => discover(pair.as_mut(), &mut state, &latest),
            Command::Connect {
                simulation,
                source,
                meter,
            } => {
                begin_connect(&mut state, &latest);
                connect_role(&mut pair, &mut state, Role::Source, simulation, &source).and_then(
                    |()| connect_role(&mut pair, &mut state, Role::Meter, simulation, &meter),
                )
            }
            Command::ConnectSource {
                simulation,
                resource,
            } => {
                begin_connect(&mut state, &latest);
                connect_role(&mut pair, &mut state, Role::Source, simulation, &resource)
            }
            Command::ConnectMeter {
                simulation,
                resource,
            } => {
                begin_connect(&mut state, &latest);
                connect_role(&mut pair, &mut state, Role::Meter, simulation, &resource)
            }
            Command::DisconnectSource => {
                disconnect_role(&mut pair, &mut state, Role::Source, &control)
            }
            Command::DisconnectMeter => {
                disconnect_role(&mut pair, &mut state, Role::Meter, &control)
            }
            Command::Run { config, path } => execute(
                &config,
                Path::new(&path),
                if state.simulation {
                    None
                } else {
                    pair.as_mut()
                },
                &control,
                &mut state,
                &latest,
                &points_tx,
            ),
            Command::OutputOff => {
                clean_output(pair.as_mut(), &mut state, None, &control);
                if state.output == "Off" {
                    state.phase = "Ready".into();
                    state.message = "Output off verified.".into();
                }
                Ok(())
            }
            Command::Disconnect => disconnect_role(&mut pair, &mut state, Role::Source, &control)
                .and_then(|()| disconnect_role(&mut pair, &mut state, Role::Meter, &control)),
            _ => Ok(()),
        };
        if let Err(error) = result {
            state.phase = "Failed".into();
            state.error = error;
            state.message = "Review the error and reconnect if needed.".into();
        }
        publish(&latest, &state);
    }
    if state.source_connected {
        clean_output(pair.as_mut(), &mut state, None, &control);
    }
    if let Some(mut device) = pair {
        let _ = device.close();
    }
    clear_connections(&mut state);
    publish(&latest, &state);
    if let Ok(mut pending) = latest.pending.lock() {
        pending.closed = true;
        latest.changed.notify_one();
    }
    // Wait for actual final-state delivery rather than an unconditional delay.
    // Bound this wait even when the consumer leaves its stdout pipe blocked.
    let _ = writer_ack.recv_timeout(Duration::from_millis(200));
    Ok(())
}

fn clear_connections(state: &mut Snapshot) {
    state.source_connected = false;
    state.meter_connected = false;
    state.connected = false;
}

fn refresh_connections(state: &mut Snapshot) {
    state.connected = state.source_connected && state.meter_connected;
    state.simulation = state.source_simulation && state.meter_simulation;
}

fn begin_connect(state: &mut Snapshot, latest: &StateFeed) {
    state.phase = "Connecting".into();
    state.message = "Verifying instrument identity…".into();
    state.error.clear();
    publish(latest, state);
}

fn connect_role(
    pair: &mut Option<DevicePair>,
    state: &mut Snapshot,
    role: Role,
    simulation: bool,
    resource: &str,
) -> Result<(), String> {
    let source = role == Role::Source;
    let connected = if source {
        state.source_connected
    } else {
        state.meter_connected
    };
    let previous_simulation = if source {
        state.source_simulation
    } else {
        state.meter_simulation
    };
    let previous_resource = if source {
        &state.source_resource
    } else {
        &state.meter_resource
    };
    if connected {
        if simulation == previous_simulation && (simulation || resource == previous_resource) {
            state.phase = "Ready".into();
            state.message = "Instrument already connected.".into();
            return Ok(());
        }
        return Err(
            "Disconnect this instrument before choosing a different device or resource.".into(),
        );
    }
    let identity = if simulation {
        if source {
            "Simulated source".into()
        } else {
            "Simulated meter · 1 kOhm reference".into()
        }
    } else {
        if pair.is_none() {
            *pair = Some(DevicePair::new(&HelperPaths::discover()?));
        }
        let device = pair
            .as_mut()
            .ok_or("Instrument transport is unavailable.")?;
        device.connect(role, resource)?;
        if source {
            device.source_identity.clone()
        } else {
            device.meter_identity.clone()
        }
    };
    if source {
        state.source_connected = true;
        state.source_simulation = simulation;
        state.source_identity = identity.clone();
        state.source_resource = if simulation {
            "SIMULATED_SOURCE".into()
        } else {
            resource.into()
        };
        state.output = if simulation { "Off" } else { "Unknown" }.into();
    } else {
        state.meter_connected = true;
        state.meter_simulation = simulation;
        state.meter_identity = identity.clone();
        state.meter_resource = if simulation {
            "SIMULATED_METER".into()
        } else {
            resource.into()
        };
    }
    refresh_connections(state);
    state.phase = "Ready".into();
    state.message = format!(
        "Connected {}: {identity}",
        if source {
            "source.gs210"
        } else {
            "meter.34411a"
        }
    );
    Ok(())
}

fn disconnect_role(
    pair: &mut Option<DevicePair>,
    state: &mut Snapshot,
    role: Role,
    control: &Control,
) -> Result<(), String> {
    let source = role == Role::Source;
    let connected = if source {
        state.source_connected
    } else {
        state.meter_connected
    };
    if !connected {
        return Ok(());
    }
    let simulation = if source {
        state.source_simulation
    } else {
        state.meter_simulation
    };
    if source {
        clean_output(pair.as_mut(), state, None, control);
        if state.output != "Off" {
            return Err(state.error.clone());
        }
    }
    let detach = if !simulation && let Some(device) = pair.as_mut() {
        device.disconnect(role)
    } else {
        Ok(())
    };
    if detach.is_err()
        && pair
            .as_ref()
            .is_some_and(|device| device.is_connected(role))
    {
        return detach;
    }
    if source {
        state.source_connected = false;
    } else {
        state.meter_connected = false;
    }
    refresh_connections(state);
    if !state.source_connected
        && !state.meter_connected
        && let Some(mut device) = pair.take()
    {
        device.close()?;
    }
    state.phase = if state.source_connected || state.meter_connected {
        "Ready"
    } else {
        "Idle"
    }
    .into();
    state.message = format!("Disconnected {}.", if source { "source" } else { "meter" });
    detach?;
    Ok(())
}

fn publish(shared: &StateFeed, state: &Snapshot) {
    if let Ok(mut pending) = shared.pending.lock() {
        pending.snapshot = Some(state.clone());
        shared.changed.notify_one();
    }
}

fn publish_progress(shared: &StateFeed, state: &Snapshot, last: &mut Instant) {
    if last.elapsed() >= Duration::from_millis(50) {
        publish(shared, state);
        *last = Instant::now();
    }
}

fn discover(
    pair: Option<&mut DevicePair>,
    state: &mut Snapshot,
    latest: &StateFeed,
) -> Result<(), String> {
    state.phase = "Scanning".into();
    state.message = "Discovering USB instruments and GPIB listeners…".into();
    state.error.clear();
    publish(latest, state);
    let paths = HelperPaths::discover()?;
    let found = if let Some(pair) = pair {
        pair.discover(&paths)
    } else {
        iv_transport::discover_resources(&paths)
    };
    state.resources = found.resources;
    state.warnings = found.warnings;
    state.phase = if state.connected { "Ready" } else { "Idle" }.into();
    state.message = format!("Found {} connected resources.", state.resources.len());
    Ok(())
}

fn sleep(seconds: f64, control: &Control) -> bool {
    let end = Instant::now() + Duration::from_secs_f64(seconds.max(0.0));
    while Instant::now() < end {
        if control.cancelled() {
            return false;
        }
        std::thread::sleep(
            Duration::from_millis(10).min(end.saturating_duration_since(Instant::now())),
        );
    }
    !control.cancelled()
}

fn timestamp() -> String {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_default()
}

fn read_level(pair: &mut DevicePair) -> Result<Option<f64>, String> {
    for command in [":SOUR:LEV?", ":SOUR:LEV:FIX?", ":SOUR:LEV:AUTO?"] {
        // A transport timeout poisons the session and requires output-off
        // recovery. Only a received, unparsable reply can try another dialect.
        let reply = pair.query(Role::Source, command)?;
        if let Ok(value) = reply.trim().parse::<f64>() {
            if !value.is_finite() {
                return Err("Source returned a nonfinite level.".into());
            }
            return Ok(Some(value));
        }
    }
    Ok(None)
}

#[derive(Default)]
struct ResistanceFit {
    count: usize,
    mean_i: f64,
    mean_v: f64,
    covariance: f64,
    variance: f64,
}
impl ResistanceFit {
    fn add(&mut self, voltage: f64, current: f64) -> Option<f64> {
        self.count += 1;
        let di = current - self.mean_i;
        let dv = voltage - self.mean_v;
        self.mean_i += di / self.count as f64;
        self.mean_v += dv / self.count as f64;
        self.covariance += di * (voltage - self.mean_v);
        self.variance += di * (current - self.mean_i);
        if self.count < 2 || self.variance == 0.0 {
            None
        } else {
            let fit = self.covariance / self.variance;
            fit.is_finite().then_some(fit)
        }
    }
}

fn configure(
    pair: &mut DevicePair,
    config: &RunConfig,
    range: f64,
    compliance: f64,
) -> Result<(), String> {
    for (role, expected) in [
        (Role::Source, pair.source_identity.clone()),
        (Role::Meter, pair.meter_identity.clone()),
    ] {
        if pair.query(role, "*IDN?")?.trim() != expected.trim() {
            return Err(
                "Instrument identity changed. Disconnect and select the correct resource.".into(),
            );
        }
    }
    pair.write(Role::Source, ":OUTP OFF")?;
    pair.write(Role::Source, "*CLS")?;
    pair.write(Role::Meter, "*CLS")?;
    let (function, protect, measure) = if config.mode == Mode::Iv {
        ("VOLT", "CURR", "CURR")
    } else {
        ("CURR", "VOLT", "VOLT")
    };
    pair.write(Role::Source, &format!(":SOUR:FUNC {function}"))?;
    pair.write(Role::Source, &format!(":SOUR:RANG {range:.12e}"))?;
    pair.write(
        Role::Source,
        &format!(":SOUR:PROT:{protect} {compliance:.12e}"),
    )?;
    pair.write(Role::Source, ":SOUR:LEV:FIX 0")?;
    pair.write(Role::Meter, &format!("CONF:{measure}:DC"))?;
    pair.write(Role::Meter, &format!("SENS:{measure}:DC:RANG:AUTO ON"))?;
    pair.write(
        Role::Meter,
        &format!("SENS:{measure}:DC:NPLC {:.12e}", config.nplc),
    )?;
    pair.write(Role::Meter, &format!("SENS:{measure}:DC:ZERO:AUTO ON"))?;
    pair.write(Role::Meter, "TRIG:SOUR IMM")?;
    for role in [Role::Source, Role::Meter] {
        let response = pair.query(role, "SYST:ERR?")?;
        if !response.trim_start().starts_with("0,") && !response.trim_start().starts_with("+0,") {
            return Err(format!("Instrument configuration failed: {response}"));
        }
    }
    Ok(())
}

fn set_level(pair: &mut DevicePair, level: f64) -> Result<(), String> {
    pair.write(Role::Source, &format!(":SOUR:LEV:FIX {level:.12e}"))
}

fn clean_output(
    pair: Option<&mut DevicePair>,
    state: &mut Snapshot,
    ramp: Option<(&RunConfig, f64)>,
    control: &Control,
) {
    state.phase = "Cleaning up".into();
    state.output = "Unknown".into();
    if state.source_simulation && state.source_connected {
        state.output = "Off".into();
        return;
    }
    if !state.source_connected {
        state.error = "Connect a source before using Output Off.".into();
        return;
    }
    let Some(pair) = pair else {
        state.error =
            "Source transport is unavailable. Check the instrument output manually.".into();
        return;
    };
    let mut errors = vec![];
    if let Some((config, current)) = ramp
        && !control.off.load(Ordering::Acquire)
    {
        let mut level = current;
        let budget = Instant::now();
        while level != 0.0 && budget.elapsed() < Duration::from_secs(10) {
            if control.off.load(Ordering::Acquire) {
                break;
            }
            level = if level.abs() <= config.ramp_step {
                0.0
            } else {
                level - level.signum() * config.ramp_step
            };
            if let Err(error) = set_level(pair, level) {
                errors.push(error);
                break;
            }
            if config.ramp_wait_s > 0.0 {
                std::thread::sleep(Duration::from_secs_f64(config.ramp_wait_s.min(0.1)));
            }
        }
    }
    // Emergency/error cleanup never queues a level write ahead of Output Off.
    // The transport replaces poisoned helpers before trying this operation.
    match pair.force_output_off() {
        Ok(report) if report.output_off_verified => {
            state.output = "Off".into();
            if let Some(warning) = report.zeroing_warning {
                state.warnings.push(format!(
                    "Output off was verified; resetting the source level to zero failed: {warning}"
                ));
            }
        }
        Ok(_) => errors.push("Source output off was not verified.".into()),
        Err(error) => errors.push(error),
    }
    if !errors.is_empty() {
        state.error = format!(
            "{} {}",
            errors.join(" · "),
            if state.output == "Off" {
                "Output off was verified."
            } else {
                "Check the instrument output manually."
            }
        );
    }
}

fn execute(
    config: &RunConfig,
    path: &Path,
    mut pair: Option<&mut DevicePair>,
    control: &Control,
    state: &mut Snapshot,
    latest: &StateFeed,
    events: &mpsc::SyncSender<(u64, Record)>,
) -> Result<(), String> {
    if !state.connected {
        return Err("Connect the source and meter before Start.".into());
    }
    if state.source_simulation != state.meter_simulation {
        return Err(
            "Use two real instruments or two simulated instruments for a measurement.".into(),
        );
    }
    let planned = plan(config)?;
    state.phase = "Preparing".into();
    state.message = "Creating the result file before enabling output…".into();
    state.error.clear();
    state.points = 0;
    state.total = planned.points.len();
    state.last = None;
    state.resistance = None;
    state.run_id += 1;
    state.path = path.display().to_string();
    publish(latest, state);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("Create result folder: {e}"))?;
    }
    let mut writer = CsvWriter::create(path)?;
    let begin = Instant::now();
    let mut level = 0.0;
    let mut stopped = false;
    let mut compliance_hit = false;
    let mut fit = ResistanceFit::default();
    let mut last_progress = Instant::now();
    let work = (|| -> Result<(), String> {
        if let Some(device) = pair.as_deref_mut() {
            configure(
                device,
                config,
                planned.source_range,
                planned.hardware_compliance,
            )?;
            if control.cancelled() {
                stopped = true;
                return Ok(());
            }
            device.write(Role::Source, ":OUTP ON")?;
        }
        state.output = "On".into();
        state.phase = "Running".into();
        publish(latest, state);
        for point in &planned.points {
            if control.cancelled() {
                stopped = true;
                break;
            }
            state.message.clear();
            state
                .message
                .push_str("Setting source and waiting for settling…");
            publish_progress(latest, state, &mut last_progress);
            if let Some(device) = pair.as_deref_mut() {
                while (level - point.target).abs() > config.ramp_step {
                    if control.cancelled() {
                        stopped = true;
                        break;
                    }
                    level += (point.target - level).signum() * config.ramp_step;
                    set_level(device, level)?;
                    if !sleep(config.ramp_wait_s, control) {
                        stopped = true;
                        break;
                    }
                }
                if stopped {
                    break;
                }
                set_level(device, point.target)?;
            }
            level = point.target;
            let settling = if state.points == 0 {
                config.start_settle_s
            } else {
                config.settle_s
            };
            if !sleep(settling, control) {
                stopped = true;
                break;
            }
            let mut record = if let Some(device) = pair.as_deref_mut() {
                state.message.clear();
                state.message.push_str("Reading meter…");
                publish_progress(latest, state, &mut last_progress);
                let mut value = 0.0;
                for _ in 0..config.average_count {
                    if control.cancelled() {
                        stopped = true;
                        break;
                    }
                    let text = device.query(Role::Meter, "READ?")?;
                    let measured = text
                        .trim()
                        .parse::<f64>()
                        .map_err(|_| format!("Invalid meter reading: {text}"))?;
                    if !measured.is_finite() || measured.abs() > 1e30 {
                        return Err("Meter reported overload or an invalid reading.".into());
                    }
                    value += measured / config.average_count as f64;
                }
                if stopped {
                    break;
                }
                let readback = read_level(device)?;
                let mut record = simulate_record(config, point, begin.elapsed().as_secs_f64());
                record.source_set = level;
                record.source_readback = readback;
                record.meter_value = value;
                record.source_resource = "source.gs210".into();
                record.meter_resource = "meter.dmm_34411a".into();
                record.compliance = value.abs() >= config.compliance;
                record
            } else {
                simulate_record(config, point, begin.elapsed().as_secs_f64())
            };
            record.timestamp = timestamp();
            record.status = if record.compliance {
                "compliance"
            } else {
                "running"
            }
            .into();
            compliance_hit = record.compliance;
            writer.append(&record, config)?;
            let source_value = record.source_readback.unwrap_or(record.source_set);
            let (voltage, current) = if config.mode == Mode::Iv {
                (source_value, record.meter_value)
            } else {
                (record.meter_value, source_value)
            };
            state.resistance = fit.add(voltage, current);
            state.last = Some(record.clone());
            state.points += 1;
            state.elapsed = begin.elapsed().as_secs_f64();
            let _ = events.try_send((state.run_id, record));
            if last_progress.elapsed() >= Duration::from_millis(50) {
                state.message = format!("Point {} of {} saved.", state.points, state.total);
                publish_progress(latest, state, &mut last_progress);
            }
            if compliance_hit {
                break;
            }
        }
        Ok(())
    })();
    state.phase = "Cleaning up".into();
    state.message = "Returning source to zero and verifying output off…".into();
    publish(latest, state);
    clean_output(
        pair,
        state,
        if work.is_ok() {
            Some((config, level))
        } else {
            None
        },
        control,
    );
    state.elapsed = begin.elapsed().as_secs_f64();
    state.phase = if work.is_err() || !state.error.is_empty() {
        "Failed"
    } else if compliance_hit {
        "Compliance stop"
    } else if stopped {
        "Stopped"
    } else {
        "Completed"
    }
    .into();
    state.message = format!(
        "{} · {} points saved · output {}",
        state.phase,
        state.points,
        state.output.to_lowercase()
    );
    if let Err(error) = work {
        state.error = if state.error.is_empty() {
            error
        } else {
            format!("{error} · {}", state.error)
        };
    }
    let manifest = path.with_extension("run.json");
    let metadata = serde_json::json!({"schema_version":1,"application_version":env!("CARGO_PKG_VERSION"),"config":config,"outcome":state,
        "simulation":state.simulation,"csv":path.display().to_string()});
    if let Err(error) = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(manifest)
        .and_then(|mut f| {
            serde_json::to_writer_pretty(&mut f, &metadata).map_err(std::io::Error::other)?;
            f.flush()
        })
    {
        state
            .warnings
            .push(format!("Run metadata could not be saved: {error}"));
    }
    publish(latest, state);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::Event;
    fn live_control() -> Control {
        Control {
            stop: AtomicBool::new(false),
            off: AtomicBool::new(false),
            closed: AtomicBool::new(false),
            heartbeat: Mutex::new(Instant::now()),
        }
    }

    #[test]
    fn batched_events_keep_wire_format_and_final_state_order() {
        let config = RunConfig::default();
        let point = plan(&config).unwrap().points[0].clone();
        let record = simulate_record(&config, &point, 0.25);
        let state = Snapshot {
            phase: "Completed".into(),
            output: "Off".into(),
            ..Default::default()
        };
        let (tx, rx) = mpsc::sync_channel(2);
        tx.send((7, record.clone())).unwrap();
        let mut output = Vec::new();
        let mut scratch = Vec::new();
        write_events(&mut output, &mut scratch, &rx, Some(&state)).unwrap();
        let expected = [
            serde_json::to_string(&Event::Point {
                run_id: 7,
                record: Box::new(record),
            })
            .unwrap(),
            serde_json::to_string(&Event::State(Box::new(state))).unwrap(),
        ]
        .join("\n")
            + "\n";
        assert_eq!(output, expected.as_bytes());
        assert!(rx.try_recv().is_err());
        let capacity = scratch.capacity();
        write_events(&mut output, &mut scratch, &rx, None).unwrap();
        assert_eq!(scratch.capacity(), capacity);
        assert_eq!(output, expected.as_bytes());
    }

    #[test]
    fn progress_is_coalesced_but_safety_state_is_immediate() {
        let shared = StateFeed::default();
        let mut state = Snapshot {
            phase: "Running".into(),
            output: "On".into(),
            ..Default::default()
        };
        let mut tick = Instant::now();
        publish_progress(&shared, &state, &mut tick);
        assert!(shared.pending.lock().unwrap().snapshot.is_none());
        tick -= Duration::from_millis(51);
        state.points = 123;
        publish_progress(&shared, &state, &mut tick);
        assert_eq!(
            shared
                .pending
                .lock()
                .unwrap()
                .snapshot
                .as_ref()
                .unwrap()
                .points,
            123
        );
        state.output = "Off".into();
        state.phase = "Completed".into();
        publish(&shared, &state);
        let latest = shared.pending.lock().unwrap().snapshot.take().unwrap();
        assert_eq!(latest.output, "Off");
        assert_eq!(latest.phase, "Completed");
        assert_eq!(latest.points, 123);
    }

    #[test]
    fn independent_connections_preserve_the_other_role() {
        let mut state = Snapshot::default();
        let mut pair = None;
        connect_role(&mut pair, &mut state, Role::Source, true, "").unwrap();
        assert!(state.source_connected);
        assert!(!state.connected);
        assert!(
            connect_role(
                &mut pair,
                &mut state,
                Role::Source,
                false,
                "GPIB0::5::INSTR"
            )
            .is_err()
        );
        assert!(state.source_connected && state.source_simulation);
        connect_role(&mut pair, &mut state, Role::Meter, true, "").unwrap();
        assert!(state.connected && state.simulation);
        disconnect_role(&mut pair, &mut state, Role::Source, &live_control()).unwrap();
        assert!(!state.source_connected && state.meter_connected);
        assert_eq!(state.output, "Off");
        connect_role(&mut pair, &mut state, Role::Source, true, "").unwrap();
        disconnect_role(&mut pair, &mut state, Role::Meter, &live_control()).unwrap();
        assert!(state.source_connected && !state.meter_connected);
        clean_output(pair.as_mut(), &mut state, None, &live_control());
        assert_eq!(state.output, "Off");
    }

    #[test]
    fn mixed_simulation_is_rejected_before_any_result_file_or_io() {
        let mut state = Snapshot {
            connected: true,
            source_connected: true,
            meter_connected: true,
            source_simulation: true,
            ..Default::default()
        };
        let shared = StateFeed::default();
        let (tx, _) = mpsc::sync_channel(1);
        let result = execute(
            &RunConfig::default(),
            Path::new("unused.csv"),
            None,
            &live_control(),
            &mut state,
            &shared,
            &tx,
        );
        assert!(result.unwrap_err().contains("two real instruments"));
        assert_eq!(state.run_id, 0);
        assert!(state.path.is_empty());
    }
    #[test]
    fn streaming_fit_preserves_slope_and_offset() {
        let mut fit = ResistanceFit::default();
        assert_eq!(fit.add(0.005, 0.0), None);
        assert!((fit.add(0.006, 1e-6).unwrap() - 1000.0).abs() < 1e-8);
        assert!((fit.add(0.004, -1e-6).unwrap() - 1000.0).abs() < 1e-8);
    }
    #[test]
    fn expired_lease_cancels_without_gui_or_io() {
        let control = Control {
            stop: AtomicBool::new(false),
            off: AtomicBool::new(false),
            closed: AtomicBool::new(false),
            heartbeat: Mutex::new(Instant::now() - Duration::from_secs(3)),
        };
        assert!(control.cancelled());
        *control.heartbeat.lock().unwrap() = Instant::now();
        assert!(!control.cancelled());
        control.closed.store(true, Ordering::Release);
        assert!(control.cancelled());
    }
}
