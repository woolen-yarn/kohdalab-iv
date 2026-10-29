use crate::protocol::{Command, Event, Snapshot, gui_job, validate_gui_job};
use crate::state::{OutputState, Phase};
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, Command as Process, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

const SAFETY_NONE: u8 = 0;
// Safety intents are a bitmask so individual Source DC OFF requests cannot
// overwrite a concurrent Gate OFF request in the client-side priority path.
const SAFETY_STOP: u8 = 1;
const SAFETY_GATE_OFF: u8 = 2;
const SAFETY_SOURCE_OFF: u8 = 4;
const SAFETY_SOURCE2_OFF: u8 = 8;
const SAFETY_OUTPUT_OFF: u8 = 16;
const SAFETY_SHUTDOWN: u8 = 32;

pub struct Client {
    child: Child,
    commands: mpsc::SyncSender<Command>,
    safety: Arc<AtomicU8>,
    writer_alive: Arc<AtomicBool>,
    pub state: Arc<Mutex<Snapshot>>,
    state_revision: Arc<AtomicU64>,
    points: mpsc::Receiver<(u64, iv_core::Record)>,
}

impl Client {
    pub fn start(wake: impl Fn() + Send + Sync + 'static) -> Result<Self, String> {
        let mut process = Process::new(std::env::current_exe().map_err(|e| e.to_string())?);
        process
            .arg("--engine")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            process.creation_flags(0x08000000);
        }
        let mut child = process
            .spawn()
            .map_err(|e| format!("Start measurement engine: {e}"))?;
        let input = child.stdin.take().ok_or("Missing engine input")?;
        let output = child.stdout.take().ok_or("Missing engine output")?;
        let (commands, receive) = mpsc::sync_channel::<Command>(16);
        let safety = Arc::new(AtomicU8::new(SAFETY_NONE));
        let writer_alive = Arc::new(AtomicBool::new(true));
        let writer_safety = safety.clone();
        let writer_alive_thread = writer_alive.clone();
        std::thread::spawn(move || {
            let _ = writer_loop(input, receive, writer_safety, writer_alive_thread);
        });
        let state = Arc::new(Mutex::new(Snapshot::default()));
        let reader_state = state.clone();
        let reader_alive = writer_alive.clone();
        let state_revision = Arc::new(AtomicU64::new(0));
        let reader_revision = state_revision.clone();
        let (send_point, points) = mpsc::sync_channel(2048);
        let wake = Arc::new(wake);
        std::thread::spawn(move || {
            let mut reader = BufReader::new(output);
            let mut line = Vec::with_capacity(4096);
            loop {
                line.clear();
                match reader.by_ref().take(262145).read_until(b'\n', &mut line) {
                    Ok(0) => break,
                    Ok(_) if line.len() <= 262144 && line.last() == Some(&b'\n') => {}
                    _ => break,
                }
                match serde_json::from_slice::<Event>(&line) {
                    Ok(Event::State(next)) => {
                        if let Ok(mut state) = reader_state.lock() {
                            *state = *next;
                            reader_revision.fetch_add(1, Ordering::Release);
                            wake();
                        }
                    }
                    Ok(Event::Point { run_id, record }) => {
                        let _ = send_point.try_send((run_id, *record));
                    }
                    Err(_) => break,
                }
            }
            if let Ok(mut state) = reader_state.lock() {
                if !matches!(state.phase, Phase::Completed | Phase::Stopped | Phase::Idle)
                    && !(state.phase == Phase::SourceLimitStop && state.output == OutputState::Off)
                {
                    state.phase = Phase::Failed;
                    state.error =
                        "The measurement engine exited. Reconnect before continuing.".into();
                    if !state.simulation && state.output != OutputState::Off {
                        state.output = OutputState::Unknown;
                        state
                            .error
                            .push_str(" Check the instrument output manually.");
                    }
                }
                state.connected = false;
                state.source_connected = false;
                state.source2_connected = false;
                state.gate_connected = false;
                state.meter_connected = false;
                reader_revision.fetch_add(1, Ordering::Release);
            }
            // A closed engine output stream means no further final job state
            // can arrive, even if the stdin writer thread has not observed
            // its broken pipe yet.
            reader_alive.store(false, Ordering::Release);
            wake();
        });
        Ok(Self {
            child,
            commands,
            safety,
            writer_alive,
            state,
            state_revision,
            points,
        })
    }

    pub fn send(&self, command: Command) -> Result<(), String> {
        submit_command(&self.commands, &self.safety, &self.writer_alive, command)
    }

    /// Submit a normal engine operation whose completed state can be matched
    /// to `id` through `Snapshot::gui_job`.
    pub fn send_job(&self, id: String, command: Command) -> Result<(), String> {
        self.send(gui_job(id, command)?)
    }

    pub fn snapshot(&self) -> Snapshot {
        self.state.lock().map(|s| s.clone()).unwrap_or_default()
    }

    /// False once either the command writer or engine-state reader has lost
    /// its pipe.  GUI API job tracking uses this to distinguish an engine
    /// exit from a completed tagged operation.
    pub fn is_alive(&self) -> bool {
        self.writer_alive.load(Ordering::Acquire)
    }

    /// Returns a cloned state only after the engine has published a newer one.
    /// `snapshot` remains available for the CLI's polling path.
    pub fn snapshot_since(&self, seen_revision: &mut u64) -> Option<Snapshot> {
        let revision = self.state_revision.load(Ordering::Acquire);
        if revision == *seen_revision {
            return None;
        }
        let snapshot = self.state.lock().ok()?.clone();
        *seen_revision = revision;
        Some(snapshot)
    }

    pub fn drain_points(&self) -> Vec<(u64, iv_core::Record)> {
        self.points.try_iter().collect()
    }
}

