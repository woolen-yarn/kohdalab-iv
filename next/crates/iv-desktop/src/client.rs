use crate::protocol::{Command, Event, Snapshot};
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, Command as Process, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

const SAFETY_NONE: u8 = 0;
const SAFETY_STOP: u8 = 1;
const SAFETY_OUTPUT_OFF: u8 = 2;
const SAFETY_SHUTDOWN: u8 = 3;

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
                if !matches!(state.phase.as_str(), "Completed" | "Stopped" | "Idle") {
                    state.phase = "Failed".into();
                    state.error =
                        "The measurement engine exited. Reconnect before continuing.".into();
                    if !state.simulation && state.output != "Off" {
                        state.output = "Unknown".into();
                        state
                            .error
                            .push_str(" Check the instrument output manually.");
                    }
                }
                state.connected = false;
                state.source_connected = false;
                state.meter_connected = false;
                reader_revision.fetch_add(1, Ordering::Release);
            }
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

    pub fn snapshot(&self) -> Snapshot {
        self.state.lock().map(|s| s.clone()).unwrap_or_default()
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
    if let Some(level) = safety_level(&command) {
        if !writer_alive.load(Ordering::Acquire) {
            return Err("Measurement engine command pipe is disconnected.".into());
        }
        safety.fetch_max(level, Ordering::AcqRel);
        return Ok(());
    }
    if matches!(command, Command::Run { .. }) && safety.load(Ordering::Acquire) != SAFETY_NONE {
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

fn safety_level(command: &Command) -> Option<u8> {
    match command {
        Command::Stop => Some(SAFETY_STOP),
        Command::OutputOff => Some(SAFETY_OUTPUT_OFF),
        Command::Shutdown => Some(SAFETY_SHUTDOWN),
        _ => None,
    }
}

fn safety_command(level: u8) -> Command {
    match level {
        SAFETY_STOP => Command::Stop,
        SAFETY_OUTPUT_OFF => Command::OutputOff,
        SAFETY_SHUTDOWN => Command::Shutdown,
        _ => Command::Heartbeat,
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
            // A command already removed from the queue counts toward the 16
            // queued commands. Sending it before safety preserves FIFO: an
            // earlier Run cannot arrive after its Stop command.
            let mut drained = 0usize;
            if let Some(command) = pending_normal.take() {
                if write_command(&mut output, &command).is_err() {
                    break;
                }
                drained += 1;
            }
            while drained < 16 {
                match receive.try_recv() {
                    Ok(command) => {
                        if write_command(&mut output, &command).is_err() {
                            alive.store(false, Ordering::Release);
                            return output;
                        }
                        drained += 1;
                    }
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
            if write_command(&mut output, &safety_command(selected)).is_err() {
                break;
            }
            let _ =
                safety.compare_exchange(selected, SAFETY_NONE, Ordering::AcqRel, Ordering::Acquire);
            if selected == SAFETY_SHUTDOWN {
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
    }

    #[test]
    fn pending_run_is_written_before_stop() {
        let (sender, receiver) = mpsc::sync_channel(16);
        sender
            .send(Command::Run {
                config: iv_core::RunConfig::default(),
                path: "result.csv".into(),
            })
            .unwrap();
        let safety = Arc::new(AtomicU8::new(SAFETY_STOP));
        let alive = Arc::new(AtomicBool::new(true));
        let writer = TestWriter::default();
        let bytes = writer.0.clone();
        let thread_safety = safety.clone();
        let thread_alive = alive.clone();
        let handle =
            std::thread::spawn(move || writer_loop(writer, receiver, thread_safety, thread_alive));
        let deadline = Instant::now() + Duration::from_secs(1);
        while Instant::now() < deadline {
            if String::from_utf8_lossy(&bytes.lock().unwrap()).contains("\"stop\"") {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        submit_command(&sender, &safety, &alive, Command::Shutdown).unwrap();
        let _ = handle.join().unwrap();
        let wire = String::from_utf8(bytes.lock().unwrap().clone()).unwrap();
        let run = wire.find("\"run\"").unwrap();
        let stop = wire.find("\"stop\"").unwrap();
        let shutdown = wire.find("\"shutdown\"").unwrap();
        assert!(run < stop && stop < shutdown, "wire order was {wire:?}");
    }
}
