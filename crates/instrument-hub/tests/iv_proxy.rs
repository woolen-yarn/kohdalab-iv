//! Exercises the real IV transport through the actual stdio proxy, without
//! environment mutation in the test runner or any physical controller access.
use instrument_protocol::{Client, Endpoint, PROTOCOL_VERSION, Request, SafetyPolicy};
use iv_transport::{DevicePair, HelperPaths, Role};
use kohdalab_instrument_hub::{
    core::{Controller, Core},
    server,
};
use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::{Child, Command, Output, Stdio};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::thread;
use std::time::{Duration, Instant};

const SOURCE_MODEL: &str = "YOKOGAWA_GS210";
const CHILD_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Clone, Default)]
struct Fake {
    log: Arc<Mutex<Vec<String>>>,
}
impl Controller for Fake {
    fn transfer(
        &mut self,
        op: &str,
        address: u8,
        _: Duration,
        payload: &[u8],
    ) -> Result<String, String> {
        let command = String::from_utf8_lossy(payload);
        self.log
            .lock()
            .unwrap()
            .push(format!("{op} {address} {command}"));
        Ok(if op == "QUERY" && command.trim() == ":OUTP?" {
            "0".into()
        } else if op == "QUERY" {
            match address {
                3 => "YOKOGAWA,GS210,TEST_SOURCE,1.0".into(),
                5 => "Agilent Technologies,34411A,TEST_METER,1.0".into(),
                7 => "TRKR_TEST_REPLY".into(),
                _ => return Err("Unexpected fake address".into()),
            }
        } else {
            String::new()
        })
    }
    fn verify_source(&mut self, address: u8, _: &str) -> Result<(), String> {
        assert_eq!(address, 3);
        Ok(())
    }
    fn source_off(&mut self, address: u8, _: &str) -> Result<(), String> {
        self.log
            .lock()
            .unwrap()
            .push(format!("VERIFIED_OFF {address}"));
        Ok(())
    }
}

struct Fixture {
    endpoint: Endpoint,
    endpoint_file: PathBuf,
    fake: Fake,
    stop: Arc<AtomicBool>,
    server: Option<thread::JoinHandle<Result<(), String>>>,
    _directory: tempfile::TempDir,
}

impl Fixture {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = Endpoint {
            protocol: PROTOCOL_VERSION,
            address: listener.local_addr().unwrap(),
            token: uuid::Uuid::new_v4().to_string(),
        };
        let fake = Fake::default();
        let devices = BTreeMap::from([
            (
                3,
                SafetyPolicy::Source {
                    model: SOURCE_MODEL.into(),
                },
            ),
            (5, SafetyPolicy::Instrument),
            (7, SafetyPolicy::Instrument),
        ]);
        let core = Core::new(fake.clone(), directory.path().join("source.json"), devices).unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let server_stop = stop.clone();
        let server_endpoint = endpoint.clone();
        let server =
            thread::spawn(move || server::run(listener, server_endpoint, core, server_stop));
        let endpoint_file = directory.path().join("endpoint.json");
        std::fs::write(&endpoint_file, serde_json::to_vec(&endpoint).unwrap()).unwrap();
        Self {
            endpoint,
            endpoint_file,
            fake,
            stop,
            server: Some(server),
            _directory: directory,
        }
    }

    fn client(&self, app: &str) -> Client {
        Client::connect(&self.endpoint, app).unwrap()
    }

    fn configure_child(&self, command: &mut Command) {
        command
            .env("KOHDALAB_HUB_FAKE_CHILD", "1")
            .env("KOHDALAB_INSTRUMENT_HUB_CONFIG", &self.endpoint_file)
            .env(
                "KOHDALAB_INSTRUMENT_HUB_CLIENT",
                env!("CARGO_BIN_EXE_kohdalab-instrument-hub"),
            )
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
    }

    fn spawn_iv_child(&self) -> ChildGuard {
        let mut command = Command::new(std::env::current_exe().unwrap());
        command.args(["--exact", "isolated_iv_child", "--ignored", "--nocapture"]);
        self.configure_child(&mut command);
        ChildGuard::new("IV child", command.spawn().unwrap())
    }

    fn spawn_trkr_probe(&self, probe: impl AsRef<OsStr>) -> ChildGuard {
        let mut command = Command::new(probe);
        self.configure_child(&mut command);
        ChildGuard::new("TRKR child", command.spawn().unwrap())
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(server) = self.server.take() {
            let _ = server.join();
        }
    }
}

struct ChildGuard {
    _label: &'static str,
    child: Option<Child>,
}

impl ChildGuard {
    fn new(label: &'static str, child: Child) -> Self {
        Self {
            _label: label,
            child: Some(child),
        }
    }

    fn wait(&mut self) -> Output {
        let deadline = Instant::now() + CHILD_TIMEOUT;
        while self.child.as_mut().unwrap().try_wait().unwrap().is_none()
            && Instant::now() < deadline
        {
            thread::sleep(Duration::from_millis(20));
        }
        if self.child.as_mut().unwrap().try_wait().unwrap().is_none() {
            let _ = self.child.as_mut().unwrap().kill();
        }
        self.child.take().unwrap().wait_with_output().unwrap()
    }
}