fn submit_command(
    commands: &mpsc::SyncSender<Command>,
    safety: &AtomicU8,
    writer_alive: &AtomicBool,
    command: Command,
) -> Result<(), String> {
    if let Command::GuiJob { id, command } = &command {
        validate_gui_job(id, command)?;
    }
    if let Some(level) = safety_level(&command) {
        if !writer_alive.load(Ordering::Acquire) {
            return Err("Measurement engine command pipe is disconnected.".into());
        }
        safety.fetch_or(level, Ordering::AcqRel);
        return Ok(());
    }
    if needs_clear_safety(&command) && safety.load(Ordering::Acquire) != SAFETY_NONE {
        return Err(
            "A safety command is pending. Wait for Stop or Output Off before starting a run."
                .into(),
        );
    }
    commands.try_send(command).map_err(|error| match error {
        mpsc::TrySendError::Full(_) => "Engine is busy. Try again shortly.".into(),
        mpsc::TrySendError::Disconnected(_) => {
            "Measurement engine command pipe is disconnected.".into()
        }
    })
}

fn needs_clear_safety(command: &Command) -> bool {
    let command = match command {
        Command::GuiJob { command, .. } => command.as_ref(),
        command => command,
    };
    matches!(
        command,
        Command::Run { .. } | Command::SetGateVoltage { .. } | Command::SetSourceDc { .. }
    )
}

fn safety_level(command: &Command) -> Option<u8> {
    match command {
        Command::Stop => Some(SAFETY_STOP),
        Command::GateOutputOff => Some(SAFETY_GATE_OFF),
        Command::SourceDcOff { secondary: false } => Some(SAFETY_SOURCE_OFF),
        Command::SourceDcOff { secondary: true } => Some(SAFETY_SOURCE2_OFF),
        Command::OutputOff => Some(SAFETY_OUTPUT_OFF),
        Command::Shutdown => Some(SAFETY_SHUTDOWN),
        _ => None,
    }
}

