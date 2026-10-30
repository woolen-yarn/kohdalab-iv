mod balanced;
mod gate;
mod meter;
mod reference;
mod safety;
mod source_dc;
mod workflow;
use crate::compatibility::{CSV_SCHEMA, RUN_METADATA_SCHEMA_VERSION};
use crate::protocol::{
    Command, GuiJobCompletion, ProtectionAlert, ProtectionKind, Snapshot, unwrap_gui_job,
};
use crate::raw_csv::{self, RawWriter};
use crate::state::{Activity, ErrorKind, Failure, OutputState, Phase};
use iv_core::{CsvWriter, Mode, Record, RunConfig, plan, simulate_record};
use iv_transport::{DevicePair, HelperPaths, Role, TransportExchange};
use std::io::{BufRead, Read, Write};
use std::path::Path;
use std::sync::{
    Arc, Condvar, Mutex,
    atomic::{AtomicBool, AtomicU8, Ordering},
    mpsc,
};
use std::time::{Duration, Instant};
use workflow::{RampPolicy, completion_phase};

const SAFETY_STOP: u8 = 1;
const SAFETY_GATE_OFF: u8 = 2;
const SAFETY_SOURCE_OFF: u8 = 4;
const SAFETY_SOURCE2_OFF: u8 = 8;
const SAFETY_ALL_OFF: u8 = 16;

struct Control {
    stop: AtomicBool,
    off: AtomicBool,
    /// Set by the input reader when a gate update arrives during an IV run.
    /// The engine consumes it after the run has made every output safe.
    gate_request_rejected: Mutex<Option<Option<String>>>,
    /// As for gate updates: a DC request received after Run admission is
    /// discarded, never replayed after the sweep owns the source outputs.
    source_dc_request_rejected: Mutex<Option<Option<String>>>,
    /// A second Run must not clear a pending safety latch or queue behind an
    /// already-reserved sweep owner.
    run_request_rejected: Mutex<Option<Option<String>>>,
    /// A reader-side safety request remains latched until the engine owner
    /// has processed its matching OFF/Stop command. Later ON requests never
    /// clear this bitmask.
    pending_safety: AtomicU8,
    /// True from admission until the complete synchronous output operation
    /// finishes (Run, Gate DC, or Source DC).
    running: AtomicBool,
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
            || self.pending_safety.load(Ordering::Acquire) != 0
            || self.closed.load(Ordering::Acquire)
            || self
                .heartbeat
                .lock()
                .map_or(true, |t| t.elapsed() > Duration::from_secs(2))
    }
}

fn safety_bit(command: &Command) -> u8 {
    match command {
        Command::Stop => SAFETY_STOP,
        Command::GateOutputOff => SAFETY_GATE_OFF,
        Command::SourceDcOff { secondary: false } => SAFETY_SOURCE_OFF,
        Command::SourceDcOff { secondary: true } => SAFETY_SOURCE2_OFF,
        Command::OutputOff => SAFETY_ALL_OFF,
        _ => 0,
    }
}

fn output_operation(command: &Command) -> bool {
    matches!(
        command,
        Command::Run { .. } | Command::SetGateVoltage { .. } | Command::SetSourceDc { .. }
    )
}

/// Clear only the safety intent whose owner command has completed.  A later
/// ON request cannot acknowledge an OFF request it did not execute.
fn acknowledge_safety(control: &Control, bit: u8) {
    if bit == 0 {
        return;
    }
    control.pending_safety.fetch_and(!bit, Ordering::AcqRel);
    if control.pending_safety.load(Ordering::Acquire) == 0 {
        control.stop.store(false, Ordering::Release);
        control.off.store(false, Ordering::Release);
    }
}

