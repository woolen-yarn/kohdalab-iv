//! Full engine with a stateful four-instrument bridge. No hardware is opened.
#![cfg(unix)]
use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader, Write},
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command, Stdio},
    sync::mpsc,
    time::{Duration, Instant},
};
struct Engine {
    child: Child,
    input: ChildStdin,
    events: mpsc::Receiver<Value>,
}
impl Engine {
    fn new(dir: &Path) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_iv-cli"))
            .arg("--engine")
            .env("KOHDALAB_IV_HELPER_DIR", dir)
            .env("USERPROFILE", dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let input = child.stdin.take().unwrap();
        let output = child.stdout.take().unwrap();
        let (tx, events) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(output).lines() {
                let Ok(line) = line else {
                    break;
                };
                let Ok(value) = serde_json::from_str(&line) else {
                    break;
                };
                if tx.send(value).is_err() {
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
        let until = Instant::now() + Duration::from_secs(12);
        let mut last = Value::Null;
        while Instant::now() < until {
            self.send(json!({"command":"heartbeat"}));
            if let Ok(v) = self.events.recv_timeout(Duration::from_millis(100))
                && v["event"] == "state"
            {
                last = v["data"].clone();
                if check(&last) {
                    return last;
                }
            }
        }
        panic!("No matching engine state: {last}");
    }
    fn connect_primary(&mut self) {
        self.send(json!({"command":"connect","simulation":false,"source":"GPIB0::3::INSTR","meter":"GPIB0::7::INSTR"}));
        let state = self.wait(|s| s["connected"] == true);
        assert_eq!(state["error"], "");
    }
    fn connect(&mut self, gate: bool) {
        self.connect_primary();
        self.send(json!({"command":"connect_source2","simulation":false,"resource":"GPIB0::4::INSTR","model":"YOKOGAWA_GS210"}));
        let state = self.wait(|s| s["source2_connected"] == true);
        assert_eq!(state["error"], "");
        if gate {
            self.send(json!({"command":"connect_gate","simulation":false,"resource":"GPIB0::5::INSTR","model":"YOKOGAWA_GS210"}));
            self.wait(|s| s["gate_connected"] == true);
            self.send(json!({"command":"set_gate_voltage","voltage":0.02,"compliance":0.001,"ramp_step":0.01}));
            let state = self.wait(|s| s["gate_output"] == "On");
            assert_eq!(state["error"], "");
        }
    }
}
impl Drop for Engine {
    fn drop(&mut self) {
        let _ = writeln!(self.input, "{{\"command\":\"shutdown\"}}");
        let _ = self.input.flush();
        let until = Instant::now() + Duration::from_secs(8);
        while Instant::now() < until {
            if self.child.try_wait().unwrap().is_some() {
                return;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        panic!("mock engine shutdown did not finish");
    }
}
struct Bridge {
    dir: tempfile::TempDir,
}
impl Bridge {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let code = r#"#!/usr/bin/env python3
import sys,json,pathlib,time
root=pathlib.Path(__file__).parent
statefile=root/'state.json'
def initial(): return {str(a):{'on':False,'level':0.,'range':'1','compliance':'0.001','function':'VOLT'} for a in (3,4,5,7)}
state=json.loads(statefile.read_text()) if statefile.exists() else initial()
def reply(value='',error=False):
 print('KIV\t'+('ERR' if error else 'OK')+'\t'+value.encode().hex(),flush=True)
if '--list' in sys.argv:
 reply('');sys.exit()
reply()
for line in sys.stdin:
 p=line.split();op=p[0];a=p[1] if len(p)>1 else '0';payload=bytes.fromhex(p[3]).decode() if len(p)>3 and p[3]!='-' else '';cmd=payload.strip()
 if a not in state: state[a]=initial()['3'].copy()
 d=state[a];response=''
 with (root/'commands.jsonl').open('a') as f:f.write(json.dumps({'op':op,'address':a,'command':cmd})+'\n')
 if op=='WRITE':
  if cmd==':OUTP OFF' and (root/('fail-off-write-'+a)).exists():reply('injected output OFF write failure',True);continue
  if cmd==':OUTP OFF' and (root/('fail-off-once-'+a)).exists():
   (root/('fail-off-once-'+a)).unlink();reply('injected one-time OFF failure',True);continue
  if cmd==':OUTP ON':d['on']=not (root/('refuse-on-'+a)).exists()
  elif cmd==':OUTP OFF' and not (root/('refuse-off-'+a)).exists():d['on']=False
  elif cmd.startswith(':SOUR:LEV:FIX '):
   level=float(cmd.split()[-1])
   if d['on'] and abs(level)<abs(d['level']) and (root/'slow-zero-return').exists():time.sleep(0.26)
   d['level']=level
  elif cmd.startswith(':SOUR:RANG '):d['range']=cmd.split()[-1]
  elif cmd.startswith(':SOUR:PROT:CURR '):d['compliance']=cmd.split()[-1]
  elif cmd.startswith(':SOUR:FUNC '):d['function']=cmd.split()[-1]
  statefile.write_text(json.dumps(state))
 if op=='QUERY':
  if cmd=='*IDN?' and a=='5' and (root/'gate-identity-once').exists():
   (root/'gate-identity-once').unlink();reply('injected gate validation failure',True);continue
  if cmd=='*IDN?':response='Agilent Technologies,34411A,METER,1' if a=='7' else 'YOKOGAWA,GS210,SOURCE'+a+',1'
  elif cmd==':OUTP?':
   if a=='5' and (root/'gate-output-query-once').exists():
    (root/'gate-output-query-once').unlink();reply('injected gate output query failure',True);continue
   response=str(int(d['on']))
  elif cmd==':SOUR:FUNC?':response=d['function']
  elif cmd==':SOUR:RANG?':response=d['range']
  elif cmd==':SOUR:PROT:CURR?':response=d['compliance']
  elif cmd in (':SOUR:LEV?',':SOUR:LEV:FIX?',':SOUR:LEV:AUTO?'):response=repr(d['level'])
  elif cmd==':STAT:COND?':
   if a in ('3','4') and d['on'] and abs(d['level'])>0.00001 and (root/'slow-source-status').exists():time.sleep(0.12)
   if (root/('status-error-'+a)).exists():reply('injected status error',True);continue
   response='2048' if d['on'] and abs(d['level'])>0.00001 and (root/('trip-'+a)).exists() else '256'
  elif cmd=='SYST:ERR?':response='0,No error'
  elif cmd=='READ?':
   if state['3']['on'] and (root/'fail-meter-read').exists():reply('injected meter read failure',True);continue
   response=repr((state['3']['level']-state['4']['level'])/1000.)
  elif 'RANG?' in cmd:response='0.001'
  elif 'RES?' in cmd:response='1e-9'
 reply(response)
 if op=='EXIT':break
"#;
        for n in ["kohdalab-gpib-helper", "kohdalab-usbtmc-helper"] {
            let f = dir.path().join(n);
            std::fs::write(&f, code).unwrap();
            std::fs::set_permissions(&f, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        Self { dir }
    }
    fn log(&self) -> Vec<Value> {
        std::fs::read_to_string(self.dir.path().join("commands.jsonl"))
            .unwrap()
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect()
    }
    fn marker(&self, name: &str) {
        std::fs::write(self.dir.path().join(name), "").unwrap();
    }
    fn path(&self, name: &str) -> PathBuf {
        self.dir.path().join(format!("{name}.csv"))
    }
}

/// Per-test manual-range fault injection. The normal bridge keeps its original
/// Auto behavior unless this helper is called.
fn install_fixed_meter_range_drift_after_energized_read(bridge: &Bridge) {
    let arm = bridge.dir.path().join("arm-fixed-meter-range-drift");
    let tripped = bridge.dir.path().join("fixed-meter-range-drifted");
    for name in ["kohdalab-gpib-helper", "kohdalab-usbtmc-helper"] {
        let path = bridge.dir.path().join(name);
        let script = std::fs::read_to_string(&path).unwrap();
        let script = script.replace(
            "  elif cmd=='READ?':\n   if state['3']['on'] and (root/'fail-meter-read').exists():reply('injected meter read failure',True);continue\n   response=repr((state['3']['level']-state['4']['level'])/1000.)\n  elif 'RANG?' in cmd:response='0.001'",
            &format!(
                "  elif cmd=='READ?':\n   if state['3']['on'] and (root/'fail-meter-read').exists():reply('injected meter read failure',True);continue\n   response=repr((state['3']['level']-state['4']['level'])/1000.)\n   if (state['3']['on'] or state['4']['on']) and (root/'{}').exists():(root/'{}').touch()\n  elif 'AUTO?' in cmd:response='0'\n  elif 'RANG?' in cmd:response='0.01' if (root/'{}').exists() else '0.001'",
                arm.file_name().unwrap().to_str().unwrap(),
                tripped.file_name().unwrap().to_str().unwrap(),
                tripped.file_name().unwrap().to_str().unwrap(),
            ),
        );
        assert!(
            script.contains("'AUTO?' in cmd"),
            "balanced fake range hook was not installed"
        );
        std::fs::write(path, script).unwrap();
    }
}

fn trigger_fixed_meter_range_drift(bridge: &Bridge) {
    std::fs::write(bridge.dir.path().join("arm-fixed-meter-range-drift"), "").unwrap();
}
fn config() -> iv_core::RunConfig {
    iv_core::RunConfig {
        pattern: iv_core::Pattern::Linear,
        start: -0.001,
        stop: 0.001,
        step: 0.001,
        source_limit: 0.01,
        source_compliance: Some(0.001),
        balanced_source: Some(iv_core::BalancedSourceConfig::default()),
        start_settle_s: 0.,
        settle_s: 0.,
        ramp_wait_s: 0.,
        ..Default::default()
    }
}
fn terminal(s: &Value) -> bool {
    matches!(
        s["phase"].as_str(),
        Some("Completed" | "Stopped" | "Failed" | "Source limit stop" | "Compliance stop")
    )
}
fn assert_both_off_before_queries(log: &[Value], after: usize) {
    let tail = &log[after + 1..];
    assert_eq!(tail[0]["op"], "WRITE", "{tail:?}");
    assert_eq!(tail[0]["address"], "3");
    assert_eq!(tail[0]["command"], ":OUTP OFF");
    assert_eq!(tail[1]["op"], "WRITE");
    assert_eq!(tail[1]["address"], "4");
    assert_eq!(tail[1]["command"], ":OUTP OFF");
}
#[test]
fn balanced_sweep_keeps_gate_bias_and_records_both_source_values() {
    let bridge = Bridge::new();
    let mut engine = Engine::new(bridge.dir.path());
    engine.connect(true);
    let path = bridge.path("normal");
    engine.send(json!({"command":"run","config":config(),"path":path}));
    let state = engine.wait(terminal);
    assert_eq!(state["phase"], "Completed", "{state}");
    assert_eq!(state["output"], "Off");
    assert_eq!(state["source2_output"], "Off");
    assert_eq!(state["gate_output"], "On");
    assert_eq!(state["gate_voltage"], 0.02);
    assert_eq!(state["points"], 3);
    let metadata: Value =
        serde_json::from_slice(&std::fs::read(path.with_extension("run.json")).unwrap()).unwrap();
    assert_eq!(
        metadata["balanced_drive"]["hardware_compliances"],
        json!([0.001, 0.001])
    );
    assert!(
        !metadata["file_hashes"]["csv_sha256"]
            .as_str()
            .unwrap()
            .is_empty()
    );
    let mut csv = csv::Reader::from_path(&path).unwrap();
    assert_eq!(metadata["csv_schema"], iv_core::DUAL_CSV_SCHEMA);
    assert_eq!(
        metadata["csv_fields"],
        json!(csv.headers().unwrap().iter().collect::<Vec<_>>())
    );
    assert_eq!(
        csv.headers().unwrap().iter().collect::<Vec<_>>(),
        [
            "run_id",
            "point_index",
            "timestamp",
            "elapsed_s",
            "sweep_index",
            "direction",
            "target_value",
            "target_unit",
            "voltage_V",
            "current_A",
            "voltage_origin",
            "current_origin",
            "readings_requested",
            "readings_completed",
            "meter_stddev",
            "meter_unit",
            "software_limit_hit",
            "quality_flags",
            "V_source_V",
            "V_meter_V",
            "I_source_A",
            "I_meter_A",
            "R_source_ohm",
            "R_meter_ohm",
            "G_source_S",
            "G_meter_S",
            "source1_setpoint_V",
            "source2_setpoint_V",
            "source1_level_readback_V",
            "source2_level_readback_V",
        ]
    );
    assert_eq!(metadata["csv"], path.display().to_string());
    assert_eq!(metadata["balanced_drive"]["ratio"], json!([1, 1]));
    let source_points = metadata["dual_source_points"].as_array().unwrap();
    assert_eq!(source_points.len(), 3);
    assert_eq!(source_points[0]["point_index"], 1);
    assert_eq!(source_points[0]["source1"]["setpoint_V"], -0.0005);
    assert_eq!(source_points[0]["source2"]["setpoint_V"], 0.0005);
    assert_eq!(source_points[0]["source1"]["level_readback_V"], -0.0005);
    assert_eq!(source_points[0]["source2"]["level_readback_V"], 0.0005);
    assert_eq!(source_points[0]["differential_level_readback_V"], -0.001);
    assert!(source_points[0]["source1"]["output_voltage_V"].is_null());
    assert!(source_points[0]["source2"]["output_voltage_V"].is_null());
    let mut raw_reader = csv::Reader::from_path(path.with_extension("raw.csv")).unwrap();
    assert_eq!(
        raw_reader.headers().unwrap().iter().collect::<Vec<_>>(),
        [
            "run_id",
            "event_index",
            "point_index",
            "reading_index",
            "event_type",
            "device_role",
            "quantity",
            "command",
            "raw_response",
            "value",
            "unit",
            "host_started_at_utc",
            "host_completed_at_utc",
            "started_elapsed_s",
            "completed_elapsed_s",
            "result",
            "error",
            "details_json",
        ]
    );
    let rows: Vec<_> = csv.records().map(Result::unwrap).collect();
    assert_eq!(rows[0].get(8), Some("-1e-3"));
    assert_eq!(rows[0].get(19), Some(""));
    assert_eq!(
        rows[0].iter().skip(26).collect::<Vec<_>>(),
        ["-5e-4", "5e-4", "-5e-4", "5e-4"]
    );
    assert_eq!(rows[2].get(8), Some("1e-3"));
    assert_eq!(rows[2].get(10), Some("source_readback"));
    let raw = std::fs::read_to_string(path.with_extension("raw.csv")).unwrap();
    assert!(raw.contains("balanced_source_readback"));
    assert!(raw.contains("source2"));
    let log = bridge.log();
    let first_on = log
        .iter()
        .position(|l| l["address"] == "3" && l["command"] == ":OUTP ON")
        .unwrap();
    assert!(
        !log[first_on..]
            .iter()
            .any(|l| l["address"] == "5" && l["command"] == ":OUTP OFF")
    );
}
#[test]
fn second_source_h_stops_both_and_gate_before_saving_limited_point() {
    let bridge = Bridge::new();
    let mut engine = Engine::new(bridge.dir.path());
    engine.connect(true);
    bridge.marker("trip-4");
    let path = bridge.path("limited");
    let mut c = config();
    c.start = 0.001;
    c.stop = 0.001;
    engine.send(json!({"command":"run","config":c,"path":path}));
    let state = engine.wait(terminal);
    assert_eq!(state["phase"], "Source limit stop", "{state}");
    assert_eq!(state["output"], "Off");
    assert_eq!(state["source2_output"], "Off");
    assert_eq!(state["gate_output"], "Off");
    assert_eq!(state["points"], 0);
    assert_eq!(state["protection_alert"]["source_index"], 2);
    let log = bridge.log();
    let condition = log
        .iter()
        .position(|l| {
            l["address"] == "4"
                && l["command"] == ":STAT:COND?"
                && log.iter().take_while(|x| *x != l).any(|l| {
                    l["address"] == "4"
                        && l["command"]
                            .as_str()
                            .is_some_and(|c| c.starts_with(":SOUR:LEV:FIX -5"))
                })
        })
        .unwrap();
    assert_both_off_before_queries(&log, condition);
}
#[test]
fn meter_limit_dispatches_off_to_both_before_other_queries() {
    let bridge = Bridge::new();
    let mut engine = Engine::new(bridge.dir.path());
    engine.connect(false);
    let path = bridge.path("meter-limit");
    let mut c = config();
    c.start = 0.001;
    c.stop = 0.001;
    c.compliance = 0.5e-6;
    engine.send(json!({"command":"run","config":c,"path":path}));
    let state = engine.wait(terminal);
    assert_eq!(state["phase"], "Compliance stop", "{state}");
    assert_eq!(state["output"], "Off");
    assert_eq!(state["source2_output"], "Off");
    let log = bridge.log();
    let on = log
        .iter()
        .position(|l| l["address"] == "3" && l["command"] == ":OUTP ON")
        .unwrap();
    let read = on
        + log[on..]
            .iter()
            .position(|l| l["command"] == "READ?")
            .unwrap();
    assert_both_off_before_queries(&log, read);
}

#[test]
fn auto_resistance_transition_stops_both_balanced_outputs_before_saving() {
    for settle in [0.0, 0.25] {
        let bridge = Bridge::new();
        install_fixed_meter_range_drift_after_energized_read(&bridge);
        let mut engine = Engine::new(bridge.dir.path());
        engine.connect(false);
        let path = bridge.path("auto-range-transition");
        let mut c = config();
        c.start = 0.;
        c.stop = 0.;
        c.repeat = 2;
        c.start_settle_s = settle;
        c.settle_s = settle;
        trigger_fixed_meter_range_drift(&bridge);
        engine.send(json!({"command":"run","config":c,"path":path}));
        let state = engine.wait(terminal);
        assert_eq!(state["phase"], "Failed", "{state}");
        assert_eq!(state["output"], "Off");
        assert_eq!(state["source2_output"], "Off");
        assert_eq!(state["points"], 0);
        assert!(
            state["error"]
                .as_str()
                .unwrap()
                .contains("Meter range transition detected"),
            "{state}"
        );
        let log = bridge.log();
        let range = log
            .iter()
            .rposition(|e| e["address"] == "7" && e["command"] == "SENS:CURR:DC:RANG?")
            .unwrap();
        assert_both_off_before_queries(&log, range);
        assert_eq!(csv::Reader::from_path(&path).unwrap().records().count(), 0);
    }
}

#[test]
fn fixed_meter_range_drift_during_balanced_repeated_read_stops_both_before_saving() {
    let bridge = Bridge::new();
    install_fixed_meter_range_drift_after_energized_read(&bridge);
    let mut engine = Engine::new(bridge.dir.path());
    engine.connect(false);
    let path = bridge.path("fixed-range-repeat-drift");
    let mut c = config();
    c.start = 0.0;
    c.stop = 0.0;
    c.repeat = 2;
    c.meter_current_range = Some(0.001);
    trigger_fixed_meter_range_drift(&bridge);
    engine.send(json!({"command":"run","config":c,"path":path}));
    let state = engine.wait(terminal);
    assert_eq!(state["phase"], "Failed", "{state}");
    assert_eq!(state["output"], "Off");
    assert_eq!(state["source2_output"], "Off");
    assert_eq!(state["points"], 0);
    let log = bridge.log();
    let range = log
        .iter()
        .rposition(|entry| entry["address"] == "7" && entry["command"] == "SENS:CURR:DC:RANG?")
        .expect("fixed meter range must be checked after the read");
    assert_both_off_before_queries(&log, range);
    assert_eq!(
        csv::Reader::from_path(&path).unwrap().records().count(),
        0,
        "the range-changed repeated reading must not create a main CSV point"
    );
}

#[test]
fn fixed_meter_range_drift_during_balanced_settling_stops_both_before_follow_up_queries() {
    let bridge = Bridge::new();
    install_fixed_meter_range_drift_after_energized_read(&bridge);
    let mut engine = Engine::new(bridge.dir.path());
    engine.connect(false);
    let path = bridge.path("fixed-range-settle-drift");
    let mut c = config();
    c.start = 0.0;
    c.stop = 0.0;
    c.start_settle_s = 0.25;
    c.settle_s = 0.25;
    c.meter_current_range = Some(0.001);
    trigger_fixed_meter_range_drift(&bridge);
    engine.send(json!({"command":"run","config":c,"path":path}));
    let state = engine.wait(terminal);
    assert_eq!(state["phase"], "Failed", "{state}");
    assert_eq!(state["output"], "Off");
    assert_eq!(state["source2_output"], "Off");
    assert_eq!(state["points"], 0);
    let log = bridge.log();
    let range = log
        .iter()
        .rposition(|entry| entry["address"] == "7" && entry["command"] == "SENS:CURR:DC:RANG?")
        .expect("settling conversion must re-check its fixed range");
    assert_both_off_before_queries(&log, range);
}

#[test]
fn failed_secondary_enable_or_status_never_leaves_the_primary_on() {
    for fault in ["refuse-on-4", "status-error-4"] {
        let bridge = Bridge::new();
        let mut engine = Engine::new(bridge.dir.path());
        engine.connect(false);
        bridge.marker(fault);
        let path = bridge.path(fault);
        engine.send(json!({"command":"run","config":config(),"path":path}));
        let state = engine.wait(terminal);
        assert_eq!(state["phase"], "Failed", "{fault}: {state}");
        assert_eq!(state["output"], "Off");
        assert_eq!(state["source2_output"], "Off");
        assert_eq!(state["points"], 0);
        let log = bridge.log();
        let on = log
            .iter()
            .position(|l| l["address"] == "3" && l["command"] == ":OUTP ON")
            .unwrap();
        assert!(!log[on..].iter().any(|l| l["command"] == "READ?"));
    }
}

#[test]
fn integer_ratio_commands_follow_the_exact_plan_including_ramps() {
    let bridge = Bridge::new();
    let mut engine = Engine::new(bridge.dir.path());
    engine.connect(false);
    let mut c = config();
    c.start = -0.003;
    c.stop = 0.003;
    c.step = 0.0015;
    c.balanced_source.as_mut().unwrap().ratio = [3, 7];
    engine.send(json!({"command":"run","config":c,"path":bridge.path("ratio-3-7")}));
    let state = engine.wait(terminal);
    assert_eq!(state["phase"], "Completed", "{state}");
    let log = bridge.log();
    let on = log
        .iter()
        .position(|l| l["address"] == "4" && l["command"] == ":OUTP ON")
        .unwrap();
    let mut first = 0i64;
    let mut pairs = 0;
    for exchange in &log[on + 1..] {
        if exchange["command"] == ":OUTP OFF" {
            break;
        }
        if exchange["op"] != "WRITE" {
            continue;
        }
        if let Some(level) = exchange["command"]
            .as_str()
            .and_then(|s| s.strip_prefix(":SOUR:LEV:FIX "))
        {
            let value = (level.parse::<f64>().unwrap() * 1e6).round() as i64;
            if exchange["address"] == "3" {
                first = value;
            }
            if exchange["address"] == "4" {
                assert_eq!(first * 7, -value * 3, "{exchange}");
                pairs += 1;
            }
        }
    }
    assert!(pairs >= 5);
}

#[test]
fn failed_secondary_off_is_not_certified_and_also_turns_gate_off() {
    let bridge = Bridge::new();
    let mut engine = Engine::new(bridge.dir.path());
    engine.connect(true);
    bridge.marker("refuse-off-4");
    bridge.marker("gate-identity-once");
    engine.send(json!({"command":"run","config":config(),"path":bridge.path("failed-off")}));
    let state = engine.wait(terminal);
    assert_eq!(state["phase"], "Failed", "{state}");
    assert_ne!(state["output"], "Off");
    assert_ne!(state["source2_output"], "Off");
    assert_eq!(state["gate_output"], "Off", "{state}");
    // Allow mock cleanup during Drop; the failure has already been certified unknown.
    std::fs::remove_file(bridge.dir.path().join("refuse-off-4")).unwrap();
}

#[test]
fn balanced_live_updates_during_wait_do_not_add_csv_points_and_enforce_limit() {
    for (limited, slow_status) in [(false, false), (true, false), (true, true)] {
        let bridge = Bridge::new();
        let mut engine = Engine::new(bridge.dir.path());
        engine.connect(true);
        if slow_status {
            bridge.marker("slow-source-status");
        }
        let mut c = config();
        c.start = 0.001;
        c.stop = 0.001;
        c.settle_s = 0.2;
        if limited {
            c.compliance = 0.5e-6;
        }
        engine.send(json!({"command":"run","config":c,"path":bridge.path(if limited{"live-limit"}else{"live-wait"})}));
        let state = engine.wait(terminal);
        assert_eq!(
            state["phase"],
            if limited {
                "Compliance stop"
            } else {
                "Completed"
            },
            "{state}"
        );
        assert_eq!(state["points"], if limited { 0 } else { 1 });
        let log = bridge.log();
        let on = log
            .iter()
            .position(|l| l["address"] == "4" && l["command"] == ":OUTP ON")
            .unwrap();
        let conversions = log[on..].iter().filter(|l| l["command"] == "READ?").count();
        if limited {
            assert_eq!(conversions, 1);
            assert_eq!(state["gate_output"], "Off");
            let at = on
                + log[on..]
                    .iter()
                    .position(|l| l["command"] == "READ?")
                    .unwrap();
            assert_both_off_before_queries(&log, at);
        } else {
            assert!(conversions > 1);
            assert_eq!(state["gate_output"], "On");
        }
    }
}

#[test]
fn single_source_with_gate_preserves_the_same_failure_cleanup() {
    for fault in ["trip-3", "meter-limit", "fail-meter-read"] {
        let bridge = Bridge::new();
        let mut engine = Engine::new(bridge.dir.path());
        engine.connect(true);
        engine.send(json!({"command":"disconnect_source2"}));
        engine.wait(|s| s["source2_connected"] == false);
        let mut c = config();
        c.balanced_source = None;
        c.start = 0.001;
        c.stop = 0.001;
        if fault == "meter-limit" {
            c.compliance = 0.5e-6;
        } else {
            bridge.marker(fault);
        }
        engine.send(json!({"command":"run","config":c,"path":bridge.path(fault)}));
        let state = engine.wait(terminal);
        assert_eq!(
            state["phase"],
            match fault {
                "trip-3" => "Source limit stop",
                "meter-limit" => "Compliance stop",
                _ => "Failed",
            },
            "{state}"
        );
        assert_eq!(state["output"], "Off", "{state}");
        assert_eq!(state["gate_output"], "Off", "{state}");
    }
}

fn assert_off_readbacks(log: &[Value], first_off: usize, roles: &[&str]) {
    for role in roles {
        let mut commands = log[first_off..]
            .iter()
            .filter(|row| row["address"] == *role)
            .map(|row| row["command"].as_str().unwrap());
        assert_eq!(commands.next(), Some(":OUTP OFF"));
        assert_eq!(commands.next(), Some(":OUTP?"));
    }
}

fn programmed_level(row: &Value) -> Option<f64> {
    row["command"]
        .as_str()?
        .strip_prefix(":SOUR:LEV:FIX ")?
        .parse()
        .ok()
}

fn check_balanced_zero_return(stop: bool) {
    let bridge = Bridge::new();
    let mut engine = Engine::new(bridge.dir.path());
    engine.connect(false);
    let mut c = config();
    c.start = 0.3;
    c.source_limit = 0.5;
    c.stop = c.start;
    c.step = 0.1;
    c.balanced_source.as_mut().unwrap().ratio = [3, 7];
    if stop {
        c.stop = 0.4;
        c.start_settle_s = 8.0;
    }
    engine.send(json!({"command":"run","config":c,"path":bridge.path("zero")}));
    if stop {
        engine.wait(|s| s["source2_level"] == -0.21);
        engine.send(json!({"command":"stop"}));
    }
    let state = engine.wait(terminal);
    assert_eq!(state["phase"], if stop { "Stopped" } else { "Completed" });
    assert_eq!(state["output"], "Off");
    assert_eq!(state["source2_output"], "Off");
    assert_eq!(state["warnings"], json!([]));

    let log = bridge.log();
    let at_target = log
        .iter()
        .rposition(|row| row["address"] == "4" && programmed_level(row) == Some(-0.21))
        .unwrap();
    let off = at_target
        + log[at_target..]
            .iter()
            .position(|row| row["command"] == ":OUTP OFF")
            .unwrap();
    let steps: Vec<_> = log[at_target + 1..off]
        .iter()
        .filter_map(|row| Some((row["address"].as_str()?, programmed_level(row)?)))
        .collect();
    assert_eq!(
        steps,
        [
            ("3", 0.06),
            ("4", -0.14),
            ("3", 0.03),
            ("4", -0.07),
            ("3", 0.0),
            ("4", 0.0),
        ],
        "stop={stop}"
    );
    assert_off_readbacks(&log, off, &["3", "4"]);
}

#[test]
fn balanced_completion_reaches_zero_in_ratio_before_verified_off() {
    check_balanced_zero_return(false);
}

#[test]
fn balanced_stop_reaches_zero_in_ratio_before_verified_off() {
    check_balanced_zero_return(true);
}

fn check_zero_return_timeout(balanced: bool) {
    let bridge = Bridge::new();
    let mut engine = Engine::new(bridge.dir.path());
    if balanced {
        engine.connect(false);
    } else {
        engine.connect_primary();
    }
    bridge.marker("slow-zero-return");
    let mut c = config();
    c.start = 0.8;
    c.source_limit = 1.0;
    c.stop = c.start;
    c.step = 0.0001;
    if !balanced {
        c.balanced_source = None;
    }
    let path = bridge.path("timeout");
    engine.send(json!({"command":"run","config":c,"path":path}));
    let state = engine.wait(terminal);
    assert_eq!(state["phase"], "Completed", "{state}");
    assert_eq!(state["output"], "Off");
    assert!(
        state["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|warning| warning.as_str().unwrap().contains("10-second")),
        "{state}"
    );

    let roles: &[_] = if balanced { &["3", "4"] } else { &["3"] };
    let log = bridge.log();
    let on = log
        .iter()
        .rposition(|row| row["command"] == ":OUTP ON")
        .unwrap();
    let off = on
        + log[on..]
            .iter()
            .position(|row| row["command"] == ":OUTP OFF")
            .unwrap();
    let levels: Vec<_> = roles
        .iter()
        .map(|role| {
            let level = log[on..off]
                .iter()
                .rev()
                .find_map(|row| {
                    (row["address"] == *role)
                        .then(|| programmed_level(row))
                        .flatten()
                })
                .unwrap();
            assert_ne!(level, 0.0);
            level
        })
        .collect();
    assert_off_readbacks(&log, off, roles);

    let metadata: Value =
        serde_json::from_slice(&std::fs::read(path.with_extension("run.json")).unwrap()).unwrap();
    let lifecycle = std::fs::read_to_string(path.with_extension("log")).unwrap();
    let context = lifecycle
        .lines()
        .find_map(|line| {
            let fields: Vec<_> = line.splitn(5, '\t').collect();
            (fields.get(3) == Some(&"zero_return_timeout")).then(|| {
                assert_eq!(fields[1], "WARN");
                serde_json::from_str::<Value>(fields[4]).unwrap()
            })
        })
        .unwrap();
    let recorded = &metadata["zero_return_timeout"]["last_programmed_levels"];
    assert_eq!(recorded, &context["last_programmed_levels"]);
    for (index, level) in levels.into_iter().enumerate() {
        let key = if index == 0 { "source" } else { "source2" };
        assert_eq!(recorded[key].as_f64().unwrap(), level);
    }
}

#[test]
fn single_zero_return_timeout_records_last_programmed_level() {
    check_zero_return_timeout(false);
}

#[test]
fn balanced_zero_return_timeout_records_both_programmed_levels() {
    check_zero_return_timeout(true);
}

#[test]
fn balanced_meter_transport_failure_dispatches_off_without_a_level_write() {
    let bridge = Bridge::new();
    let mut engine = Engine::new(bridge.dir.path());
    engine.connect(false);
    bridge.marker("fail-meter-read");
    engine.send(json!({"command":"run","config":config(),"path":bridge.path("meter-failed")}));
    let state = engine.wait(terminal);
    assert_eq!(state["phase"], "Failed");
    assert_eq!(state["output"], "Off");
    let log = bridge.log();
    let on = log
        .iter()
        .rposition(|row| row["command"] == ":OUTP ON")
        .unwrap();
    let read = on
        + log[on..]
            .iter()
            .position(|row| row["command"] == "READ?")
            .unwrap();
    let off = read
        + log[read..]
            .iter()
            .position(|row| row["command"] == ":OUTP OFF")
            .unwrap();
    // A poisoned bridge can require an identity check before the OFF write.
    assert!(
        log[read + 1..off]
            .iter()
            .all(|row| programmed_level(row).is_none())
    );
    for role in ["3", "4"] {
        let commands: Vec<_> = log[read + 1..]
            .iter()
            .filter(|row| row["address"] == role)
            .cloned()
            .collect();
        let off = commands
            .iter()
            .position(|row| row["command"] == ":OUTP OFF")
            .unwrap();
        assert!(
            commands[..off]
                .iter()
                .all(|row| programmed_level(row).is_none())
        );
        assert_off_readbacks(&commands, off, &[role]);
    }
}

#[test]
fn idle_gate_output_query_failure_attempts_off_and_reports_verification() {
    for refused in [false, true] {
        let bridge = Bridge::new();
        let mut engine = Engine::new(bridge.dir.path());
        engine.connect(true);
        if refused {
            bridge.marker("refuse-off-5");
        }
        let before_fault = bridge.log().len();
        bridge.marker("gate-output-query-once");
        let state = engine.wait(terminal);
        assert!(
            state["error"]
                .as_str()
                .unwrap()
                .contains("injected gate output query failure")
        );
        assert_eq!(
            state["gate_output"],
            if refused { "Unknown" } else { "Off" }
        );
        let log = bridge.log();
        let off = before_fault
            + log[before_fault..]
                .iter()
                .position(|row| row["address"] == "5" && row["command"] == ":OUTP OFF")
                .unwrap();
        assert_off_readbacks(&log, off, &["5"]);
        if refused {
            std::fs::remove_file(bridge.dir.path().join("refuse-off-5")).unwrap();
        }
    }
}

#[test]
fn command_validation_error_turns_held_gate_off_without_a_ramp() {
    for refused in [false, true] {
        let bridge = Bridge::new();
        let mut engine = Engine::new(bridge.dir.path());
        engine.connect(true);
        if refused {
            bridge.marker("refuse-off-5");
        }
        let before_command = bridge.log().len();
        let mut c = config();
        c.step = 0.0;
        engine.send(json!({"command":"run","config":c,"path":bridge.path("invalid")}));
        let state = engine.wait(terminal);
        assert!(
            state["error"]
                .as_str()
                .unwrap()
                .to_lowercase()
                .contains("step")
        );
        assert!(state["gate_dc_limits"].is_null(), "{state}");
        if refused {
            assert_eq!(state["gate_output"], "Unknown", "{state}");
            assert!(
                state["error"]
                    .as_str()
                    .unwrap()
                    .contains("Could not confirm gate")
            );
        } else {
            assert_eq!(state["gate_output"], "Off");
        }
        let log = bridge.log();
        let after = &log[before_command..];
        let off = after
            .iter()
            .position(|row| row["address"] == "5" && row["command"] == ":OUTP OFF")
            .unwrap();
        assert!(
            after[..off]
                .iter()
                .all(|row| row["address"] != "5" || programmed_level(row).is_none())
        );
        assert_off_readbacks(after, off, &["5"]);
        if refused {
            std::fs::remove_file(bridge.dir.path().join("refuse-off-5")).unwrap();
        }
    }
}

#[test]
fn command_validation_error_reports_unknown_when_gate_off_write_fails() {
    let bridge = Bridge::new();
    let mut engine = Engine::new(bridge.dir.path());
    engine.connect(true);
    bridge.marker("fail-off-write-5");
    let before_command = bridge.log().len();
    let mut c = config();
    c.step = 0.0;
    engine.send(json!({"command":"run","config":c,"path":bridge.path("invalid")}));
    let state = engine.wait(terminal);
    std::fs::remove_file(bridge.dir.path().join("fail-off-write-5")).unwrap();

    let error = state["error"].as_str().unwrap();
    assert!(error.to_lowercase().contains("step"), "{error}");
    assert!(
        error.contains("injected output OFF write failure"),
        "{error}"
    );
    let log = bridge.log();
    let after = &log[before_command..];
    let off = after
        .iter()
        .position(|row| row["address"] == "5" && row["command"] == ":OUTP OFF")
        .unwrap();
    assert!(
        after[..off]
            .iter()
            .all(|row| row["address"] != "5" || programmed_level(row).is_none())
    );
    assert_eq!(state["gate_output"], "Unknown", "{state}");
    assert!(state["gate_dc_limits"].is_null(), "{state}");
}

#[test]
fn all_off_reports_gate_failure_and_gate_off_can_be_retried() {
    let bridge = Bridge::new();
    let mut engine = Engine::new(bridge.dir.path());
    engine.send(json!({"command":"connect_gate","simulation":false,"resource":"GPIB0::5::INSTR","model":"YOKOGAWA_GS210"}));
    engine.wait(|s| s["gate_connected"] == true);
    engine.send(
        json!({"command":"set_gate_voltage","voltage":0.02,"compliance":0.001,"ramp_step":0.01}),
    );
    let held = engine.wait(|s| s["gate_output"] == "On");

    bridge.marker("fail-off-write-5");
    engine.send(json!({"command":"output_off"}));
    let failed = engine
        .wait(|s| s["safety_revision"] != held["safety_revision"] && s["gate_output"] == "Unknown");
    std::fs::remove_file(bridge.dir.path().join("fail-off-write-5")).unwrap();

    assert_eq!(failed["phase"], "Failed", "{failed}");
    assert_eq!(failed["error_kind"], "output_safety", "{failed}");
    assert!(
        failed["error"]
            .as_str()
            .unwrap()
            .contains("injected output OFF write failure"),
        "{failed}"
    );
    assert_eq!(failed["gate_connected"], true);
    assert_eq!(failed["gate_voltage"], held["gate_voltage"]);
    assert!(failed["gate_dc_limits"].is_null());
    assert_eq!(failed["gate_revision"], held["gate_revision"]);

    engine.send(json!({"command":"gate_output_off"}));
    let off = engine.wait(|s| s["gate_output"] == "Off");
    assert_ne!(off["gate_revision"], held["gate_revision"]);
    assert_eq!(off["gate_voltage"], 0.0);
}

#[test]
fn gpib_inventory_keeps_parked_sources_readable_and_includes_them_in_all_off() {
    let bridge = Bridge::new();
    let mut engine = Engine::new(bridge.dir.path());
    for (id, address, assign) in [(20, 3, true), (21, 4, false)] {
        engine.send(json!({"command":"connect_inventory","id":id,"source":true,"assign":assign,
            "simulation":false,"model":"YOKOGAWA_GS210","resource":format!("GPIB0::{address}::INSTR"),"mode":"dc_iv"}));
    }
    let before = engine.wait(|s| s["inventory_devices"][1]["reading"]["value"].is_number());
    assert_eq!(before["inventory_devices"][0]["assigned"], true);
    assert_eq!(before["inventory_devices"][1]["output"], "Off");
    let before_swap = bridge.log().len();
    engine.send(json!({"command":"select_inventory","id":21,"source":true}));
    let after = engine.wait(|s| {
        s["inventory_devices"][1]["assigned"] == true
            && s["inventory_devices"][0]["reading"]["value"].is_number()
    });
    assert_eq!(after["source_resource"], "GPIB0::4::INSTR");
    assert_eq!(after["inventory_devices"][0]["resource"], "GPIB0::3::INSTR");
    let swap_commands = bridge.log();
    assert!(
        swap_commands[before_swap..]
            .iter()
            .all(|row| row["op"] != "LOCAL" && row["command"] != "*IDN?")
    );

    let before_off = swap_commands.len();
    bridge.marker("fail-off-write-3");
    engine.send(json!({"command":"output_off"}));
    let failed = engine.wait(|s| s["safety_revision"] != after["safety_revision"]);
    assert_eq!(failed["error_kind"], "output_safety");
    assert_eq!(failed["inventory_devices"][0]["output"], "Unknown");
    assert!(
        failed["error"]
            .as_str()
            .unwrap()
            .contains("Inventory source 20")
    );
    std::fs::remove_file(bridge.dir.path().join("fail-off-write-3")).unwrap();

    engine.send(json!({"command":"output_off"}));
    let recovered = engine.wait(|s| s["safety_revision"] != failed["safety_revision"]);
    assert_eq!(recovered["inventory_devices"][0]["output"], "Off");
    let commands = bridge.log();
    for address in ["3", "4"] {
        assert!(
            commands[before_off..]
                .iter()
                .any(|row| row["address"] == address && row["command"] == ":OUTP OFF")
        );
    }
}

#[test]
fn unassigned_sources_share_verified_dc_and_turn_off_independently() {
    let bridge = Bridge::new();
    let mut engine = Engine::new(bridge.dir.path());
    for (id, address) in [(20, 3), (21, 4)] {
        engine.send(json!({"command":"connect_inventory","id":id,"source":true,
            "simulation":false,"model":"YOKOGAWA_GS210","resource":format!("GPIB0::{address}::INSTR"),"mode":"dc_iv"}));
    }
    engine.wait(|s| s["inventory_devices"][1]["output"] == "Off");
    for (index, id) in [20, 21].into_iter().enumerate() {
        engine.send(json!({"command":"set_inventory_dc","id":id,"request":{
            "mode":"dc_iv","value":0.002,"compliance":0.2,"ramp_step":0.001,"range":0.1
        }}));
        let state = engine.wait(|s| {
            s["inventory_devices"][index]["output"] == "On" || s["inventory_error"].is_string()
        });
        assert_eq!(state["inventory_devices"][index]["output"], "On", "{state}");
        assert_eq!(state["inventory_devices"][index]["dc"]["value"], 0.002);
        assert_eq!(state["inventory_devices"][index]["dc_limits"]["range"], 0.1);
        assert_eq!(state["source_connected"], false);
    }
    let before_off = bridge.log().len();
    bridge.marker("refuse-off-3");
    engine.send(json!({"command":"inventory_dc_off","id":20}));
    let failed = engine.wait(|s| s["inventory_devices"][0]["output"] == "Unknown");
    assert!(failed["inventory_devices"][0]["dc_limits"].is_null());
    assert_eq!(failed["inventory_devices"][1]["output"], "On");
    assert!(
        failed["inventory_error"]
            .as_str()
            .unwrap()
            .contains("Inventory source 20")
    );
    std::fs::remove_file(bridge.dir.path().join("refuse-off-3")).unwrap();

    engine.send(json!({"command":"inventory_dc_off","id":20}));
    let after = engine.wait(|s| s["inventory_devices"][0]["output"] == "Off");
    assert_eq!(after["inventory_devices"][1]["output"], "On");
    assert!(after["inventory_devices"][0]["dc"].is_null());
    let log = bridge.log();
    assert!(
        log[before_off..]
            .iter()
            .any(|row| row["address"] == "3" && row["command"] == ":OUTP OFF")
    );
    assert!(
        log[before_off..]
            .iter()
            .all(|row| row["address"] != "4" || row["command"] != ":OUTP OFF")
    );

    bridge.marker("trip-4");
    let stopped = engine.wait(|s| s["inventory_devices"][1]["output"] == "Off");
    assert!(
        stopped["inventory_devices"][1]["error"]
            .as_str()
            .unwrap()
            .contains("protection")
    );
    assert!(stopped["inventory_devices"][1]["dc_limits"].is_null());
}

#[test]
fn balanced_sweep_shares_the_bus_with_independent_dc_and_protection() {
    let bridge = Bridge::new();
    let mut engine = Engine::new(bridge.dir.path());
    engine.connect(false);
    engine.send(json!({"command":"connect_inventory","id":20,"source":true,
        "simulation":false,"model":"YOKOGAWA_GS210","resource":"GPIB0::5::INSTR","mode":"dc_iv"}));
    engine.wait(|s| s["inventory_devices"][0]["output"] == "Off");
    let path = bridge.path("independent-balanced");
    let mut sweep = config();
    sweep.start_settle_s = 8.0;
    engine.send(json!({"command":"run","config":sweep,"path":path}));
    let running = engine.wait(|s| s["phase"] == "Running" && s["live"].is_object());
    assert_eq!(running["output"], "On");
    assert_eq!(running["source2_output"], "On");

    engine.send(json!({"command":"set_inventory_dc","id":20,"request":{
        "mode":"dc_iv","value":0.002,"compliance":0.2,"ramp_step":0.001,"range":0.1
    }}));
    let running = engine.wait(|s| s["inventory_devices"][0]["reading"]["value"] == 0.002);
    assert_eq!(running["phase"], "Running");
    assert_eq!(running["source2_output"], "On");
    bridge.marker("trip-5");
    let protected = engine.wait(|s| s["inventory_devices"][0]["output"] == "Off");
    assert_eq!(protected["phase"], "Running");
    assert_eq!(protected["output"], "On");
    assert!(
        protected["inventory_devices"][0]["error"]
            .as_str()
            .unwrap()
            .contains("protection")
    );

    engine.send(json!({"command":"stop"}));
    let stopped = engine.wait(|s| s["phase"] == "Stopped" && s["source2_output"] == "Off");
    assert_eq!(stopped["output"], "Off");
    assert_eq!(stopped["inventory_devices"][0]["output"], "Off");
    assert_eq!(stopped["error"], "");
    assert!(path.is_file());
}

#[test]
fn shared_helper_off_failure_aborts_sweep_and_rejoins_parked_source_for_recovery() {
    let bridge = Bridge::new();
    let mut engine = Engine::new(bridge.dir.path());
    engine.connect(false);
    engine.send(json!({"command":"connect_inventory","id":20,"source":true,
        "simulation":false,"model":"YOKOGAWA_GS210","resource":"GPIB0::5::INSTR","mode":"dc_iv"}));
    engine.wait(|s| s["inventory_devices"][0]["output"] == "Off");
    let path = bridge.path("recover-parked");
    let mut sweep = config();
    sweep.start_settle_s = 8.0;
    engine.send(json!({"command":"run","config":sweep,"path":path}));
    engine.wait(|s| s["phase"] == "Running" && s["live"].is_object());
    engine.send(json!({"command":"set_inventory_dc","id":20,"request":{
        "mode":"dc_iv","value":0.002,"compliance":0.2,"ramp_step":0.001,"range":0.1
    }}));
    engine.wait(|s| s["inventory_devices"][0]["output"] == "On");

    let before_off = bridge.log().len();
    bridge.marker("fail-off-once-5");
    engine.send(json!({"command":"inventory_dc_off","id":20}));
    let stopped =
        engine.wait(|s| s["phase"] == "Failed" && s["inventory_devices"][0]["output"] == "Off");
    assert_eq!(stopped["output"], "Off");
    assert_eq!(stopped["source2_output"], "Off");
    assert!(
        stopped["error"]
            .as_str()
            .unwrap()
            .contains("independent source")
    );
    let commands = bridge.log();
    for address in ["3", "4", "5"] {
        assert!(
            commands[before_off..]
                .iter()
                .any(|row| row["address"] == address && row["command"] == ":OUTP OFF")
        );
    }
    assert!(
        commands[before_off..]
            .iter()
            .any(|row| row["address"] == "5" && row["command"] == "*IDN?")
    );
}