fn safety_command(flags: u8) -> (u8, Command) {
    // OutputOff and Shutdown are global fail-closed fallbacks.  Otherwise
    // dispatch each source-specific OFF before a plain Stop.
    if flags & SAFETY_SHUTDOWN != 0 {
        (SAFETY_SHUTDOWN, Command::Shutdown)
    } else if flags & SAFETY_OUTPUT_OFF != 0 {
        (SAFETY_OUTPUT_OFF, Command::OutputOff)
    } else if flags & SAFETY_GATE_OFF != 0 {
        (SAFETY_GATE_OFF, Command::GateOutputOff)
    } else if flags & SAFETY_SOURCE_OFF != 0 {
        (SAFETY_SOURCE_OFF, Command::SourceDcOff { secondary: false })
    } else if flags & SAFETY_SOURCE2_OFF != 0 {
        (SAFETY_SOURCE2_OFF, Command::SourceDcOff { secondary: true })
    } else {
        (SAFETY_STOP, Command::Stop)
    }
}

fn write_command<W: Write>(output: &mut W, command: &Command) -> Result<(), ()> {
    let mut line = serde_json::to_vec(command).map_err(|_| ())?;
    line.push(b'\n');
    output
        .write_all(&line)
        .and_then(|()| output.flush())
        .map_err(|_| ())
}

/// Runs the single writer thread. Safety is deliberately outside the bounded
/// normal-command queue, so Stop/Output Off stays accepted when the queue is full.
fn writer_loop<W: Write>(
    mut output: W,
    receive: mpsc::Receiver<Command>,
    safety: Arc<AtomicU8>,
    alive: Arc<AtomicBool>,
) -> W {
    let mut heartbeat = Instant::now();
    let mut pending_normal: Option<Command> = None;
    loop {
        let level = safety.load(Ordering::Acquire);
        if level != SAFETY_NONE {
            // Discard queued operations rather than allow Run/Gate ON to
            // execute after a safety request. Already-written commands see
            // the engine reader's immediate cancellation latch.
            pending_normal.take();
            for _ in 0..16 {
                match receive.try_recv() {
                    Ok(_) => {}
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        safety.fetch_max(SAFETY_SHUTDOWN, Ordering::AcqRel);
                        break;
                    }
                }
            }
            let selected = safety.load(Ordering::Acquire);
            if selected == SAFETY_NONE {
                continue;
            }
            let (bit, command) = safety_command(selected);
            if write_command(&mut output, &command).is_err() {
                break;
            }
            safety.fetch_and(!bit, Ordering::AcqRel);
            if bit == SAFETY_SHUTDOWN {
                break;
            }
            heartbeat = Instant::now();
            continue;
        }

        let remaining = Duration::from_millis(500).saturating_sub(heartbeat.elapsed());
        match receive.recv_timeout(remaining.min(Duration::from_millis(50))) {
            Ok(command) => {
                // Safety may have arrived after this normal command was removed.
                // Defer it to the bounded FIFO drain above in that case.
                if safety.load(Ordering::Acquire) != SAFETY_NONE {
                    pending_normal = Some(command);
                    continue;
                }
                if write_command(&mut output, &command).is_err() {
                    break;
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout)
                if heartbeat.elapsed() >= Duration::from_millis(500) =>
            {
                if write_command(&mut output, &Command::Heartbeat).is_err() {
                    break;
                }
                heartbeat = Instant::now();
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                safety.fetch_max(SAFETY_SHUTDOWN, Ordering::AcqRel);
            }
        }
    }
    alive.store(false, Ordering::Release);
    output
}

