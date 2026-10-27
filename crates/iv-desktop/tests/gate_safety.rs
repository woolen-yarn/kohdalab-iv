//! Engine-level safety admission checks that require no physical instruments.
#![cfg(unix)]

use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader, Write},
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
        let deadline = Instant::now() + Duration::from_secs(8);
        while Instant::now() < deadline {
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
        panic!("engine state timed out");
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
        panic!("simulated engine did not exit");
    }
}

#[test]
fn single_source_run_rejects_an_attached_second_source_before_output() {
    let mut engine = Engine::start();
    engine.send(json!({"command":"connect","simulation":true,"source":"","meter":""}));
    engine.wait(|state| state["connected"] == true);
    engine.send(json!({"command":"connect_source2","simulation":true,"resource":"","model":"YOKOGAWA_GS210"}));
    engine.wait(|state| state["source2_connected"] == true);
    let path = std::env::temp_dir().join(format!("iv-gate-safety-{}.csv", std::process::id()));
    engine.send(json!({"command":"run","config":iv_core::RunConfig::default(),"path":path}));
    let state = engine.wait(|state| !state["error"].as_str().unwrap_or_default().is_empty());
    assert!(
        state["error"]
            .as_str()
            .unwrap()
            .contains("Source 2 is connected")
    );
    assert_eq!(state["output"], "Off");
    assert!(!path.exists());
}

#[test]
fn simulated_gate_only_accepts_a_finite_verified_setting() {
    let mut engine = Engine::start();
    engine.send(json!({"command":"connect","simulation":true,"source":"","meter":""}));
    engine.wait(|state| state["connected"] == true);
    engine.send(
        json!({"command":"connect_gate","simulation":true,"resource":"","model":"YOKOGAWA_GS210"}),
    );
    engine.wait(|state| state["gate_connected"] == true);
    engine.send(
        json!({"command":"set_gate_voltage","voltage":0.01,"compliance":0.001,"ramp_step":0.001}),
    );
    let state = engine.wait(|state| state["gate_output"] == "On" && state["gate_voltage"] == 0.01);
    assert_eq!(state["gate_output"], "On");
    assert_eq!(state["gate_voltage"], 0.01);
    assert_eq!(state["gate_compliance"], 0.001);
}

#[test]
fn disconnected_simulated_gate_cannot_recreate_an_on_state() {
    let mut engine = Engine::start();
    engine.send(json!({"command":"connect","simulation":true,"source":"","meter":""}));
    engine.wait(|state| state["connected"] == true);
    engine.send(
        json!({"command":"connect_gate","simulation":true,"resource":"","model":"YOKOGAWA_GS210"}),
    );
    engine.wait(|state| state["gate_connected"] == true);
    engine.send(json!({"command":"disconnect_gate"}));
    engine.wait(|state| state["gate_connected"] == false);
    engine.send(
        json!({"command":"set_gate_voltage","voltage":0.01,"compliance":0.001,"ramp_step":0.001}),
    );
    let state = engine.wait(|state| !state["error"].as_str().unwrap_or_default().is_empty());
    assert!(
        state["error"]
            .as_str()
            .unwrap()
            .contains("Connect the gate source")
    );
    assert_eq!(state["gate_connected"], false);
    assert_eq!(state["gate_output"], "Off");
}

#[test]
fn pending_run_reserves_gate_ownership_before_the_owner_thread_dequeues_it() {
    let mut engine = Engine::start();
    engine.send(json!({"command":"connect","simulation":true,"source":"","meter":""}));
    engine.wait(|s| s["connected"] == true);
    engine.send(json!({"command":"connect_gate","simulation":true,"resource":""}));
    engine.wait(|s| s["gate_connected"] == true);
    let output = tempfile::tempdir().unwrap();
    let config = iv_core::RunConfig {
        pattern: iv_core::Pattern::Linear,
        start: 0.001,
        stop: 0.001,
        settle_s: 0.1,
        ..Default::default()
    };
    engine
        .send(json!({"command":"run","config":config,"path":output.path().join("admission.csv")}));
    engine.send(
        json!({"command":"set_gate_voltage","voltage":0.02,"compliance":0.001,"ramp_step":0.001}),
    );
    let state = engine.wait(|s| {
        s["error"]
            .as_str()
            .is_some_and(|e| e.contains("Gate voltage"))
    });
    assert_ne!(state["gate_voltage"], 0.02);
    assert_eq!(state["gate_output"], "Off");
}

#[test]
fn latched_gate_error_requires_reconnection_before_a_retry_can_enable_output() {
    let mut engine = Engine::start();
    engine.send(json!({"command":"connect_gate","simulation":true,"resource":""}));
    engine.wait(|s| s["gate_connected"] == true);
    engine.send(
        json!({"command":"set_gate_voltage","voltage":0.01,"compliance":-1.0,"ramp_step":0.001}),
    );
    engine.wait(|s| s["phase"] == "Failed");
    engine.send(
        json!({"command":"set_gate_voltage","voltage":0.01,"compliance":0.001,"ramp_step":0.001}),
    );
    let state = engine.wait(|s| {
        s["error"]
            .as_str()
            .is_some_and(|e| e.contains("reconnect the gate"))
    });
    assert_eq!(state["gate_output"], "Off");
}