pub fn serve() -> Result<(), String> {
    let control = Arc::new(Control {
        stop: AtomicBool::new(false),
        off: AtomicBool::new(false),
        gate_request_rejected: Mutex::new(None),
        source_dc_request_rejected: Mutex::new(None),
        run_request_rejected: Mutex::new(None),
        pending_safety: AtomicU8::new(0),
        running: AtomicBool::new(false),
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
            // Job envelopes are removed before any immediate-safety or run
            // ownership logic.  Invalid envelopes never enter the normal
            // queue, where they could otherwise delay a safety command.
            let Ok((gui_job, command)) = unwrap_gui_job(command) else {
                continue;
            };
            if let Ok(mut heartbeat) = reader_control.heartbeat.lock() {
                *heartbeat = Instant::now();
            }
            match command {
                Command::Heartbeat => {}
                Command::Stop => {
                    reader_control
                        .pending_safety
                        .fetch_or(SAFETY_STOP, Ordering::AcqRel);
                    reader_control.stop.store(true, Ordering::Release);
                    if tx.try_send((None, Command::Stop)).is_err() {
                        reader_control.closed.store(true, Ordering::Release);
                        break;
                    }
                }
                Command::GateOutputOff => {
                    reader_control
                        .pending_safety
                        .fetch_or(SAFETY_GATE_OFF, Ordering::AcqRel);
                    reader_control.off.store(true, Ordering::Release);
                    reader_control.stop.store(true, Ordering::Release);
                    if tx.try_send((None, Command::GateOutputOff)).is_err() {
                        reader_control.closed.store(true, Ordering::Release);
                    }
                }
                Command::SourceDcOff { secondary } => {
                    // Latch cancellation before enqueueing so an active run
                    // fails closed even when its normal queue is saturated.
                    reader_control.pending_safety.fetch_or(
                        if secondary {
                            SAFETY_SOURCE2_OFF
                        } else {
                            SAFETY_SOURCE_OFF
                        },
                        Ordering::AcqRel,
                    );
                    reader_control.off.store(true, Ordering::Release);
                    reader_control.stop.store(true, Ordering::Release);
                    if tx
                        .try_send((None, Command::SourceDcOff { secondary }))
                        .is_err()
                    {
                        // A full normal queue must never turn an OFF request
                        // into a delayed ON-capable operation. Closing makes
                        // the owner leave its loop and execute its existing
                        // all-source verified-OFF cleanup path.
                        reader_control.closed.store(true, Ordering::Release);
                    }
                }
                Command::OutputOff => {
                    reader_control
                        .pending_safety
                        .fetch_or(SAFETY_ALL_OFF, Ordering::AcqRel);
                    reader_control.off.store(true, Ordering::Release);
                    reader_control.stop.store(true, Ordering::Release);
                    if tx.try_send((None, Command::OutputOff)).is_err() {
                        reader_control.closed.store(true, Ordering::Release);
                    }
                }
                Command::Shutdown => break,
                other => {
                    let output_operation = matches!(
                        other,
                        Command::Run { .. }
                            | Command::SetGateVoltage { .. }
                            | Command::SetSourceDc { .. }
                    );
                    if output_operation
                        && (reader_control.running.load(Ordering::Acquire)
                            || reader_control.pending_safety.load(Ordering::Acquire) != 0)
                    {
                        let rejected_slot = if matches!(other, Command::Run { .. }) {
                            &reader_control.run_request_rejected
                        } else if matches!(other, Command::SetSourceDc { .. }) {
                            &reader_control.source_dc_request_rejected
                        } else {
                            &reader_control.gate_request_rejected
                        };
                        if let Ok(mut rejected) = rejected_slot.lock() {
                            if rejected.is_some() {
                                break;
                            }
                            *rejected = Some(gui_job);
                        }
                        continue;
                    }
                    if output_operation {
                        // Reserve ownership at admission so no second ON
                        // request can slip into a ramp/configuration gap.
                        reader_control.running.store(true, Ordering::Release);
                    }
                    if tx.try_send((gui_job, other)).is_err() {
                        if output_operation {
                            reader_control.running.store(false, Ordering::Release);
                        }
                        reader_control.closed.store(true, Ordering::Release);
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
    let mut monitor_guard = None;
    let mut monitor_reference = None;
    let mut monitor_ranges = [None, None];
    let mut simulation_mode = Mode::Iv;
    publish(&latest, &state);
    while !control.closed.load(Ordering::Acquire) {
        if (state.source_connected
            || state.source2_connected
            || state.gate_connected
            || state.meter_connected
            || state.reference_meter_connected)
            && control
                .heartbeat
                .lock()
                .map_or(true, |t| t.elapsed() > Duration::from_secs(2))
        {
            if state.source_connected || state.source2_connected {
                clean_output(pair.as_mut(), &mut state, None, &control);
            }
            if let Err(error) = shutdown_gate(pair.as_mut(), &mut state) {
                state.error = error.into();
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
        let (gui_job_id, command) = match rx.recv_timeout(wait) {
            Ok(command) => command,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if state.gate_connected
                    && state.gate_output == OutputState::On
                    && let Err(error) = gate::check(pair.as_mut(), &mut state)
                {
                    clean_output(pair.as_mut(), &mut state, None, &control);
                    state.phase = Phase::Failed;
                    state.error = error.into();
                    publish(&latest, &state);
                    continue;
                }
                let mut dc_fault = false;
                for secondary in [false, true] {
                    if let Err(error) = source_dc::check(pair.as_mut(), &mut state, secondary) {
                        // A DC limiter/query fault shares the IV safety
                        // domain: close both ordinary outputs and any held
                        // gate before reporting the latched failure.
                        clean_output(pair.as_mut(), &mut state, None, &control);
                        let gate_error = shutdown_gate(pair.as_mut(), &mut state).err();
                        state.phase = Phase::Failed;
                        state.error = gate_error
                            .map_or(error.clone(), |other| format!("{error} · {other}"))
                            .into();
                        publish(&latest, &state);
                        dc_fault = true;
                        break;
                    }
                }
                if dc_fault {
                    continue;
                }

                if monitor_ready && last_monitor.elapsed() >= monitor_interval {
                    last_monitor = Instant::now();
                    state.balanced_active = false;
                    if state.source2_connected {
                        if state.source2_simulation {
                            state.source2_level = state
                                .source2_dc
                                .as_ref()
                                .filter(|_| state.source2_output == OutputState::On)
                                .map(|dc| dc.value)
                                .or(Some(0.0));
                        } else if let Some(device) = pair.as_mut() {
                            match device.query(Role::Source2, ":SOUR:LEV?") {
                                Ok(reply) => {
                                    state.source2_level =
                                        reply.trim().parse::<f64>().ok().filter(|v| v.is_finite());
                                }
                                Err(error) => {
                                    clean_output(pair.as_mut(), &mut state, None, &control);
                                    let gate_error = shutdown_gate(pair.as_mut(), &mut state).err();
                                    state.phase = Phase::Failed;
                                    state.error = match gate_error {
                                        Some(gate_error) => format!(
                                            "Source 2 idle read failed: {error} · {gate_error}"
                                        ),
                                        None => format!("Source 2 idle read failed: {error}"),
                                    }
                                    .into();
                                    publish(&latest, &state);
                                    continue;
                                }
                            }
                        }
                    }
                    let result = if state.simulation {
                        let dc = state
                            .source_dc
                            .as_ref()
                            .filter(|_| state.output == OutputState::On);
                        let config = RunConfig {
                            mode: dc.map_or(simulation_mode, |dc| dc.mode),
                            reference_meter: state.reference_meter_connected.then(|| {
                                iv_core::ReferenceMeterConfig {
                                    model: "AGILENT_34411A".into(),
                                }
                            }),
                            ..Default::default()
                        };
                        let point = iv_core::SweepPoint {
                            target: dc.map_or(0.0, |dc| dc.value),
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
                            &mut state,
                            monitor_ranges,
                            &mut monitor_guard,
                            &mut monitor_reference,
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
                                format!("Live monitor failed: {error}. Reconnect the instruments.")
                                    .into();
                            state.phase = Phase::Failed;
                            clean_output(pair.as_mut(), &mut state, None, &control);
                            // Cleanup publishes its own transient phase. Keep the
                            // completed live fault terminal and visible afterward.
                            state.phase = Phase::Failed;
                        }
                    }
                    publish(&latest, &state);
                }
                continue;
            }
            Err(_) => break,
        };
        // Preserve meter configuration and the baseline across held DC edits.
        if matches!(
            &command,
            Command::Connect { .. }
                | Command::ConnectSource { .. }
                | Command::ConnectMeter { .. }
                | Command::ConnectReferenceMeter { .. }
                | Command::DisconnectReferenceMeter
                | Command::DisconnectSource
                | Command::DisconnectMeter
                | Command::Disconnect
                | Command::SetLiveMode { .. }
                | Command::Run { .. }
        ) {
            monitor_mode = None;
            monitor_guard = None;
            monitor_reference = None;
        }
        let owns_output = output_operation(&command);
        let safety_intent = safety_bit(&command);
        let result = match command {
            Command::Discover => discover(pair.as_mut(), &mut state, &latest),
            Command::SetLiveMode { mode } => {
                state.phase = Phase::SwitchingMode;
                state.message = "Verifying output off before switching live channels…".into();
                publish(&latest, &state);
                let result = set_live_mode(pair.as_mut(), &mut state, mode, monitor_ranges);
                if result.is_ok() {
                    simulation_mode = mode;
                }
                result
            }
            Command::SetMeterRanges {
                meter_voltage_range,
                meter_current_range,
            } => {
                let ranges = [meter_voltage_range, meter_current_range];
                if ranges.iter().flatten().any(|v| !v.is_finite() || *v <= 0.0) {
                    Err("Fixed Meter ranges must be finite and positive.".into())
                } else if ranges != monitor_ranges && !sweep_outputs_off(&state) {
                    Err("Turn all source outputs OFF before changing Meter range.".into())
                } else {
                    if ranges != monitor_ranges {
                        monitor_mode = None;
                        monitor_guard = None;
                        monitor_reference = None;
                    }
                    monitor_ranges = ranges;
                    Ok(())
                }
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
            Command::ConnectSource2 {
                simulation,
                resource,
                model,
            } => {
                begin_connect(&mut state, &latest);
                connect_role(
                    &mut pair,
                    &mut state,
                    Role::Source2,
                    simulation,
                    &resource,
                    &model,
                )
            }
            Command::ConnectGate {
                simulation,
                resource,
                model,
            } => {
                begin_connect(&mut state, &latest);
                connect_role(
                    &mut pair,
                    &mut state,
                    Role::Gate,
                    simulation,
                    &resource,
                    &model,
                )
            }
            Command::SetGateVoltage {
                voltage,
                compliance,
                ramp_step,
                range,
            } => gate::set_voltage(
                pair.as_mut(),
                &mut state,
                voltage,
                compliance,
                ramp_step,
                range,
                &control,
            ),
            Command::GateOutputOff => {
                state.safety_revision = state.safety_revision.wrapping_add(1);
                gate::output_off(pair.as_mut(), &mut state)
            }
            Command::SetSourceDc {
                secondary,
                mode,
                value,
                compliance,
                ramp_step,
                range,
            } => {
                let prepared = if state.reference_meter_connected && mode != simulation_mode {
                    Err("Disconnect both Meters before changing the source function; Meter (V) and Meter (I) assignments must stay unchanged.".to_owned())
                } else if control.cancelled() {
                    Ok(())
                } else if state.source_connected && state.meter_connected && !state.simulation {
                    pair.as_mut()
                        .ok_or_else(|| "Live meter transport missing".to_owned())
                        .and_then(|device| {
                            prepare_live_meter(
                                device,
                                &state,
                                mode,
                                monitor_ranges,
                                &mut monitor_mode,
                                &mut monitor_guard,
                                &mut monitor_reference,
                                &control,
                            )
                        })
                } else {
                    Ok(())
                };
                prepared.and_then(|()| {
                    source_dc::set(
                        pair.as_mut(),
                        &mut state,
                        secondary,
                        crate::protocol::DcOutputState {
                            mode,
                            value,
                            compliance,
                            ramp_step,
                            range,
                        },
                        &control,
                    )
                })
            }
            Command::SourceDcOff { secondary } => {
                state.safety_revision = state.safety_revision.wrapping_add(1);
                source_dc::output_off(pair.as_mut(), &mut state, secondary)
            }
            Command::DisconnectSource2 => {
                disconnect_role(&mut pair, &mut state, Role::Source2, &control)
            }
            Command::DisconnectGate => disconnect_role(&mut pair, &mut state, Role::Gate, &control),
            Command::ConnectReferenceMeter {
                simulation,
                resource,
                model,
            } => {
                monitor_mode = None;
                begin_connect(&mut state, &latest);
                connect_role(
                    &mut pair,
                    &mut state,
                    Role::ReferenceMeter,
                    simulation,
                    &resource,
                    &model,
                )
            }
            Command::DisconnectReferenceMeter => {
                monitor_mode = None;
                disconnect_role(&mut pair, &mut state, Role::ReferenceMeter, &control)
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
                monitor_ranges = [config.meter_voltage_range, config.meter_current_range];
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
            Command::Stop => {
                state.safety_revision = state.safety_revision.wrapping_add(1);
                if state.source_connected || state.source2_connected {
                    clean_output(pair.as_mut(), &mut state, None, &control);
                }
                if state.error.is_empty() {
                    if !matches!(state.phase, Phase::ComplianceStop | Phase::SourceLimitStop) {
                        state.phase = Phase::Stopped;
                        state.message = "Stop acknowledged. Sweep outputs off.".into();
                    }
                    Ok(())
                } else {
                    shutdown_gate(pair.as_mut(), &mut state)
                }
            }
            Command::OutputOff => {
                state.safety_revision = state.safety_revision.wrapping_add(1);
                if state.source_connected || state.source2_connected {
                    clean_output(pair.as_mut(), &mut state, None, &control);
                }
                let gate_result = shutdown_gate(pair.as_mut(), &mut state);
                if state.output == OutputState::Off && gate_result.is_ok() {
                    state.phase = Phase::Ready;
                    state.message = "Output off verified.".into();
                }
                Ok(())
            }
            Command::Disconnect => {
                let mut errors = Vec::new();
                for role in [
                    Role::Source,
                    Role::Source2,
                    Role::Gate,
                    Role::Meter,
                    Role::ReferenceMeter,
                ] {
                    if let Err(error) = disconnect_role(&mut pair, &mut state, role, &control) {
                        errors.push(error);
                    }
                }
                if errors.is_empty() {
                    Ok(())
                } else {
                    Err(errors.join(" · "))
                }
            }
            _ => Ok(()),
        };
        if let Err(error) = result {
            if state.source2_connected && state.output != OutputState::Off {
                clean_output(pair.as_mut(), &mut state, None, &control);
            }
            let gate_error = shutdown_gate(pair.as_mut(), &mut state).err();
            state.phase = Phase::Failed;
            state.error = match gate_error {
                Some(other) => format!("{error} · {other}"),
                None => error,
            }
            .into();
            state.message = "Review the error and reconnect if needed.".into();
        }
        acknowledge_safety(&control, safety_intent);
        if owns_output {
            control.running.store(false, Ordering::Release);
        }
        let rejected_gate_job = control
            .gate_request_rejected
            .lock()
            .ok()
            .and_then(|mut rejected| rejected.take());
        let rejected_source_dc_job = control
            .source_dc_request_rejected
            .lock()
            .ok()
            .and_then(|mut rejected| rejected.take());
        let rejected_run_job = control
            .run_request_rejected
            .lock()
            .ok()
            .and_then(|mut rejected| rejected.take());
        if rejected_gate_job.is_some() {
            // This is a command-admission error, not a late configuration
            // change.  The request was discarded while the run owned both
            // the IV outputs and the gate remained at its verified level.
            state.phase = Phase::Failed;
            state.error = "Gate voltage changes are rejected while an IV run is active. Wait for the run to finish, then set the gate voltage.".into();
            state.message = "Gate voltage was not changed during the IV run.".into();
        }
        if rejected_source_dc_job.is_some() {
            state.phase = Phase::Failed;
            state.error = "Source DC changes are rejected while an IV run is active. Wait for the run to finish, then enable DC output.".into();
            state.message = "Source DC output was not changed during the IV run.".into();
        }
        if rejected_run_job.is_some() {
            state.phase = Phase::Failed;
            state.error = "A measurement is already active or queued. Wait for it to finish before starting another run.".into();
            state.message = "The second Start request was not queued.".into();
        }
        if let Some(id) = gui_job_id {
            complete_gui_job(&mut state, id);
        }
        // A tagged gate update rejected while a run held the outputs has no
        // queued command to complete.  Record its final admission error only
        // after the run's cleanup and error handling have finished.
        if let Some(Some(id)) = rejected_gate_job {
            complete_gui_job(&mut state, id);
        }
        if let Some(Some(id)) = rejected_source_dc_job {
            complete_gui_job(&mut state, id);
        }
        if let Some(Some(id)) = rejected_run_job {
            complete_gui_job(&mut state, id);
        }
        publish(&latest, &state);
    }
    if state.source_connected || state.source2_connected {
        clean_output(pair.as_mut(), &mut state, None, &control);
    }
    if let Err(error) = shutdown_gate(pair.as_mut(), &mut state) {
        state.error = error.into();
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
    state.source2_connected = false;
    state.gate_connected = false;
    state.meter_connected = false;
    state.reference_meter_connected = false;
    state.connected = false;
    state.source_dc = None;
    state.source2_dc = None;
    state.source_dc_limits = None;
    state.source2_dc_limits = None;
    state.gate_dc_limits = None;
}

fn refresh_connections(state: &mut Snapshot) {
    state.connected = state.source_connected && state.meter_connected;
    state.simulation = state.source_simulation && state.meter_simulation;
}

fn role_label(role: Role) -> &'static str {
    match role {
        Role::Source => "source",
        Role::Source2 => "source2",
        Role::Gate => "gate",
        Role::Meter => "meter",
        Role::ReferenceMeter => "reference_meter",
    }
}
fn model_label(role: Role, model: &str) -> String {
    format!("{}.{model}", role_label(role))
}
fn role_state(state: &Snapshot, role: Role) -> (bool, bool, &str) {
    match role {
        Role::ReferenceMeter => (
            state.reference_meter_connected,
            state.reference_meter_simulation,
            &state.reference_meter_resource,
        ),
        Role::Source => (
            state.source_connected,
            state.source_simulation,
            &state.source_resource,
        ),
        Role::Source2 => (
            state.source2_connected,
            state.source2_simulation,
            &state.source2_resource,
        ),
        Role::Gate => (
            state.gate_connected,
            state.gate_simulation,
            &state.gate_resource,
        ),
        Role::Meter => (
            state.meter_connected,
            state.meter_simulation,
            &state.meter_resource,
        ),
    }
}
fn begin_connect(state: &mut Snapshot, latest: &StateFeed) {
    state.phase = Phase::Connecting;
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
    let source = !matches!(role, Role::Meter | Role::ReferenceMeter);
    let (connected, previous_simulation, previous_resource) = role_state(state, role);
    let previous_model = if previous_simulation {
        Some(if source {
            "SIMULATED_SOURCE"
        } else {
            "SIMULATED_METER"
        })
    } else {
        pair.as_ref().map(|device| device.model(role))
    };
    if connected {
        if simulation == previous_simulation
            && previous_model == Some(model)
            && (simulation || resource == previous_resource)
        {
            state.phase = Phase::Ready;
            state.message = "Instrument already connected.".into();
            return Ok(());
        }
        return Err(
            "Disconnect this instrument before choosing a different device or resource.".into(),
        );
    }
    if simulation
        && model
            != if source {
                "SIMULATED_SOURCE"
            } else {
                "SIMULATED_METER"
            }
    {
        // Existing full-connect simulation passes physical model IDs; retain that API.
        if iv_core::instrument_descriptor(model).is_none() {
            return Err("Unsupported simulation model".into());
        }
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
        match role {
            Role::Source => device.source_identity.clone(),
            Role::Source2 => device.source2_identity.clone(),
            Role::Gate => device.gate_identity.clone(),
            Role::Meter => device.meter_identity.clone(),
            Role::ReferenceMeter => device.reference_meter_identity.clone(),
        }
    };
    let resource = if simulation {
        if source {
            "SIMULATED_SOURCE"
        } else {
            "SIMULATED_METER"
        }
    } else {
        resource
    }
    .to_owned();
    match role {
        Role::Source => {
            state.source_connected = true;
            state.source_simulation = simulation;
            state.source_identity = identity.clone();
            state.source_resource = resource;
            state.output = if simulation {
                OutputState::Off
            } else {
                OutputState::Unknown
            };
        }
        Role::Source2 => {
            state.source2_connected = true;
            state.source2_simulation = simulation;
            state.source2_identity = identity.clone();
            state.source2_resource = resource;
            state.source2_output = if simulation {
                OutputState::Off
            } else {
                OutputState::Unknown
            };
        }
        Role::Gate => {
            state.gate_connected = true;
            state.gate_simulation = simulation;
            state.gate_identity = identity.clone();
            state.gate_resource = resource;
            state.gate_output = if simulation {
                OutputState::Off
            } else {
                OutputState::Unknown
            };
        }
        Role::ReferenceMeter => {
            state.reference_meter_connected = true;
            state.reference_meter_simulation = simulation;
            state.reference_meter_identity = identity.clone();
            state.reference_meter_resource = resource;
        }
        Role::Meter => {
            state.meter_connected = true;
            state.meter_simulation = simulation;
            state.meter_identity = identity.clone();
            state.meter_resource = resource;
        }
    }
    refresh_connections(state);
    state.phase = Phase::Ready;
    state.message = format!("Connected {}: {identity}", model_label(role, model));
    Ok(())
}

fn disconnect_role(
    pair: &mut Option<DevicePair>,
    state: &mut Snapshot,
    role: Role,
    control: &Control,
) -> Result<(), String> {
    let (connected, simulation, _) = role_state(state, role);
    if !connected {
        return Ok(());
    }
    if role == Role::Gate {
        gate::output_off(pair.as_mut(), state)?;
    } else if !matches!(role, Role::Meter | Role::ReferenceMeter) {
        clean_output(pair.as_mut(), state, None, control);
        if state.output != OutputState::Off {
            return Err(state.error.to_string());
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
    match role {
        Role::Source => state.source_connected = false,
        Role::Source2 => state.source2_connected = false,
        Role::Gate => state.gate_connected = false,
        Role::Meter => state.meter_connected = false,
        Role::ReferenceMeter => state.reference_meter_connected = false,
    }
    refresh_connections(state);
    if !state.source_connected
        && !state.source2_connected
        && !state.gate_connected
        && !state.meter_connected
        && !state.reference_meter_connected
        && let Some(mut device) = pair.take()
    {
        device.close()?;
    }
    state.phase = if state.source_connected
        || state.source2_connected
        || state.gate_connected
        || state.meter_connected
        || state.reference_meter_connected
    {
        Phase::Ready
    } else {
        Phase::Idle
    };
    state.message = format!("Disconnected {}.", role_label(role));
    detach
}

fn publish(shared: &StateFeed, state: &Snapshot) {
    if let Ok(mut pending) = shared.pending.lock() {
        pending.snapshot = Some(state.clone());
        shared.changed.notify_one();
    }
}

/// API job bookkeeping is session state, not measurement provenance. Keep
/// previous jobs out of both normal and Dual measurement manifests.
fn recording_snapshot(state: &Snapshot) -> serde_json::Value {
    let mut value = serde_json::json!(state);
    if let Some(object) = value.as_object_mut() {
        object.remove("gui_job");
        object.remove("gui_jobs");
    }
    value
}

fn complete_gui_job(state: &mut Snapshot, id: String) {
    let mut completed = state.clone();
    completed.gui_job = None;
    completed.gui_jobs.clear();
    completed.resources.clear();
    completed.warnings.clear();
    // Snapshot only contains serializable protocol data.  If that invariant
    // ever changes, retain an explicit error object rather than recursively
    // serializing the completion wrapper itself.
    let snapshot = serde_json::to_value(completed).unwrap_or_else(|error| {
        serde_json::json!({"phase":"Failed","error":format!("Cannot serialize GUI job result: {error}")})
    });
    let completion = GuiJobCompletion { id, snapshot };
    if state.gui_jobs.len() == 4 {
        state.gui_jobs.remove(0);
    }
    state.gui_jobs.push(completion.clone());
    state.gui_job = Some(completion);
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
    state.phase = Phase::Scanning;
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
    state.phase = if state.connected {
        Phase::Ready
    } else {
        Phase::Idle
    };
    state.message = format!("Found {} connected resources.", state.resources.len());
    Ok(())
}

fn timestamp() -> String {
    format_timestamp(time::OffsetDateTime::now_utc())
}

fn format_timestamp(now: time::OffsetDateTime) -> String {
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:06}Z",
        now.year(),
        now.month() as u8,
        now.day(),
        now.hour(),
        now.minute(),
        now.second(),
        now.nanosecond() / 1_000,
    )
}

#[derive(Clone, Debug)]
struct HostTiming {
    started_at_utc: String,
    completed_at_utc: String,
    started_elapsed_s: f64,
    completed_elapsed_s: f64,
}

impl HostTiming {
    fn start(begin: Instant) -> (String, f64) {
        // Capture adjacent host clock observations before formatting either.
        let utc = time::OffsetDateTime::now_utc();
        let elapsed = begin.elapsed().as_secs_f64();
        (format_timestamp(utc), elapsed)
    }

    fn finish(begin: Instant, started_at_utc: String, started_elapsed_s: f64) -> Self {
        let (completed_at_utc, completed_elapsed_s) = Self::start(begin);
        Self {
            started_at_utc,
            completed_at_utc,
            started_elapsed_s,
            completed_elapsed_s,
        }
    }

    fn logical_now(begin: Instant) -> Self {
        let (utc, elapsed) = Self::start(begin);
        Self {
            started_at_utc: utc.clone(),
            completed_at_utc: utc,
            started_elapsed_s: elapsed,
            completed_elapsed_s: elapsed,
        }
    }

    fn json(&self) -> serde_json::Value {
        serde_json::json!({
            "host_started_at_utc":self.started_at_utc,
            "host_completed_at_utc":self.completed_at_utc,
            "host_started_elapsed_s":self.started_elapsed_s,
            "host_completed_elapsed_s":self.completed_elapsed_s,
            "device_timestamp_utc":serde_json::Value::Null,
        })
    }
}

#[derive(Clone, Debug)]
struct RecordedObservation {
    reply: String,
    timing: HostTiming,
}

// A compliance reading must be preserved, but persisting it must never delay
// the emergency output-off request. Keep the exact helper exchanges in memory
// until protection has completed, then write the provenance record.
struct DeferredMeterEvidence {
    reply: Option<String>,
    query_error: Option<String>,
    timing: HostTiming,
    exchanges: Vec<TransportExchange>,
}

fn query_without_audit(
    pair: &mut DevicePair,
    role: Role,
    command: &str,
    begin: Instant,
) -> (Result<String, String>, Vec<TransportExchange>, HostTiming) {
    let (started_at_utc, started_elapsed_s) = HostTiming::start(begin);
    let recorded = pair.query_recorded(role, command);
    let timing = HostTiming::finish(begin, started_at_utc, started_elapsed_s);
    (recorded.result, recorded.exchanges, timing)
}

fn append_meter_reading_audit(
    audit: &mut RawWriter,
    point_index: usize,
    reading_index: Option<u32>,
    reply: Option<&str>,
    query_error: Option<&str>,
    exchanges: &[TransportExchange],
    timing: &HostTiming,
) -> Result<(), String> {
    append_transport_exchanges(
        audit,
        "meter",
        "READ?",
        point_index,
        reading_index,
        exchanges,
        timing,
    )?;
    audit.append(&serde_json::json!({
        "event":"meter_reading", "point_index":point_index,
        "reading_index":reading_index, "command":"READ?", "normalized_response":reply,
        "query_error":query_error, "error":query_error,
        "parsed_value":reply.and_then(|reply| reply.trim().parse::<f64>().ok()),
        "timing":timing.json(),
    }))
}

// This intentionally attempts audit persistence even when the protection stop
// fails. The caller gets every failure, while the next instrument operation
// after the triggering READ remains source output-off.
fn protect_and_persist_meter_evidence(
    pair: Option<&mut DevicePair>,
    state: &mut Snapshot,
    audit: &mut RawWriter,
    point_index: usize,
    reading_index: Option<u32>,
    evidence: DeferredMeterEvidence,
) -> Result<(), String> {
    let protection_error = protection_output_off(pair, state).err();
    let evidence_error = append_meter_reading_audit(
        audit,
        point_index,
        reading_index,
        evidence.reply.as_deref(),
        evidence.query_error.as_deref(),
        &evidence.exchanges,
        &evidence.timing,
    )
    .err();
    let mut errors = Vec::new();
    if let Some(error) = protection_error {
        errors.push(format!("Protection stop failed: {error}"));
    }
    if let Some(error) = evidence_error {
        errors.push(format!("Could not persist meter evidence: {error}"));
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join(" · "))
    }
}

fn read_level(
    pair: &mut DevicePair,
    audit: &mut RawWriter,
    point_index: usize,
    begin: Instant,
) -> Result<Option<f64>, String> {
    for command in [":SOUR:LEV?", ":SOUR:LEV:FIX?", ":SOUR:LEV:AUTO?"] {
        // A transport timeout poisons the session and requires output-off
        // recovery. Only a received, unparsable reply can try another dialect.
        let observation =
            query_with_audit(pair, Role::Source, command, audit, point_index, None, begin)?;
        let parsed = observation
            .reply
            .trim()
            .parse::<f64>()
            .ok()
            .filter(|value| value.is_finite());
        audit.append(&serde_json::json!({
            "event":"source_readback", "point_index":point_index,
            "command":command, "normalized_response":observation.reply, "parsed_value":parsed,
            "timing":observation.timing.json(),
        }))?;
        if let Some(value) = parsed {
            return Ok(Some(value));
        }
    }
    Err("Source level could not be read back as a finite number. Measurement stopped; check the source and reconnect.".into())
}

fn query_with_audit(
    pair: &mut DevicePair,
    role: Role,
    command: &str,
    audit: &mut RawWriter,
    point_index: usize,
    reading_index: Option<u32>,
    begin: Instant,
) -> Result<RecordedObservation, String> {
    let (started_at_utc, started_elapsed_s) = HostTiming::start(begin);
    let recorded = pair.query_recorded(role, command);
    let timing = HostTiming::finish(begin, started_at_utc, started_elapsed_s);
    let role_name = role_label(role);
    let audit_result = append_transport_exchanges(
        audit,
        role_name,
        command,
        point_index,
        reading_index,
        &recorded.exchanges,
        &timing,
    );
    match (recorded.result, audit_result) {
        (Ok(reply), Ok(())) => Ok(RecordedObservation { reply, timing }),
        (Err(query_error), Ok(())) => Err(query_error),
        (Ok(_), Err(audit_error)) => Err(audit_error),
        (Err(query_error), Err(audit_error)) => Err(format!(
            "{query_error} · additionally could not persist transport provenance: {audit_error}"
        )),
    }
}

fn append_transport_exchanges(
    audit: &mut RawWriter,
    role: &str,
    logical_command: &str,
    point_index: usize,
    reading_index: Option<u32>,
    exchanges: &[TransportExchange],
    timing: &HostTiming,
) -> Result<(), String> {
    for (exchange_index, exchange) in exchanges.iter().enumerate() {
        audit.append(&serde_json::json!({
            "event":"transport_exchange", "point_index":point_index,
            "reading_index":reading_index, "role":role,
            "logical_command":logical_command, "exchange_index":exchange_index,
            "operation":exchange.operation, "wire_command":exchange.command,
            "wire_response":exchange.response, "wire_error":exchange.error,
            "timing":timing.json(),
        }))?;
    }
    Ok(())
}

#[derive(Default)]
struct ResistanceFit {
    count: usize,
    mean_i: f64,
    mean_v: f64,
    covariance: f64,
    variance: f64,
}

/// Streaming sample dispersion for the individual meter values retained in
/// `*.raw.csv`. This does not change the existing incremental mean used by
/// acquisition or its compliance policy.
#[derive(Default)]
struct SampleStats {
    count: u32,
    mean: f64,
    m2: f64,
}

impl SampleStats {
    fn add(&mut self, value: f64) {
        self.count += 1;
        let delta = value - self.mean;
        self.mean += delta / f64::from(self.count);
        self.m2 += delta * (value - self.mean);
    }

    fn sample_stddev(&self) -> Option<f64> {
        (self.count >= 2)
            .then(|| (self.m2 / f64::from(self.count - 1)).sqrt())
            .filter(|value| value.is_finite())
    }
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
    audit: &mut RawWriter,
    begin: Instant,
) -> Result<(), String> {
    for (role, expected) in [
        (Role::Source, pair.source_identity.clone()),
        (Role::Meter, pair.meter_identity.clone()),
    ] {
        if query_with_audit(pair, role, "*IDN?", audit, 0, None, begin)?
            .reply
            .trim()
            != expected.trim()
        {
            return Err(
                "Instrument identity changed. Disconnect and select the correct resource.".into(),
            );
        }
    }
    write_with_audit(pair, Role::Source, ":OUTP OFF", audit, begin)?;
    write_with_audit(pair, Role::Source, "*CLS", audit, begin)?;
    write_with_audit(pair, Role::Meter, "*CLS", audit, begin)?;
    let definition = iv_core::measurement_definition(config.mode);
    let (function, protect, measure) = (
        definition.source_function,
        definition.protection_function,
        definition.meter_function,
    );
    write_with_audit(
        pair,
        Role::Source,
        &format!(":SOUR:FUNC {function}"),
        audit,
        begin,
    )?;
    write_with_audit(
        pair,
        Role::Source,
        &format!(":SOUR:RANG {range:.12e}"),
        audit,
        begin,
    )?;
    write_with_audit(
        pair,
        Role::Source,
        &format!(":SOUR:PROT:{protect} {compliance:.12e}"),
        audit,
        begin,
    )?;
    write_with_audit(pair, Role::Source, ":SOUR:LEV:FIX 0", audit, begin)?;
    write_with_audit(
        pair,
        Role::Meter,
        &format!("CONF:{measure}:DC"),
        audit,
        begin,
    )?;
    for command in meter::commands(config)? {
        write_with_audit(pair, Role::Meter, &command, audit, begin)?;
    }
    write_with_audit(
        pair,
        Role::Meter,
        &format!("SENS:{measure}:DC:NPLC {:.12e}", config.nplc),
        audit,
        begin,
    )?;
    write_with_audit(
        pair,
        Role::Meter,
        &format!("SENS:{measure}:DC:ZERO:AUTO ON"),
        audit,
        begin,
    )?;
    write_with_audit(pair, Role::Meter, "TRIG:SOUR IMM", audit, begin)?;
    for role in [Role::Source, Role::Meter] {
        let response = query_with_audit(pair, role, "SYST:ERR?", audit, 0, None, begin)?.reply;
        if !response.trim_start().starts_with("0,") && !response.trim_start().starts_with("+0,") {
            return Err(format!("Instrument configuration failed: {response}"));
        }
    }
    meter::verify(config, |command| {
        Ok(query_with_audit(pair, Role::Meter, command, audit, 0, None, begin)?.reply)
    })?;
    Ok(())
}

fn write_with_audit(
    pair: &mut DevicePair,
    role: Role,
    command: &str,
    audit: &mut RawWriter,
    begin: Instant,
) -> Result<(), String> {
    let (started_at_utc, started_elapsed_s) = HostTiming::start(begin);
    let recorded = pair.write_recorded(role, command);
    let timing = HostTiming::finish(begin, started_at_utc, started_elapsed_s);
    let role_name = role_label(role);
    let audit_result = append_transport_exchanges(
        audit,
        role_name,
        command,
        0,
        None,
        &recorded.exchanges,
        &timing,
    );
    match (recorded.result, audit_result) {
        (Ok(_), Ok(())) => Ok(()),
        (Err(error), Ok(())) | (Ok(_), Err(error)) => Err(error),
        (Err(error), Err(audit_error)) => Err(format!(
            "{error} · additionally could not persist transport provenance: {audit_error}"
        )),
    }
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
    ranges: [Option<f64>; 2],
) -> Result<(), String> {
    if state.source2_connected && mode != Mode::Iv {
        return Err("Balanced dual-source requires voltage-source mode. Disconnect Source 2 before using current-source mode.".into());
    }
    if !state.error.is_empty() {
        return Err(state.error.to_string());
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
        state.output = OutputState::Off;
        state.source_dc = None;
        state.source_dc_limits = None;
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
            let config = RunConfig {
                mode,
                meter_model: device.meter_model.clone(),
                meter_voltage_range: ranges[0],
                meter_current_range: ranges[1],
                ..Default::default()
            };
            let range_commands = meter::commands(&config)?;
            device.write(Role::Meter, &format!("CONF:{measure}:DC"))?;
            for command in range_commands {
                device.write(Role::Meter, &command)?;
            }
            device.write(Role::Meter, &format!("SENS:{measure}:DC:NPLC 1"))?;
            device.write(Role::Meter, "TRIG:SOUR IMM")?;
            meter::verify(&config, |command| device.query(Role::Meter, command))?;
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
    // A mode edit never preserves or recreates a manual DC output.
    state.source_dc = None;
    state.source_dc_limits = None;
    state.output = OutputState::Off;
    state.live_error.clear();
    state.phase = Phase::Ready;
    state.message = format!("Live mode {} · output off verified", mode.as_str());
    Ok(())
}

fn sweep_outputs_off(state: &Snapshot) -> bool {
    (!state.source_connected || state.output == OutputState::Off)
        && (!state.source2_connected || state.source2_output == OutputState::Off)
}

/// Establish the loading baseline while outputs are OFF, before even the
/// first manual DC ON. Held DC edits never reconfigure or re-prime the meter.
#[allow(clippy::too_many_arguments)]
fn prepare_live_meter(
    pair: &mut DevicePair,
    state: &Snapshot,
    mode: Mode,
    ranges: [Option<f64>; 2],
    configured: &mut Option<Mode>,
    guard: &mut Option<meter::RangeGuard>,
    reference: &mut Option<reference::Observer>,
    control: &Control,
) -> Result<(), String> {
    if *configured == Some(mode) && guard.is_some() {
        return Ok(());
    }
    let output = monitor_query(pair, Role::Source, ":OUTP?", control)?;
    if safety::output_state(&output)? != OutputState::Off || state.source2_output == OutputState::On
    {
        return Err(
            "Turn all source outputs OFF before initializing Meter range monitoring.".into(),
        );
    }
    let config = RunConfig {
        mode,
        meter_model: pair.meter_model.clone(),
        meter_voltage_range: ranges[0],
        meter_current_range: ranges[1],
        ..Default::default()
    };
    let measure = iv_core::measurement_definition(mode).meter_function;
    let commands = meter::commands(&config)?;
    pair.write(Role::Meter, &format!("CONF:{measure}:DC"))?;
    for command in commands {
        pair.write(Role::Meter, &command)?;
    }
    pair.write(Role::Meter, &format!("SENS:{measure}:DC:NPLC 1"))?;
    pair.write(Role::Meter, "TRIG:SOUR IMM")?;
    meter::verify(&config, |cmd| {
        monitor_query(pair, Role::Meter, cmd, control)
    })?;
    let mut ready = meter::RangeGuard::new(&config);
    if meter::monitored(&config) {
        let range = positive_reading(&monitor_query(
            pair,
            Role::Meter,
            &format!("SENS:{measure}:DC:RANG?"),
            control,
        )?);
        ready.check(&config, range)?;
    }
    *guard = Some(ready);
    *reference = reference::prepare_live(pair, &config, control)?;
    *configured = Some(mode);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn monitor_reading(
    pair: &mut DevicePair,
    configured: &mut Option<Mode>,
    elapsed: f64,
    control: &Control,
    state: &mut Snapshot,
    ranges: [Option<f64>; 2],
    guard: &mut Option<meter::RangeGuard>,
    reference: &mut Option<reference::Observer>,
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
    let config = RunConfig {
        mode,
        meter_model: pair.meter_model.clone(),
        meter_voltage_range: ranges[0],
        meter_current_range: ranges[1],
        ..Default::default()
    };
    prepare_live_meter(
        pair, state, mode, ranges, configured, guard, reference, control,
    )?;
    if control.closed.load(Ordering::Acquire) || control.off.load(Ordering::Acquire) {
        return Err("Live acquisition interrupted by output-off/shutdown".into());
    }
    // Confirm OFF in this cycle before allowing unguarded idle autoranging.
    let raw_output = monitor_query(pair, Role::Source, ":OUTP?", control)?;
    state.output = match raw_output.trim() {
        "0" | "+0" | "OFF" => OutputState::Off,
        "1" | "+1" | "ON" => OutputState::On,
        _ => return Err("Unknown source output state".into()),
    };
    let measured = monitor_query(pair, Role::Meter, "READ?", control)?
        .trim()
        .parse::<f64>()
        .map_err(|_| "Invalid live meter reading")?;
    if !measured.is_finite() || measured.abs() > 1e30 {
        return Err("Live meter overload".into());
    }
    let (meter_range, meter_resolution) = if let Some((range_query, resolution_query)) =
        iv_core::meter_display_queries(&pair.meter_model, mode)?
    {
        let range = positive_reading(&monitor_query(pair, Role::Meter, range_query, control)?);
        // Idle OFF readings may autorange freely. Once any source is ON,
        // retain the baseline and enforce the same loading policy as a sweep.
        if state.output == OutputState::Off && state.source2_output != OutputState::On {
            guard
                .as_mut()
                .ok_or("Missing live meter range guard")?
                .reset_baseline();
        }
        let checked = guard
            .as_mut()
            .ok_or("Missing live meter range guard")?
            .check(&config, range);
        if let Err(error) = checked {
            let off = protection_output_off(Some(pair), state).err();
            return Err(off.map_or(error.clone(), |e| format!("{error} · {e}")));
        }
        (
            range,
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
    meter::check(&config, meter_range)?;
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
    record.quality_flags = "live".into();
    if let Some(observer) = reference.as_mut() {
        observer.read_live(pair, state, control, &mut record)?;
        let status = (|| -> Result<(), String> {
            for secondary in [false, true] {
                source_dc::check(Some(pair), state, secondary)?;
            }
            gate::check(Some(pair), state)
        })();
        if let Err(error) = status {
            let off = protection_output_off(Some(pair), state).err();
            return Err(off.map_or(error.clone(), |e| format!("{error} · {e}")));
        }
    }
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

fn write_json_new(path: &Path, value: &impl serde::Serialize) -> std::io::Result<()> {
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    {
        let mut buffered = std::io::BufWriter::with_capacity(65536, &mut file);
        serde_json::to_writer_pretty(&mut buffered, value).map_err(std::io::Error::other)?;
        buffered.flush()?;
    }
    file.sync_all()
}

fn replace_json(path: &Path, value: &impl serde::Serialize) -> std::io::Result<()> {
    let temporary = path.with_extension("json.tmp");
    write_json_new(&temporary, value)?;
    std::fs::rename(temporary, path)
}

/// The aggregate OFF state is only verified after every connected sweep source.
/// The Gate is intentionally excluded here: ordinary IV completion preserves bias.
fn shutdown_gate(pair: Option<&mut DevicePair>, state: &mut Snapshot) -> Result<(), String> {
    if !state.gate_connected {
        return Ok(());
    }
    gate::output_off(pair, state)
}

/// A completed or user-stopped sweep deliberately preserves an external gate
/// bias.  Recording integrity failures are different: the run no longer has
/// trustworthy evidence, so an enabled gate must fail closed as well.
fn shutdown_gate_for_recording_failure(pair: Option<&mut DevicePair>, state: &mut Snapshot) {
    if !state.gate_connected || state.gate_output != OutputState::On {
        return;
    }
    if let Err(error) = shutdown_gate(pair, state) {
        let previous = if state.error.is_empty() {
            String::new()
        } else {
            format!("{} · ", state.error)
        };
        state.error = Failure::new(
            ErrorKind::OutputSafety,
            format!("{previous}Gate shutdown after recording failure failed: {error}"),
        );
        state.phase = Phase::Failed;
    }
}

fn shutdown_sweep_sources(
    pair: Option<&mut DevicePair>,
    state: &mut Snapshot,
) -> Result<(), String> {
    state.output = OutputState::Unknown;
    if state.source2_connected {
        state.source2_output = OutputState::Unknown;
    }
    let mut verified = [!state.source_connected, !state.source2_connected];
    if state.source_simulation && state.source_connected {
        verified[0] = true;
    }
    if state.source2_simulation && state.source2_connected {
        verified[1] = true;
        state.source2_output = OutputState::Off;
    }
    let mut errors = Vec::new();
    if let Some(pair) = pair {
        for (role, result) in pair.force_sources_output_off() {
            let index = if role == Role::Source { 0 } else { 1 };
            match result {
                Ok(report) if report.output_off_verified => {
                    verified[index] = true;
                    if index == 1 {
                        state.source2_output = OutputState::Off;
                    }
                    if let Some(warning) = report.zeroing_warning {
                        state.warnings.push(format!(
                            "{} output OFF verified; {warning}",
                            role_label(role)
                        ));
                    }
                }
                Ok(_) => errors.push(format!("{} output OFF not verified", role_label(role))),
                Err(error) => errors.push(format!("{}: {error}", role_label(role))),
            }
        }
    }
    if verified.into_iter().all(|v| v) {
        state.output = OutputState::Off;
        state.source2_level = Some(0.0);
        state.source_dc = None;
        state.source2_dc = None;
        state.source_dc_limits = None;
        state.source2_dc_limits = None;
    } else if errors.is_empty() {
        errors.push("Not all sweep sources could be verified OFF. Check both instruments.".into());
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join(" · "))
    }
}

fn clean_output(
    pair: Option<&mut DevicePair>,
    state: &mut Snapshot,
    ramp: Option<(&RunConfig, f64)>,
    control: &Control,
) {
    state.phase = Phase::CleaningUp;
    state.output = OutputState::Unknown;
    if state.source2_connected {
        if let Err(error) = shutdown_sweep_sources(pair, state) {
            state.error = error.into();
        }
        return;
    }
    if state.source_simulation && state.source_connected {
        state.output = OutputState::Off;
        state.source_dc = None;
        state.source_dc_limits = None;
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
        state.activity = Activity::ReturningToZero;
        if let Err(error) = workflow::ramp(
            &mut level,
            0.0,
            config.ramp_step,
            RampPolicy::ReturnToZero,
            || budget.elapsed() >= Duration::from_secs(10) || control.off.load(Ordering::Acquire),
            |level| {
                set_level(pair, level)?;
                if config.ramp_wait_s > 0.0 {
                    std::thread::sleep(Duration::from_secs_f64(config.ramp_wait_s.min(0.1)));
                }
                Ok(true)
            },
        ) {
            errors.push(error);
        }
    }
    // Emergency/error cleanup never queues a level write ahead of Output Off.
    // The transport replaces poisoned helpers before trying this operation.
    state.activity = Activity::VerifyingOutputOff;
    match pair.force_output_off() {
        Ok(report) if report.output_off_verified => {
            state.output = OutputState::Off;
            state.source_dc = None;
            state.source_dc_limits = None;
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
            if state.output == OutputState::Off {
                "Output off was verified."
            } else {
                "Check the instrument output manually."
            }
        )
        .into();
    }
}

// Protection is deliberately separate from normal Stop: no ramp or unrelated
// instrument queries may precede the source OFF request after a limit trip.
fn protection_output_off(
    pair: Option<&mut DevicePair>,
    state: &mut Snapshot,
) -> Result<(), String> {
    state.activity = Activity::VerifyingOutputOff;
    state.output = OutputState::Unknown;
    if state.source2_connected {
        return shutdown_sweep_sources(pair, state);
    }
    if state.source_simulation {
        state.output = OutputState::Off;
        return Ok(());
    }
    let report = pair
        .ok_or("Source transport unavailable during protection stop")?
        .force_output_off()?;
    if !report.output_off_verified {
        return Err("Protection stop could not verify source output OFF".into());
    }
    state.output = OutputState::Off;
    if let Some(warning) = report.zeroing_warning {
        state.warnings.push(format!(
            "Protection stop: output OFF verified; zeroing warning: {warning}"
        ));
    }
    Ok(())
}

#[derive(Clone, Debug, serde::Serialize)]
struct SourceProtectionStop {
    point_index: usize,
    source_target: f64,
    observed_at_utc: String,
    condition: safety::SourceCondition,
    failure: Option<String>,
}

/// Capture status in memory, request OFF first on a limit/unknown status,
/// then persist both the source status and any already acquired meter reply.
/// No normal analysis point is emitted for a hardware-limited target.
fn check_source_protection(
    pair: &mut DevicePair,
    state: &mut Snapshot,
    audit: &mut RawWriter,
    begin: Instant,
    point_index: usize,
    source_target: f64,
    pending_meter: Option<(Option<u32>, DeferredMeterEvidence)>,
) -> Result<Option<SourceProtectionStop>, String> {
    let profile = iv_core::source_protection_profile(&pair.source_model)?;
    let Some(profile) = profile else {
        if let Some((reading_index, evidence)) = pending_meter {
            append_meter_reading_audit(
                audit,
                point_index,
                reading_index,
                evidence.reply.as_deref(),
                evidence.query_error.as_deref(),
                &evidence.exchanges,
                &evidence.timing,
            )?;
        }
        return Ok(None);
    };
    let (response, exchanges, timing) =
        query_without_audit(pair, Role::Source, profile.query, begin);
    let parsed = response
        .as_ref()
        .map_err(Clone::clone)
        .and_then(|reply| safety::source_condition(reply, profile));
    let unsafe_status = parsed
        .as_ref()
        .map_or(true, |condition| condition.stopped());
    let off_error = if unsafe_status {
        protection_output_off(Some(pair), state).err()
    } else {
        None
    };
    let mut evidence_errors = Vec::new();
    if let Some((reading_index, evidence)) = pending_meter
        && let Err(error) = append_meter_reading_audit(
            audit,
            point_index,
            reading_index,
            evidence.reply.as_deref(),
            evidence.query_error.as_deref(),
            &evidence.exchanges,
            &evidence.timing,
        )
    {
        evidence_errors.push(error);
    }
    if let Err(error) = append_transport_exchanges(
        audit,
        "source",
        profile.query,
        point_index,
        None,
        &exchanges,
        &timing,
    ) {
        evidence_errors.push(error);
    }
    let mut stop = parsed
        .as_ref()
        .ok()
        .filter(|condition| condition.stopped())
        .map(|condition| SourceProtectionStop {
            point_index,
            source_target,
            observed_at_utc: timing.completed_at_utc.clone(),
            condition: condition.clone(),
            failure: None,
        });
    if let Err(error) = audit.append(&serde_json::json!({
        "event":if stop.is_some() { "source_protection_stop" } else { "source_condition" },
        "point_index":point_index, "device_role":"source", "command":profile.query,
        "raw_response":response.as_ref().ok(), "error":parsed.as_ref().err(),
        "condition":parsed.as_ref().ok(), "source_target":source_target,
        "timing":timing.json(), "output_off_verified":if unsafe_status { Some(state.output == OutputState::Off) } else { None },
        "analysis_point_excluded":stop.is_some(),
    })) { evidence_errors.push(error); }
    let mut errors = Vec::new();
    if let Err(error) = parsed.as_ref() {
        errors.push(format!("Source hardware status check failed: {error}"));
    }
    if parsed.as_ref().is_ok_and(|condition| condition.emergency) {
        errors.push("Source reports an emergency condition (EMR / abnormal temperature). Check the instrument before restarting.".into());
    }
    if let Some(error) = off_error {
        errors.push(format!(
            "Source protection stop could not verify output OFF: {error}"
        ));
    }
    if !evidence_errors.is_empty() {
        errors.push(format!(
            "Cannot persist protection evidence: {}",
            evidence_errors.join(" · ")
        ));
    }
    if !errors.is_empty() {
        let failure = errors.join(" · ");
        if let Some(stop) = &mut stop {
            stop.failure = Some(failure);
        } else {
            return Err(failure);
        }
    }
    Ok(stop)
}

// One owner serializes device I/O. Live samples use the already configured
// sweep NPLC and never enter the CSV or resistance fit.
// Resolution is fixed by range + the NPLC/function configured for this run.
// Still inspect range after EVERY conversion, including each averaged sample.
// Reconfiguration creates a new cache; never carry idle-monitor precision into a run.
#[derive(Default)]
struct MeterResolutionCache {
    last: Option<(f64, f64)>,
    last_response: Option<String>,
    last_query_at: Option<String>,
}
impl MeterResolutionCache {
    fn read(
        &mut self,
        range: Option<f64>,
        query: impl FnOnce() -> Result<String, String>,
    ) -> Result<Option<f64>, String> {
        if let (Some(range), Some((previous, resolution))) = (range, self.last)
            && range == previous
        {
            return Ok(Some(resolution));
        }
        // Invalidate first: failed/unknown metadata must not retain old precision.
        self.last = None;
        self.last_response = None;
        self.last_query_at = None;
        let reply = query()?;
        let resolution = positive_reading(&reply);
        self.last_response = Some(reply);
        self.last_query_at = Some(timestamp());
        self.last = range.zip(resolution);
        Ok(resolution)
    }
}

struct SweepMonitor<'a> {
    config: &'a RunConfig,
    control: &'a Control,
    latest: &'a StateFeed,
    begin: Instant,
    last: Instant,
    source_resolution: Option<f64>,
    meter_resolution: MeterResolutionCache,
    meter_guard: meter::RangeGuard,
    reference: Option<reference::Observer>,
    compliance_stop: Option<Record>,
    compliance_evidence: Option<DeferredMeterEvidence>,
    source_stop: Option<SourceProtectionStop>,
    last_source_check: Instant,
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

    fn acquire(
        &mut self,
        mut pair: Option<&mut DevicePair>,
        level: f64,
        state: &mut Snapshot,
        audit: &mut RawWriter,
        point_index: usize,
    ) -> Result<Record, String> {
        let mut record = self.record(level);
        if let Some(device) = pair.as_deref_mut() {
            reference::clear_reading(&mut record);
            record.quality_flags = "live".into();
            // The gate is allowed to remain biased during an IV sweep, but it
            // must be polled on the same cadence as the sweep source.  A gate
            // limiter/status failure fails closed before any further meter I/O.
            gate::check(Some(device), state)?;
            if self.control.closed.load(Ordering::Acquire)
                || self.control.off.load(Ordering::Acquire)
                || self
                    .control
                    .heartbeat
                    .lock()
                    .map_or(true, |h| h.elapsed() > Duration::from_secs(2))
            {
                return Err("Live acquisition interrupted".into());
            }
            let (result, exchanges, timing) =
                query_without_audit(device, Role::Meter, "READ?", self.begin);
            let reply = match result {
                Ok(reply) => reply,
                Err(error) => {
                    self.compliance_evidence = Some(DeferredMeterEvidence {
                        reply: None,
                        query_error: Some(error.clone()),
                        timing,
                        exchanges,
                    });
                    return Err(format!("Live meter READ failed: {error}"));
                }
            };
            record.meter_value = match reply.trim().parse::<f64>() {
                Ok(value) => value,
                Err(_) => {
                    self.compliance_evidence = Some(DeferredMeterEvidence {
                        reply: Some(reply),
                        query_error: Some("Invalid live meter reading".into()),
                        timing,
                        exchanges,
                    });
                    return Err("Invalid live meter reading".into());
                }
            };
            if !record.meter_value.is_finite() || record.meter_value.abs() > 1e30 {
                self.compliance_evidence = Some(DeferredMeterEvidence {
                    reply: Some(reply),
                    query_error: Some("Meter overload during live settling acquisition".into()),
                    timing,
                    exchanges,
                });
                return Err("Meter overload during live settling acquisition".into());
            }
            // Check every conversion before doing any further instrument I/O.
            if record.meter_value.abs() >= self.config.compliance {
                state.protection_alert = Some(ProtectionAlert::measurement(
                    self.config,
                    record.meter_value,
                ));
                record.compliance = true;
                record.status = "compliance".into();
                record.source_readback = None;
                record.source_resolution = self.source_resolution;
                record.meter_resolution = None;
                self.compliance_evidence = Some(DeferredMeterEvidence {
                    reply: Some(reply),
                    query_error: None,
                    timing,
                    exchanges,
                });
                return Ok(record);
            }
            let fixed = meter::monitored(self.config);
            let mut evidence = Some(DeferredMeterEvidence {
                reply: Some(reply),
                query_error: None,
                timing,
                exchanges,
            });
            let source_check = check_source_protection(
                device,
                state,
                audit,
                self.begin,
                point_index,
                level,
                if fixed {
                    None
                } else {
                    Some((None, evidence.take().unwrap()))
                },
            );
            if !source_check.as_ref().is_ok_and(|stop| stop.is_none())
                && let Some(reading) = evidence.take()
            {
                append_meter_reading_audit(
                    audit,
                    point_index,
                    None,
                    reading.reply.as_deref(),
                    reading.query_error.as_deref(),
                    &reading.exchanges,
                    &reading.timing,
                )?;
            }
            self.source_stop = source_check?;
            self.last_source_check = Instant::now();
            if let Some(stop) = &self.source_stop {
                record.status = "source_limit".into();
                record.quality_flags = "source_limit_hit".into();
                record.instrument_status =
                    format!("Source hardware limiter {}", stop.condition.indicator());
                record.source_readback = None;
                return Ok(record);
            }
            let fixed_range = if let Some(reading) = evidence.as_ref() {
                Some(self.meter_guard.conversion(
                    device,
                    self.config,
                    state,
                    audit,
                    (point_index, None),
                    (self.begin, reading),
                )?)
            } else {
                None
            };
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
                record.meter_range = if meter::monitored(self.config) {
                    positive_reading(
                        &fixed_range
                            .as_ref()
                            .ok_or("Missing fixed meter observation")?
                            .reply,
                    )
                } else {
                    positive_reading(&monitor_query(device, Role::Meter, range, self.control)?)
                };
                record.meter_resolution = self.meter_resolution.read(record.meter_range, || {
                    monitor_query(device, Role::Meter, resolution, self.control)
                })?;
            } else {
                record.meter_range = None;
                record.meter_resolution = None;
            }
            record.source_resource = model_label(Role::Source, &device.source_model);
            record.meter_resource = model_label(Role::Meter, &device.meter_model);
        }
        if record.meter_value.abs() < self.config.compliance
            && let Some(device) = pair
            && let Some(observer) = self.reference.as_mut()
        {
            record.reference_meter_value = None;
            if !observer.read(device, state, audit, self.control, self.begin, &mut record)? {
                return Err("Source-quantity acquisition cancelled".into());
            }
            self.source_stop = check_source_protection(
                device,
                state,
                audit,
                self.begin,
                point_index,
                level,
                None,
            )?;
            if self.source_stop.is_some() {
                return Err(
                    "Source protection reached during source-quantity Meter conversion".into(),
                );
            }
            gate::check(Some(device), state)?;
        }
        record.compliance = record.meter_value.abs() >= self.config.compliance;
        if record.compliance {
            state.protection_alert = Some(ProtectionAlert::measurement(
                self.config,
                record.meter_value,
            ));
            record.status = "compliance".into();
        }
        Ok(record)
    }

    fn wait(
        &mut self,
        seconds: f64,
        mut pair: Option<&mut DevicePair>,
        level: f64,
        state: &mut Snapshot,
        mut audit: Option<&mut RawWriter>,
        point_index: usize,
    ) -> Result<bool, String> {
        if self.control.cancelled() {
            return Ok(false);
        }
        // Every new source level is checked, including zero-duration ramp waits.
        if let Some(device) = pair.as_deref_mut() {
            gate::check(Some(device), state)?;
            let writer = audit
                .as_deref_mut()
                .ok_or("Source protection monitoring requires a recording audit")?;
            self.source_stop = check_source_protection(
                device,
                state,
                writer,
                self.begin,
                point_index,
                level,
                None,
            )?;
            self.last_source_check = Instant::now();
            if self.source_stop.is_some() {
                return Ok(false);
            }
        }
        if seconds <= 0.0 {
            return Ok(!self.control.cancelled());
        }
        let end = Instant::now() + Duration::from_secs_f64(seconds);
        // A due live protection conversion must not be skipped because
        // source-status I/O or scheduling consumed a positive settle interval.
        // Zero-duration waits keep their fast path above.
        loop {
            if self.control.cancelled() {
                return Ok(false);
            }
            if self.last_source_check.elapsed() >= Duration::from_millis(50)
                && let (Some(device), Some(audit)) = (pair.as_deref_mut(), audit.as_deref_mut())
            {
                gate::check(Some(device), state)?;
                self.source_stop = check_source_protection(
                    device,
                    state,
                    audit,
                    self.begin,
                    point_index,
                    level,
                    None,
                )?;
                self.last_source_check = Instant::now();
                if self.source_stop.is_some() {
                    return Ok(false);
                }
            }
            if self.last.elapsed() >= Duration::from_millis(50) {
                self.last = Instant::now();
                let Some(writer) = audit.as_deref_mut() else {
                    return Err("Active live acquisition requires a recording audit".into());
                };
                let mut reading = match self.acquire(
                    pair.as_deref_mut(),
                    level,
                    state,
                    writer,
                    point_index,
                ) {
                    Ok(reading) => reading,
                    Err(read_error) => {
                        if let Some(evidence) = self.compliance_evidence.take() {
                            let stop_result = if let Some(audit) = audit.as_deref_mut() {
                                protect_and_persist_meter_evidence(
                                    pair.as_deref_mut(),
                                    state,
                                    audit,
                                    point_index,
                                    None,
                                    evidence,
                                )
                            } else {
                                let protection_error =
                                    protection_output_off(pair.as_deref_mut(), state).err();
                                Err(match protection_error {
                                    Some(error) => format!(
                                        "Protection stop failed: {error} · Cannot persist live meter evidence without an audit writer"
                                    ),
                                    None => {
                                        "Cannot persist live meter evidence without an audit writer"
                                            .into()
                                    }
                                })
                            };
                            return Err(match stop_result {
                                Ok(()) => read_error,
                                Err(stop_error) => format!("{read_error} · {stop_error}"),
                            });
                        }
                        return Err(read_error);
                    }
                };
                if self.source_stop.is_some() {
                    self.publish(reading, state);
                    return Ok(false);
                }
                if reading.compliance {
                    reading.timestamp = timestamp();
                    self.compliance_stop = Some(reading.clone());
                    if let Some(evidence) = self.compliance_evidence.take() {
                        let audit = audit.as_deref_mut().ok_or_else(|| {
                            let protection_error =
                                protection_output_off(pair.as_deref_mut(), state).err();
                            match protection_error {
                                Some(error) => format!(
                                    "Protection stop failed: {error} · Cannot persist live compliance provenance without an audit writer"
                                ),
                                None => "Cannot persist live compliance provenance without an audit writer".into(),
                            }
                        })?;
                        protect_and_persist_meter_evidence(
                            pair.as_deref_mut(),
                            state,
                            audit,
                            point_index,
                            None,
                            evidence,
                        )?;
                        audit.append(&serde_json::json!({
                            "event":"live_compliance_stop", "point_index":point_index,
                            "trigger_value":reading.meter_value,
                            "software_limit":self.config.compliance,
                        }))?;
                    }
                    self.publish(reading, state);
                    return Ok(false);
                }
                if self.control.cancelled() {
                    return Ok(false);
                }
                self.publish(reading, state);
            }
            // Do not loop forever when each conversion exceeds the interval.
            if Instant::now() >= end {
                return Ok(!self.control.cancelled());
            }
            std::thread::sleep(
                Duration::from_millis(10).min(end.saturating_duration_since(Instant::now())),
            );
        }
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
    // A new attempt must not attribute an early validation/preflight failure
    // to a previous run's protection event.
    state.protection_alert = None;
    // A gate belongs to the same safety domain as the IV sweep.  A simulated
    // gate alongside real sweep instruments (or the reverse) cannot be
    // monitored or shut down as one experiment, so reject it before any I/O.
    if state.gate_connected && state.gate_simulation != state.simulation {
        return Err(
            "Gate and IV sweep must both use real instruments or both use simulation. Disconnect and reconnect the gate with the same mode before Start."
                .into(),
        );
    }
    reference::validate_connections(config, state, pair.as_deref())?;
    if config.balanced_source.is_some() {
        return balanced::execute(config, path, pair, control, state, latest, events);
    }
    state.balanced_active = false;
    if state.source2_connected {
        return Err(
            "Source 2 is connected. Use a balanced differential sweep or disconnect Source 2 before starting a single-source run."
                .into(),
        );
    }
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
    } else if state.source_dc.is_some() {
        // Simulation follows the same ownership transition without creating
        // a synthetic ON/OFF transport exchange.
        state.output = OutputState::Off;
        state.source_dc = None;
        state.source_dc_limits = None;
    }
    state.activity = Activity::Preflight;
    let requested_config = config;
    let effective_config = iv_core::prepare_config(config)?;
    let config = &effective_config;
    let planned = plan(config)?;
    let mut preflight_output_verification = None;
    // Cached UI state is not proof that a real source is safe to reconfigure.
    // Enforce this here so GUI, CLI and notebook runs share the same preflight.
    if !state.source_simulation {
        state.output = OutputState::Unknown;
        let device = pair
            .as_deref_mut()
            .ok_or("Cannot start: reconnect the source and meter before measuring.")?;
        let report = device.force_output_off().map_err(|error| {
            format!("Cannot start: source output OFF verification failed. {error}")
        })?;
        if !report.output_off_verified {
            return Err("Cannot start: source output OFF was not verified. Turn output off on the instrument and reconnect.".into());
        }
        preflight_output_verification = Some(serde_json::json!({
            "verified": report.output_off_verified,
            "zeroing_warning": report.zeroing_warning,
            "result": "ok",
        }));
        state.output = OutputState::Off;
        // The source has now been physically verified OFF, so this manual
        // setting cannot survive into the sweep's reconfiguration sequence.
        state.source_dc = None;
        state.source_dc_limits = None;
        if let Some(warning) = report.zeroing_warning {
            return Err(format!(
                "Cannot start: output OFF was verified, but resetting the source level to zero failed. Reconnect before measuring. {warning}"
            ));
        }
    }
    state.phase = Phase::Preparing;
    state.message = "Creating the result file before enabling output…".into();
    state.error.clear();
    state.points = 0;
    state.total = planned.points.len();
    state.last = None;
    state.resistance = None;
    state.reference_resistance = None;
    state.run_id += 1;
    state.path = path.display().to_string();
    publish(latest, state);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("Create result folder: {e}"))?;
    }
    let run_id = raw_csv::new_run_id();
    let mut writer = CsvWriter::create(path)?;
    let manifest = path.with_extension("run.json");
    let raw_path = path.with_extension("raw.csv");
    let raw_filename = raw_path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("raw.csv");
    let instruments = if let Some(device) = pair.as_deref() {
        serde_json::json!({
            "source":{"model":device.source_model,"identity":device.source_identity,"resource":device.resource(Role::Source)},
            "meter":{"model":device.meter_model,"identity":device.meter_identity,"resource":device.resource(Role::Meter)},
            "reference_meter":{"model":device.reference_meter_model,"identity":device.reference_meter_identity,"resource":device.resource(Role::ReferenceMeter)},
        })
    } else {
        serde_json::json!({
            "source":{"model":"SIMULATED_SOURCE","identity":"synthetic","resource":null},
            "meter":{"model":"SIMULATED_METER","identity":"synthetic","resource":null},
        })
    };
    let provenance_policy = serde_json::json!({
        "schema":"raw_csv_v1",
        "coverage":"audited acquisition and configuration operations; emergency output-off/recovery is represented by its final verification result, not a complete wire trace",
        "numeric_precision":"main CSV uses shortest f64 round-trip scientific notation; replies from audited operations are retained verbatim in raw.csv",
        "resolution":"recorded only when an existing display query supplies it; otherwise null/unknown",
        "uncertainty":"unknown; neither raw text nor resolution is treated as accuracy or uncertainty",
        "simulation":"synthetic records have no fabricated hardware replies",
        "timing":"host UTC timestamps use RFC3339 microsecond precision and monotonic elapsed seconds relative to run start; they are host observation times, not instrument/device-clock timestamps",
    });
    let audit_start = serde_json::json!({
        "event":"run_started", "schema":"raw_csv_v1", "csv":path.display().to_string(),
        "run_metadata":manifest.display().to_string(), "config":config,
        "requested_config":requested_config, "instruments":instruments,
        "provenance_policy":provenance_policy,
    });
    let mut audit = match RawWriter::create(&raw_path, &run_id) {
        Ok(audit) => audit,
        Err(error) => {
            let _ = std::fs::remove_file(path);
            return Err(format!(
                "Cannot create raw acquisition evidence before enabling output: {error}"
            ));
        }
    };
    if let Err(error) = audit.append(&audit_start) {
        drop(audit);
        let _ = std::fs::remove_file(path);
        let _ = std::fs::remove_file(&raw_path);
        return Err(format!(
            "Cannot initialize raw acquisition evidence before enabling output: {error}"
        ));
    }
    if let Some(verification) = preflight_output_verification {
        audit.append(&serde_json::json!({
            "event":"output_off_verification", "device_role":"source",
            "quantity":"output_off", "result":"ok", "details":verification,
        }))?;
    }
    let mut initial_metadata = serde_json::json!({"schema_version":RUN_METADATA_SCHEMA_VERSION,"csv_schema":CSV_SCHEMA,"csv_fields":iv_core::CSV_FIELDS,"csv_numeric_format":"shortest_roundtrip_scientific","application_version":env!("CARGO_PKG_VERSION"),"experiment":config.experiment,"config":config,"requested_config":requested_config,"outcome":recording_snapshot(state),"in_progress":true,
        "run_id":run_id,"source_range_setting":planned.source_range,"hardware_compliance_setting":planned.hardware_compliance,"display_settings_changes":[],"simulation":state.simulation,"csv":path.display().to_string(),"raw_csv":{"path":raw_path.display().to_string(),"filename":raw_filename,"schema":"raw_csv_v1","provenance_policy":provenance_policy}});
    if let Some(commit) = option_env!("KOHDALAB_GIT_COMMIT") {
        initial_metadata["git_commit"] = serde_json::Value::String(commit.into());
    }
    if let Err(error) = write_json_new(&manifest, &initial_metadata) {
        let _ = std::fs::remove_file(path);
        let _ = std::fs::remove_file(&raw_path);
        return Err(format!(
            "Cannot save settings before enabling output: {error}"
        ));
    }
    let begin = Instant::now();
    let mut level = 0.0;
    let mut stopped = false;
    let mut compliance_hit = false;
    let mut fit = ResistanceFit::default();
    let mut reference_fit = ResistanceFit::default();
    let mut last_progress = Instant::now();
    let mut actual_source_resolution = None;
    let mut live = SweepMonitor {
        config,
        control,
        latest,
        begin,
        last: Instant::now(),
        source_resolution: None,
        meter_resolution: MeterResolutionCache::default(),
        meter_guard: meter::RangeGuard::new(config),
        reference: None,
        compliance_stop: None,
        compliance_evidence: None,
        source_stop: None,
        last_source_check: Instant::now(),
    };
    let mut display_settings_changes = Vec::new();
    let mut instrument_status_events = Vec::new();
    let mut previous_display_settings = None;
    let work = (|| -> Result<(), String> {
        state.activity = Activity::Configuring;
        if let Some(device) = pair.as_deref_mut() {
            configure(
                device,
                config,
                planned.source_range,
                planned.hardware_compliance,
                &mut audit,
                begin,
            )?;
            if meter::monitored(config) && iv_core::selected_meter_range(config).is_none() {
                live.meter_guard
                    .observed(device, config, state, &mut audit, (0, None), begin)?;
            }
            audit.append(&serde_json::json!({
                "event":"configuration_applied", "point_index":0,
                "result":"unverified", "details":{
                    "requested_config":config, "source_range_setting":planned.source_range,
                    "hardware_compliance_setting":planned.hardware_compliance,
                    "identity_checked":true,
                    "note":"commands were issued using the established configure sequence; settings not independently read back unless a later query records one"
                }
            }))?;
            // The transport translates this canonical query for models such as
            // the 7651 that do not use GS210 SCPI directly.
            let range_query =
                iv_core::source_range_query(&device.source_model)?.unwrap_or(":SOUR:RANG?");
            let range = query_with_audit(
                device,
                Role::Source,
                range_query,
                &mut audit,
                0,
                None,
                begin,
            )?;
            audit.append(&serde_json::json!({
                "event":"source_range", "command":range_query, "normalized_response":range.reply,
                "parsed_range":positive_reading(&range.reply), "timing":range.timing.json(),
            }))?;
            let resolution = safety::verify_range(
                &device.source_model,
                config.mode,
                planned.source_range,
                &range.reply,
            )?;
            let function = query_with_audit(
                device,
                Role::Source,
                ":SOUR:FUNC?",
                &mut audit,
                0,
                None,
                begin,
            )?;
            audit.append(&serde_json::json!({
                "event":"source_function", "command":":SOUR:FUNC?",
                "normalized_response":function.reply, "timing":function.timing.json(),
            }))?;
            safety::verify_function(&function.reply, config.mode)?;
            if iv_core::source_protection_profile(&device.source_model)?.is_some() {
                let query = if config.mode == Mode::Iv {
                    ":SOUR:PROT:CURR?"
                } else {
                    ":SOUR:PROT:VOLT?"
                };
                let protection =
                    query_with_audit(device, Role::Source, query, &mut audit, 0, None, begin)?;
                safety::verify_hardware_limit(planned.hardware_compliance, &protection.reply)?;
                audit.append(&serde_json::json!({"event":"source_protection_setting_verified",
                    "hardware_limit":planned.hardware_compliance,"source_protection_requested":config.source_compliance,
                    "meter_stop_limit":config.compliance,"command":query,"raw_response":protection.reply,"timing":protection.timing.json()}))?;
            }
            let zero = read_level(device, &mut audit, 0, begin)?
                .ok_or("Source zero level is unavailable")?;
            safety::verify_zero(zero, resolution)?;
            actual_source_resolution = Some(resolution);
            live.source_resolution = actual_source_resolution;
            if control.cancelled() {
                stopped = true;
                return Ok(());
            }
            // Once ON is requested, the previously verified OFF state is no
            // longer authoritative while a potentially slow query is pending.
            state.output = OutputState::Unknown;
            publish(latest, state);
            live.reference = reference::prepare(device, config, &mut audit, begin)?;
            write_with_audit(device, Role::Source, ":OUTP ON", &mut audit, begin)?;
            let output =
                query_with_audit(device, Role::Source, ":OUTP?", &mut audit, 0, None, begin)?;
            audit.append(&serde_json::json!({
                "event":"source_output_on_confirmation", "command":":OUTP?",
                "normalized_response":output.reply, "timing":output.timing.json(),
            }))?;
            if safety::output_state(&output.reply)? != OutputState::On {
                return Err("Source did not confirm output ON. Measurement stopped and output OFF will be verified.".into());
            }
        }
        state.output = OutputState::On;
        state.phase = Phase::Running;
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
                state.activity = Activity::Ramping;
                if !workflow::ramp(
                    &mut level,
                    point.target,
                    config.ramp_step,
                    RampPolicy::ApproachTarget,
                    || control.cancelled(),
                    |level| {
                        set_level(device, level)?;
                        live.wait(
                            config.ramp_wait_s,
                            Some(device),
                            level,
                            state,
                            Some(&mut audit),
                            point.point_index,
                        )
                    },
                )? {
                    stopped = true;
                }
                if stopped {
                    break;
                }
                set_level(device, point.target)?;
            }
            level = point.target;
            state.activity = Activity::Settling;
            let settling = if state.points == 0 {
                config.start_settle_s
            } else {
                config.settle_s
            };
            if !live.wait(
                settling,
                pair.as_deref_mut(),
                level,
                state,
                Some(&mut audit),
                point.point_index,
            )? {
                stopped = true;
                break;
            }
            state.activity = Activity::Acquiring;
            let readings_completed_for_record;
            let mut record = if let Some(device) = pair.as_deref_mut() {
                state.message.clear();
                state.message = format!(
                    "Reading meter ({} readings per point, NPLC {})…",
                    config.average_count, config.nplc
                );
                publish_progress(latest, state, &mut last_progress);
                let mut value = 0.0;
                let mut readings_completed = 0_u32;
                let mut sample_limit_hit = false;
                let mut trigger_value = 0.0;
                let mut meter_resolution: Option<f64> = None;
                let mut meter_range: Option<f64> = None;
                let mut meter_stats = SampleStats::default();
                let mut resolution_known = true;
                let mut first_conversion_timing: Option<HostTiming> = None;
                let mut last_conversion_timing: Option<HostTiming> = None;
                let display_queries =
                    iv_core::meter_display_queries(&device.meter_model, config.mode)?;
                for reading_index in 1..=config.average_count {
                    if control.cancelled() {
                        stopped = true;
                        break;
                    }
                    gate::check(Some(device), state)?;
                    live.source_stop = check_source_protection(
                        device,
                        state,
                        &mut audit,
                        begin,
                        point.point_index,
                        level,
                        None,
                    )?;
                    live.last_source_check = Instant::now();
                    if live.source_stop.is_some() {
                        stopped = true;
                        break;
                    }
                    live.last = Instant::now();
                    // Retain the physical exchange in memory first. A limit
                    // trip must request source OFF before audit/CSV I/O, but
                    // the triggering measurement still belongs in raw evidence.
                    let (result, exchanges, timing) =
                        query_without_audit(device, Role::Meter, "READ?", begin);
                    let text = match result {
                        Ok(text) => text,
                        Err(error) => {
                            let stop_result = protect_and_persist_meter_evidence(
                                Some(device),
                                state,
                                &mut audit,
                                point.point_index,
                                Some(reading_index),
                                DeferredMeterEvidence {
                                    reply: None,
                                    query_error: Some(error.clone()),
                                    timing,
                                    exchanges,
                                },
                            );
                            return Err(match stop_result {
                                Ok(()) => format!("Meter READ failed: {error}"),
                                Err(stop_error) => {
                                    format!("Meter READ failed: {error} · {stop_error}")
                                }
                            });
                        }
                    };
                    let parsed = text.trim().parse::<f64>();
                    let measured = match parsed {
                        Ok(measured) => measured,
                        Err(_) => {
                            let stop_result = protect_and_persist_meter_evidence(
                                Some(device),
                                state,
                                &mut audit,
                                point.point_index,
                                Some(reading_index),
                                DeferredMeterEvidence {
                                    reply: Some(text.clone()),
                                    query_error: Some("Invalid meter reading".into()),
                                    timing,
                                    exchanges,
                                },
                            );
                            return Err(match stop_result {
                                Ok(()) => format!("Invalid meter reading: {text}"),
                                Err(stop_error) => {
                                    format!("Invalid meter reading: {text} · {stop_error}")
                                }
                            });
                        }
                    };
                    if !measured.is_finite() || measured.abs() > 1e30 {
                        let stop_result = protect_and_persist_meter_evidence(
                            Some(device),
                            state,
                            &mut audit,
                            point.point_index,
                            Some(reading_index),
                            DeferredMeterEvidence {
                                reply: Some(text.clone()),
                                query_error: Some(
                                    "Meter reported overload or an invalid reading.".into(),
                                ),
                                timing,
                                exchanges,
                            },
                        );
                        return Err(match stop_result {
                            Ok(()) => "Meter reported overload or an invalid reading.".into(),
                            Err(stop_error) => format!(
                                "Meter reported overload or an invalid reading. · {stop_error}"
                            ),
                        });
                    }
                    if first_conversion_timing.is_none() {
                        first_conversion_timing = Some(timing.clone());
                    }
                    last_conversion_timing = Some(timing.clone());
                    value += measured / config.average_count as f64;
                    readings_completed += 1;
                    meter_stats.add(measured);
                    // Do not allow averaging to hide an individual over-limit sample.
                    if measured.abs() >= config.compliance {
                        state.protection_alert =
                            Some(ProtectionAlert::measurement(config, measured));
                        sample_limit_hit = true;
                        compliance_hit = true;
                        trigger_value = measured;
                        resolution_known = false;
                        let stop_result = protect_and_persist_meter_evidence(
                            Some(device),
                            state,
                            &mut audit,
                            point.point_index,
                            Some(reading_index),
                            DeferredMeterEvidence {
                                reply: Some(text),
                                query_error: None,
                                timing,
                                exchanges,
                            },
                        );
                        if let Err(error) = stop_result {
                            return Err(format!(
                                "Software compliance limit reached ({measured:.12e}) · {error}"
                            ));
                        }
                        break;
                    }
                    gate::check(Some(device), state)?;
                    let fixed = meter::monitored(config);
                    let mut evidence = Some(DeferredMeterEvidence {
                        reply: Some(text),
                        query_error: None,
                        exchanges,
                        timing,
                    });
                    let source_check = check_source_protection(
                        device,
                        state,
                        &mut audit,
                        begin,
                        point.point_index,
                        level,
                        if fixed {
                            None
                        } else {
                            Some((Some(reading_index), evidence.take().unwrap()))
                        },
                    );
                    if (!source_check.as_ref().is_ok_and(|stop| stop.is_none())
                        || control.cancelled())
                        && let Some(reading) = evidence.take()
                    {
                        append_meter_reading_audit(
                            &mut audit,
                            point.point_index,
                            Some(reading_index),
                            reading.reply.as_deref(),
                            reading.query_error.as_deref(),
                            &reading.exchanges,
                            &reading.timing,
                        )?;
                    }
                    live.source_stop = source_check?;
                    live.last_source_check = Instant::now();
                    if live.source_stop.is_some() {
                        stopped = true;
                        break;
                    }
                    if control.cancelled() {
                        stopped = true;
                        break;
                    }
                    let fixed_range = if let Some(reading) = evidence.as_ref() {
                        Some(live.meter_guard.conversion(
                            device,
                            config,
                            state,
                            &mut audit,
                            (point.point_index, Some(reading_index)),
                            (begin, reading),
                        )?)
                    } else {
                        None
                    };
                    // Query the range/resolution after READ? so autoranging is
                    // reflected. Preserve the coarsest resolution when averaging.
                    let (range, resolution) = if let Some((range_query, resolution_query)) =
                        display_queries
                    {
                        let range_text = if meter::monitored(config) {
                            fixed_range.ok_or("Missing fixed meter observation")?
                        } else {
                            query_with_audit(
                                device,
                                Role::Meter,
                                range_query,
                                &mut audit,
                                point.point_index,
                                Some(reading_index),
                                begin,
                            )?
                        };
                        if control.cancelled() {
                            stopped = true;
                            break;
                        }
                        let range = positive_reading(&range_text.reply);
                        let mut raw_resolution_response = None;
                        let resolution = live.meter_resolution.read(range, || {
                            let reply = query_with_audit(
                                device,
                                Role::Meter,
                                resolution_query,
                                &mut audit,
                                point.point_index,
                                Some(reading_index),
                                begin,
                            )?;
                            raw_resolution_response = Some(reply.reply.clone());
                            Ok(reply.reply)
                        })?;
                        audit.append(&serde_json::json!({
                            "event":"meter_display", "point_index":point.point_index,
                            "reading_index":reading_index, "range_command":range_query,
                            "normalized_range_response":range_text.reply, "range":range,
                                "timing":range_text.timing.json(),
                                "resolution_command":resolution_query, "resolution":resolution,
                                "resolution_origin":if raw_resolution_response.is_some() { "existing_query" } else { "range_cache" },
                                "normalized_resolution_response":live.meter_resolution.last_response,
                                "resolution_query_at":live.meter_resolution.last_query_at,
                        }))?;
                        (range, resolution)
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
                    reference::clear_reading(&mut reading);
                    reading.quality_flags = "live".into();
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
                readings_completed_for_record = readings_completed;
                let signal_timing =
                    last_conversion_timing.ok_or("No meter conversion completed.")?;
                // force_output_off may reset the programmed source level. Do
                // not issue a post-trip source query or save that zero as the
                // value read during the conversion.
                let readback = if sample_limit_hit {
                    None
                } else {
                    read_level(device, &mut audit, point.point_index, begin)?
                };
                let mut record = simulate_record(config, point, signal_timing.completed_elapsed_s);
                record.reference_meter_value = None;
                record.reference_meter_range = None;
                record.reference_meter_resolution = None;
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
                record.readings_requested = config.average_count;
                record.readings_completed = Some(readings_completed);
                record.meter_stddev = meter_stats.sample_stddev();
                record.quality_flags = if record.compliance {
                    "software_limit_hit".into()
                } else if readings_completed < config.average_count {
                    "partial_average".into()
                } else {
                    "normal".into()
                };
                audit.append(&serde_json::json!({
                    "event":"average_window", "point_index":point.point_index,
                    "readings_requested":config.average_count, "readings_completed":readings_completed,
                    "first_conversion":first_conversion_timing.map(|timing| timing.json()),
                    "last_conversion":signal_timing.json(),
                }))?;
                record.timestamp = signal_timing.completed_at_utc.clone();
                record
            } else {
                let signal_timing = HostTiming::logical_now(begin);
                let mut record = simulate_record(config, point, signal_timing.completed_elapsed_s);
                record.compliance = record.meter_value.abs() >= config.compliance;
                record.readings_requested = config.average_count;
                record.readings_completed = None;
                record.meter_stddev = None;
                record.quality_flags = "simulated".into();
                audit.append(&serde_json::json!({
                    "event":"synthetic_reading", "point_index":point.point_index,
                    "reading_index":1, "parsed_value":record.meter_value,
                    "raw_response":serde_json::Value::Null,
                    "hardware_reply":"not_applicable_synthetic_simulation",
                    "timing":signal_timing.json(), "time_basis":"synthetic_logical_host_time",
                }))?;
                readings_completed_for_record = 1;
                record.timestamp = signal_timing.completed_at_utc;
                record
            };
            if !record.compliance
                && let Some(observer) = live.reference.as_mut()
            {
                let device = pair
                    .as_deref_mut()
                    .ok_or("Source-quantity Meter transport missing")?;
                if !observer.read(device, state, &mut audit, control, begin, &mut record)? {
                    stopped = true;
                    break;
                }
                // The second DMM conversion extends the powered interval.
                // Recheck source protection before accepting this point.
                live.source_stop = check_source_protection(
                    device,
                    state,
                    &mut audit,
                    begin,
                    record.point.point_index,
                    level,
                    None,
                )?;
                if live.source_stop.is_some() {
                    stopped = true;
                    break;
                }
            }
            record.status = if record.compliance {
                "compliance"
            } else {
                "running"
            }
            .into();
            record.run_id = run_id.clone();
            compliance_hit = record.compliance;
            if compliance_hit {
                state.protection_alert =
                    Some(ProtectionAlert::measurement(config, record.meter_value));
            }
            if compliance_hit && state.output != OutputState::Off {
                protection_output_off(pair.as_deref_mut(), state)?;
            }
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
            state.activity = Activity::Saving;
            let (csv_started_at_utc, csv_started_elapsed_s) = HostTiming::start(begin);
            writer.append(&record)?;
            let csv_timing = HostTiming::finish(begin, csv_started_at_utc, csv_started_elapsed_s);
            // Account for the flushed CSV row, last value and fit even if
            // the companion audit fails.
            state.points += 1;
            let (source_voltage, source_current, _, _) = record.source_electrical_values();
            state.resistance = fit.add(source_voltage, source_current);
            if let Some((v, i, _, _)) = record.measured_electrical_values() {
                state.reference_resistance = reference_fit.add(v, i);
            }
            state.last = Some(record.clone());
            audit.append(&serde_json::json!({
                "event":"csv_write", "point_index":record.point.point_index,
                "csv_row_count":state.points, "timing":csv_timing.json(),
                "meaning":"host CSV write start and completion after row append/flush",
            }))?;
            audit.append(&serde_json::json!({
                "event":"saved_point", "point_index":record.point.point_index,
                "readings_requested":config.average_count,
                "readings_completed":readings_completed_for_record,
                "compliance":record.compliance, "meter_value":record.meter_value,
                "source_readback":record.source_readback, "source_resolution":record.source_resolution,
                "meter_range":record.meter_range, "meter_resolution":record.meter_resolution,
                "csv_values":record.native_csv_values(),
            }))?;
            if state.simulation {
                live.last = Instant::now();
                live.publish(record.clone(), state);
            }
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
    if let Some(record) = live.compliance_stop.take() {
        compliance_hit = true;
        instrument_status_events.push(serde_json::json!({
            "event":"live_compliance_stop", "timestamp":record.timestamp,
            "source_level":record.source_set, "trigger_value":record.meter_value,
            "trigger_unit":if config.mode == Mode::Iv { "A" } else { "V" },
            "software_limit":config.compliance, "saved_sweep_points":state.points
        }));
        // A live protection sample remains in the live display/run metadata;
        // it must not invent a saved sweep point or enter the resistance fit.
        state.live = Some(record);
    }
    let source_stop = live.source_stop.take();
    if let Some(stop) = &source_stop {
        state.protection_alert = Some(ProtectionAlert {
            kind: ProtectionKind::SourceCompliance,
            limit: planned.hardware_compliance,
            unit: if config.mode == Mode::Iv { "A" } else { "V" }.into(),
            observed: None,
            indicator: stop.condition.indicator().into(),
            source_index: Some(1),
        });
        instrument_status_events
            .push(serde_json::json!({"event":"source_protection_stop","details":stop}));
    }
    state.phase = Phase::CleaningUp;
    state.activity = if compliance_hit || source_stop.is_some() {
        Activity::VerifyingOutputOff
    } else {
        Activity::ReturningToZero
    };
    state.message = if compliance_hit || source_stop.is_some() {
        "Protection stop: verifying source output off…"
    } else {
        "Returning source to zero and verifying output off…"
    }
    .into();
    publish(latest, state);
    clean_output(
        pair.as_deref_mut(),
        state,
        if work.is_ok() && !compliance_hit && source_stop.is_none() {
            Some((config, level))
        } else {
            None
        },
        control,
    );
    // Ordinary successful IV completion preserves a verified external gate
    // bias.  Every sweep failure or protection stop instead shuts the gate
    // down as part of the same fail-closed transition.
    if (work.is_err() || compliance_hit || source_stop.is_some())
        && let Err(error) = shutdown_gate(pair.as_deref_mut(), state)
    {
        state.error = Failure::new(
            ErrorKind::OutputSafety,
            format!("Gate shutdown after IV safety stop failed: {error}"),
        );
    }
    let verification_audit = audit.append(&serde_json::json!({
        "event":"final_output_off_verification", "device_role":"source",
        "result": if state.output == OutputState::Off { "ok" } else { "error" },
        "verified": state.output == OutputState::Off,
    }));
    if let Err(error) = verification_audit {
        let previous = if state.error.is_empty() {
            String::new()
        } else {
            format!("{} · ", state.error)
        };
        let kind = if state.error.is_empty() {
            ErrorKind::Storage
        } else {
            state.error.kind
        };
        state.error = Failure::new(
            kind,
            format!("{previous}Cannot record final output verification: {error}"),
        );
        shutdown_gate_for_recording_failure(pair.as_deref_mut(), state);
    }
    state.elapsed = begin.elapsed().as_secs_f64();
    state.phase = completion_phase(
        work.is_err(),
        !state.error.is_empty(),
        compliance_hit,
        stopped,
    );
    if source_stop.is_some() && work.is_ok() && state.error.is_empty() {
        state.phase = Phase::SourceLimitStop;
    }
    if let Some(failure) = source_stop.as_ref().and_then(|stop| stop.failure.as_ref()) {
        state.error = Failure::from(failure.clone());
        state.phase = Phase::Failed;
    }
    state.activity = Activity::Idle;
    state.message = format!(
        "{} · {} points saved · output {}",
        state.phase,
        state.points,
        state.output.to_lowercase()
    );
    if let Some(stop) = &source_stop {
        let protection_unit = if config.mode == Mode::Vi { "V" } else { "A" };
        state.message = format!(
            "Source hardware limit reached ({}). Source protection: {:e} {}; Meter stop: {:e} {}. {} Analysis stopped before saving the limited target; actual source current/voltage is not verified.",
            stop.condition.indicator(),
            planned.hardware_compliance,
            protection_unit,
            config.compliance,
            protection_unit,
            state.message
        );
        state.warnings.push(state.message.clone());
    }
    if let Err(error) = work {
        state.error = if state.error.is_empty() {
            Failure::from(error)
        } else {
            Failure::new(
                ErrorKind::OutputSafety,
                format!("{error} · {}", state.error),
            )
        };
    }
    let audit_finish = audit.append(&serde_json::json!({
        "event":"run_finished", "outcome":state.phase.to_string(), "points_saved":state.points,
        "in_progress":false, "error":if state.error.is_empty() { None::<String> } else { Some(state.error.to_string()) },
    })).and_then(|()| audit.finish());
    if let Err(error) = audit_finish {
        let previous = if state.error.is_empty() {
            String::new()
        } else {
            format!("{} · ", state.error)
        };
        state.error = Failure::new(
            ErrorKind::OutputSafety,
            format!("{previous}{error} · acquisition evidence may be incomplete"),
        );
        state.phase = Phase::Failed;
        state.message = format!(
            "{} · {} points saved · output {}",
            state.phase,
            state.points,
            state.output.to_lowercase()
        );
        state.warnings.push("The raw acquisition evidence could not be finalized; retain the partial .raw.csv file with this run.".into());
        shutdown_gate_for_recording_failure(pair.as_deref_mut(), state);
    }
    let (hashes, hash_error) = match recording_hashes(path, &raw_path) {
        Ok(hashes) => (hashes, None),
        Err(error) => {
            let previous = if state.error.is_empty() {
                String::new()
            } else {
                format!("{} · ", state.error)
            };
            let kind = if state.error.is_empty() {
                ErrorKind::Storage
            } else {
                state.error.kind
            };
            state.error = Failure::new(
                kind,
                format!("{previous}Cannot finalize recording checksums: {error}"),
            );
            state.phase = Phase::Failed;
            state
                .warnings
                .push(format!("Recording checksums are unavailable: {error}"));
            state.message = format!(
                "{} · {} points saved · output {}",
                state.phase,
                state.points,
                state.output.to_lowercase()
            );
            (serde_json::Value::Null, Some(error))
        }
    };
    if hash_error.is_some() {
        shutdown_gate_for_recording_failure(pair.as_deref_mut(), state);
    }
    let mut metadata = serde_json::json!({"schema_version":RUN_METADATA_SCHEMA_VERSION,"csv_schema":CSV_SCHEMA,"csv_fields":iv_core::CSV_FIELDS,"csv_numeric_format":"shortest_roundtrip_scientific","application_version":env!("CARGO_PKG_VERSION"),"experiment":config.experiment,"run_id":run_id,"config":config,"requested_config":requested_config,"outcome":recording_snapshot(state),"in_progress":false,
        "source_range_setting":planned.source_range,"hardware_compliance_setting":planned.hardware_compliance,"source_protection_stop":source_stop,"display_settings_changes":display_settings_changes,"instrument_status_events":instrument_status_events,"simulation":state.simulation,"csv":path.display().to_string(),"instruments":instruments,"raw_csv":{"path":raw_path.display().to_string(),"filename":raw_filename,"schema":"raw_csv_v1","provenance_policy":provenance_policy},"file_hashes":hashes,"file_hash_error":hash_error});
    if let Some(commit) = option_env!("KOHDALAB_GIT_COMMIT") {
        metadata["git_commit"] = serde_json::Value::String(commit.into());
    }
    if let Err(error) = replace_json(&manifest, &metadata) {
        let previous = if state.error.is_empty() {
            String::new()
        } else {
            format!("{} · ", state.error)
        };
        state.error = Failure::new(
            ErrorKind::Storage,
            format!("{previous}Run metadata could not be saved: {error}"),
        );
        state.phase = Phase::Failed;
        state
            .warnings
            .push(format!("Run metadata could not be saved: {error}"));
        state.message = format!(
            "{} · {} points saved · output {}",
            state.phase,
            state.points,
            state.output.to_lowercase()
        );
        shutdown_gate_for_recording_failure(pair, state);
    }
    // The owner loop publishes the terminal state after releasing its output
    // reservation. Publishing it here would let a client submit its next Run
    // while reader admission still sees the prior operation as busy.
    Ok(())
}

fn recording_hashes(main: &Path, raw: &Path) -> Result<serde_json::Value, String> {
    Ok(
        serde_json::json!({"csv_sha256":raw_csv::file_sha256(main)?,"raw_csv_sha256":raw_csv::file_sha256(raw)?}),
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn checksum_failure_is_explicit_and_sample_dispersion_uses_ddof_one() {
        let dir = tempfile::tempdir().unwrap();
        let main = dir.path().join("main.csv");
        let raw = dir.path().join("raw.csv");
        std::fs::write(&main, b"main").unwrap();
        assert!(super::recording_hashes(&main, &raw).is_err());
        std::fs::write(&raw, b"raw").unwrap();
        assert!(super::recording_hashes(&main, &raw).is_ok());
        let mut stats = super::SampleStats::default();
        assert_eq!(stats.sample_stddev(), None);
        stats.add(1.);
        assert_eq!(stats.sample_stddev(), None);
        stats.add(2.);
        stats.add(3.);
        assert_eq!(stats.sample_stddev(), Some(1.));
    }
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
    fn raw_initialization_refuses_foreign_file() {
        let path = std::env::temp_dir().join(format!(
            "iv-raw-initial-{}-{}.csv",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::write(&path, b"foreign audit").unwrap();
        assert!(RawWriter::create(&path, "run-test").is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"foreign audit");
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
            gate_request_rejected: Mutex::new(None),
            source_dc_request_rejected: Mutex::new(None),
            run_request_rejected: Mutex::new(None),
            pending_safety: AtomicU8::new(0),
            running: AtomicBool::new(false),
            closed: AtomicBool::new(false),
            heartbeat: Mutex::new(Instant::now()),
        }
    }

    #[test]
    fn resolution_cache_reuses_only_the_current_range_and_recovers_unknowns() {
        let mut cache = MeterResolutionCache::default();
        let calls = std::cell::Cell::new(0);
        for _ in 0..10 {
            assert_eq!(
                cache
                    .read(Some(1.0), || {
                        calls.set(calls.get() + 1);
                        Ok("1e-6".into())
                    })
                    .unwrap(),
                Some(1e-6)
            );
        }
        assert_eq!(
            calls.get(),
            1,
            "Repeat 10 should not query identical resolution 10 times"
        );
        assert_eq!(
            cache.read(Some(10.0), || Ok("1e-5".into())).unwrap(),
            Some(1e-5)
        );
        assert_eq!(
            cache.read(Some(1.0), || Ok("2e-6".into())).unwrap(),
            Some(2e-6),
            "range changes must refresh, even on returning to an old range"
        );
        assert!(
            cache
                .read(Some(100.0), || Err("transport failure".into()))
                .is_err()
        );
        assert_eq!(
            cache.read(Some(1.0), || Ok("3e-6".into())).unwrap(),
            Some(3e-6)
        );
        assert_eq!(cache.read(None, || Ok("NaN".into())).unwrap(), None);
        assert_eq!(
            cache.read(Some(1.0), || Ok("4e-6".into())).unwrap(),
            Some(4e-6)
        );
        let mut new_run = MeterResolutionCache::default();
        assert_eq!(
            new_run.read(Some(1.0), || Ok("5e-6".into())).unwrap(),
            Some(5e-6)
        );
    }

    #[test]
    fn zero_wait_does_not_acquire_or_publish_an_extra_conversion() {
        let config = RunConfig::default();
        let control = live_control();
        let latest = StateFeed::default();
        let mut monitor = SweepMonitor {
            config: &config,
            control: &control,
            latest: &latest,
            begin: Instant::now(),
            last: Instant::now() - Duration::from_secs(1),
            source_resolution: None,
            meter_resolution: MeterResolutionCache::default(),
            meter_guard: meter::RangeGuard::new(&config),
            reference: None,
            compliance_stop: None,
            compliance_evidence: None,
            source_stop: None,
            last_source_check: Instant::now(),
        };
        let mut state = Snapshot::default();
        assert!(monitor.wait(0.0, None, 0.0, &mut state, None, 0).unwrap());
        assert!(state.live.is_none());
        assert!(latest.pending.lock().unwrap().snapshot.is_none());
        control.stop.store(true, Ordering::Release);
        assert!(!monitor.wait(0.0, None, 0.0, &mut state, None, 0).unwrap());
    }

    #[test]
    fn batched_events_keep_wire_format_and_final_state_order() {
        let config = RunConfig::default();
        let point = plan(&config).unwrap().points[0].clone();
        let record = simulate_record(&config, &point, 0.25);
        let state = Snapshot {
            phase: Phase::Completed,
            output: OutputState::Off,
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
    fn gui_job_completion_contains_a_final_non_recursive_snapshot() {
        let mut state = Snapshot {
            phase: Phase::Completed,
            points: 3,
            ..Default::default()
        };
        complete_gui_job(&mut state, "job-42".into());
        let completion = state.gui_job.as_ref().unwrap();
        assert_eq!(completion.id, "job-42");
        assert_eq!(completion.snapshot["phase"], "Completed");
        assert_eq!(completion.snapshot["points"], 3);
        assert!(completion.snapshot["gui_job"].is_null());

        // Idle/live state publications retain the last tagged completion until
        // another tagged command replaces it.
        let retained = state.gui_job.clone();
        state.message = "Live monitor updated.".into();
        assert_eq!(state.gui_job, retained);
        for index in 0..8 {
            complete_gui_job(&mut state, format!("job-{index}"));
        }
        assert_eq!(state.gui_jobs.len(), 4);
        assert_eq!(state.gui_jobs[0].id, "job-4");
        let persisted = recording_snapshot(&state);
        assert!(persisted.get("gui_job").is_none());
        assert!(persisted.get("gui_jobs").is_none());
        assert_eq!(persisted["points"], 3);
        assert!(
            state
                .gui_jobs
                .iter()
                .all(|job| job.snapshot["gui_jobs"].as_array().unwrap().is_empty())
        );
    }

    #[test]
    fn progress_is_coalesced_but_safety_state_is_immediate() {
        let shared = StateFeed::default();
        let mut state = Snapshot {
            phase: Phase::Running,
            output: OutputState::On,
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
        state.output = OutputState::Off;
        state.phase = Phase::Completed;
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
            gate_request_rejected: Mutex::new(None),
            source_dc_request_rejected: Mutex::new(None),
            run_request_rejected: Mutex::new(None),
            pending_safety: AtomicU8::new(0),
            running: AtomicBool::new(false),
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