impl Drop for Client {
    fn drop(&mut self) {
        let _ = self.send(Command::Shutdown);
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(40) {
            if matches!(self.child.try_wait(), Ok(Some(_))) {
                return;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        // Never kill an engine while hardware cleanup may still be running.
        // Closing the input pipe / expiring its lease requests cleanup there.
        eprintln!(
            "Engine {} is still cleaning up. Hardware output is not certified off.",
            self.child.id()
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Default)]
    struct TestWriter(Arc<Mutex<Vec<u8>>>);

    impl Write for TestWriter {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn full_normal_queue_still_accepts_safety_and_rejects_run() {
        let (sender, _receiver) = mpsc::sync_channel(16);
        for _ in 0..16 {
            sender.try_send(Command::Discover).unwrap();
        }
        let safety = AtomicU8::new(SAFETY_NONE);
        let alive = AtomicBool::new(true);
        submit_command(&sender, &safety, &alive, Command::Stop).unwrap();
        assert_eq!(safety.load(Ordering::Acquire), SAFETY_STOP);
        assert!(
            submit_command(
                &sender,
                &safety,
                &alive,
                Command::Run {
                    config: iv_core::RunConfig::default(),
                    path: "result.csv".into(),
                },
            )
            .is_err()
        );
        assert!(
            submit_command(
                &sender,
                &safety,
                &alive,
                gui_job(
                    "run-1".into(),
                    Command::Run {
                        config: iv_core::RunConfig::default(),
                        path: "result.csv".into(),
                    },
                )
                .unwrap(),
            )
            .is_err(),
            "tagged runs must observe the same pending-safety admission rule"
        );
    }

    #[test]
    fn safety_discards_queued_run_and_gate_before_they_can_energize() {
        let (sender, receiver) = mpsc::sync_channel(16);
        sender
            .send(Command::Run {
                config: iv_core::RunConfig::default(),
                path: "result.csv".into(),
            })
            .unwrap();
        sender
            .send(Command::SetGateVoltage {
                voltage: 1.0,
                compliance: 0.001,
                ramp_step: 0.01,
                range: None,
            })
            .unwrap();
        let safety = Arc::new(AtomicU8::new(SAFETY_OUTPUT_OFF));
        let alive = Arc::new(AtomicBool::new(true));
        let writer = TestWriter::default();
        let bytes = writer.0.clone();
        let thread_safety = safety.clone();
        let thread_alive = alive.clone();
        let handle =
            std::thread::spawn(move || writer_loop(writer, receiver, thread_safety, thread_alive));
        let deadline = Instant::now() + Duration::from_secs(1);
        while Instant::now() < deadline {
            if String::from_utf8_lossy(&bytes.lock().unwrap()).contains("\"output_off\"") {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        submit_command(&sender, &safety, &alive, Command::Shutdown).unwrap();
        let _ = handle.join().unwrap();
        let wire = String::from_utf8(bytes.lock().unwrap().clone()).unwrap();
        assert!(!wire.contains("\"run\""), "{wire}");
        assert!(!wire.contains("\"set_gate_voltage\""), "{wire}");
        let stop = wire.find("\"output_off\"").unwrap();
        let shutdown = wire.find("\"shutdown\"").unwrap();
        assert!(stop < shutdown, "wire order was {wire:?}");
    }

    #[test]
    fn source_dc_off_is_a_priority_intent_and_does_not_replace_gate_off() {
        let (sender, receiver) = mpsc::sync_channel(16);
        let safety = Arc::new(AtomicU8::new(SAFETY_NONE));
        let alive = Arc::new(AtomicBool::new(true));
        submit_command(&sender, &safety, &alive, Command::GateOutputOff).unwrap();
        submit_command(
            &sender,
            &safety,
            &alive,
            Command::SourceDcOff { secondary: true },
        )
        .unwrap();
        let writer = TestWriter::default();
        let bytes = writer.0.clone();
        let handle = std::thread::spawn({
            let safety = safety.clone();
            let alive = alive.clone();
            move || writer_loop(writer, receiver, safety, alive)
        });
        let deadline = Instant::now() + Duration::from_secs(1);
        while Instant::now() < deadline {
            let captured = bytes.lock().unwrap().clone();
            let wire = String::from_utf8_lossy(&captured);
            if wire.contains("\"gate_output_off\"") && wire.contains("\"source_dc_off\"") {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        submit_command(&sender, &safety, &alive, Command::Shutdown).unwrap();
        let _ = handle.join().unwrap();
        let wire = String::from_utf8(bytes.lock().unwrap().clone()).unwrap();
        assert!(wire.contains("\"gate_output_off\""), "{wire}");
        assert!(wire.contains("\"source_dc_off\""), "{wire}");
        assert!(wire.contains("\"secondary\":true"), "{wire}");
    }
}
