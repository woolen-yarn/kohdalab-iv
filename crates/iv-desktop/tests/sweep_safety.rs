//! Native-engine safety contracts.  The helper below is a stateful fake GPIB
//! bridge; it never opens a real instrument.

#![cfg(unix)]

use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

include!("support/source_protection.rs");

const METERS: &[(&str, &str)] = &[
    ("AGILENT_34401A", "Agilent Technologies,34401A,MOCK,1.0"),
    ("AGILENT_34411A", "Agilent Technologies,34411A,MOCK,1.0"),
    ("KEYSIGHT_34411A", "Keysight Technologies,34411A,MOCK,1.0"),
    ("KEYSIGHT_34465A", "Keysight Technologies,34465A,MOCK,1.0"),
    ("ADCMT_7461A", "ADCMT,7461A,MOCK,1.0"),
];

fn hex(text: &str) -> String {
    text.as_bytes().iter().map(|b| format!("{b:02x}")).collect()
}

struct Engine {
    child: Child,
    input: ChildStdin,
    events: mpsc::Receiver<Value>,
}
impl Engine {
    fn start(dir: &Path) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_iv-cli"))
            .arg("--engine")
            .env("KOHDALAB_IV_HELPER_DIR", dir)
            .env("USERPROFILE", dir)
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
    fn send(&mut self, value: Value) {
        writeln!(self.input, "{value}").unwrap();
        self.input.flush().unwrap();
    }
    fn wait(&mut self, check: impl Fn(&Value) -> bool) -> Value {
        let end = Instant::now() + Duration::from_secs(10);
        loop {
            assert!(Instant::now() < end, "engine timed out");
            self.send(json!({"command":"heartbeat"}));
            if let Ok(event) = self.events.recv_timeout(Duration::from_millis(100))
                && event["event"] == "state"
            {
                let state = event["data"].clone();
                if check(&state) {
                    return state;
                }
            }
        }
    }
    fn connect_resources(
        &mut self,
        source_model: &str,
        meter: &str,
        source: &str,
        meter_resource: &str,
    ) {
        self.send(json!({"command":"connect","simulation":false,
            "source":source,"meter":meter_resource,
            "source_model":source_model,"meter_model":meter}));
        let state = self.wait(|s| s["connected"] == true);
        assert!(state["error"].as_str().unwrap().is_empty(), "{state}");
    }
    fn connect(&mut self, source_model: &str, meter: &str) {
        self.connect_resources(source_model, meter, "GPIB0::3::INSTR", "GPIB0::7::INSTR");
    }
}
impl Drop for Engine {
    fn drop(&mut self) {
        let _ = writeln!(self.input, "{{\"command\":\"shutdown\"}}");
        let _ = self.input.flush();
        let end = Instant::now() + Duration::from_secs(5);
        while Instant::now() < end {
            if self.child.try_wait().unwrap().is_some() {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("test engine did not exit cleanly");
    }
}

struct Bridge {
    dir: PathBuf,
    log: PathBuf,
}
impl Bridge {
    fn new(identity: &str, fault: &str, reading: &str) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "iv-safety-{}-{:?}-{}",
            std::process::id(),
            std::thread::current().id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&dir).unwrap();
        let log = dir.join("commands.log");
        let script = format!(
            "#!/bin/sh\nprintf 'KIV\\tOK\\t\\n'\noutput='{zero}'\nfunction='{volt}'\nsource_range='{range}'\nwhile read operation address timeout payload; do\n printf '%s %s %s\\n' \"$operation\" \"$address\" \"$payload\" >> '{log}'\n response=''\n if [ \"$operation\" = WRITE ]; then case \"$address:$payload\" in 3:{on}|0:{on}) output='{one}' ;; 3:{off}|0:{off}) output='{zero}' ;; 3:{function_volt}|0:{function_volt}) function='{volt}'; source_range='{range}' ;; 3:{function_curr}|0:{function_curr}) function='{curr}'; source_range='{current_source_range}' ;; esac; fi\n if [ \"$operation\" = QUERY ] || [ \"$operation\" = READ ]; then case \"$address:$payload\" in 3:{idn}) response='{source_id}' ;; 7:{idn}) response='{identity}' ;; 0:{idn}) case \"$1\" in METER) response='{identity}' ;; *) response='{source_id}' ;; esac ;; 3:{outq}|0:{outq}) case '{fault}' in on_refusal) response='{zero}' ;; *) response=\"$output\" ;; esac ;; 3:{funcq}|0:{funcq}) case '{fault}' in function_mismatch) response='{curr}' ;; *) response=\"$function\" ;; esac ;; 3:{rangq}|0:{rangq}) case '{fault}' in range_mismatch) response='{zero}' ;; *) response=\"$source_range\" ;; esac ;; 3:{levq}|0:{levq}|3:{levfixq}|0:{levfixq}|3:{levautoq}|0:{levautoq}) case '{fault}' in malformed_zero) response='{bad}' ;; *) response='{zero}' ;; esac ;; 3:{errq}|0:{errq}|7:{errq}|7:{adcerrq}|0:{adcerrq}) response='{no_error}' ;; 7:{readq}|0:{readq}) response='{reading}' ;; 7:{crange}|0:{crange}|7:{vrange}|0:{vrange}) response='{range}' ;; 7:{cres}|0:{cres}|7:{vres}|0:{vres}) response='{resolution}' ;; *) response='' ;; esac; fi\n printf 'KIV\\tOK\\t%s\\n' \"$response\"\n [ \"$operation\" = EXIT ] && break\ndone\n",
            log = log.display(),
            source_id = hex("YOKOGAWA,GS210,MOCK,1.0"),
            identity = hex(identity),
            fault = fault,
            reading = hex(reading),
            zero = hex("0"),
            one = hex("1"),
            volt = hex("VOLT"),
            curr = hex("CURR"),
            range = hex("0.01"),
            current_source_range = hex("0.001"),
            bad = hex("bad"),
            no_error = hex("0,No error"),
            resolution = hex("1e-9"),
            idn = hex("*IDN?\n"),
            on = hex(":OUTP ON\n"),
            off = hex(":OUTP OFF\n"),
            outq = hex(":OUTP?\n"),
            function_volt = hex(":SOUR:FUNC VOLT\n"),
            function_curr = hex(":SOUR:FUNC CURR\n"),
            funcq = hex(":SOUR:FUNC?\n"),
            rangq = hex(":SOUR:RANG?\n"),
            levq = hex(":SOUR:LEV?\n"),
            levfixq = hex(":SOUR:LEV:FIX?\n"),
            levautoq = hex(":SOUR:LEV:AUTO?\n"),
            errq = hex("SYST:ERR?\n"),
            adcerrq = hex(":SYSTem:ERRor?\n"),
            readq = hex("READ?\n"),
            crange = hex("SENS:CURR:DC:RANG?\n"),
            vrange = hex("SENS:VOLT:DC:RANG?\n"),
            cres = hex("SENS:CURR:DC:RES?\n"),
            vres = hex("SENS:VOLT:DC:RES?\n")
        );
        let script = with_source_protection(script, "operation", &dir);
        for name in ["kohdalab-gpib-helper", "kohdalab-usbtmc-helper"] {
            let path = dir.join(name);
            std::fs::write(&path, &script).unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
        }
        Self { dir, log }
    }
    fn commands(&self) -> Vec<String> {
        std::fs::read_to_string(&self.log)
            .unwrap_or_default()
            .lines()
            .map(str::to_owned)
            .collect()
    }
}
impl Drop for Bridge {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// A separate fixture keeps the 7651's panel/status protocol visible in this
/// desktop-level test.  It deliberately accepts only the canonical commands
/// translated by `iv-transport`, rather than pretending that the 7651 speaks
/// GS210 SCPI.
struct LegacyBridge {
    dir: PathBuf,
    log: PathBuf,
}
impl LegacyBridge {
    fn new(identity: &str, reading: &str) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "iv-safety-7651-{}-{:?}-{}",
            std::process::id(),
            std::thread::current().id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&dir).unwrap();
        let log = dir.join("commands.log");
        let script = format!(
            "#!/bin/sh\nprintf 'KIV\\tOK\\t\\n'\nout=0\nf=1\nr=2\nwhile read op address timeout payload; do\n printf '%s %s %s\\n' \"$op\" \"$address\" \"$payload\" >> '{log}'\n response=''\n if [ \"$op\" = WRITE ]; then case \"$address:$payload\" in 3:{o1}) out=16 ;; 3:{o0}) out=0 ;; 3:{f1}) f=1 ;; 3:{f5}) f=5 ;; 3:{r2}) r=2 ;; 3:{r4}) r=4 ;; esac; fi\n case \"$op:$address:$payload\" in QUERY:3:{os}) case \"$f:$r\" in 1:2) response='{panel12}' ;; 5:4) response='{panel54}' ;; *) response='{panel12}' ;; esac ;; QUERY:3:{oc}) if [ \"$out\" = 16 ]; then response='{status_on}'; else response='{status_off}'; fi ;; QUERY:3:{od}) response='{zero}' ;; QUERY:7:{idn}) response='{identity}' ;; QUERY:7:{read}) response='{reading}' ;; QUERY:7:{err}|QUERY:7:{adcerr}) response='{no_error}' ;; QUERY:7:{crange}|QUERY:7:{vrange}) response='{range}' ;; QUERY:7:{cres}|QUERY:7:{vres}) response='{resolution}' ;; WRITE:*:*|LOCAL:*:*|REN:*:*|CLEAR:*:*|EXIT:*:*) response='' ;; *) response='' ;; esac\n printf 'KIV\\tOK\\t%s\\n' \"$response\"\n [ \"$op\" = EXIT ] && break\ndone\n",
            log = log.display(),
            identity = hex(identity),
            reading = hex(reading),
            os = hex("OS;E\n"),
            oc = hex("OC;E\n"),
            od = hex("OD;E\n"),
            idn = hex("*IDN?\n"),
            read = hex("READ?\n"),
            err = hex("SYST:ERR?\n"),
            adcerr = hex(":SYSTem:ERRor?\n"),
            o1 = hex("O1;E\n"),
            o0 = hex("O0;E\n"),
            f1 = hex("H0;F1;E\n"),
            f5 = hex("H0;F5;E\n"),
            r2 = hex("R2;E\n"),
            r4 = hex("R4;E\n"),
            panel12 = hex("7651 REV1\nF1R2S+0.0000E0\nT0\nLV30LA5\nEND\n"),
            panel54 = hex("7651 REV1\nF5R4S+0.0000E0\nT0\nLV30LA5\nEND\n"),
            status_on = hex("STS1=16\n"),
            status_off = hex("STS1=0\n"),
            zero = hex("0\n"),
            no_error = hex("0,No error\n"),
            range = hex("0.001"),
            resolution = hex("1e-9"),
            crange = hex("SENS:CURR:DC:RANG?\n"),
            vrange = hex("SENS:VOLT:DC:RANG?\n"),
            cres = hex("SENS:CURR:DC:RES?\n"),
            vres = hex("SENS:VOLT:DC:RES?\n")
        );
        for name in ["kohdalab-gpib-helper", "kohdalab-usbtmc-helper"] {
            let path = dir.join(name);
            std::fs::write(&path, &script).unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
        }
        Self { dir, log }
    }
    fn commands(&self) -> Vec<String> {
        std::fs::read_to_string(&self.log)
            .unwrap_or_default()
            .lines()
            .map(str::to_owned)
            .collect()
    }
}
impl Drop for LegacyBridge {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn config(
    source: &str,
    meter: &str,
    mode: iv_core::Mode,
    settle: f64,
    average: u32,
) -> iv_core::RunConfig {
    iv_core::RunConfig {
        source_model: source.into(),
        meter_model: meter.into(),
        mode,
        pattern: iv_core::Pattern::Linear,
        start: 0.0,
        stop: 0.0,
        step: 0.001,
        source_limit: 0.001,
        compliance: 0.01,
        start_settle_s: settle,
        settle_s: 0.0,
        ramp_wait_s: 0.0,
        average_count: average,
        ..Default::default()
    }
}
fn path(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "iv-safety-{label}-{}-{}.csv",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}
fn cleanup(path: &Path) {
    let _ = std::fs::remove_file(path);
    let _ = std::fs::remove_file(path.with_extension("run.json"));
    let _ = std::fs::remove_file(path.with_extension("raw.csv"));
}

fn assert_immediate_off(commands: &[String]) {
    let read = format!("QUERY 7 {}", hex("READ?\n"));
    let off = format!("WRITE 3 {}", hex(":OUTP OFF\n"));
    let at = commands
        .iter()
        .rposition(|line| line == &read)
        .expect("triggering READ?");
    assert_eq!(
        commands.get(at + 1),
        Some(&off),
        "the next I/O after an over-limit READ must be source OFF: {commands:?}"
    );
    for line in &commands[at + 1..] {
        if line.starts_with("WRITE 3 ") {
            assert!(
                line == &off || line == &format!("WRITE 3 {}", hex(":SOUR:LEV:FIX 0\n")),
                "no nonzero cleanup write: {line}"
            );
        }
    }
}

fn assert_legacy_immediate_off(commands: &[String]) {
    let read = format!("QUERY 7 {}", hex("READ?\n"));
    let off = format!("WRITE 3 {}", hex("O0;E\n"));
    let at = commands
        .iter()
        .rposition(|line| line == &read)
        .expect("triggering READ?");
    assert_eq!(
        commands.get(at + 1),
        Some(&off),
        "7651 must receive O0;E immediately after the triggering READ?: {commands:?}"
    );
    for line in &commands[at + 1..] {
        if line.starts_with("WRITE 3 ") {
            assert!(
                line == &off || line == &format!("WRITE 3 {}", hex("S0;E\n")),
                "no nonzero 7651 cleanup write: {line}"
            );
        }
    }
}

#[test]
fn live_overlimit_stops_before_csv_and_before_followup_io() {
    let bridge = Bridge::new(METERS[1].1, "", "0.1");
    let mut engine = Engine::start(&bridge.dir);
    engine.connect("YOKOGAWA_GS210", METERS[1].0);
    let output = path("live");
    engine.send(json!({"command":"run","config":config("YOKOGAWA_GS210", METERS[1].0, iv_core::Mode::Iv, 0.20, 1),"path":output}));
    let state = engine.wait(|s| s["phase"] == "Compliance stop");
    assert_eq!(state["output"], "Off");
    assert_eq!(state["points"], 0);
    assert_eq!(std::fs::read_to_string(&output).unwrap().lines().count(), 1);
    assert_immediate_off(&bridge.commands());
    cleanup(&output);
}

#[test]
fn averaged_overlimit_is_immediately_safe_and_never_ramps_nonzero_afterward() {
    let bridge = Bridge::new(METERS[1].1, "", "0.1");
    let mut engine = Engine::start(&bridge.dir);
    engine.connect("YOKOGAWA_GS210", METERS[1].0);
    let output = path("formal");
    engine.send(json!({"command":"run","config":config("YOKOGAWA_GS210", METERS[1].0, iv_core::Mode::Iv, 0.0, 3),"path":output}));
    let state = engine.wait(|s| s["phase"] == "Compliance stop");
    assert_eq!(state["output"], "Off");
    assert_eq!(state["points"], 1);
    assert_immediate_off(&bridge.commands());
    cleanup(&output);
}

#[test]
fn source_confirmation_faults_fail_safely_before_data_creation() {
    for fault in [
        "range_mismatch",
        "function_mismatch",
        "malformed_zero",
        "on_refusal",
    ] {
        let bridge = Bridge::new(METERS[1].1, fault, "0.0001");
        let mut engine = Engine::start(&bridge.dir);
        engine.connect("YOKOGAWA_GS210", METERS[1].0);
        let output = path(fault);
        engine.send(json!({"command":"run","config":config("YOKOGAWA_GS210", METERS[1].0, iv_core::Mode::Iv, 0.0, 1),"path":output}));
        let state = engine.wait(|s| s["phase"] == "Failed");
        assert_eq!(state["output"], "Off", "{fault}: {state}");
        let commands = bridge.commands();
        let on = format!("WRITE 3 {}", hex(":OUTP ON\n"));
        if fault != "on_refusal" {
            assert!(
                !commands.contains(&on),
                "{fault} enabled output: {commands:?}"
            );
        }
        assert!(
            commands.contains(&format!("WRITE 3 {}", hex(":OUTP OFF\n"))),
            "{fault} did not force output off"
        );
        cleanup(&output);
    }
}

#[test]
fn all_registered_physical_meters_follow_the_same_safe_sweep_contract() {
    for mode in [iv_core::Mode::Iv, iv_core::Mode::Vi] {
        for &(model, identity) in METERS {
            let bridge = Bridge::new(identity, "", "0.0001");
            let mut engine = Engine::start(&bridge.dir);
            engine.connect("YOKOGAWA_GS210", model);
            let output = path(&format!("{model}-{}", mode.as_str()));
            let mut run = config("YOKOGAWA_GS210", model, mode, 0.0, 1);
            if mode == iv_core::Mode::Vi {
                run.source_limit = 0.01;
            }
            engine.send(json!({"command":"run","config":run,"path":output}));
            let state = engine.wait(|s| s["phase"] == "Completed" || s["phase"] == "Failed");
            assert_eq!(state["output"], "Off", "{model}/{:?}: {state}", mode);
            assert_eq!(state["points"], 1, "{model}/{:?}: {state}", mode);
            assert_eq!(std::fs::read_to_string(&output).unwrap().lines().count(), 2);
            cleanup(&output);
        }
    }
}

#[test]
fn legacy_7651_uses_panel_range_codes_in_both_modes_and_stops_safely() {
    for mode in [iv_core::Mode::Iv, iv_core::Mode::Vi] {
        let bridge = LegacyBridge::new(METERS[1].1, "0.1");
        let mut engine = Engine::start(&bridge.dir);
        engine.connect("YOKOGAWA_7651", METERS[1].0);
        let output = path(&format!("7651-{}", mode.as_str()));
        engine.send(json!({"command":"run","config":config("YOKOGAWA_7651", METERS[1].0, mode, 0.0, 2),"path":output}));
        let state = engine.wait(|s| s["phase"] == "Compliance stop");
        assert_eq!(state["output"], "Off", "{mode:?}: {state}");
        assert_legacy_immediate_off(&bridge.commands());
        let expected_range = if mode == iv_core::Mode::Iv {
            "R2;E\n"
        } else {
            "R4;E\n"
        };
        assert!(
            bridge
                .commands()
                .iter()
                .any(|line| line == &format!("WRITE 3 {}", hex(expected_range))),
            "{mode:?} must use the 7651 panel range code"
        );
        cleanup(&output);
    }
}

#[test]
fn native_direct_usb_uses_the_same_safe_contract_for_gs210_and_34411a() {
    for (source_resource, meter_resource) in [
        ("USB0::0x0B21::0x0039::SOURCE::INSTR", "GPIB0::7::INSTR"),
        ("GPIB0::3::INSTR", "USB0::0x0957::0x0A07::METER::INSTR"),
    ] {
        let bridge = Bridge::new(METERS[1].1, "", "0.0001");
        let mut engine = Engine::start(&bridge.dir);
        engine.connect_resources(
            "YOKOGAWA_GS210",
            METERS[1].0,
            source_resource,
            meter_resource,
        );
        let output = path("native-usb");
        engine.send(json!({"command":"run","config":config("YOKOGAWA_GS210", METERS[1].0, iv_core::Mode::Iv, 0.0, 1),"path":output}));
        let state = engine.wait(|s| s["phase"] == "Completed");
        assert_eq!(
            state["output"], "Off",
            "{source_resource}/{meter_resource}: {state}"
        );
        assert_eq!(state["points"], 1);
        cleanup(&output);
    }
}

#[test]
fn failed_readings_retain_raw_evidence_after_immediate_output_off() {
    for settle in [0.0, 0.20] {
        for reading in ["broken reading", "NaN", "9.9e37"] {
            let bridge = Bridge::new(METERS[1].1, "", reading);
            let mut engine = Engine::start(&bridge.dir);
            engine.connect("YOKOGAWA_GS210", METERS[1].0);
            let output = path("failed-reading-evidence");
            engine.send(json!({"command":"run","config":config("YOKOGAWA_GS210", METERS[1].0, iv_core::Mode::Iv, settle, 1),"path":output}));
            let state = engine.wait(|s| s["phase"] == "Failed");
            assert_eq!(state["output"], "Off", "{state}");
            assert_eq!(state["points"], 0);
            assert_immediate_off(&bridge.commands());
            let raw = std::fs::read_to_string(output.with_extension("raw.csv")).unwrap();
            assert!(
                raw.contains(reading),
                "missing exact failed reply {reading}: {raw}"
            );
            assert!(raw.contains("meter_reading"));
            let mut reader = csv::Reader::from_path(output.with_extension("raw.csv")).unwrap();
            let header = reader.headers().unwrap().clone();
            let event = header.iter().position(|v| v == "event_type").unwrap();
            let result = header.iter().position(|v| v == "result").unwrap();
            let error = header.iter().position(|v| v == "error").unwrap();
            let row = reader
                .records()
                .map(Result::unwrap)
                .find(|row| &row[event] == "meter_reading")
                .unwrap();
            assert_eq!(&row[result], "error");
            assert!(!row[error].is_empty());
            cleanup(&output);
        }
    }
}

#[test]
fn failed_protection_confirmation_still_preserves_the_triggering_reply() {
    for settle in [0.0, 0.20] {
        let bridge = Bridge::new(METERS[1].1, "", "0.123456789");
        // Refuse OFF only after ON: initial configuration stays valid, while
        // post-READ protection cannot confirm OFF. No real device is involved.
        for name in ["kohdalab-gpib-helper", "kohdalab-usbtmc-helper"] {
            let file = bridge.dir.join(name);
            let script = std::fs::read_to_string(&file).unwrap();
            let off_assignment = format!(
                "3:{}|0:{}) output='{}'",
                hex(":OUTP OFF\n"),
                hex(":OUTP OFF\n"),
                hex("0")
            );
            assert!(script.contains(&off_assignment));
            let marker = bridge.dir.join("output-was-enabled");
            let script = script.replace(
                &off_assignment,
                &format!("3:{}|0:{}) :", hex(":OUTP OFF\n"), hex(":OUTP OFF\n")),
            );
            let on_assignment = format!("output='{}' ;;", hex("1"));
            let script = script.replace(
                &on_assignment,
                &format!("output='{}'; touch '{}' ;;", hex("1"), marker.display()),
            );
            // Recovery must also fail instead of inventing a successful OFF
            // from a restarted fake helper's default state.
            let script = script.replace(" response=''\n", &format!(" response=''\n if [ -f '{}' ] && [ \"$payload\" = '{}' ]; then printf 'KIV\\tERR\\t{}\\n'; continue; fi\n", marker.display(), hex("*IDN?\n"), hex("injected recovery identity failure")));
            std::fs::write(file, script).unwrap();
        }
        let mut engine = Engine::start(&bridge.dir);
        engine.connect("YOKOGAWA_GS210", METERS[1].0);
        let output = path("failed-off-evidence");
        engine.send(json!({"command":"run","config":config("YOKOGAWA_GS210", METERS[1].0, iv_core::Mode::Iv, settle, 1),"path":output}));
        let state = engine.wait(|s| s["phase"] == "Failed");
        assert!(!state["error"].as_str().unwrap().is_empty(), "{state}");
        assert_immediate_off(&bridge.commands());
        let raw = std::fs::read_to_string(output.with_extension("raw.csv")).unwrap();
        assert!(raw.contains("0.123456789"), "trigger reply lost: {raw}");
        cleanup(&output);
    }
}

#[test]
fn read_transport_errors_retain_error_evidence_and_stop_before_further_queries() {
    for settle in [0.0, 0.20] {
        let bridge = Bridge::new(METERS[1].1, "", "0.0001");
        for name in ["kohdalab-gpib-helper", "kohdalab-usbtmc-helper"] {
            let file = bridge.dir.join(name);
            let script = std::fs::read_to_string(&file).unwrap();
            let injection = format!(
                " response=''\n if [ \"$payload\" = '{}' ]; then printf 'KIV\\tERR\\t{}\\n'; continue; fi\n",
                hex("READ?\n"),
                hex("injected meter read failure")
            );
            std::fs::write(file, script.replace(" response=''\n", &injection)).unwrap();
        }
        let mut engine = Engine::start(&bridge.dir);
        engine.connect("YOKOGAWA_GS210", METERS[1].0);
        let output = path("read-transport-evidence");
        engine.send(json!({"command":"run","config":config("YOKOGAWA_GS210", METERS[1].0, iv_core::Mode::Iv, settle, 1),"path":output}));
        let state = engine.wait(|s| s["phase"] == "Failed");
        assert_eq!(state["output"], "Off", "{state}");
        assert_eq!(state["points"], 0);
        let commands = bridge.commands();
        let read = format!("QUERY 7 {}", hex("READ?\n"));
        let at = commands.iter().rposition(|line| line == &read).unwrap();
        let mut next = at + 1;
        // A failed shared-GPIB session must be replaced and its exact source
        // identity verified before it is safe to address the recovered device.
        if commands.get(next) == Some(&format!("QUERY 3 {}", hex("*IDN?\n"))) {
            next += 1;
        }
        assert_eq!(
            commands.get(next),
            Some(&format!("WRITE 3 {}", hex(":OUTP OFF\n"))),
            "{commands:?}"
        );
        assert!(
            !commands[at + 1..]
                .iter()
                .any(|line| line.starts_with("QUERY 7 ")),
            "No meter/range I/O may precede the protected stop: {commands:?}"
        );
        let raw = std::fs::read_to_string(output.with_extension("raw.csv")).unwrap();
        assert!(
            raw.contains("injected meter read failure"),
            "error lost: {raw}"
        );
        assert!(raw.contains("meter_reading"));
        cleanup(&output);
    }
}

fn assert_source_status_stop(commands: &[String]) {
    let status = format!("QUERY 3 {}", hex(":STAT:COND?\n"));
    let at = commands
        .iter()
        .rposition(|line| line == &status)
        .expect("source condition checked");
    assert_eq!(
        commands.get(at + 1),
        Some(&format!("WRITE 3 {}", hex(":OUTP OFF\n"))),
        "Source OFF must be next after H/L: {commands:?}"
    );
    for line in &commands[at + 1..] {
        if line.starts_with("WRITE 3 ") {
            assert!(
                line == &format!("WRITE 3 {}", hex(":OUTP OFF\n"))
                    || line == &format!("WRITE 3 {}", hex(":SOUR:LEV:FIX 0\n")),
                "No live cleanup ramp after a hardware stop: {commands:?}"
            );
        }
    }
}

#[test]
fn source_h_l_stops_even_when_the_external_four_terminal_voltage_is_below_meter_limit() {
    for flag in ["2048", "1024"] {
        for (mode, reading) in [(iv_core::Mode::Vi, "0.1816"), (iv_core::Mode::Iv, "0.0001")] {
            let bridge = Bridge::new(METERS[1].1, "", reading);
            std::fs::write(bridge.dir.join("source-condition-after-read"), hex(flag)).unwrap();
            std::fs::write(bridge.dir.join("use-real-source-range"), "").unwrap();
            let mut engine = Engine::start(&bridge.dir);
            engine.connect("YOKOGAWA_GS210", METERS[1].0);
            let output = path("four-wire-source-limit");
            let mut run = config("YOKOGAWA_GS210", METERS[1].0, mode, 0.0, 3);
            run.compliance = if mode == iv_core::Mode::Vi { 1.0 } else { 0.01 };
            run.source_compliance = Some(if mode == iv_core::Mode::Vi { 5.0 } else { 0.02 });
            engine.send(json!({"command":"run","config":run,"path":output}));
            let state =
                engine.wait(|s| s["phase"] == "Source limit stop" || s["phase"] == "Failed");
            assert_eq!(state["phase"], "Source limit stop", "{state}");
            assert_eq!(state["protection_alert"]["kind"], "source_compliance");
            assert_eq!(
                state["protection_alert"]["limit"],
                run.source_compliance.unwrap()
            );
            assert_eq!(
                state["protection_alert"]["unit"],
                if mode == iv_core::Mode::Vi { "V" } else { "A" }
            );
            assert!(state["protection_alert"]["observed"].is_null());
            assert_eq!(state["output"], "Off");
            assert_eq!(state["points"], 0);
            assert!(state["error"].as_str().unwrap().is_empty());
            assert_source_status_stop(&bridge.commands());
            assert_eq!(std::fs::read_to_string(&output).unwrap().lines().count(), 1);
            let meta: Value =
                serde_json::from_slice(&std::fs::read(output.with_extension("run.json")).unwrap())
                    .unwrap();
            assert_eq!(
                meta["source_protection_stop"]["condition"]["register"],
                flag.parse::<u32>().unwrap()
            );
            assert!(meta["file_hash_error"].is_null());
            let raw = std::fs::read_to_string(output.with_extension("raw.csv")).unwrap();
            assert!(raw.contains(reading));
            assert!(raw.contains("source_protection_stop"));
            cleanup(&output);
        }
    }
}

#[test]
fn source_limits_are_detected_during_settling_before_any_new_conversion() {
    let bridge = Bridge::new(METERS[1].1, "", "0.1816");
    let mut engine = Engine::start(&bridge.dir);
    engine.connect("YOKOGAWA_GS210", METERS[1].0);
    std::fs::write(bridge.dir.join("source-condition"), hex("2048")).unwrap();
    let output = path("source-limit-during-settle");
    let mut run = config("YOKOGAWA_GS210", METERS[1].0, iv_core::Mode::Vi, 0.4, 1);
    run.compliance = 1.0;
    run.source_compliance = Some(5.0);
    engine.send(json!({"command":"run","config":run,"path":output}));
    let state = engine.wait(|s| s["phase"] == "Source limit stop" || s["phase"] == "Failed");
    assert_eq!(state["phase"], "Source limit stop", "{state}");
    assert_eq!(state["points"], 0);
    assert_eq!(state["output"], "Off");
    assert_source_status_stop(&bridge.commands());
    let raw = std::fs::read_to_string(output.with_extension("raw.csv")).unwrap();
    assert!(!raw.contains(",meter_reading,"));
    cleanup(&output);
}

#[test]
fn unknown_source_status_and_mismatched_hardware_settings_fail_closed() {
    for fault in [
        "source-condition",
        "source-condition-error",
        "source-protection-mismatch",
    ] {
        let bridge = Bridge::new(METERS[1].1, "", "0.0001");
        let mut engine = Engine::start(&bridge.dir);
        engine.connect("YOKOGAWA_GS210", METERS[1].0);
        std::fs::write(bridge.dir.join(fault), hex("broken")).unwrap();
        let output = path(fault);
        engine.send(json!({"command":"run","config":config("YOKOGAWA_GS210",METERS[1].0,iv_core::Mode::Iv,0.0,1),"path":output}));
        let state = engine.wait(|s| s["phase"] == "Failed");
        assert_eq!(state["output"], "Off", "{state}");
        assert_eq!(state["points"], 0);
        assert!(!state["error"].as_str().unwrap().is_empty());
        if fault == "source-protection-mismatch" {
            assert!(
                !bridge
                    .commands()
                    .iter()
                    .any(|c| c == &format!("WRITE 3 {}", hex(":OUTP ON\n")))
            );
        }
        cleanup(&output);
    }
}

#[test]
fn zero_wait_ramps_check_source_protection_before_the_next_nonzero_output() {
    let bridge = Bridge::new(METERS[1].1, "", "0.1816");
    let mut engine = Engine::start(&bridge.dir);
    engine.connect("YOKOGAWA_GS210", METERS[1].0);
    std::fs::write(bridge.dir.join("source-condition"), hex("2048")).unwrap();
    let output = path("source-limit-zero-wait-ramp");
    let mut run = config("YOKOGAWA_GS210", METERS[1].0, iv_core::Mode::Vi, 0.0, 1);
    run.start = -0.0004;
    run.stop = 0.0004;
    run.step = 0.00004;
    run.source_compliance = Some(5.0);
    run.compliance = 1.0;
    engine.send(json!({"command":"run","config":run,"path":output}));
    let state = engine.wait(|s| s["phase"] == "Source limit stop" || s["phase"] == "Failed");
    assert_eq!(state["phase"], "Source limit stop", "{state}");
    assert_eq!(state["output"], "Off");
    assert_eq!(state["points"], 0);
    let commands = bridge.commands();
    assert_source_status_stop(&commands);
    let status = commands
        .iter()
        .position(|c| c == &format!("QUERY 3 {}", hex(":STAT:COND?\n")))
        .unwrap();
    let writes_before_check = commands[..status]
        .iter()
        .filter_map(|c| c.strip_prefix(&format!("WRITE 3 {}", hex(":SOUR:LEV:FIX "))))
        .map(|encoded| {
            let bytes: Vec<u8> = encoded
                .as_bytes()
                .as_chunks::<2>()
                .0
                .iter()
                .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
                .collect();
            String::from_utf8(bytes)
                .unwrap()
                .trim()
                .parse::<f64>()
                .unwrap()
        })
        .filter(|level| *level != 0.0)
        .count();
    assert!(
        writes_before_check == 1,
        "exactly the first ramp level should precede the status check: {commands:?}"
    );
    cleanup(&output);
}

#[test]
fn emergency_source_status_is_retained_in_run_metadata() {
    let bridge = Bridge::new(METERS[1].1, "", "0.1816");
    let mut engine = Engine::start(&bridge.dir);
    engine.connect("YOKOGAWA_GS210", METERS[1].0);
    std::fs::write(bridge.dir.join("source-condition"), hex("8192")).unwrap();
    let output = path("source-emergency");
    engine.send(json!({"command":"run","config":config("YOKOGAWA_GS210",METERS[1].0,iv_core::Mode::Vi,0.0,1),"path":output}));
    let state = engine.wait(|s| s["phase"] == "Failed");
    assert_eq!(state["output"], "Off");
    assert!(state["error"].as_str().unwrap().contains("emergency"));
    let meta: Value =
        serde_json::from_slice(&std::fs::read(output.with_extension("run.json")).unwrap()).unwrap();
    assert_eq!(
        meta["source_protection_stop"]["condition"]["register"],
        8192
    );
    assert!(
        meta["source_protection_stop"]["failure"]
            .as_str()
            .unwrap()
            .contains("emergency")
    );
    cleanup(&output);
}

#[test]
fn a_new_invalid_run_does_not_reuse_the_previous_protection_alert() {
    let bridge = Bridge::new(METERS[1].1, "", "0.1816");
    let mut engine = Engine::start(&bridge.dir);
    engine.connect("YOKOGAWA_GS210", METERS[1].0);
    std::fs::write(bridge.dir.join("source-condition"), hex("2048")).unwrap();
    let output = path("previous-protection-alert");
    engine.send(json!({"command":"run","config":config("YOKOGAWA_GS210",METERS[1].0,iv_core::Mode::Vi,0.0,1),"path":output}));
    let state = engine.wait(|s| s["phase"] == "Source limit stop");
    assert_eq!(state["protection_alert"]["kind"], "source_compliance");
    let mut invalid = config("YOKOGAWA_GS210", METERS[1].0, iv_core::Mode::Vi, 0.0, 1);
    invalid.step = 0.0;
    engine.send(json!({"command":"run","config":invalid,"path":path("invalid-retry")}));
    let state = engine.wait(|s| s["phase"] == "Failed");
    assert!(state["protection_alert"].is_null(), "{state}");
    cleanup(&output);
}
