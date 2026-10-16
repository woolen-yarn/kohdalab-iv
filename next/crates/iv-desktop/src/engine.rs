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
    let monitor_begin = Instant::now();
    let mut last_monitor = Instant::now();
    let mut monitor_mode = None;
    let mut simulation_mode = Mode::Iv;
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
        // Schedule from the start of the preceding acquisition, so transport
        // time does not add another fixed delay. Check queued controls first.
        let monitor_interval = Duration::from_millis(50);
        let monitor_ready = state.connected
            && state.source_simulation == state.meter_simulation
            && state.error.is_empty();
        let wait = if monitor_ready {
            monitor_interval.saturating_sub(last_monitor.elapsed())
        } else {
            Duration::from_millis(100)
        };
        let command = match rx.recv_timeout(wait) {
            Ok(command) => command,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if monitor_ready && last_monitor.elapsed() >= monitor_interval {
                    last_monitor = Instant::now();
                    let result = if state.simulation {
                        let config = RunConfig {
                            mode: simulation_mode,
                            ..Default::default()
                        };
                        let point = iv_core::SweepPoint {
                            target: 0.0,
                            direction: "monitor".into(),
                            repeat_index: 0,
                            sweep_index: 0,
                            point_index: 0,
                            total_points: 0,
                        };
                        let mut record =
                            simulate_record(&config, &point, monitor_begin.elapsed().as_secs_f64());
                        record.timestamp = timestamp();
                        Ok(record)
                    } else {
                        monitor_reading(
                            pair.as_mut().ok_or("Live monitor transport missing")?,
                            &mut monitor_mode,
                            monitor_begin.elapsed().as_secs_f64(),
                            &control,
                            &mut state.output,
                        )
                    };
                    match result {
                        Ok(record) => {
                            state.live = Some(record);
                            state.live_error.clear();
                        }
                        Err(_)
                            if control.closed.load(Ordering::Acquire)
                                || control.off.load(Ordering::Acquire) => {}
                        Err(error) => {
                            state.live_error = error.clone();
                            state.error =
                                format!("Live monitor failed: {error}. Reconnect the instruments.");
                            state.phase = "Failed".into();
                            clean_output(pair.as_mut(), &mut state, None, &control);
                        }
                    }
                    publish(&latest, &state);
                }
                continue;
            }
            Err(_) => break,
        };
        monitor_mode = None;
        let resume_monitor = matches!(
            &command,
            Command::OutputOff
                | Command::Connect { .. }
                | Command::ConnectSource { .. }
                | Command::ConnectMeter { .. }
                | Command::SetLiveMode { .. }
        );
        let result = match command {
            Command::Discover => discover(pair.as_mut(), &mut state, &latest),
            Command::SetLiveMode { mode } => {
                state.phase = "Switching mode".into();
                state.message = "Verifying output off before switching live channels…".into();
                publish(&latest, &state);
                let result = set_live_mode(pair.as_mut(), &mut state, mode);
                if result.is_ok() {
                    simulation_mode = mode;
                }
                result
            }
            Command::Connect {
                simulation,
                source,
                meter,
                source_model,
                meter_model,
            } => {
                begin_connect(&mut state, &latest);
                connect_role(
                    &mut pair,
                    &mut state,
                    Role::Source,
                    simulation,
                    &source,
                    &source_model,
                )
                .and_then(|()| {
                    connect_role(
                        &mut pair,
                        &mut state,
                        Role::Meter,
                        simulation,
                        &meter,
                        &meter_model,
                    )
                })
            }
            Command::ConnectSource {
                simulation,
                resource,
                model,
            } => {
                begin_connect(&mut state, &latest);
                connect_role(
                    &mut pair,
                    &mut state,
                    Role::Source,
                    simulation,
                    &resource,
                    &model,
                )
            }
            Command::ConnectMeter {
                simulation,
                resource,
                model,
            } => {
                begin_connect(&mut state, &latest);
                connect_role(
                    &mut pair,
                    &mut state,
                    Role::Meter,
                    simulation,
                    &resource,
                    &model,
                )
            }
            Command::DisconnectSource => {
                disconnect_role(&mut pair, &mut state, Role::Source, &control)
            }
            Command::DisconnectMeter => {
                disconnect_role(&mut pair, &mut state, Role::Meter, &control)
            }
            Command::Run { config, path } => {
                simulation_mode = config.mode;
                execute(
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
                )
            }
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
        if result.is_ok() && resume_monitor && state.output == "Off" {
            control.off.store(false, Ordering::Release);
        }
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

fn model_label(role: Role, model: &str) -> String {
    let role = if role == Role::Source {
        "source"
    } else {
        "meter"
    };
    format!("{role}.{model}")
}

fn begin_connect(state: &mut Snapshot, latest: &StateFeed) {
    state.phase = "Connecting".into();
    state.message = "Verifying instrument identity…".into();
    state.error.clear();
    state.live_error.clear();
    publish(latest, state);
}

fn connect_role(
    pair: &mut Option<DevicePair>,
    state: &mut Snapshot,
    role: Role,
    simulation: bool,
    resource: &str,
    model: &str,
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
    let previous_model = if previous_simulation {
        Some(if source {
            "SIMULATED_SOURCE"
        } else {
            "SIMULATED_METER"
        })
    } else {
        pair.as_ref().map(|device| {
            if source {
                device.source_model.as_str()
            } else {
                device.meter_model.as_str()
            }
        })
    };
    if connected {
        if simulation == previous_simulation
            && previous_model == Some(model)
            && (simulation || resource == previous_resource)
        {
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
        device.connect_model(role, resource, model)?;
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
    state.message = format!("Connected {}: {identity}", model_label(role, model));
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

fn monitor_query(
    pair: &mut DevicePair,
    role: Role,
    command: &str,
    control: &Control,
) -> Result<String, String> {
    if control.closed.load(Ordering::Acquire)
        || control.off.load(Ordering::Acquire)
        || control
            .heartbeat
            .lock()
            .map_or(true, |h| h.elapsed() > Duration::from_secs(2))
    {
        return Err("Live acquisition interrupted".into());
    }
    pair.query(role, command)
}

// Live mode changes happen only on the device-owner thread, between runs.
// Require verified OFF and zero level before changing the source function.
fn set_live_mode(
    pair: Option<&mut DevicePair>,
    state: &mut Snapshot,
    mode: Mode,
) -> Result<(), String> {
    if !state.error.is_empty() {
        return Err(state.error.clone());
    }
    if !state.source_connected {
        return Err("Connect the source before changing live mode".into());
    }
    if state.connected && state.source_simulation != state.meter_simulation {
        return Err("Live mode requires two real or two simulated instruments".into());
    }
    state.live = None;
    if !state.source_simulation {
        let device = pair.ok_or("Source transport unavailable")?;
        let report = device.force_output_off()?;
        if !report.output_off_verified {
            return Err("Output OFF was not verified; live mode unchanged".into());
        }
        state.output = "Off".into();
        device.write(Role::Source, ":SOUR:LEV:FIX 0")?;
        let zero = device
            .query(Role::Source, ":SOUR:LEV?")?
            .trim()
            .parse::<f64>()
            .map_err(|_| "Source zero level could not be verified")?;
        if zero != 0.0 {
            return Err("Source zero level could not be verified; live mode unchanged".into());
        }
        let function = if mode == Mode::Iv { "VOLT" } else { "CURR" };
        device.write(Role::Source, &format!(":SOUR:FUNC {function}"))?;
        let actual = device
            .query(Role::Source, ":SOUR:FUNC?")?
            .trim()
            .trim_matches('"')
            .to_ascii_uppercase();
        if !actual.starts_with(function) {
            return Err("Source function change was not confirmed".into());
        }
        // The instrument may retain a separate level for each source function.
        device.write(Role::Source, ":SOUR:LEV:FIX 0")?;
        let new_zero = device
            .query(Role::Source, ":SOUR:LEV?")?
            .trim()
            .parse::<f64>()
            .map_err(|_| "New source function zero level could not be verified")?;
        if new_zero != 0.0 {
            return Err("New source function zero level could not be verified".into());
        }
        if state.meter_connected {
            let measure = if mode == Mode::Iv { "CURR" } else { "VOLT" };
            device.write(Role::Meter, &format!("CONF:{measure}:DC"))?;
            device.write(Role::Meter, &format!("SENS:{measure}:DC:RANG:AUTO ON"))?;
            device.write(Role::Meter, &format!("SENS:{measure}:DC:NPLC 1"))?;
            device.write(Role::Meter, "TRIG:SOUR IMM")?;
        }
        for role in [Role::Source, Role::Meter] {
            if role == Role::Meter && !state.meter_connected {
                continue;
            }
            let error = device.query(role, "SYST:ERR?")?;
            if !error.trim_start().starts_with("0,") && !error.trim_start().starts_with("+0,") {
                return Err(format!("Live mode configuration failed: {error}"));
            }
        }
    }
    state.output = "Off".into();
    state.live_error.clear();
    state.phase = "Ready".into();
    state.message = format!("Live mode {} · output off verified", mode.as_str());
    Ok(())
}

fn monitor_reading(
    pair: &mut DevicePair,
    configured: &mut Option<Mode>,
    elapsed: f64,
    control: &Control,
    output: &mut String,
) -> Result<Record, String> {
    let function = monitor_query(pair, Role::Source, ":SOUR:FUNC?", control)?;
    let mode = match function
        .trim()
        .trim_matches('"')
        .to_ascii_uppercase()
        .as_str()
    {
        "VOLT" | "VOLTAGE" => Mode::Iv,
        "CURR" | "CURRENT" => Mode::Vi,
        _ => return Err(format!("Unknown source function: {function}")),
    };
    let measure = if mode == Mode::Iv { "CURR" } else { "VOLT" };
    if *configured != Some(mode) {
        pair.write(Role::Meter, &format!("CONF:{measure}:DC"))?;
        pair.write(Role::Meter, &format!("SENS:{measure}:DC:RANG:AUTO ON"))?;
        pair.write(Role::Meter, &format!("SENS:{measure}:DC:NPLC 1"))?;
        pair.write(Role::Meter, "TRIG:SOUR IMM")?;
        *configured = Some(mode);
    }
    if control.closed.load(Ordering::Acquire) || control.off.load(Ordering::Acquire) {
        return Err("Live acquisition interrupted by output-off/shutdown".into());
    }
    let measured = monitor_query(pair, Role::Meter, "READ?", control)?
        .trim()
        .parse::<f64>()
        .map_err(|_| "Invalid live meter reading")?;
    if !measured.is_finite() || measured.abs() > 1e30 {
        return Err("Live meter overload".into());
    }
    let raw_source = monitor_query(pair, Role::Source, ":SOUR:LEV?", control)?;
    let source = raw_source
        .trim()
        .parse::<f64>()
        .map_err(|_| "Live source level unavailable")?;
    if !source.is_finite() {
        return Err("Invalid live source level".into());
    }
    // The transport translates this canonical range query for legacy models.
    let range_query = iv_core::source_range_query(&pair.source_model)?.unwrap_or(":SOUR:RANG?");
    let range = monitor_query(pair, Role::Source, range_query, control)?;
    let (meter_range, meter_resolution) = if let Some((range_query, resolution_query)) =
        iv_core::meter_display_queries(&pair.meter_model, mode)?
    {
        (
            positive_reading(&monitor_query(pair, Role::Meter, range_query, control)?),
            positive_reading(&monitor_query(
                pair,
                Role::Meter,
                resolution_query,
                control,
            )?),
        )
    } else {
        // Protocol support has not been verified for this meter. Do not infer
        // a range or resolution from a different instrument family.
        (None, None)
    };
    let raw_output = monitor_query(pair, Role::Source, ":OUTP?", control)?;
    *output = match raw_output.trim() {
        "0" | "+0" | "OFF" => "Off",
        "1" | "+1" | "ON" => "On",
        _ => return Err("Unknown source output state".into()),
    }
    .into();
    let config = RunConfig {
        mode,
        ..Default::default()
    };
    let point = iv_core::SweepPoint {
        target: source,
        direction: "monitor".into(),
        repeat_index: 0,
        sweep_index: 0,
        point_index: 0,
        total_points: 0,
    };
    let mut record = simulate_record(&config, &point, elapsed);
    record.timestamp = timestamp();
    record.source_set = source;
    record.source_readback = Some(source);
    record.meter_value = measured;
    record.source_resolution =
        positive_reading(&range).and_then(|r| source_resolution(&pair.source_model, mode, r));
    record.meter_range = meter_range;
    record.meter_resolution = meter_resolution;
    record.source_resource = model_label(Role::Source, &pair.source_model);
    record.meter_resource = model_label(Role::Meter, &pair.meter_model);
    Ok(record)
}

fn set_level(pair: &mut DevicePair, level: f64) -> Result<(), String> {
    pair.write(Role::Source, &format!(":SOUR:LEV:FIX {level:.12e}"))
}

// Resolve display precision from the shared, reviewed instrument TOML.
fn source_resolution(model: &str, mode: Mode, range: f64) -> Option<f64> {
    iv_core::source_display_resolution(model, mode, range)
        .ok()
        .flatten()
}

fn positive_reading(text: &str) -> Option<f64> {
    text.trim()
        .parse::<f64>()
        .ok()
        .filter(|v| v.is_finite() && *v > 0.0 && *v < 1e30)
}

fn write_json_new(path: &Path, value: &serde_json::Value) -> std::io::Result<()> {
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    serde_json::to_writer_pretty(&mut file, value).map_err(std::io::Error::other)?;
    file.sync_all()
}

fn replace_json(path: &Path, value: &serde_json::Value) -> std::io::Result<()> {
    let temporary = path.with_extension("json.tmp");
    write_json_new(&temporary, value)?;
    std::fs::rename(temporary, path)
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

// One owner serializes device I/O. Live samples use the already configured
// sweep NPLC and never enter the CSV or resistance fit.
struct SweepMonitor<'a> {
    config: &'a RunConfig,
    control: &'a Control,
    latest: &'a StateFeed,
    begin: Instant,
    last: Instant,
    source_resolution: Option<f64>,
}
impl SweepMonitor<'_> {
    fn record(&self, level: f64) -> Record {
        let point = iv_core::SweepPoint {
            target: level,
            direction: "monitor".into(),
            repeat_index: 0,
            sweep_index: 0,
            point_index: 0,
            total_points: 0,
        };
        let mut record = simulate_record(self.config, &point, self.begin.elapsed().as_secs_f64());
        record.status = "live".into();
        record
    }

    fn publish(&mut self, mut record: Record, state: &mut Snapshot) {
        record.timestamp = timestamp();
        state.live = Some(record);
        publish(self.latest, state);
    }

    fn acquire(&self, pair: Option<&mut DevicePair>, level: f64) -> Result<Record, String> {
        let mut record = self.record(level);
        if let Some(device) = pair {
            let measured = monitor_query(device, Role::Meter, "READ?", self.control)?;
            record.meter_value = measured
                .trim()
                .parse::<f64>()
                .map_err(|_| "Invalid live meter reading")?;
            if !record.meter_value.is_finite() || record.meter_value.abs() > 1e30 {
                return Err("Meter overload during live settling acquisition".into());
            }
            let readback = monitor_query(device, Role::Source, ":SOUR:LEV?", self.control)?;
            let readback = readback
                .trim()
                .parse::<f64>()
                .map_err(|_| "Invalid live source level")?;
            if !readback.is_finite() {
                return Err("Invalid live source level".into());
            }
            record.source_readback = Some(readback);
            record.source_resolution = self.source_resolution;
            if let Some((range, resolution)) =
                iv_core::meter_display_queries(&device.meter_model, self.config.mode)?
            {
                record.meter_range =
                    positive_reading(&monitor_query(device, Role::Meter, range, self.control)?);
                record.meter_resolution = positive_reading(&monitor_query(
                    device,
                    Role::Meter,
                    resolution,
                    self.control,
                )?);
            } else {
                record.meter_range = None;
                record.meter_resolution = None;
            }
            record.source_resource = model_label(Role::Source, &device.source_model);
            record.meter_resource = model_label(Role::Meter, &device.meter_model);
        }
        Ok(record)
    }

    fn wait(
        &mut self,
        seconds: f64,
        mut pair: Option<&mut DevicePair>,
        level: f64,
        state: &mut Snapshot,
    ) -> Result<bool, String> {
        let end = Instant::now() + Duration::from_secs_f64(seconds.max(0.0));
        // Avoid starting a conversion at the end of a short ramp/settling wait.
        // Budget conservatively for 50 Hz mains, autozero and transport overhead.
        let budget = if pair.is_some() {
            Duration::from_secs_f64((self.config.nplc * 2.0 / 50.0 + 0.03).max(0.05))
        } else {
            Duration::from_millis(1)
        };
        while Instant::now() < end {
            if self.control.cancelled() {
                return Ok(false);
            }
            if self.last.elapsed() >= Duration::from_millis(50)
                && end.saturating_duration_since(Instant::now()) >= budget
            {
                self.last = Instant::now();
                let reading = self.acquire(pair.as_deref_mut(), level);
                if self.control.cancelled() {
                    return Ok(false);
                }
                self.publish(reading?, state);
            } else {
                std::thread::sleep(
                    Duration::from_millis(10).min(end.saturating_duration_since(Instant::now())),
                );
            }
        }
        Ok(!self.control.cancelled())
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
    if !state.source_simulation {
        let device = pair
            .as_deref()
            .ok_or("Cannot start: reconnect the source and meter before measuring.")?;
        if config.source_model != device.source_model || config.meter_model != device.meter_model {
            return Err(format!(
                "Selected run models ({}/{}) do not match the connected instruments ({}/{}). Disconnect, choose the matching models, and reconnect before Start.",
                config.source_model, config.meter_model, device.source_model, device.meter_model
            ));
        }
    }
    let requested_config = config;
    let effective_config = iv_core::prepare_config(config)?;
    let config = &effective_config;
    let planned = plan(config)?;
    // Cached UI state is not proof that a real source is safe to reconfigure.
    // Enforce this here so GUI, CLI and notebook runs share the same preflight.
    if !state.source_simulation {
        state.output = "Unknown".into();
        let device = pair
            .as_deref_mut()
            .ok_or("Cannot start: reconnect the source and meter before measuring.")?;
        let report = device.force_output_off().map_err(|error| {
            format!("Cannot start: source output OFF verification failed. {error}")
        })?;
        if !report.output_off_verified {
            return Err("Cannot start: source output OFF was not verified. Turn output off on the instrument and reconnect.".into());
        }
        state.output = "Off".into();
        if let Some(warning) = report.zeroing_warning {
            return Err(format!(
                "Cannot start: output OFF was verified, but resetting the source level to zero failed. Reconnect before measuring. {warning}"
            ));
        }
    }
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
    let manifest = path.with_extension("run.json");
    let initial_metadata = serde_json::json!({"schema_version":3,"csv_schema":"compact_v2","csv_fields":iv_core::CSV_FIELDS,"csv_numeric_format":"shortest_roundtrip_scientific","application_version":env!("CARGO_PKG_VERSION"),"config":config,"requested_config":requested_config,"outcome":state,"in_progress":true,
        "source_range_setting":planned.source_range,"hardware_compliance_setting":planned.hardware_compliance,"display_settings_changes":[],"simulation":state.simulation,"csv":path.display().to_string()});
    write_json_new(&manifest, &initial_metadata)
        .map_err(|error| format!("Cannot save settings before enabling output: {error}"))?;
    let begin = Instant::now();
    let mut level = 0.0;
    let mut stopped = false;
    let mut compliance_hit = false;
    let mut fit = ResistanceFit::default();
    let mut last_progress = Instant::now();
    let mut actual_source_resolution = None;
    let mut live = SweepMonitor {
        config,
        control,
        latest,
        begin,
        last: Instant::now(),
        source_resolution: None,
    };
    let mut display_settings_changes = Vec::new();
    let mut instrument_status_events = Vec::new();
    let mut previous_display_settings = None;
    let work = (|| -> Result<(), String> {
        if let Some(device) = pair.as_deref_mut() {
            configure(
                device,
                config,
                planned.source_range,
                planned.hardware_compliance,
            )?;
            // The transport translates this canonical query for models such as
            // the 7651 that do not use GS210 SCPI directly.
            let range_query =
                iv_core::source_range_query(&device.source_model)?.unwrap_or(":SOUR:RANG?");
            let range = device.query(Role::Source, range_query)?;
            actual_source_resolution = positive_reading(&range)
                .and_then(|range| source_resolution(&device.source_model, config.mode, range));
            live.source_resolution = actual_source_resolution;
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
                    if !live.wait(config.ramp_wait_s, Some(device), level, state)? {
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
            if !live.wait(settling, pair.as_deref_mut(), level, state)? {
                stopped = true;
                break;
            }
            let mut record = if let Some(device) = pair.as_deref_mut() {
                state.message.clear();
                state.message.push_str("Reading meter…");
                publish_progress(latest, state, &mut last_progress);
                let mut value = 0.0;
                let mut readings_completed = 0_u32;
                let mut sample_limit_hit = false;
                let mut trigger_value = 0.0;
                let mut meter_resolution: Option<f64> = None;
                let mut meter_range: Option<f64> = None;
                let mut resolution_known = true;
                let display_queries =
                    iv_core::meter_display_queries(&device.meter_model, config.mode)?;
                for _ in 0..config.average_count {
                    if control.cancelled() {
                        stopped = true;
                        break;
                    }
                    live.last = Instant::now();
                    let text = device.query(Role::Meter, "READ?")?;
                    let measured = text
                        .trim()
                        .parse::<f64>()
                        .map_err(|_| format!("Invalid meter reading: {text}"))?;
                    if !measured.is_finite() || measured.abs() > 1e30 {
                        return Err("Meter reported overload or an invalid reading.".into());
                    }
                    value += measured / config.average_count as f64;
                    readings_completed += 1;
                    // Do not allow averaging to hide an individual over-limit sample.
                    if measured.abs() >= config.compliance {
                        sample_limit_hit = true;
                        trigger_value = measured;
                        resolution_known = false;
                        break;
                    }
                    if control.cancelled() {
                        stopped = true;
                        break;
                    }
                    // Query the range/resolution after READ? so autoranging is
                    // reflected. Preserve the coarsest resolution when averaging.
                    let (range, resolution) =
                        if let Some((range_query, resolution_query)) = display_queries {
                            let range = device.query(Role::Meter, range_query)?;
                            if control.cancelled() {
                                stopped = true;
                                break;
                            }
                            let resolution = device.query(Role::Meter, resolution_query)?;
                            (positive_reading(&range), positive_reading(&resolution))
                        } else {
                            // A profile without verified display commands can still
                            // acquire readings, but its precision metadata stays unknown.
                            (None, None)
                        };
                    resolution_known &= resolution.is_some() || display_queries.is_none();
                    if let Some(resolution) = resolution {
                        meter_resolution = Some(meter_resolution.unwrap_or(0.0).max(resolution));
                    }
                    if let Some(range) = range {
                        meter_range = Some(meter_range.unwrap_or(0.0).max(range));
                    }
                    // Show each actual conversion, rather than waiting for
                    // the complete average and the next saved sweep point.
                    let mut reading = live.record(level);
                    reading.meter_value = measured;
                    reading.source_readback = None;
                    reading.source_resolution = actual_source_resolution;
                    reading.meter_range = range;
                    reading.meter_resolution = resolution;
                    reading.source_resource = model_label(Role::Source, &device.source_model);
                    reading.meter_resource = model_label(Role::Meter, &device.meter_model);
                    live.publish(reading, state);
                }
                if stopped {
                    break;
                }
                if sample_limit_hit {
                    value *= config.average_count as f64 / readings_completed as f64;
                    instrument_status_events.push(serde_json::json!({
                        "point_index":point.point_index,"event":"averaging_compliance_stop",
                        "readings_completed":readings_completed,"readings_requested":config.average_count,
                        "trigger_value":trigger_value,"trigger_unit":if config.mode == Mode::Iv { "A" } else { "V" },
                        "software_limit":config.compliance
                    }));
                }
                let readback = read_level(device)?;
                let mut record = simulate_record(config, point, begin.elapsed().as_secs_f64());
                record.source_set = level;
                record.source_readback = readback;
                record.meter_value = value;
                record.source_resolution = actual_source_resolution;
                record.meter_resolution = if resolution_known {
                    meter_resolution
                } else {
                    None
                };
                record.meter_range = meter_range;
                record.source_resource = model_label(Role::Source, &device.source_model);
                record.meter_resource = model_label(Role::Meter, &device.meter_model);
                record.compliance = sample_limit_hit || value.abs() >= config.compliance;
                record
            } else {
                let mut record = simulate_record(config, point, begin.elapsed().as_secs_f64());
                record.compliance = record.meter_value.abs() >= config.compliance;
                record
            };
            record.timestamp = timestamp();
            record.status = if record.compliance {
                "compliance"
            } else {
                "running"
            }
            .into();
            compliance_hit = record.compliance;
            if !record.instrument_status.is_empty() {
                instrument_status_events.push(serde_json::json!({"point_index":record.point.point_index,"instrument_status":record.instrument_status}));
            }
            let display_settings = (
                record.source_resolution,
                record.meter_range,
                record.meter_resolution,
            );
            if previous_display_settings != Some(display_settings) {
                display_settings_changes.push(serde_json::json!({"point_index":record.point.point_index,"source_resolution":record.source_resolution,"meter_range":record.meter_range,"meter_resolution":record.meter_resolution}));
                previous_display_settings = Some(display_settings);
            }
            writer.append(&record)?;
            let source_value = record.source_readback.unwrap_or(record.source_set);
            let (voltage, current) = if config.mode == Mode::Iv {
                (source_value, record.meter_value)
            } else {
                (record.meter_value, source_value)
            };
            state.resistance = fit.add(voltage, current);
            state.last = Some(record.clone());
            if state.simulation {
                live.last = Instant::now();
                live.publish(record.clone(), state);
            }
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
    let metadata = serde_json::json!({"schema_version":3,"csv_schema":"compact_v2","csv_fields":iv_core::CSV_FIELDS,"csv_numeric_format":"shortest_roundtrip_scientific","application_version":env!("CARGO_PKG_VERSION"),"config":config,"requested_config":requested_config,"outcome":state,"in_progress":false,
        "source_range_setting":planned.source_range,"hardware_compliance_setting":planned.hardware_compliance,"display_settings_changes":display_settings_changes,"instrument_status_events":instrument_status_events,"simulation":state.simulation,"csv":path.display().to_string()});
    if let Err(error) = replace_json(&manifest, &metadata) {
        state
            .warnings
            .push(format!("Run metadata could not be saved: {error}"));
    }
    publish(latest, state);
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn metadata_update_keeps_initial_settings_when_final_write_fails() {
        let path = std::env::temp_dir().join(format!(
            "iv-metadata-{}-{}.run.json",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let initial = serde_json::json!({"in_progress":true,"config":{"nplc":1.0}});
        super::write_json_new(&path, &initial).unwrap();
        assert!(super::write_json_new(&path, &serde_json::json!({})).is_err());
        let temporary = path.with_extension("json.tmp");
        std::fs::write(&temporary, "collision").unwrap();
        let final_value = serde_json::json!({"in_progress":false,"config":{"nplc":1.0},"points":3});
        assert!(super::replace_json(&path, &final_value).is_err());
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&std::fs::read(&path).unwrap()).unwrap(),
            initial
        );
        std::fs::remove_file(&temporary).unwrap();
        super::replace_json(&path, &final_value).unwrap();
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&std::fs::read(&path).unwrap()).unwrap(),
            final_value
        );
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn gs210_resolution_uses_actual_range_and_rejects_unknown_ranges() {
        assert_eq!(
            super::source_resolution("YOKOGAWA_GS210", iv_core::Mode::Iv, 0.01),
            Some(1e-7)
        );
        assert_eq!(
            super::source_resolution("YOKOGAWA_GS210", iv_core::Mode::Iv, 30.0),
            Some(1e-3)
        );
        assert_eq!(
            super::source_resolution("YOKOGAWA_GS210", iv_core::Mode::Vi, 0.2),
            Some(1e-6)
        );
        assert_eq!(
            super::source_resolution("YOKOGAWA_GS210", iv_core::Mode::Iv, 0.2),
            None
        );
        assert_eq!(
            super::source_resolution("YOKOGAWA_7651", iv_core::Mode::Iv, 3.0),
            Some(1e-6)
        );
        assert_eq!(super::positive_reading("+1.5E-09\n"), Some(1.5e-9));
        for invalid in ["NaN", "inf", "0", "-1", "9.9e37", "invalid"] {
            assert_eq!(super::positive_reading(invalid), None);
        }
    }

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
        connect_role(
            &mut pair,
            &mut state,
            Role::Source,
            true,
            "",
            "SIMULATED_SOURCE",
        )
        .unwrap();
        assert!(state.source_connected);
        assert!(!state.connected);
        // Repeating the same model/resource is idempotent, but a different
        // requested model requires an explicit disconnect first.
        connect_role(
            &mut pair,
            &mut state,
            Role::Source,
            true,
            "",
            "SIMULATED_SOURCE",
        )
        .unwrap();
        assert!(
            connect_role(
                &mut pair,
                &mut state,
                Role::Source,
                true,
                "",
                "YOKOGAWA_GS210",
            )
            .is_err()
        );
        assert!(
            connect_role(
                &mut pair,
                &mut state,
                Role::Source,
                false,
                "GPIB0::5::INSTR",
                "YOKOGAWA_GS210",
            )
            .is_err()
        );
        assert!(state.source_connected && state.source_simulation);
        connect_role(
            &mut pair,
            &mut state,
            Role::Meter,
            true,
            "",
            "SIMULATED_METER",
        )
        .unwrap();
        assert!(state.connected && state.simulation);
        disconnect_role(&mut pair, &mut state, Role::Source, &live_control()).unwrap();
        assert!(!state.source_connected && state.meter_connected);
        assert_eq!(state.output, "Off");
        connect_role(
            &mut pair,
            &mut state,
            Role::Source,
            true,
            "",
            "SIMULATED_SOURCE",
        )
        .unwrap();
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
