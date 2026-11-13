//! IPC acceptance checks: continuous idle monitoring must not create sweep data.
#[cfg(unix)]
mod protection_fixture {
    include!("support/source_protection.rs");
    pub(super) fn decorate(s: String, dir: &std::path::Path) -> String {
        with_source_protection(s, "op", dir)
    }
}
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
        self.wait_state(|state| {
            assert!(state["error"].as_str().unwrap().is_empty(), "{state}");
            predicate(state)
        })
    }
    fn wait_state(&mut self, predicate: impl Fn(&Value) -> bool) -> Value {
        let deadline = Instant::now() + Duration::from_secs(8);
        loop {
            assert!(Instant::now() < deadline, "Engine state timed out");
            self.send(json!({"command":"heartbeat"}));
            if let Ok(event) = self.events.recv_timeout(Duration::from_millis(100))
                && event["event"] == "state"
            {
                let state = event["data"].clone();
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
fn preexisting_audit_refuses_run_before_output_and_preserves_existing_file() {
    let directory = std::env::temp_dir().join(format!(
        "iv-audit-collision-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&directory).unwrap();
    let path = directory.join("collision.csv");
    let audit = path.with_extension("raw.csv");
    std::fs::write(&audit, b"existing audit must survive").unwrap();
    let mut engine = Engine::start();
    engine.send(json!({"command":"connect","simulation":true,"source":"","meter":""}));
    engine.wait(|state| state["connected"] == true);
    engine.send(json!({"command":"run","config":iv_core::RunConfig::default(),"path":path}));
    let failed = engine.wait_state(|state| !state["error"].as_str().unwrap().is_empty());
    assert_eq!(failed["output"], "Off");
    assert!(
        failed["error"]
            .as_str()
            .unwrap()
            .contains("raw acquisition evidence")
    );
    assert!(!path.exists());
    assert_eq!(
        std::fs::read(&audit).unwrap(),
        b"existing audit must survive"
    );
    engine.send(json!({"command":"disconnect"}));
    engine.wait_state(|state| state["connected"] == false);
    std::fs::remove_dir_all(directory).unwrap();
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
    assert_eq!(metadata["run_bundle"]["schema"], "kohdalab_run_bundle_v1");
    assert_eq!(metadata["run_bundle"]["filenames"]["log"], "sweep.log");
    let log = std::fs::read_to_string(path.with_extension("log")).unwrap();
    assert!(
        log.contains("\trun_started\t")
            && log.contains("\tpoint_saved\t")
            && log.contains("\trun_finished\t")
    );
    assert_eq!(
        metadata["file_hashes"]["log_sha256"]
            .as_str()
            .unwrap()
            .len(),
        64
    );
    engine.send(json!({"command":"disconnect"}));
    engine.wait(|s| s["connected"] == false);
    std::fs::remove_dir_all(directory).unwrap();
}
#[test]
fn one_sided_zero_centered_csv_stays_in_range_and_invalid_interval_never_starts() {
    let mut engine = Engine::start();
    engine.send(json!({"command":"connect","simulation":true,"source":"","meter":""}));
    engine.wait(|s| s["connected"] == true);
    let directory = std::env::temp_dir().join(format!(
        "iv-zero-bounds-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&directory).unwrap();
    let path = directory.join("negative-only.csv");
    let mut config = iv_core::RunConfig {
        start: -0.0016,
        stop: 0.0,
        step: 0.0004,
        source_limit: 0.002,
        start_settle_s: 0.0,
        settle_s: 0.0,
        ramp_wait_s: 0.0,
        ..Default::default()
    };
    engine.send(json!({"command":"run","config":config,"path":path}));
    let finished = engine.wait(|s| s["phase"] == "Completed");
    assert_eq!(finished["points"], 9);
    assert_eq!(finished["output"], "Off");
    let saved = std::fs::read(&path).unwrap();
    let targets: Vec<f64> = csv_reader(&path)
        .iter()
        .skip(1)
        .map(|line| line.split(',').nth(6).unwrap().parse().unwrap())
        .collect();
    assert_eq!(
        targets,
        vec![
            0.0, -0.0004, -0.0008, -0.0012, -0.0016, -0.0012, -0.0008, -0.0004, 0.0
        ]
    );
    config.start = 0.0002;
    config.stop = 0.0004;
    let invalid = directory.join("must-not-exist.csv");
    engine.send(json!({"command":"run","config":config,"path":invalid}));
    let rejected = engine.wait_state(|s| {
        s["phase"] == "Failed" && s["error"].as_str().unwrap().contains("must include zero")
    });
    assert_eq!(rejected["output"], "Off");
    assert!(!invalid.exists());
    assert!(!invalid.with_extension("run.json").exists());
    assert_eq!(std::fs::read(path).unwrap(), saved);
    drop(engine);
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
    // Enable a controlled safe/safe/over-limit sequence only during the last
    // sweep. Idle monitoring must not consume the sequence.
    let limit_marker = directory.join("mixed-limit-readings");
    let off_failure = directory.join("cannot-verify-off");
    script = script.replace(" response=''\n", &format!(
        " response=''\n if [ \"$op\" = WRITE ] && [ \"$payload\" = '{}' ]; then output_on=1; conversion=0; fi\n if [ \"$op\" = WRITE ] && [ \"$payload\" = '{}' ]; then output_on=0; fi\n",
        hex(":OUTP ON\n"), hex(":OUTP OFF\n")
    ));
    for (address, command, response) in [
        (3, "*IDN?", "YOKOGAWA,GS210,MOCK,2.0"),
        (7, "*IDN?", "Agilent Technologies,34411A,MOCK,2.0"),
        (3, ":OUTP?", "0"),
        (3, ":SOUR:FUNC?", "CURR"),
        (3, ":SOUR:LEV?", "0.0002"),
        (3, ":SOUR:RANG?", "0.001"),
        (7, "READ?", "0.000212000\r\n"),
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
        )
        .replace(
            &format!("response='{}';;", hex("0")),
            "if [ \"$output_on\" = 1 ]; then response='31'; else response='30'; fi;;",
        );
    script.push_str(" *) printf 'KIV\\tERR\\t756e6578706563746564207175657279\\n'; continue;;\n esac\n fi\n printf 'KIV\\tOK\\t%s\\n' \"$response\"\n [ \"$op\" = EXIT ] && break\ndone\n");
    script = script.replace(" printf 'KIV\\tOK\\t%s\\n'", &format!(
        " if [ \"$op\" = QUERY ] && [ \"$address\" = 7 ] && [ \"$payload\" = '{}' ] && [ \"$output_on\" = 1 ] && [ -f '{}' ]; then conversion=$((conversion + 1)); if [ \"$conversion\" -le 2 ]; then response='{}'; fi; fi\n printf 'KIV\\tOK\\t%s\\n'",
        hex("READ?\n"), limit_marker.display(), hex("0")
    ));
    script = script.replace(" printf 'KIV\\tOK\\t%s\\n'", &format!(
        " if [ \"$op\" = QUERY ] && [ \"$address\" = 3 ] && [ \"$payload\" = '{}' ] && [ -f '{}' ]; then response='{}'; fi\n printf 'KIV\\tOK\\t%s\\n'",
        hex(":OUTP?\n"), off_failure.display(), hex("1")
    ));
    let script = protection_fixture::decorate(script, &directory);
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
    engine.send(json!({"command":"connect_meter","model":"AGILENT_34411A","mode":"dc_vi","simulation":false,"resource":"GPIB0::7::INSTR"}));
    let meter_only = engine.wait(|s| s["idle_reading"]["source"] == false);
    assert_eq!(meter_only["idle_reading"]["mode"], "dc_vi");
    assert_eq!(meter_only["idle_reading"]["value"], 0.000212);
    assert!(
        !std::fs::read_to_string(&log)
            .unwrap()
            .lines()
            .any(|line| line.starts_with("QUERY 3 "))
    );
    engine.send(json!({"command":"disconnect_meter"}));
    engine.wait(|s| s["meter_connected"] == false);
    let standalone_log_len = std::fs::read_to_string(&log).unwrap().len();

    engine.send(json!({"command":"connect_source","model":"YOKOGAWA_GS210","simulation":false,"resource":"GPIB0::3::INSTR"}));
    let source_only = engine.wait(|s| s["idle_reading"]["source"] == true);
    assert_eq!(source_only["idle_reading"]["mode"], "dc_vi");
    assert_eq!(source_only["idle_reading"]["value"], 0.0002);
    assert_eq!(source_only["idle_reading"]["resolution"], 1e-8);
    assert_eq!(source_only["output"], "Off");

    engine.send(json!({"command":"connect_meter","model":"AGILENT_34411A","simulation":false,"resource":"GPIB0::7::INSTR"}));
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
        during[standalone_log_len..]
            .matches(&hex("CONF:VOLT:DC\n"))
            .count(),
        2,
        "idle setup plus one sweep setup; no monitor reconfiguration during sweep"
    );
    let completed = engine.wait(|s| s["phase"] == "Completed");
    assert_eq!(completed["points"], 3);
    assert_eq!(completed["output"], "Off");
    assert_eq!(csv_reader(&path).len(), 4);
    let expected = (0..3).fold(0.0, |sum, _| sum + 0.000212 / 3.0);
    assert_eq!(completed["last"]["meter_value"].as_f64().unwrap(), expected);
    let mut raw = csv::Reader::from_path(path.with_extension("raw.csv")).unwrap();
    let rows = raw.records().collect::<Result<Vec<_>, _>>().unwrap();
    assert!(rows.iter().any(|row| {
        row.get(4) == Some("meter_reading")
            && row
                .get(8)
                .is_some_and(|value| value.contains("0.000212000"))
    }));
    assert!(rows.iter().any(|row| {
        row.get(4) == Some("transport_exchange")
            && row.get(7) == Some("READ?\n")
            && row.get(8) == Some("0.000212000\r\n")
    }));
    assert!(rows.iter().any(|row| row.get(4) == Some("csv_write")));
    let average = rows
        .iter()
        .find(|row| &row[4] == "average_window" && &row[2] == "1")
        .unwrap();
    let window: Value = serde_json::from_str(&average[17]).unwrap();
    let signal = &window["last_conversion"];
    let write = rows
        .iter()
        .find(|row| &row[4] == "csv_write" && &row[2] == "1")
        .unwrap();
    let mut analysis = csv::Reader::from_path(&path).unwrap();
    let header = analysis.headers().unwrap().clone();
    let saved = analysis.records().next().unwrap().unwrap();
    let cell = |key| &saved[header.iter().position(|value| value == key).unwrap()];
    assert_eq!(
        cell("timestamp"),
        signal["host_completed_at_utc"].as_str().unwrap()
    );
    assert_eq!(
        cell("elapsed_s").parse::<f64>().unwrap(),
        signal["host_completed_elapsed_s"].as_f64().unwrap()
    );
    assert!(
        write[13].parse::<f64>().unwrap() >= signal["host_completed_elapsed_s"].as_f64().unwrap()
    );
    assert!(write[14].parse::<f64>().unwrap() >= write[13].parse::<f64>().unwrap());
    assert_eq!(cell("readings_completed"), "3");
    assert_eq!(cell("meter_stddev").parse::<f64>().unwrap(), 0.);

    let commands = std::fs::read_to_string(&log).unwrap();
    assert!(commands.matches(&hex("READ?\n")).count() > 9);
    // Repeat is per-point accumulation, not another traversal of the source.
    // Use zero settling so the command trace contains only the saved conversions.
    let repeat_config = iv_core::RunConfig {
        mode: iv_core::Mode::Vi,
        pattern: iv_core::Pattern::Linear,
        start: 1e-6,
        stop: 1e-6,
        step: 1e-6,
        source_limit: 1e-6,
        repeat: 4,
        average_count: 1,
        compliance: 1.0,
        settle_s: 0.0,
        start_settle_s: 0.0,
        ramp_wait_s: 0.0,
        ..Default::default()
    };
    let repeat_path = directory.join("repeat.csv");
    let offset = std::fs::read_to_string(&log).unwrap().len();
    engine.send(json!({"command":"run","config":repeat_config,"path":repeat_path}));
    let repeated = engine.wait(|s| s["phase"] == "Completed" && s["run_id"] != completed["run_id"]);
    assert_eq!(repeated["points"], 1);
    assert_eq!(repeated["output"], "Off");
    assert_eq!(csv_reader(&repeat_path).len(), 2);
    let commands = std::fs::read_to_string(&log).unwrap();
    let trace = &commands[offset..];
    let off_query = format!("QUERY 3 {}", hex(":OUTP?\n"));
    let configuration = format!("WRITE 3 {}", hex(":SOUR:FUNC CURR\n"));
    assert!(
        trace.find(&off_query).unwrap() < trace.find(&configuration).unwrap(),
        "each sweep must verify output OFF before changing source configuration"
    );
    let target = format!("WRITE 3 {}", hex(":SOUR:LEV:FIX 1.000000000000e-6\n"));
    assert_eq!(trace.matches(&target).count(), 1);
    let at_target = trace.split_once(&target).unwrap().1;
    let conversions = at_target
        .split_once(&format!("QUERY 3 {}", hex(":SOUR:LEV?\n")))
        .unwrap()
        .0;
    assert_eq!(
        conversions
            .matches(&format!("QUERY 7 {}", hex("READ?\n")))
            .count(),
        4
    );
    assert_eq!(
        conversions
            .matches(&format!("QUERY 7 {}", hex("SENS:VOLT:DC:RANG?\n")))
            .count(),
        4,
        "autorange must be checked after each saved conversion"
    );
    assert_eq!(
        conversions
            .matches(&format!("QUERY 7 {}", hex("SENS:VOLT:DC:RES?\n")))
            .count(),
        1,
        "same-range repeat readings should share one resolution query per run"
    );
    assert_eq!(repeated["last"]["meter_resolution"], 1e-7);
    assert!(
        !conversions.contains("WRITE 3 "),
        "source must stay fixed during accumulation"
    );
    let metadata: Value =
        serde_json::from_slice(&std::fs::read(repeat_path.with_extension("run.json")).unwrap())
            .unwrap();
    assert_eq!(metadata["config"]["repeat"], 4);
    assert_eq!(metadata["config"]["average_count"], 4);

    // A single conversion above the software limit stops the accumulation early.
    std::fs::write(&limit_marker, "enable mixed readings").unwrap();
    let limited_path = directory.join("repeat-limit.csv");
    let limited = iv_core::RunConfig {
        compliance: 0.0001,
        ..repeat_config
    };
    engine.send(json!({"command":"run","config":limited,"path":limited_path}));
    let cutoff = engine.wait(|s| s["phase"] == "Compliance stop");
    assert_eq!(cutoff["points"], 1);
    assert_eq!(cutoff["output"], "Off");
    let partial_mean = cutoff["last"]["meter_value"].as_f64().unwrap();
    assert!((partial_mean - 0.000212 / 3.0).abs() < 1e-19);
    assert!(
        partial_mean < 0.0001,
        "the trigger sample must be retained even when the mean is below the limit"
    );
    let metadata: Value =
        serde_json::from_slice(&std::fs::read(limited_path.with_extension("run.json")).unwrap())
            .unwrap();
    assert_eq!(
        metadata["instrument_status_events"][0]["readings_completed"],
        3
    );
    assert_eq!(
        metadata["instrument_status_events"][0]["readings_requested"],
        4
    );
    assert_eq!(
        metadata["instrument_status_events"][0]["trigger_value"],
        0.000212
    );
    assert_eq!(metadata["instrument_status_events"][0]["trigger_unit"], "V");
    let mut limited_raw = csv::Reader::from_path(limited_path.with_extension("raw.csv")).unwrap();
    let limited_rows = limited_raw
        .records()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert!(limited_rows.iter().any(|row| {
        row.get(4) == Some("saved_point")
            && row
                .get(17)
                .is_some_and(|details| details.contains("\"readings_requested\":4"))
            && row
                .get(17)
                .is_some_and(|details| details.contains("\"readings_completed\":3"))
    }));
    assert!(
        limited_rows
            .iter()
            .any(|row| row.get(4) == Some("average_window"))
    );
    let window_row = limited_rows
        .iter()
        .find(|row| &row[4] == "average_window")
        .unwrap();
    let window: Value = serde_json::from_str(&window_row[17]).unwrap();
    let mut limited_analysis = csv::Reader::from_path(&limited_path).unwrap();
    let header = limited_analysis.headers().unwrap().clone();
    let saved = limited_analysis.records().next().unwrap().unwrap();
    let cell = |key| &saved[header.iter().position(|value| value == key).unwrap()];
    assert_eq!(
        cell("timestamp"),
        window["last_conversion"]["host_completed_at_utc"]
            .as_str()
            .unwrap()
    );
    assert_eq!(cell("readings_requested"), "4");
    assert_eq!(cell("readings_completed"), "3");
    let expected_stddev = (0.000212_f64.powi(2) / 3.).sqrt();
    assert!((cell("meter_stddev").parse::<f64>().unwrap() - expected_stddev).abs() < 1e-19);
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
            3,
            "only the explicit sweeps may enable output"
        );
    }
    // Even when the last cached state is Off, a fresh verification failure
    // must reject a run before configuration, level changes or file creation.
    std::fs::write(&off_failure, "source does not confirm OFF").unwrap();
    let rejected_path = directory.join("must-not-start.csv");
    let offset = std::fs::read_to_string(&log).unwrap().len();
    engine.send(json!({"command":"run","config":limited,"path":rejected_path}));
    let rejected = engine.wait_state(|s| s["phase"] == "Failed");
    assert_eq!(rejected["output"], "Unknown");
    assert!(
        rejected["error"]
            .as_str()
            .unwrap()
            .contains("Cannot start: source output OFF verification failed")
    );
    assert!(!rejected_path.exists());
    let commands = std::fs::read_to_string(&log).unwrap();
    for line in commands[offset..]
        .lines()
        .filter(|line| line.starts_with("WRITE "))
    {
        assert_eq!(line, format!("WRITE 3 {}", hex(":OUTP OFF\n")));
    }
    std::fs::remove_file(&off_failure).unwrap();
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

#[test]
fn automatic_settings_are_applied_and_preserve_requested_values_in_metadata() {
    let mut engine = Engine::start();
    engine.send(json!({"command":"connect","simulation":true,"source":"","meter":""}));
    engine.wait(|s| s["connected"] == true);
    let directory = std::env::temp_dir().join(format!(
        "iv-auto-settings-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&directory).unwrap();
    let path = directory.join("sweep.csv");
    let config = iv_core::RunConfig {
        mode: iv_core::Mode::Vi,
        pattern: iv_core::Pattern::Linear,
        start: 0.0,
        stop: 2e-8,
        step: 1e-9,
        source_limit: 1e-6,
        compliance: 1.0,
        ramp_step: 1e-9,
        start_settle_s: 0.0,
        settle_s: 0.0,
        ..Default::default()
    };
    engine.send(json!({"command":"run","config":config,"path":path}));
    let finished = engine.wait(|s| s["phase"] == "Completed");
    assert_eq!(finished["points"], 3);
    assert_eq!(finished["output"], "Off");
    let metadata: Value =
        serde_json::from_slice(&std::fs::read(path.with_extension("run.json")).unwrap()).unwrap();
    assert_eq!(metadata["config"]["step"], 1e-8);
    assert_eq!(metadata["config"]["ramp_step"], 1e-5);
    assert_eq!(metadata["requested_config"]["step"], 1e-9);
    assert_eq!(metadata["config"]["source_limit"], config.source_limit);
    assert_eq!(metadata["in_progress"], false);
    assert_eq!(csv_reader(&path).len(), 4);
    drop(engine);
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn combined_connect_attempts_meter_after_invalid_source() {
    let mut engine = Engine::start();
    engine.send(json!({"command":"connect","simulation":true,"source":"","meter":"","source_model":"INVALID_MODEL","meter_model":"SIMULATED_METER"}));
    let state = engine
        .wait_state(|s| s["meter_connected"] == true && !s["error"].as_str().unwrap().is_empty());
    assert_eq!(state["source_connected"], false);
    assert!(state["error"].as_str().unwrap().contains("source"));
}

#[test]
fn simulated_devices_start_independent_live_readings_on_connect() {
    for source in [true, false] {
        let mut engine = Engine::start();
        let (command, model) = if source {
            ("connect_source", "SIMULATED_SOURCE")
        } else {
            ("connect_meter", "SIMULATED_METER")
        };
        engine.send(json!({"command":command,"model":model,"simulation":true,"resource":""}));
        let state = engine.wait(|s| s["idle_reading"]["source"] == source);
        assert_eq!(state["connected"], false);
        assert_eq!(state["source_connected"], source);
        assert_eq!(state["meter_connected"], !source);
        assert!(state["idle_reading"]["value"].is_number());
        assert!(state["live"].is_null());
        assert_eq!(state["points"], 0);
        if source {
            assert_eq!(state["output"], "Off");
        }

        let first = &state["idle_reading"]["timestamp"];
        engine.wait(|s| {
            s["idle_reading"]["timestamp"].is_string() && s["idle_reading"]["timestamp"] != *first
        });
        engine.send(json!({"command":"disconnect"}));
        let state = engine.wait(|s| {
            !s["source_connected"].as_bool().unwrap() && !s["meter_connected"].as_bool().unwrap()
        });
        assert!(state["idle_reading"].is_null());
    }
}

#[test]
fn registered_devices_keep_connections_when_the_measurement_source_changes() {
    let mut engine = Engine::start();
    engine.send(json!({"command":"connect_inventory","id":1,"source":true,"assign":true,"simulation":true,"model":"SIMULATED_SOURCE","resource":"","mode":"dc_iv"}));
    engine.send(json!({"command":"connect_inventory","id":2,"source":true,"simulation":true,"model":"SIMULATED_SOURCE","resource":"","mode":"dc_iv"}));
    engine.send(json!({"command":"connect_inventory","id":3,"source":false,"simulation":true,"model":"SIMULATED_METER","resource":"","mode":"dc_vi"}));
    let initial = engine.wait(|s| {
        s["inventory_devices"][0]["assigned"] == true
            && s["inventory_devices"][1]["reading"]["value"].is_number()
            && s["inventory_devices"][2]["reading"]["value"].is_number()
    });
    assert_eq!(initial["inventory_devices"][2]["reading"]["mode"], "dc_vi");
    assert_eq!(initial["meter_connected"], false);
    assert_eq!(initial["points"], 0);

    engine.send(json!({"command":"select_inventory","id":2,"source":true}));
    let changed = engine.wait(|s| {
        s["inventory_devices"][0]["reading"]["value"].is_number()
            && s["inventory_devices"][1]["assigned"] == true
    });
    assert_eq!(changed["inventory_devices"][0]["id"], 1);
    assert_eq!(changed["inventory_devices"][0]["assigned"], false);
    assert_eq!(changed["inventory_devices"][2]["id"], 3);
    assert!(changed["inventory_devices"][2]["reading"]["value"].is_number());

    engine.send(json!({"command":"select_inventory","id":3,"source":false}));
    let paired = engine.wait(|s| s["connected"] == true && !s["live"].is_null());
    assert_eq!(paired["inventory_devices"][2]["assigned"], true);
    assert_eq!(paired["output"], "Off");
    engine.send(json!({"command":"disconnect"}));
    let disconnected =
        engine.wait(|s| !s["inventory_devices"].is_array() && s["connected"] == false);
    assert_eq!(disconnected["phase"], "Idle");
}

#[test]
fn partial_connections_bind_successes_and_name_the_failed_meter() {
    let mut engine = Engine::start();
    let connection = |id, source, model, name| {
        json!({"id":id,"source":source,"simulation":true,"model":model,
               "resource":"","mode":"dc_iv","name":name})
    };
    let bind = |connections| {
        json!({"command":"bind_inventory","source":1,"secondary":2,"meter":25,
               "reference":26,"mode":"dc_iv","connections":connections})
    };
    engine.send(bind(vec![connection(
        2,
        true,
        "SIMULATED_SOURCE",
        "Bias 2",
    )]));
    let secondary = engine.wait(|s| s["source2_connected"] == true);
    assert_eq!(secondary["source_connected"], false);
    assert_eq!(secondary["source2_output"], "Off");

    engine.send(bind(vec![
        connection(1, true, "SIMULATED_SOURCE", "Bias 1"),
        connection(26, false, "SIMULATED_SOURCE", "Voltage meter"),
        connection(25, false, "SIMULATED_METER", "Current meter"),
    ]));
    let partial = engine.wait(|s| s["meter_connected"] == true);
    assert_eq!(partial["source_connected"], true);
    assert_eq!(partial["source2_connected"], true);
    assert_eq!(partial["reference_meter_connected"], false);
    assert_eq!(partial["inventory_devices"].as_array().unwrap().len(), 3);
    assert!(
        partial["message"]
            .as_str()
            .unwrap()
            .contains("Voltage meter"),
        "{partial}"
    );

    engine.send(bind(vec![connection(
        26,
        false,
        "SIMULATED_METER",
        "Voltage meter",
    )]));
    let complete = engine.wait(|s| s["reference_meter_connected"] == true);
    assert_eq!(complete["inventory_devices"].as_array().unwrap().len(), 4);
    assert_eq!(complete["output"], "Off");
    assert_eq!(complete["points"], 0);
}

#[test]
fn binds_all_measurement_roles_and_swaps_meters_without_reconnecting() {
    let mut engine = Engine::start();
    for (id, source) in [(1, true), (2, true), (6, true), (25, false), (26, false)] {
        let model = if source {
            "SIMULATED_SOURCE"
        } else {
            "SIMULATED_METER"
        };
        engine.send(
            json!({"command":"connect_inventory","id":id,"source":source,
            "simulation":true,"model":model,"resource":"","mode":"dc_iv"}),
        );
    }
    let bind = |source, secondary, meter, reference, mode| {
        json!({
            "command":"bind_inventory","source":source,"secondary":secondary,
            "meter":meter,"reference":reference,"mode":mode
        })
    };
    engine.send(bind(1, Some(2), 25, 26, "dc_iv"));
    engine.wait(|state| {
        state["source2_connected"] == true && state["reference_meter_connected"] == true
    });
    engine.send(bind(1, Some(6), 25, 26, "dc_iv"));
    let reassigned = engine.wait(|state| state["inventory_devices"][2]["auxiliary"] == true);
    assert_eq!(reassigned["inventory_devices"][1]["assigned"], false);
    assert_eq!(reassigned["source2_output"], "Off");

    engine.send(bind(1, None, 26, 25, "dc_vi"));
    let switched = engine.wait(|state| state["inventory_devices"][3]["auxiliary"] == true);
    assert_eq!(switched["inventory_devices"].as_array().unwrap().len(), 5);
    assert_eq!(switched["inventory_devices"][4]["assigned"], true);
    assert_eq!(switched["source2_connected"], false);
    assert_eq!(switched["output"], "Off");
    assert_eq!(switched["points"], 0);
}

#[test]
fn sweep_and_inventory_live_dc_remain_independent() {
    let mut engine = Engine::start();
    for (id, source, assign) in [
        (10, true, true),
        (11, false, true),
        (20, true, false),
        (21, false, false),
    ] {
        let model = if source {
            "SIMULATED_SOURCE"
        } else {
            "SIMULATED_METER"
        };
        engine.send(
            json!({"command":"connect_inventory","id":id,"source":source,"assign":assign,
            "simulation":true,"model":model,"resource":"","mode":"dc_iv"}),
        );
    }
    engine.wait(|s| {
        s["connected"] == true && s["inventory_devices"][3]["reading"]["timestamp"].is_string()
    });

    let dc = |id, value| {
        json!({"command":"set_inventory_dc","id":id,"request":{
            "mode":"dc_iv","value":value,"compliance":0.2,"ramp_step":0.001,"range":0.1
        }})
    };
    engine.send(dc(20, 0.002));
    let before = engine.wait(|s| s["inventory_devices"][2]["reading"]["value"] == 0.002);
    assert_eq!(before["output"], "Off");
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("independent.csv");
    let config = iv_core::RunConfig {
        start_settle_s: 5.0,
        ..Default::default()
    };
    engine.send(json!({"command":"run","config":&config,"path":path}));
    let running = engine.wait(|s| s["phase"] == "Running" && s["live"].is_object());
    assert_eq!(running["output"], "On");
    assert_eq!(running["inventory_devices"][2]["output"], "On");
    let meter_reading = running["inventory_devices"][3]["reading"]["timestamp"].clone();

    engine.send(dc(10, 0.004));
    let locked = engine.wait(|s| {
        s["inventory_error"]
            .as_str()
            .is_some_and(|e| e.contains("unassigned Source"))
    });
    assert_eq!(locked["phase"], "Running");
    assert_eq!(locked["output"], "On");

    engine.send(dc(20, 0.003));
    let updated = engine.wait(|s| {
        s["inventory_devices"][2]["reading"]["value"] == 0.003
            && s["inventory_devices"][3]["reading"]["timestamp"] != meter_reading
    });
    assert_eq!(updated["phase"], "Running");
    assert_eq!(updated["output"], "On");
    assert!(updated["live"].is_object());

    engine.send(json!({"command":"inventory_dc_off","id":20}));
    let off = engine.wait(|s| s["inventory_devices"][2]["output"] == "Off");
    assert_eq!(off["phase"], "Running");
    assert_eq!(off["output"], "On");
    assert!(off["inventory_devices"][2]["dc"].is_null());

    engine.send(dc(20, 0.001));
    engine.wait(|s| s["inventory_devices"][2]["output"] == "On");
    engine.send(json!({"command":"stop"}));
    let stopped = engine.wait(|s| s["phase"] == "Stopped" && s["output"] == "Off");
    assert_eq!(stopped["inventory_devices"][2]["output"], "On");

    assert!(path.is_file());
    let path = dir.path().join("all-off.csv");
    engine.send(json!({"command":"run","config":config,"path":path}));
    engine.wait(|s| s["phase"] == "Running" && s["output"] == "On");
    engine.send(json!({"command":"output_off"}));
    engine.wait(|s| s["inventory_devices"][2]["output"] == "Off" && s["output"] == "Off");
    assert!(path.is_file());
}
