//! IPC acceptance checks: continuous idle monitoring must not create sweep data.
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

struct Engine {
    child: Child,
    input: ChildStdin,
    events: mpsc::Receiver<Value>,
}
impl Engine {
    fn start() -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_iv-cli"))
            .arg("--engine")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let input = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        let (tx, events) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else {
                    break;
                };
                let Ok(event) = serde_json::from_str(&line) else {
                    break;
                };
                if tx.send(event).is_err() {
                    break;
                }
            }
        });
        Self {
            child,
            input,
            events,
        }
    }
    fn send(&mut self, command: Value) {
        writeln!(self.input, "{command}").unwrap();
        self.input.flush().unwrap();
    }
    fn wait(&mut self, predicate: impl Fn(&Value) -> bool) -> Value {
        let deadline = Instant::now() + Duration::from_secs(8);
        loop {
            assert!(Instant::now() < deadline, "Engine state timed out");
            self.send(json!({"command":"heartbeat"}));
            if let Ok(event) = self.events.recv_timeout(Duration::from_millis(100))
                && event["event"] == "state"
            {
                let state = event["data"].clone();
                assert!(state["error"].as_str().unwrap().is_empty(), "{state}");
                if predicate(&state) {
                    return state;
                }
            }
        }
    }
}
impl Drop for Engine {
    fn drop(&mut self) {
        let _ = writeln!(self.input, "{{\"command\":\"shutdown\"}}");
        let _ = self.input.flush();
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            if self.child.try_wait().unwrap().is_some() {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        // This test only creates simulated instruments; never kill a hardware engine.
        panic!("Simulated engine did not exit cleanly");
    }
}
#[test]
fn live_before_and_after_sweep_does_not_enable_output_or_pollute_csv() {
    let mut engine = Engine::start();
    engine.send(json!({"command":"connect","simulation":true,"source":"","meter":""}));
    let first = engine.wait(|s| !s["live"].is_null());
    assert_eq!(first["output"], "Off");
    assert_eq!(first["points"], 0);
    assert_eq!(first["path"], "");
    assert!(first["last"].is_null());
    let next = engine
        .wait(|s| s["live"]["timestamp"] != first["live"]["timestamp"] && !s["live"].is_null());
    assert_eq!(next["output"], "Off");
    engine.send(json!({"command":"output_off"}));
    engine.wait(|s| s["message"] == "Output off verified.");
    engine.wait(|s| s["live"]["timestamp"] != next["live"]["timestamp"]);
    let directory = std::env::temp_dir().join(format!(
        "iv-live-test-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&directory).unwrap();
    let path = directory.join("sweep.csv");
    let config = iv_core::RunConfig {
        pattern: iv_core::Pattern::Linear,
        step: 0.001,
        settle_s: 0.01,
        start_settle_s: 0.01,
        ..Default::default()
    };
    engine.send(json!({"command":"run","config":config,"path":path}));
    let finished = engine.wait(|s| s["phase"] == "Completed");
    assert_eq!(finished["points"], 3);
    assert_eq!(finished["output"], "Off");
    let saved = std::fs::read(&path).unwrap();
    let live = engine.wait(|s| s["live"]["timestamp"] != finished["live"]["timestamp"]);
    assert_eq!(live["points"], 3);
    assert_eq!(live["output"], "Off");
    assert_eq!(std::fs::read(&path).unwrap(), saved);
    let mut csv = csv_reader(&path);
    assert_eq!(csv.len(), 4);
    assert_eq!(csv.remove(0), iv_core::CSV_FIELDS.join(","));
    let metadata: Value =
        serde_json::from_slice(&std::fs::read(path.with_extension("run.json")).unwrap()).unwrap();
    assert_eq!(metadata["in_progress"], false);
    engine.send(json!({"command":"disconnect"}));
    engine.wait(|s| s["connected"] == false);
    std::fs::remove_dir_all(directory).unwrap();
}
fn csv_reader(path: &std::path::Path) -> Vec<String> {
    std::fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(str::to_owned)
        .collect()
}

#[cfg(unix)]
#[test]
fn native_mock_idle_monitor_uses_actual_source_mode_and_never_enables_output() {
    use std::os::unix::fs::PermissionsExt;
    let directory = std::env::temp_dir().join(format!(
        "iv-live-native-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&directory).unwrap();
    let log = directory.join("commands.log");
    let hex = |s: &str| {
        s.as_bytes()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    };
    let mut script = format!(
        "#!/bin/sh\nprintf 'KIV\\tOK\\t\\n'\nwhile read op address timeout payload; do\n echo \"$op $address $payload\" >> '{}'\n response=''\n if [ \"$op\" = QUERY ]; then\n case \"$address:$payload\" in\n",
        log.display()
    );
    script = script.replace(
        "#!/bin/sh\n",
        &format!(
            "#!/bin/sh\nlevel='{}'\nfunction='{}'\n",
            hex("0.0002"),
            hex("CURR")
        ),
    );
    script = script.replace(" response=''\n", &format!(" response=''\n if [ \"$op\" = WRITE ]; then\n case \"$address:$payload\" in\n '3:{}') level='{}';;\n '3:{}') function='{}';;\n '3:{}') function='{}';;\n esac\n fi\n", hex(":SOUR:LEV:FIX 0\n"), hex("0"), hex(":SOUR:FUNC VOLT\n"), hex("VOLT"), hex(":SOUR:FUNC CURR\n"), hex("CURR")));
    for (address, command, response) in [
        (3, "*IDN?", "YOKOGAWA,GS210,MOCK,2.0"),
        (7, "*IDN?", "Agilent Technologies,34411A,MOCK,2.0"),
        (3, ":OUTP?", "0"),
        (3, ":SOUR:FUNC?", "CURR"),
        (3, ":SOUR:LEV?", "0.0002"),
        (3, ":SOUR:RANG?", "0.001"),
        (7, "READ?", "0.000212"),
        (3, "SYST:ERR?", "0,No error"),
        (7, "SYST:ERR?", "0,No error"),
        (7, "SENS:VOLT:DC:RANG?", "0.1"),
        (7, "SENS:CURR:DC:RANG?", "0.001"),
        (7, "SENS:CURR:DC:RES?", "0.00000000003"),
        (7, "SENS:VOLT:DC:RES?", "0.0000001"),
    ] {
        script.push_str(&format!(
            " '{}:{}') response='{}';;\n",
            address,
            hex(&format!("{command}\n")),
            hex(response)
        ));
    }
    // Use the emulated register state for function/level queries.
    script = script
        .replace(
            &format!("response='{}';;", hex("0.0002")),
            "response=\"$level\";;",
        )
        .replace(
            &format!("response='{}';;", hex("CURR")),
            "response=\"$function\";;",
        );
    script.push_str(" *) printf 'KIV\\tERR\\t756e6578706563746564207175657279\\n'; continue;;\n esac\n fi\n printf 'KIV\\tOK\\t%s\\n' \"$response\"\n [ \"$op\" = EXIT ] && break\ndone\n");
    for name in ["kohdalab-gpib-helper", "kohdalab-usbtmc-helper"] {
        let path = directory.join(name);
        std::fs::write(&path, &script).unwrap();
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    let mut child = Command::new(env!("CARGO_BIN_EXE_iv-cli"))
        .arg("--engine")
        .env("KOHDALAB_IV_HELPER_DIR", &directory)
        .env("USERPROFILE", &directory)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let input = child.stdin.take().unwrap();
    let stdout = child.stdout.take().unwrap();
    let (tx, events) = mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            let Ok(line) = line else { break };
            let Ok(value) = serde_json::from_str(&line) else {
                break;
            };
            if tx.send(value).is_err() {
                break;
            }
        }
    });
    let mut engine = Engine {
        child,
        input,
        events,
    };
    engine.send(json!({"command":"connect","simulation":false,"source":"GPIB0::3::INSTR","meter":"GPIB0::7::INSTR"}));
    let first = engine.wait(|s| !s["live"].is_null());
    assert_eq!(first["live"]["mode"], "dc_vi");
    assert_eq!(first["live"]["meter_value"], 0.000212);
    assert_eq!(first["live"]["source_readback"], 0.0002);
    assert_eq!(first["live"]["source_resolution"], 1e-8);
    assert_eq!(first["live"]["meter_resolution"], 1e-7);
    assert_eq!(first["output"], "Off");
    assert_eq!(first["points"], 0);
    assert_eq!(first["path"], "");
    engine.wait(|s| s["live"]["timestamp"] != first["live"]["timestamp"]);
    let commands = std::fs::read_to_string(&log).unwrap();
    assert!(!commands.contains(&hex(":OUTP ON\n")));
    assert!(commands.contains(&hex("CONF:VOLT:DC\n")));
    assert!(!commands.contains(&hex(":SOUR:FUNC VOLT\n")));
    // Extra live reads during settling and averaging must not enter CSV,
    // reconfigure the measurement function/NPLC, or replace the saved average.
    let config = iv_core::RunConfig {
        mode: iv_core::Mode::Vi,
        pattern: iv_core::Pattern::Linear,
        start: -1e-6,
        stop: 1e-6,
        step: 1e-6,
        start_settle_s: 0.3,
        settle_s: 0.2,
        average_count: 3,
        ..Default::default()
    };
    let path = directory.join("sweep.csv");
    engine.send(json!({"command":"run","config":config,"path":path}));
    let live = engine
        .wait(|s| s["phase"] == "Running" && s["points"] == 0 && s["live"]["status"] == "live");
    engine.wait(|s| {
        s["phase"] == "Running"
            && s["points"] == 0
            && s["live"]["timestamp"] != live["live"]["timestamp"]
    });
    assert_eq!(
        csv_reader(&path).len(),
        1,
        "settling live reads must not add CSV rows"
    );
    let during = std::fs::read_to_string(&log).unwrap();
    assert_eq!(
        during.matches(&hex("CONF:VOLT:DC\n")).count(),
        2,
        "idle setup plus one sweep setup; no monitor reconfiguration during sweep"
    );
    let completed = engine.wait(|s| s["phase"] == "Completed");
    assert_eq!(completed["points"], 3);
    assert_eq!(completed["output"], "Off");
    assert_eq!(csv_reader(&path).len(), 4);
    let expected = (0..3).fold(0.0, |sum, _| sum + 0.000212 / 3.0);
    assert_eq!(completed["last"]["meter_value"].as_f64().unwrap(), expected);
    let commands = std::fs::read_to_string(&log).unwrap();
    assert!(commands.matches(&hex("READ?\n")).count() > 9);
    for (mode, source_command) in [
        ("dc_iv", ":SOUR:FUNC VOLT\n"),
        ("dc_vi", ":SOUR:FUNC CURR\n"),
    ] {
        let previous = completed["live"]["timestamp"].clone();
        engine.send(json!({"command":"set_live_mode","mode":mode}));
        let changed = engine.wait(|s| {
            s["phase"] == "Ready" && s["live"]["mode"] == mode && s["live"]["timestamp"] != previous
        });
        assert_eq!(changed["output"], "Off");
        assert_eq!(changed["live"]["source_readback"], 0.0);
        assert_eq!(csv_reader(&path).len(), 4);
        let commands = std::fs::read_to_string(&log).unwrap();
        assert!(commands.contains(&hex(source_command)));
        assert_eq!(
            commands.matches(&hex(":OUTP ON\n")).count(),
            1,
            "only the explicit sweep may enable output"
        );
    }
    drop(engine);
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn settling_live_updates_without_saving_points_and_stop_remains_responsive() {
    let mut engine = Engine::start();
    engine.send(json!({"command":"connect","simulation":true,"source":"","meter":""}));
    engine.wait(|s| s["connected"] == true);
    let directory = std::env::temp_dir().join(format!(
        "iv-settle-test-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&directory).unwrap();
    let path = directory.join("stopped.csv");
    let config = iv_core::RunConfig {
        start_settle_s: 10.0,
        ..Default::default()
    };
    engine.send(json!({"command":"run","config":config,"path":path}));
    let first = engine.wait(|s| s["phase"] == "Running" && s["live"]["status"] == "live");
    let mut previous = first["live"]["timestamp"].clone();
    for _ in 0..3 {
        let next = engine.wait(|s| {
            s["phase"] == "Running" && s["points"] == 0 && s["live"]["timestamp"] != previous
        });
        previous = next["live"]["timestamp"].clone();
    }
    assert_eq!(csv_reader(&path).len(), 1);
    engine.send(json!({"command":"stop"}));
    let stopped = engine.wait(|s| s["phase"] == "Stopped");
    assert_eq!(stopped["output"], "Off");
    assert_eq!(stopped["points"], 0);
    assert_eq!(csv_reader(&path).len(), 1);
    let metadata: Value =
        serde_json::from_slice(&std::fs::read(path.with_extension("run.json")).unwrap()).unwrap();
    assert_eq!(metadata["in_progress"], false);
    drop(engine);
    std::fs::remove_dir_all(directory).unwrap();
}
