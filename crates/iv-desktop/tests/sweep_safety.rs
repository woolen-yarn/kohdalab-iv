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
        let mut last = Value::Null;
        loop {
            assert!(Instant::now() < end, "engine timed out; last state: {last}");
            self.send(json!({"command":"heartbeat"}));
            if let Ok(event) = self.events.recv_timeout(Duration::from_millis(100))
                && event["event"] == "state"
            {
                let state = event["data"].clone();
                last = state.clone();
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
        let script = if fault == "slow_status" {
            script.replace(
                "QUERY:3a535441543a434f4e443f0a)",
                "QUERY:3a535441543a434f4e443f0a)\n   sleep 0.12",
            )
        } else {
            script
        };
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

/// Teach the existing fake meter the manual-range query contract without
/// changing its default Auto fixture behavior. Optional marker injection makes
/// the first post-READ range query report a different range.
fn install_fixed_meter_responses(
    bridge: &Bridge,
    current_range: &str,
    voltage_range: &str,
    change_range_after_read: bool,
    auto_reply: &str,
) {
    let marker = bridge.dir.join("meter-range-change-after-read");
    let current_auto = hex("SENS:CURR:DC:RANG:AUTO?\n");
    let voltage_auto = hex("SENS:VOLT:DC:RANG:AUTO?\n");
    let current_query = hex("SENS:CURR:DC:RANG?\n");
    let voltage_query = hex("SENS:VOLT:DC:RANG?\n");
    let read = hex("READ?\n");
    for name in ["kohdalab-gpib-helper", "kohdalab-usbtmc-helper"] {
        let path = bridge.dir.join(name);
        let script = std::fs::read_to_string(&path).unwrap();
        let original = format!(
            "7:{current_query}|0:{current_query}|7:{voltage_query}|0:{voltage_query}) response='{}'",
            hex("0.01")
        );
        assert!(
            script.contains(&original),
            "missing fake meter range branch"
        );
        let changed_current = hex("0.01");
        let changed_voltage = hex("10");
        let fixed_branch = format!(
            "7:{current_auto}|0:{current_auto}|7:{voltage_auto}|0:{voltage_auto}) response='{auto_reply}' ;; \\\n7:{current_query}|0:{current_query}) response='{current_range}' ;; \\\n7:{voltage_query}|0:{voltage_query}) response='{voltage_range}'"
        );
        let fixed_branch = if change_range_after_read {
            format!(
                "7:{current_auto}|0:{current_auto}|7:{voltage_auto}|0:{voltage_auto}) response='{auto_reply}' ;; \\\n7:{current_query}|0:{current_query}) if [ -f '{}' ]; then response='{changed_current}'; else response='{current_range}'; fi ;; \\\n7:{voltage_query}|0:{voltage_query}) if [ -f '{}' ]; then response='{changed_voltage}'; else response='{voltage_range}'; fi",
                marker.display(),
                marker.display(),
            )
        } else {
            fixed_branch
        };
        let script = script.replace(&original, &fixed_branch);
        let script = if change_range_after_read {
            let original_read = format!("7:{read}|0:{read}) response='{}'", hex("0.0001"));
            assert!(script.contains(&original_read), "missing fake READ branch");
            script.replace(
                &original_read,
                &format!(
                    "7:{read}|0:{read}) response='{}'; touch '{}'",
                    hex("0.0001"),
                    marker.display()
                ),
            )
        } else {
            script
        };
        std::fs::write(path, script).unwrap();
    }
}

fn fixed_range_config(mode: iv_core::Mode) -> iv_core::RunConfig {
    let mut run = config("YOKOGAWA_GS210", METERS[1].0, mode, 0.0, 1);
    run.start = -0.001;
    run.stop = 0.001;
    run.step = 0.001;
    run.pattern = iv_core::Pattern::RoundTrip;
    run.meter_current_range = Some(0.001);
    run.meter_voltage_range = Some(1.0);
    if mode == iv_core::Mode::Vi {
        run.source_limit = 0.01;
        run.compliance = 1.0;
    }
    run
}

/// Per-test fault: once an energized meter conversion occurs, subsequent
/// current-range queries report a different valid range. The normal fixture
/// and its Auto tests remain unchanged.
fn install_fixed_current_range_drift_after_read(bridge: &Bridge) {
    let arm = bridge.dir.join("arm-fixed-current-range-drift");
    let tripped = bridge.dir.join("fixed-current-range-drifted");
    let read = hex("READ?\n");
    let current_range = hex("SENS:CURR:DC:RANG?\n");
    let current_reply = hex("0.001");
    let changed_reply = hex("0.01");
    for name in ["kohdalab-gpib-helper", "kohdalab-usbtmc-helper"] {
        let path = bridge.dir.join(name);
        let script = std::fs::read_to_string(&path).unwrap();
        let injection = format!(
            " response=''\n if [ \"$operation\" = QUERY ] && [ \"$payload\" = '{read}' ] && [ \"$output\" = '{}' ] && [ -f '{}' ]; then touch '{}'; fi\n if [ \"$operation\" = QUERY ] && {{ [ \"$address\" = 7 ] || [ \"$address\" = 0 ]; }} && [ \"$payload\" = '{current_range}' ]; then if [ -f '{}' ]; then printf 'KIV\\tOK\\t{changed_reply}\\n'; else printf 'KIV\\tOK\\t{current_reply}\\n'; fi; continue; fi\n",
            hex("1"),
            arm.display(),
            tripped.display(),
            tripped.display(),
        );
        assert!(
            script.contains(" response=''\n"),
            "missing fake response hook"
        );
        std::fs::write(path, script.replacen(" response=''\n", &injection, 1)).unwrap();
    }
}

fn trigger_fixed_current_range_drift(bridge: &Bridge) {
    std::fs::write(bridge.dir.join("arm-fixed-current-range-drift"), "").unwrap();
}

fn assert_range_drift_stops_before_follow_up_queries(commands: &[String]) {
    let range = format!("QUERY 7 {}", hex("SENS:CURR:DC:RANG?\n"));
    let mismatch = commands
        .iter()
        .rposition(|line| line == &range)
        .expect("fixed current range query");
    assert_eq!(
        commands.get(mismatch + 1),
        Some(&format!("WRITE 3 {}", hex(":OUTP OFF\n"))),
        "range mismatch must dispatch source OFF before resolution/source queries: {commands:?}"
    );
}

fn assert_immediate_off(commands: &[String]) {
    let read = format!("QUERY 7 {}", hex("READ?\n"));
    let off = format!("WRITE 3 {}", hex(":OUTP OFF\n"));
    let on = format!("WRITE 3 {}", hex(":OUTP ON\n"));
    let enabled = commands
        .iter()
        .position(|line| line == &on)
        .expect("source ON");
    // The fixtures return an over-limit value on the first energized read.
    // Later idle reads after cleanup are not new triggering conversions.
    let at = enabled
        + 1
        + commands[enabled + 1..]
            .iter()
            .position(|line| line == &read)
            .expect("first energized READ?");
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
    let on = format!("WRITE 3 {}", hex("O1;E\n"));
    let enabled = commands
        .iter()
        .position(|line| line == &on)
        .expect("source ON");
    // The fixtures return an over-limit value on the first energized read.
    // Later idle reads after cleanup are not new triggering conversions.
    let at = enabled
        + 1
        + commands[enabled + 1..]
            .iter()
            .position(|line| line == &read)
            .expect("first energized READ?");
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
fn immediate_off_assertions_ignore_idle_reads_but_reject_delayed_shutdown() {
    for (on, off, assert_off) in [
        (
            ":OUTP ON\n",
            ":OUTP OFF\n",
            assert_immediate_off as fn(&[String]),
        ),
        (
            "O1;E\n",
            "O0;E\n",
            assert_legacy_immediate_off as fn(&[String]),
        ),
    ] {
        let on = format!("WRITE 3 {}", hex(on));
        let off = format!("WRITE 3 {}", hex(off));
        let read = format!("QUERY 7 {}", hex("READ?\n"));
        assert_off(&[
            read.clone(),
            on.clone(),
            read.clone(),
            off.clone(),
            read.clone(),
        ]);
        let delayed = [
            on,
            read.clone(),
            "QUERY 3 unrelated".into(),
            off.clone(),
            read,
            off,
        ];
        assert!(std::panic::catch_unwind(|| assert_off(&delayed)).is_err());
    }
}

#[test]
fn live_overlimit_stops_before_csv_and_before_followup_io() {
    for fault in ["", "slow_status"] {
        let bridge = Bridge::new(METERS[1].1, fault, "0.1");
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

// Manual meter range regression tests. These deliberately use the existing
// native fake bridge because the safety property is the real engine's command
// order, not an abstract transport mock.

#[test]
fn auto_resistance_transition_stops_before_follow_up_queries_and_csv_acceptance() {
    for settle in [0.0, 0.25] {
        let bridge = Bridge::new(METERS[1].1, "", "0.0001");
        install_fixed_meter_responses(&bridge, &hex("0.001"), &hex("1"), false, &hex("1"));
        install_fixed_current_range_drift_after_read(&bridge);
        let mut engine = Engine::start(&bridge.dir);
        engine.connect("YOKOGAWA_GS210", METERS[1].0);
        let output = path("auto-resistance-transition");
        let mut run = fixed_range_config(iv_core::Mode::Iv);
        run.meter_current_range = None;
        run.start = 0.;
        run.stop = 0.;
        run.pattern = iv_core::Pattern::Linear;
        run.repeat = 2;
        run.start_settle_s = settle;
        run.settle_s = settle;
        trigger_fixed_current_range_drift(&bridge);
        engine.send(json!({"command":"run","config":run,"path":output}));
        let state = engine.wait(|s| s["phase"] == "Failed");
        assert_eq!(state["output"], "Off", "{state}");
        assert_eq!(state["points"], 0, "{state}");
        assert!(
            state["error"]
                .as_str()
                .unwrap()
                .contains("Meter range transition detected"),
            "{state}"
        );
        assert_range_drift_stops_before_follow_up_queries(&bridge.commands());
        assert_eq!(
            csv::Reader::from_path(&output).unwrap().records().count(),
            0
        );
        let raw = std::fs::read_to_string(output.with_extension("raw.csv")).unwrap();
        assert!(raw.contains("meter_reading"));
        assert!(raw.contains("0.0001"));
        cleanup(&output);
    }
}

#[test]
fn fixed_meter_ranges_hold_across_forward_and_backward_iv_and_vi_sweeps() {
    for (mode, expected_auto_off, expected_range) in [
        (
            iv_core::Mode::Iv,
            "SENS:CURR:DC:RANG:AUTO OFF\n",
            "SENS:CURR:DC:RANG 1.000000000000e-3\n",
        ),
        (
            iv_core::Mode::Vi,
            "SENS:VOLT:DC:RANG:AUTO OFF\n",
            "SENS:VOLT:DC:RANG 1.000000000000e0\n",
        ),
    ] {
        let bridge = Bridge::new(METERS[1].1, "", "0.0001");
        install_fixed_meter_responses(&bridge, &hex("0.001"), &hex("1"), false, &hex("0"));
        let mut engine = Engine::start(&bridge.dir);
        engine.connect("YOKOGAWA_GS210", METERS[1].0);
        let output = path(&format!("fixed-range-{}", mode.as_str()));
        engine.send(json!({
            "command":"run", "config":fixed_range_config(mode), "path":output
        }));
        let state = engine.wait(|s| s["phase"] == "Completed" || s["phase"] == "Failed");
        assert_eq!(state["phase"], "Completed", "{mode:?}: {state}");
        assert_eq!(state["output"], "Off", "{mode:?}: {state}");
        assert_eq!(state["points"], 5, "{mode:?}: {state}");
        let commands = bridge.commands();
        let auto_off = format!("WRITE 7 {}", hex(expected_auto_off));
        let auto_off_at = commands
            .iter()
            .position(|line| line == &auto_off)
            .expect("fixed meter setup must disable autorange");
        assert!(
            commands.contains(&auto_off),
            "{mode:?} did not disable meter autorange: {commands:?}"
        );
        assert!(
            commands.contains(&format!("WRITE 7 {}", hex(expected_range))),
            "{mode:?} did not select its requested fixed range: {commands:?}"
        );
        assert!(
            !commands[auto_off_at..].contains(&format!(
                "WRITE 7 {}",
                hex(&expected_auto_off.replace(" OFF", " ON"))
            )),
            "{mode:?} re-enabled meter autorange during the fixed run: {commands:?}"
        );
        cleanup(&output);
    }
}

#[test]
fn fixed_meter_setup_mismatch_blocks_source_on() {
    for (mode, auto_reply, expected_range) in [
        (iv_core::Mode::Iv, "1", "0.001"),
        (iv_core::Mode::Vi, "0", "10"),
    ] {
        let bridge = Bridge::new(METERS[1].1, "", "0.0001");
        install_fixed_meter_responses(
            &bridge,
            &hex(expected_range),
            &hex(expected_range),
            false,
            &hex(auto_reply),
        );
        let mut engine = Engine::start(&bridge.dir);
        engine.connect("YOKOGAWA_GS210", METERS[1].0);
        let output = path(&format!("fixed-range-setup-mismatch-{}", mode.as_str()));
        engine.send(json!({
            "command":"run", "config":fixed_range_config(mode), "path":output
        }));
        let state = engine.wait(|s| s["phase"] == "Failed");
        assert_eq!(state["output"], "Off", "{mode:?}: {state}");
        assert_eq!(state["points"], 0, "{mode:?}: {state}");
        let commands = bridge.commands();
        assert!(
            !commands.contains(&format!("WRITE 3 {}", hex(":OUTP ON\n"))),
            "range setup mismatch enabled source output: {commands:?}"
        );
        assert!(
            commands.contains(&format!("WRITE 3 {}", hex(":OUTP OFF\n"))),
            "failed range setup did not force source off: {commands:?}"
        );
        cleanup(&output);
    }
}

#[test]
fn fixed_meter_range_change_after_read_protects_before_saving_invalid_point() {
    let bridge = Bridge::new(METERS[1].1, "", "0.0001");
    install_fixed_meter_responses(&bridge, &hex("0.001"), &hex("1"), true, &hex("0"));
    let mut engine = Engine::start(&bridge.dir);
    engine.connect("YOKOGAWA_GS210", METERS[1].0);
    let output = path("fixed-range-change-after-read");
    let mut run = fixed_range_config(iv_core::Mode::Iv);
    run.start = 0.0;
    run.stop = 0.0;
    run.pattern = iv_core::Pattern::Linear;
    engine.send(json!({"command":"run", "config":run, "path":output}));
    let state = engine.wait(|s| s["phase"] == "Failed");
    assert_eq!(state["output"], "Off", "{state}");
    assert_eq!(state["points"], 0, "{state}");
    let commands = bridge.commands();
    let range_query = format!("QUERY 7 {}", hex("SENS:CURR:DC:RANG?\n"));
    let last_range_query = commands
        .iter()
        .rposition(|line| line == &range_query)
        .expect("fixed current range was re-checked after READ?");
    assert_eq!(
        commands.get(last_range_query + 1),
        Some(&format!("WRITE 3 {}", hex(":OUTP OFF\n"))),
        "a post-read range change must immediately protect output: {commands:?}"
    );
    assert_eq!(
        std::fs::read_to_string(&output).unwrap().lines().count(),
        1,
        "range-changed conversion must not become a main CSV point"
    );
    cleanup(&output);
}

#[test]
fn fixed_meter_range_keeps_existing_overrange_stop_behavior() {
    let bridge = Bridge::new(METERS[1].1, "", "0.02");
    install_fixed_meter_responses(&bridge, &hex("0.001"), &hex("1"), false, &hex("0"));
    let mut engine = Engine::start(&bridge.dir);
    engine.connect("YOKOGAWA_GS210", METERS[1].0);
    let output = path("fixed-range-overrange-stop");
    let mut run = fixed_range_config(iv_core::Mode::Iv);
    run.start = 0.0;
    run.stop = 0.0;
    run.pattern = iv_core::Pattern::Linear;
    run.compliance = 0.01;
    engine.send(json!({"command":"run", "config":run, "path":output}));
    let state = engine.wait(|s| s["phase"] == "Compliance stop" || s["phase"] == "Failed");
    assert_eq!(state["phase"], "Compliance stop", "{state}");
    assert_eq!(state["output"], "Off", "{state}");
    assert_immediate_off(&bridge.commands());
    cleanup(&output);
}

#[test]
fn fixed_range_drift_during_positive_settling_stops_before_resolution_or_source_queries() {
    let bridge = Bridge::new(METERS[1].1, "", "0.0001");
    install_fixed_meter_responses(&bridge, &hex("0.001"), &hex("1"), false, &hex("0"));
    install_fixed_current_range_drift_after_read(&bridge);
    let mut engine = Engine::start(&bridge.dir);
    engine.connect("YOKOGAWA_GS210", METERS[1].0);
    let output = path("fixed-range-settle-drift");
    let mut run = fixed_range_config(iv_core::Mode::Iv);
    run.start = 0.0;
    run.stop = 0.0;
    run.pattern = iv_core::Pattern::Linear;
    run.start_settle_s = 0.25;
    run.settle_s = 0.25;
    trigger_fixed_current_range_drift(&bridge);
    engine.send(json!({"command":"run", "config":run, "path":output}));
    let state = engine.wait(|s| s["phase"] == "Failed");
    assert_eq!(state["output"], "Off", "{state}");
    assert_eq!(state["points"], 0, "{state}");
    let commands = bridge.commands();
    assert_range_drift_stops_before_follow_up_queries(&commands);
    assert_eq!(
        std::fs::read_to_string(&output).unwrap().lines().count(),
        1,
        "settling conversion with range drift must not create a main CSV row"
    );
    cleanup(&output);
}

#[test]
fn auto_resistance_transition_in_live_dc_stops_before_follow_up_queries() {
    let bridge = Bridge::new(METERS[1].1, "", "0.0001");
    install_fixed_meter_responses(&bridge, &hex("0.001"), &hex("1"), false, &hex("1"));
    install_fixed_current_range_drift_after_read(&bridge);
    let mut engine = Engine::start(&bridge.dir);
    engine.connect("YOKOGAWA_GS210", METERS[1].0);
    engine.send(json!({"command":"set_source_dc","mode":"dc_iv","value":0.0,"compliance":0.2,"ramp_step":0.0001}));
    engine.wait(|s| s["output"] == "On" && !s["source_dc"].is_null());
    trigger_fixed_current_range_drift(&bridge);
    let state = engine.wait(|s| s["phase"] == "Failed");
    assert_eq!(state["output"], "Off", "{state}");
    assert!(
        state["error"]
            .as_str()
            .unwrap()
            .contains("Meter range transition detected"),
        "{state}"
    );
    assert_range_drift_stops_before_follow_up_queries(&bridge.commands());
}

#[test]
fn manual_dc_initializes_meter_baseline_before_first_output_on() {
    let bridge = Bridge::new(METERS[1].1, "", "0.0001");
    install_fixed_meter_responses(&bridge, &hex("0.001"), &hex("1"), false, &hex("1"));
    install_fixed_current_range_drift_after_read(&bridge);
    trigger_fixed_current_range_drift(&bridge);
    let mut engine = Engine::start(&bridge.dir);
    // Queue without waiting for a monitor tick after connect.
    engine.send(json!({"command":"connect","simulation":false,"source":"GPIB0::3::INSTR","meter":"GPIB0::7::INSTR","source_model":"YOKOGAWA_GS210","meter_model":METERS[1].0}));
    engine.send(json!({"command":"set_source_dc","mode":"dc_iv","value":0.0,"compliance":0.2,"ramp_step":0.0001}));
    let state = engine.wait(|s| s["phase"] == "Failed");
    assert_eq!(state["output"], "Off", "{state}");
    assert!(
        state["error"]
            .as_str()
            .unwrap()
            .contains("Meter range transition detected"),
        "{state}"
    );
    let commands = bridge.commands();
    let on = commands
        .iter()
        .position(|l| l == &format!("WRITE 3 {}", hex(":OUTP ON\n")))
        .unwrap();
    assert!(
        commands[..on]
            .iter()
            .any(|l| l == &format!("QUERY 7 {}", hex("SENS:CURR:DC:RANG?\n")))
    );
    assert_range_drift_stops_before_follow_up_queries(&commands);
}

#[test]
fn auto_guard_survives_held_dc_value_edits_without_reconfiguring_the_meter() {
    let bridge = Bridge::new(METERS[1].1, "", "0.0001");
    install_fixed_meter_responses(&bridge, &hex("0.001"), &hex("1"), false, &hex("1"));
    install_fixed_current_range_drift_after_read(&bridge);
    // Change the reported range at the source write, before a monitor cycle
    // can re-prime. This catches accidental CONF/baseline resets on DC edits.
    for name in ["kohdalab-gpib-helper", "kohdalab-usbtmc-helper"] {
        let path = bridge.dir.join(name);
        let script = std::fs::read_to_string(&path).unwrap();
        let script = script.replace(
            &format!("[ \"$payload\" = '{}' ]", hex("READ?\n")),
            "[ \"$payload\" = 'not-a-read-hook' ]",
        );
        let hook = format!(
            " response=''\n if [ \"$operation\" = WRITE ] && [ \"$output\" = '{}' ] && [ -f '{}' ]; then case \"$payload\" in {}*) touch '{}' ;; esac; fi\n",
            hex("1"),
            bridge.dir.join("arm-fixed-current-range-drift").display(),
            hex(":SOUR:LEV:FIX "),
            bridge.dir.join("fixed-current-range-drifted").display()
        );
        let hook = format!(
            "{hook} if [ \"$operation\" = QUERY ] && [ \"$address\" = 3 ] && [ \"$payload\" = '{}' ] && [ -f '{}' ]; then printf 'KIV\\tOK\\t{}\\n'; continue; fi\n",
            hex(":SOUR:LEV?\n"),
            bridge.dir.join("fixed-current-range-drifted").display(),
            hex("0.0001")
        );
        std::fs::write(path, script.replacen(" response=''\n", &hook, 1)).unwrap();
    }
    let mut engine = Engine::start(&bridge.dir);
    engine.connect("YOKOGAWA_GS210", METERS[1].0);
    engine.send(json!({"command":"set_source_dc","mode":"dc_iv","value":0.0,"compliance":0.2,"ramp_step":0.0001}));
    engine.wait(|s| s["output"] == "On" && s["live"]["meter_range"] == 0.001);
    let before = bridge.commands().len();
    trigger_fixed_current_range_drift(&bridge);
    engine.send(json!({"command":"set_source_dc","mode":"dc_iv","value":0.0001,"compliance":0.2,"ramp_step":0.0001}));
    let state = engine.wait(|s| s["phase"] == "Failed");
    assert_eq!(state["output"], "Off", "{state}");
    assert!(
        state["error"]
            .as_str()
            .unwrap()
            .contains("Meter range transition detected"),
        "{state}"
    );
    let commands = bridge.commands();
    assert!(
        !commands[before..]
            .iter()
            .any(|line| line.starts_with(&format!("WRITE 7 {}", hex("CONF:")))),
        "{commands:?}"
    );
    assert_range_drift_stops_before_follow_up_queries(&commands);
}

#[test]
fn fixed_range_drift_in_idle_monitor_stops_an_enabled_source_before_resolution_or_output_query() {
    let bridge = Bridge::new(METERS[1].1, "", "0.0001");
    install_fixed_meter_responses(&bridge, &hex("0.001"), &hex("1"), false, &hex("0"));
    install_fixed_current_range_drift_after_read(&bridge);
    let mut engine = Engine::start(&bridge.dir);
    engine.connect("YOKOGAWA_GS210", METERS[1].0);
    engine.send(json!({"command":"set_meter_ranges","meter_voltage_range":null,"meter_current_range":0.001}));
    engine.wait(|s| s["live"]["meter_range"] == 0.001 && s["error"] == "");
    engine.send(json!({"command":"set_source_dc","mode":"dc_iv","value":0.0,"compliance":0.2,"ramp_step":0.0001}));
    let state = engine.wait(|s| s["output"] == "On");
    assert_eq!(state["error"], "");
    // SetMeterRanges is asynchronous relative to the live monitor. Arm the
    // drift only after the range request has been accepted, then wait for its
    // next energized READ?/RANG? pair.
    trigger_fixed_current_range_drift(&bridge);
    let state = engine.wait(|s| {
        s["output"] == "Off"
            && s["error"]
                .as_str()
                .is_some_and(|error| error.contains("Meter range changed"))
    });
    assert_eq!(state["output"], "Off", "{state}");
    let commands = bridge.commands();
    assert_range_drift_stops_before_follow_up_queries(&commands);
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
fn native_mock_source_dc_verifies_on_then_explicit_off() {
    let bridge = Bridge::new(METERS[1].1, "", "0.0001");
    let mut engine = Engine::start(&bridge.dir);
    engine.connect("YOKOGAWA_GS210", METERS[1].0);
    engine.send(json!({
        "command":"set_source_dc", "mode":"dc_iv", "value":0.0,
        "compliance":0.2, "ramp_step":0.0001
    }));
    let state = engine.wait(|s| {
        (s["source_dc"]["value"] == 0.0 && s["output"] == "On")
            || !s["error"].as_str().unwrap_or_default().is_empty()
    });
    assert_eq!(
        state["error"],
        "",
        "{state} commands: {:?}",
        bridge.commands()
    );
    engine.send(json!({"command":"source_dc_off"}));
    let state = engine.wait(|s| s["output"] == "Off" && s["source_dc"].is_null());
    assert_eq!(state["error"], "", "{state}");
    let commands = bridge.commands();
    assert!(
        commands
            .iter()
            .any(|line| line == &format!("WRITE 3 {}", hex(":OUTP ON\n"))),
        "{commands:?}"
    );
    assert!(
        commands
            .iter()
            .any(|line| line == &format!("WRITE 3 {}", hex(":OUTP OFF\n"))),
        "{commands:?}"
    );
}

fn use_written_source_range_responses(bridge: &Bridge) {
    std::fs::write(bridge.dir.join("use-real-source-range"), "").unwrap();
}

/// The stock safety bridge deliberately keeps source readback at zero because
/// ordinary sweep tests do not need a final DC-level confirmation.  DC range
/// changes do, so opt in to a tiny stateful level readback for these tests.
fn use_written_source_level_responses(bridge: &Bridge) {
    let level_write = hex(":SOUR:LEV:FIX ");
    let level_query = hex(":SOUR:LEV?\n");
    let zero = hex("0");
    for name in ["kohdalab-gpib-helper", "kohdalab-usbtmc-helper"] {
        let path = bridge.dir.join(name);
        let script = std::fs::read_to_string(&path).unwrap();
        let hook = format!(
            " response=''\n case \"$operation:$address:$payload\" in WRITE:3:{level_write}*|WRITE:0:{level_write}*) source_level=${{payload#{level_write}}}; source_level=${{source_level%0a}} ;; esac\n if [ \"$operation\" = QUERY ] && {{ [ \"$address\" = 3 ] || [ \"$address\" = 0 ]; }} && [ \"$payload\" = '{level_query}' ]; then printf 'KIV\\tOK\\t%s\\n' \"${{source_level:-{zero}}}\"; continue; fi\n"
        );
        assert!(
            script.contains(" response=''\n"),
            "missing fake response hook"
        );
        std::fs::write(path, script.replacen(" response=''\n", &hook, 1)).unwrap();
    }
}

/// Make output-state verification fail only after a test arms the marker.  The
/// initial DC setup remains real, so this exercises the physical re-range
/// path rather than a connection/setup failure.
fn install_off_verification_refusal(bridge: &Bridge, address: u8) -> std::path::PathBuf {
    let marker = bridge.dir.join(format!("refuse-output-off-{address}"));
    let outq = hex(":OUTP?\n");
    let one = hex("1");
    for name in ["kohdalab-gpib-helper", "kohdalab-usbtmc-helper"] {
        let path = bridge.dir.join(name);
        let script = std::fs::read_to_string(&path).unwrap();
        let hook = format!(
            " response=''\n if [ \"$operation\" = QUERY ] && [ \"$address\" = {address} ] && [ \"$payload\" = '{outq}' ] && [ -f '{marker}' ]; then printf 'KIV\\tOK\\t{one}\\n'; continue; fi\n",
            marker = marker.display(),
        );
        assert!(
            script.contains(" response=''\n"),
            "missing fake response hook"
        );
        std::fs::write(path, script.replacen(" response=''\n", &hook, 1)).unwrap();
    }
    marker
}

fn assert_reconfigure_restarts_after_verified_off(commands: &[String], start: usize, address: u8) {
    let commands = &commands[start..];
    let off = format!("WRITE {address} {}", hex(":OUTP OFF\n"));
    let range = format!("WRITE {address} {}", hex(":SOUR:RANG 1.000000000000e-1\n"));
    let on = format!("WRITE {address} {}", hex(":OUTP ON\n"));
    let off_at = commands
        .iter()
        .position(|line| line == &off)
        .expect("OFF write");
    let range_at = commands
        .iter()
        .position(|line| line == &range)
        .expect("new range write");
    let on_at = commands
        .iter()
        .position(|line| line == &on)
        .expect("ON write");
    let off_query = format!("QUERY {address} {}", hex(":OUTP?\n"));
    assert!(
        commands[off_at + 1..range_at]
            .iter()
            .any(|line| line == &off_query),
        "OFF must be read back before changing range: {commands:?}"
    );
    assert!(
        off_at < range_at && range_at < on_at,
        "a cross-range DC update must verify OFF before configuring its range and only then enable output: {commands:?}"
    );
}

fn assert_held_range_update_does_not_reconfigure(commands: &[String], start: usize, address: u8) {
    let commands = &commands[start..];
    for command in [":OUTP OFF\n", ":SOUR:RANG ", ":OUTP ON\n"] {
        assert!(
            !commands
                .iter()
                .any(|line| line.starts_with(&format!("WRITE {address} {}", hex(command)))),
            "an update within the verified DC range must only change level: {commands:?}"
        );
    }
}

#[test]
fn physical_source_dc_cross_range_restarts_only_after_verified_off() {
    let bridge = Bridge::new(METERS[1].1, "", "0.0001");
    use_written_source_range_responses(&bridge);
    use_written_source_level_responses(&bridge);
    let mut engine = Engine::start(&bridge.dir);
    engine.connect("YOKOGAWA_GS210", METERS[1].0);
    engine.send(json!({
        "command":"set_source_dc", "mode":"dc_iv", "value":0.0,
        "compliance":0.2, "ramp_step":0.0001
    }));
    engine.wait(|s| s["output"] == "On" && s["source_dc_limits"]["range"] == 0.01);

    let before = bridge.commands().len();
    engine.send(json!({
        "command":"set_source_dc", "mode":"dc_iv", "value":0.02,
        "compliance":0.2, "ramp_step":0.0001
    }));
    let state = engine.wait(|s| s["output"] == "On" && s["source_dc_limits"]["range"] == 0.1);
    assert_eq!(state["error"], "", "{state}");
    assert_reconfigure_restarts_after_verified_off(&bridge.commands(), before, 3);

    let before = bridge.commands().len();
    engine.send(json!({
        "command":"set_source_dc", "mode":"dc_iv", "value":0.03,
        "compliance":0.2, "ramp_step":0.0001
    }));
    let state = engine.wait(|s| s["source_dc"]["value"] == 0.03 && s["output"] == "On");
    assert_eq!(state["source_dc_limits"]["range"], 0.1, "{state}");
    assert_held_range_update_does_not_reconfigure(&bridge.commands(), before, 3);
}

#[test]
fn physical_source_dc_does_not_reconfigure_after_off_verification_failure() {
    let bridge = Bridge::new(METERS[1].1, "", "0.0001");
    use_written_source_range_responses(&bridge);
    use_written_source_level_responses(&bridge);
    let refusal = install_off_verification_refusal(&bridge, 3);
    let mut engine = Engine::start(&bridge.dir);
    engine.connect("YOKOGAWA_GS210", METERS[1].0);
    engine.send(json!({
        "command":"set_source_dc", "mode":"dc_iv", "value":0.0,
        "compliance":0.2, "ramp_step":0.0001
    }));
    engine.wait(|s| s["output"] == "On" && s["source_dc_limits"]["range"] == 0.01);

    std::fs::write(refusal, "").unwrap();
    let before = bridge.commands().len();
    engine.send(json!({
        "command":"set_source_dc", "mode":"dc_iv", "value":0.02,
        "compliance":0.2, "ramp_step":0.0001
    }));
    let state = engine.wait(|s| !s["error"].as_str().unwrap_or_default().is_empty());
    assert_eq!(state["output"], "Unknown", "{state}");
    let commands = bridge.commands();
    let commands = &commands[before..];
    assert!(
        !commands
            .iter()
            .any(|line| line == &format!("WRITE 3 {}", hex(":SOUR:RANG 1.000000000000e-1\n"))),
        "range must not change if OFF is unverified: {commands:?}"
    );
    assert!(
        !commands
            .iter()
            .any(|line| line == &format!("WRITE 3 {}", hex(":OUTP ON\n"))),
        "output must not be re-enabled if OFF is unverified: {commands:?}"
    );
}

#[test]
fn physical_gate_dc_cross_range_obeys_the_same_verified_off_contract() {
    let bridge = Bridge::new(METERS[1].1, "", "0.0001");
    use_written_source_range_responses(&bridge);
    use_written_source_level_responses(&bridge);
    let mut engine = Engine::start(&bridge.dir);
    engine.connect("YOKOGAWA_GS210", METERS[1].0);
    engine.send(json!({
        "command":"connect_gate", "simulation":false,
        "resource":"USB0::0x0B21::0x0039::GATE::INSTR", "model":"YOKOGAWA_GS210"
    }));
    engine.wait(|s| s["gate_connected"] == true && s["error"] == "");
    engine.send(json!({
        "command":"set_gate_voltage", "voltage":0.0,
        "compliance":0.2, "ramp_step":0.0001
    }));
    engine.wait(|s| s["gate_output"] == "On" && s["gate_dc_limits"]["range"] == 0.01);

    let before = bridge.commands().len();
    engine.send(json!({
        "command":"set_gate_voltage", "voltage":0.02,
        "compliance":0.2, "ramp_step":0.0001
    }));
    let state = engine.wait(|s| s["gate_output"] == "On" && s["gate_dc_limits"]["range"] == 0.1);
    assert_eq!(state["error"], "", "{state}");
    assert_reconfigure_restarts_after_verified_off(&bridge.commands(), before, 0);

    let before = bridge.commands().len();
    engine.send(json!({
        "command":"set_gate_voltage", "voltage":0.03,
        "compliance":0.2, "ramp_step":0.0001
    }));
    let state = engine.wait(|s| s["gate_voltage"] == 0.03 && s["gate_output"] == "On");
    assert_eq!(state["gate_dc_limits"]["range"], 0.1, "{state}");
    assert_held_range_update_does_not_reconfigure(&bridge.commands(), before, 0);
}

#[test]
fn native_dc_rejects_an_off_grid_update_before_writing_a_level() {
    let bridge = Bridge::new(METERS[1].1, "", "0.0001");
    let mut engine = Engine::start(&bridge.dir);
    engine.connect("YOKOGAWA_GS210", METERS[1].0);
    engine.send(json!({
        "command":"set_source_dc", "mode":"dc_iv", "value":0.0,
        "compliance":0.2, "ramp_step":0.0001
    }));
    let state = engine.wait(|s| s["output"] == "On" && s["source_dc_limits"]["range"] == 0.01);
    assert_eq!(state["source_dc_limits"]["resolution"], 1e-7, "{state}");
    let before = bridge.commands().len();
    engine.send(json!({
        "command":"set_source_dc", "mode":"dc_iv", "value":5e-10,
        "compliance":0.2, "ramp_step":0.0001
    }));
    let state = engine.wait(|s| !s["error"].as_str().unwrap_or_default().is_empty());
    assert!(
        state["error"].as_str().unwrap().contains("multiple of"),
        "{state}"
    );
    let after = bridge.commands();
    assert!(
        !after[before..]
            .iter()
            .any(|line| line == &format!("WRITE 3 {}", hex(":SOUR:LEV:FIX 5.000000000000e-10\n"))),
        "off-grid target must never be sent: {after:?}"
    );
}

#[test]
fn native_slow_dc_off_blocks_later_run_before_any_output_resume() {
    let bridge = Bridge::new(METERS[1].1, "slow_status", "0.0001");
    let mut engine = Engine::start(&bridge.dir);
    engine.connect("YOKOGAWA_GS210", METERS[1].0);
    let output = path("dc-off-race");
    let run = config("YOKOGAWA_GS210", METERS[1].0, iv_core::Mode::Iv, 0.0, 1);
    engine.send(json!({
        "command":"set_source_dc", "mode":"dc_iv", "value":0.0,
        "compliance":0.2, "ramp_step":0.0001
    }));
    engine.send(json!({"command":"source_dc_off"}));
    engine.send(json!({"command":"run","config":run,"path":output}));
    let state = engine.wait(|s| {
        s["error"]
            .as_str()
            .is_some_and(|error| error.contains("already active or queued"))
    });
    assert_eq!(state["output"], "Off", "{state}");
    assert!(state["source_dc"].is_null());
    assert!(
        !output.exists(),
        "a rejected Run must not create CSV output"
    );
    let commands = bridge.commands();
    assert!(
        !commands
            .iter()
            .any(|line| line == &format!("WRITE 3 {}", hex(":OUTP ON\n"))),
        "the cancelled DC setup must not re-enable output: {commands:?}"
    );
    assert!(
        commands
            .iter()
            .any(|line| line == &format!("WRITE 3 {}", hex(":OUTP OFF\n"))),
        "OFF must be dispatched even while DC setup is slow: {commands:?}"
    );
    cleanup(&output);
}

#[test]
fn failed_readings_retain_raw_evidence_after_immediate_output_off() {
    for settle in [0.0, 0.20] {
        for reading in ["broken reading", "NaN", "9.9e37"] {
            let bridge = Bridge::new(METERS[1].1, "", reading);
            // Inject bad readings only after source ON, so an unrelated idle
            // monitor cannot consume the fault before the intended sweep.
            for name in ["kohdalab-gpib-helper", "kohdalab-usbtmc-helper"] {
                let file = bridge.dir.join(name);
                let script = std::fs::read_to_string(&file).unwrap();
                let branch = format!(
                    "7:{}|0:{}) response='{}'",
                    hex("READ?\n"),
                    hex("READ?\n"),
                    hex(reading)
                );
                let replacement = format!(
                    "7:{}|0:{}) if [ \"$output\" = '{}' ]; then response='{}'; else response='{}'; fi",
                    hex("READ?\n"),
                    hex("READ?\n"),
                    hex("1"),
                    hex(reading),
                    hex("0.0001")
                );
                assert!(script.contains(&branch));
                std::fs::write(file, script.replace(&branch, &replacement)).unwrap();
            }
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
                " response=''\n if [ \"$payload\" = '{}' ] && [ \"$output\" = '31' ]; then printf 'KIV\\tERR\\t{}\\n'; continue; fi\n",
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