impl Drop for ChildGuard {
    fn drop(&mut self) {
        if let Some(child) = self.child.as_mut() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

#[test]
#[ignore = "spawned by real_iv_transport_coexists_with_peer; never accesses hardware"]
fn isolated_iv_child() {
    assert_eq!(std::env::var("KOHDALAB_HUB_FAKE_CHILD").as_deref(), Ok("1"));
    let dummy = HelperPaths {
        gpib: "MUST_NOT_LAUNCH_NATIVE".into(),
        usbtmc: "MUST_NOT_LAUNCH_USB".into(),
        setup: None,
    };
    let mut pair = DevicePair::new(&dummy);
    pair.connect_model(Role::Source, "GPIB0::3::INSTR", "YOKOGAWA_GS210")
        .unwrap();
    pair.connect_model(Role::Meter, "GPIB0::5::INSTR", "AGILENT_34411A")
        .unwrap();
    for _ in 0..20 {
        assert!(
            pair.query(Role::Source, "*IDN?\n")
                .unwrap()
                .starts_with("YOKOGAWA,GS210")
        );
        assert!(
            pair.query(Role::Meter, "*IDN?\n")
                .unwrap()
                .contains("34411A")
        );
        thread::sleep(Duration::from_millis(5));
    }
    pair.force_output_off().unwrap();
    pair.close().unwrap();
}

#[test]
fn real_iv_transport_coexists_with_peer() {
    let fixture = Fixture::new();
    let mut peer = fixture.client("KohdaLab TRKR");
    peer.request(
        Request::Claim {
            address: 7,
            policy: SafetyPolicy::Instrument,
        },
        Duration::from_secs(2),
    )
    .unwrap();
    let mut child = fixture.spawn_iv_child();
    for _ in 0..30 {
        let reply = peer
            .request(
                Request::Transfer {
                    operation: "QUERY".into(),
                    address: 7,
                    timeout_ms: 1000,
                    payload: b"OUTP?\n".to_vec(),
                },
                Duration::from_secs(2),
            )
            .unwrap();
        assert_eq!(reply, "TRKR_TEST_REPLY");
        thread::sleep(Duration::from_millis(5));
    }
    let output = child.wait();
    // Check the peer after IV's actual LOCAL/RELEASE/REN/EXIT teardown.
    let peer_after = peer.request(
        Request::Transfer {
            operation: "QUERY".into(),
            address: 7,
            timeout_ms: 1000,
            payload: b"OUTP?\n".to_vec(),
        },
        Duration::from_secs(2),
    );
    drop(peer);
    assert!(
        output.status.success(),
        "IV child: {} {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(peer_after.unwrap(), "TRKR_TEST_REPLY");
    let log = fixture.fake.log.lock().unwrap();
    assert!(log.iter().any(|s| s == "VERIFIED_OFF 3"));
    assert!(
        !log.iter()
            .any(|s| s.starts_with("REN ") || s.starts_with("EXIT "))
    );
}

#[test]
#[ignore = "requires a separately built probe linked to the updated TRKR transport"]
fn actual_trkr_transport_keeps_iv_source_connected() {
    let probe = std::env::var_os("KOHDALAB_TRKR_SHARED_PROBE").expect("TRKR probe path");
    let fixture = Fixture::new();
    let mut iv = fixture.client("KohdaLab IV");
    iv.request(
        Request::Claim {
            address: 3,
            policy: SafetyPolicy::Source {
                model: "YOKOGAWA_GS210".into(),
            },
        },
        Duration::from_secs(2),
    )
    .unwrap();
    let mut child = fixture.spawn_trkr_probe(probe);
    let query = |iv: &mut Client| {
        iv.request(
            Request::Transfer {
                operation: "QUERY".into(),
                address: 3,
                timeout_ms: 1000,
                payload: b"*IDN?\n".to_vec(),
            },
            Duration::from_secs(2),
        )
    };
    for _ in 0..30 {
        assert!(query(&mut iv).unwrap().contains("GS210"));
        thread::sleep(Duration::from_millis(5));
    }
    let output = child.wait();
    let after = query(&mut iv);
    // TRKR's EXIT must not turn off or release another app's source.
    let off_before_iv_release = fixture
        .fake
        .log
        .lock()
        .unwrap()
        .iter()
        .filter(|s| s.as_str() == "VERIFIED_OFF 3")
        .count();
    iv.request(Request::Close, Duration::from_secs(2)).unwrap();
    drop(iv);
    assert!(
        output.status.success(),
        "TRKR child: {} {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(after.unwrap().contains("GS210"));
    assert_eq!(off_before_iv_release, 1);
    assert!(
        fixture
            .fake
            .log
            .lock()
            .unwrap()
            .iter()
            .any(|s| s.starts_with("QUERY 7 "))
    );
}

#[test]
#[ignore = "requires a separately built probe linked to the updated TRKR transport"]
fn actual_iv_and_trkr_transports_share_controller() {
    let probe = std::env::var_os("KOHDALAB_TRKR_SHARED_PROBE").expect("TRKR probe path");
    let fixture = Fixture::new();
    let mut iv = fixture.spawn_iv_child();
    let mut trkr = fixture.spawn_trkr_probe(probe);

    let iv_output = iv.wait();
    let trkr_output = trkr.wait();
    assert!(
        iv_output.status.success(),
        "IV child: {} {}",
        String::from_utf8_lossy(&iv_output.stdout),
        String::from_utf8_lossy(&iv_output.stderr)
    );
    assert!(
        trkr_output.status.success(),
        "TRKR child: {} {}",
        String::from_utf8_lossy(&trkr_output.stdout),
        String::from_utf8_lossy(&trkr_output.stderr)
    );

    let log = fixture.fake.log.lock().unwrap();
    for address in [3, 5, 7] {
        assert!(
            log.iter()
                .any(|row| row.starts_with(&format!("QUERY {address} "))),
            "missing query for GPIB address {address}: {log:?}"
        );
    }
    assert!(log.iter().any(|row| row == "VERIFIED_OFF 3"));
    assert!(
        !log.iter()
            .any(|row| row.starts_with("REN ") || row.starts_with("EXIT "))
    );
}
