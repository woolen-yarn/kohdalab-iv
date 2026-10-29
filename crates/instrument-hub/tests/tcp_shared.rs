use instrument_protocol::{
    Client, Endpoint, Envelope, ErrorCode, Hello, HelloReply, PROTOCOL_VERSION, Reply, Request,
    SafetyPolicy, read_json, write_json,
};
use kohdalab_instrument_hub::{
    core::{Controller, Core},
    server,
};
use std::collections::BTreeMap;
use std::io::BufReader;
use std::net::{Shutdown, TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::{
    Arc, Condvar, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use tempfile::TempDir;

const TIMEOUT: Duration = Duration::from_secs(2);
const SOURCE_MODEL: &str = "YOKOGAWA_GS210";

fn instrument_devices() -> BTreeMap<u8, SafetyPolicy> {
    BTreeMap::from([(3, SafetyPolicy::Instrument), (7, SafetyPolicy::Instrument)])
}

fn source_at_three_devices() -> BTreeMap<u8, SafetyPolicy> {
    BTreeMap::from([
        (
            3,
            SafetyPolicy::Source {
                model: SOURCE_MODEL.into(),
            },
        ),
        (7, SafetyPolicy::Instrument),
    ])
}

type QueryGate = Arc<(Mutex<bool>, Condvar)>;

#[derive(Clone, Default)]
struct FakeController {
    log: Arc<Mutex<Vec<String>>>,
    slow_query: bool,
    gate: Option<QueryGate>,
    fail_prepare: bool,
}

impl FakeController {
    fn logged(&self) -> Vec<String> {
        self.log.lock().unwrap().clone()
    }
}

impl Controller for FakeController {
    fn prepare(&mut self) -> Result<(), String> {
        if self.fail_prepare {
            self.log.lock().unwrap().push("FAILED_PREPARE".into());
            Err("fake controller initialization failed".into())
        } else {
            Ok(())
        }
    }
    fn transfer(
        &mut self,
        operation: &str,
        address: u8,
        _timeout: Duration,
        payload: &[u8],
    ) -> Result<String, String> {
        self.log.lock().unwrap().push(format!(
            "{operation} {address} {}",
            String::from_utf8_lossy(payload)
        ));
        if operation == "QUERY"
            && payload == b"BLOCK\n"
            && let Some(gate) = &self.gate
        {
            let _ = gate
                .1
                .wait_timeout_while(gate.0.lock().unwrap(), TIMEOUT, |released| !*released)
                .unwrap();
        }
        if self.slow_query && operation == "QUERY" {
            thread::sleep(Duration::from_millis(35));
        }
        Ok(match operation {
            "QUERY" => format!("reply-{address}"),
            "LIST" => "GPIB0::3::INSTR\nGPIB0::7::INSTR\n".into(),
            _ => String::new(),
        })
    }

    fn verify_source(&mut self, address: u8, model: &str) -> Result<(), String> {
        self.log
            .lock()
            .unwrap()
            .push(format!("VERIFY {address} {model}"));
        Ok(())
    }

    fn source_off(&mut self, address: u8, model: &str) -> Result<(), String> {
        self.log
            .lock()
            .unwrap()
            .push(format!("SOURCE_OFF {address} {model}"));
        Ok(())
    }
}

#[test]
fn failed_controller_preparation_is_latched_without_a_second_open() {
    let directory = TempDir::new().unwrap();
    let fake = FakeController {
        fail_prepare: true,
        ..Default::default()
    };
    let mut core = Core::new(
        fake.clone(),
        directory.path().join("sources.json"),
        instrument_devices(),
    )
    .unwrap();
    for address in [3, 7] {
        let error = core
            .request(
                "client",
                "KohdaLab IV",
                Request::Claim {
                    address,
                    policy: SafetyPolicy::Instrument,
                },
                TIMEOUT,
            )
            .unwrap_err();
        assert_eq!(error.code, ErrorCode::ControllerFault);
    }
    assert_eq!(fake.logged(), ["FAILED_PREPARE"]);
}

#[test]
fn startup_inventory_keeps_iv_visible_when_trkr_connects_first() {
    let directory = TempDir::new().unwrap();
    let fake = FakeController::default();
    let mut core = Core::new(
        fake.clone(),
        directory.path().join("sources.json"),
        instrument_devices(),
    )
    .unwrap();
    let initial = core
        .request("startup", "Instrument service", Request::List, TIMEOUT)
        .unwrap();
    core.request(
        "trkr",
        "KohdaLab TRKR",
        Request::Claim {
            address: 7,
            policy: SafetyPolicy::Instrument,
        },
        TIMEOUT,
    )
    .unwrap();
    let refreshed = core
        .request("iv", "KohdaLab IV", Request::List, TIMEOUT)
        .unwrap();
    assert_eq!(initial, refreshed);
    assert!(refreshed.contains("GPIB0::3::INSTR"));
    assert_eq!(
        fake.logged()
            .iter()
            .filter(|row| row.starts_with("LIST "))
            .count(),
        1
    );
}

#[test]
fn pipelined_request_and_half_close_never_dispatch_a_second_write() {
    let directory = TempDir::new().unwrap();
    let journal = directory.path().join("sources.json");
    let gate = Arc::new((Mutex::new(false), Condvar::new()));
    let fake = FakeController {
        gate: Some(gate.clone()),
        ..Default::default()
    };
    let mut fixture = Fixture::with_journal(directory, journal, fake, instrument_devices());
    let mut peer = fixture.client("KohdaLab TRKR");
    claim(&mut peer, 7, SafetyPolicy::Instrument);
    let mut raw = TcpStream::connect(fixture.endpoint.address).unwrap();
    raw.set_read_timeout(Some(TIMEOUT)).unwrap();
    let mut reader = BufReader::new(raw.try_clone().unwrap());
    write_json(
        &mut raw,
        &Hello {
            protocol: PROTOCOL_VERSION,
            token: fixture.endpoint.token.clone(),
            app: "raw pipeline".into(),
        },
    )
    .unwrap();
    let hello: HelloReply = read_json(&mut reader).unwrap();
    write_json(
        &mut raw,
        &Envelope {
            id: 1,
            request_nonce: hello.request_nonce,
            timeout_ms: 2000,
            request: Request::Claim {
                address: 3,
                policy: SafetyPolicy::Instrument,
            },
        },
    )
    .unwrap();
    let reply: Reply = read_json(&mut reader).unwrap();
    assert!(reply.error.is_none());
    write_json(
        &mut raw,
        &Envelope {
            id: 2,
            request_nonce: reply.request_nonce.clone(),
            timeout_ms: 2000,
            request: Request::Transfer {
                operation: "QUERY".into(),
                address: 3,
                timeout_ms: 2000,
                payload: b"BLOCK\n".to_vec(),
            },
        },
    )
    .unwrap();
    wait_for_log(&fixture.fake, "QUERY 3 BLOCK\n");
    write_json(
        &mut raw,
        &Envelope {
            id: 3,
            // Cannot know the next nonce without receiving query #2's reply.
            request_nonce: reply.request_nonce,
            timeout_ms: 2000,
            request: Request::Transfer {
                operation: "WRITE".into(),
                address: 3,
                timeout_ms: 2000,
                payload: b"MUST_NOT_RUN\n".to_vec(),
            },
        },
    )
    .unwrap();
    raw.shutdown(Shutdown::Write).unwrap();
    // The reader must reject pipelining without waiting for the in-flight
    // controller response, even though a half-closed peer can still receive.
    let rejected = read_json::<Reply>(&mut reader);
    *gate.0.lock().unwrap() = true;
    gate.1.notify_all();
    assert_eq!(rejected.unwrap_err().code, ErrorCode::ConnectionLost);
    wait_for_log(&fixture.fake, "LOCAL 3 ");
    assert!(
        !fixture
            .fake
            .logged()
            .iter()
            .any(|line| line.contains("MUST_NOT_RUN"))
    );
    assert_eq!(query(&mut peer, 7), "reply-7");
    drop(peer);
    fixture.shutdown();
}

struct Fixture {
    endpoint: Endpoint,
    fake: FakeController,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<Result<(), String>>>,
    _directory: TempDir,
}

impl Fixture {
    fn new() -> Self {
        let directory = TempDir::new().unwrap();
        let journal = directory.path().join("sources.json");
        Self::with_journal(
            directory,
            journal,
            FakeController::default(),
            instrument_devices(),
        )
    }

    fn slow() -> Self {
        let directory = TempDir::new().unwrap();
        let journal = directory.path().join("sources.json");
        Self::with_journal(
            directory,
            journal,
            FakeController {
                slow_query: true,
                ..Default::default()
            },
            instrument_devices(),
        )
    }

    fn with_journal(
        directory: TempDir,
        journal: PathBuf,
        fake: FakeController,
        devices: BTreeMap<u8, SafetyPolicy>,
    ) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = Endpoint {
            protocol: PROTOCOL_VERSION,
            address: listener.local_addr().unwrap(),
            token: "tcp-shared-test-token".into(),
        };
        let core = Core::new(fake.clone(), journal, devices).unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let server_stop = stop.clone();
        let server_endpoint = endpoint.clone();
        let thread =
            thread::spawn(move || server::run(listener, server_endpoint, core, server_stop));
        Self {
            endpoint,
            fake,
            stop,
            thread: Some(thread),
            _directory: directory,
        }
    }

    fn client(&self, app: &str) -> Client {
        Client::connect(&self.endpoint, app).unwrap()
    }

    fn shutdown(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            thread.join().unwrap().unwrap();
        }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn claim(client: &mut Client, address: u8, policy: SafetyPolicy) {
    client
        .request(Request::Claim { address, policy }, TIMEOUT)
        .unwrap();
}

fn query(client: &mut Client, address: u8) -> String {
    client
        .request(
            Request::Transfer {
                operation: "QUERY".into(),
                address,
                timeout_ms: 1_000,
                payload: b"*IDN?\n".to_vec(),
            },
            TIMEOUT,
        )
        .unwrap()
}

fn wait_for_log(fake: &FakeController, needle: &str) {
    let deadline = Instant::now() + TIMEOUT;
    while Instant::now() < deadline {
        if fake.logged().iter().any(|row| row == needle) {
            return;
        }
        thread::sleep(Duration::from_millis(10));
    }
    panic!("timed out waiting for {needle}; log={:?}", fake.logged());
}

fn wait_for_log_count(fake: &FakeController, needle: &str, expected_at_least: usize) {
    let deadline = Instant::now() + TIMEOUT;
    while Instant::now() < deadline {
        if fake
            .logged()
            .iter()
            .filter(|row| row.as_str() == needle)
            .count()
            >= expected_at_least
        {
            return;
        }
        thread::sleep(Duration::from_millis(10));
    }
    panic!(
        "timed out waiting for {expected_at_least} instances of {needle}; log={:?}",
        fake.logged()
    );
}

fn raw_client(endpoint: &Endpoint, app: &str) -> (TcpStream, BufReader<TcpStream>, String) {
    let mut stream = TcpStream::connect(endpoint.address).unwrap();
    stream.set_read_timeout(Some(TIMEOUT)).unwrap();
    stream.set_write_timeout(Some(TIMEOUT)).unwrap();
    let reader_stream = stream.try_clone().unwrap();
    let mut reader = BufReader::new(reader_stream);
    write_json(
        &mut stream,
        &Hello {
            protocol: PROTOCOL_VERSION,
            token: endpoint.token.clone(),
            app: app.into(),
        },
    )
    .unwrap();
    let hello: HelloReply = read_json(&mut reader).unwrap();
    assert!(hello.error.is_none());
    (stream, reader, hello.request_nonce)
}

fn raw_request(
    stream: &mut TcpStream,
    reader: &mut BufReader<TcpStream>,
    nonce: &mut String,
    id: u64,
    timeout_ms: u64,
    request: Request,
) -> Reply {
    write_json(
        stream,
        &Envelope {
            id,
            request_nonce: nonce.clone(),
            timeout_ms,
            request,
        },
    )
    .unwrap();
    let reply: Reply = read_json(reader).unwrap();
    *nonce = reply.request_nonce.clone();
    reply
}

#[test]
fn separate_addresses_share_one_service_without_reply_theft() {
    let mut fixture = Fixture::new();
    let mut iv = fixture.client("KohdaLab IV");
    let mut trkr = fixture.client("KohdaLab TRKR");
    claim(&mut iv, 3, SafetyPolicy::Instrument);
    claim(&mut trkr, 7, SafetyPolicy::Instrument);

    thread::scope(|scope| {
        let iv_query = scope.spawn(|| query(&mut iv, 3));
        let trkr_query = scope.spawn(|| query(&mut trkr, 7));
        assert_eq!(iv_query.join().unwrap(), "reply-3");
        assert_eq!(trkr_query.join().unwrap(), "reply-7");
    });

    drop(iv);
    drop(trkr);
    fixture.shutdown();
}

#[test]
fn same_address_is_exclusive_without_breaking_existing_owners() {
    let mut fixture = Fixture::new();
    let mut iv = fixture.client("KohdaLab IV");
    let mut trkr = fixture.client("KohdaLab TRKR");
    claim(&mut iv, 3, SafetyPolicy::Instrument);
    claim(&mut trkr, 7, SafetyPolicy::Instrument);

    let error = trkr
        .request(
            Request::Claim {
                address: 3,
                policy: SafetyPolicy::Instrument,
            },
            TIMEOUT,
        )
        .unwrap_err();
    assert_eq!(error.code, ErrorCode::ResourceInUse);
    assert_eq!(query(&mut iv, 3), "reply-3");
    assert_eq!(query(&mut trkr, 7), "reply-7");

    drop(iv);
    drop(trkr);
    fixture.shutdown();
}

#[test]
fn releasing_one_address_keeps_peer_usable_without_global_bus_commands() {
    let mut fixture = Fixture::new();
    let mut iv = fixture.client("KohdaLab IV");
    let mut trkr = fixture.client("KohdaLab TRKR");
    claim(&mut iv, 3, SafetyPolicy::Instrument);
    claim(&mut trkr, 7, SafetyPolicy::Instrument);
    iv.request(Request::Release { address: 3 }, TIMEOUT)
        .unwrap();
    assert_eq!(query(&mut trkr, 7), "reply-7");

    let log = fixture.fake.logged();
    assert!(log.iter().any(|row| row.starts_with("LOCAL 3 ")));
    assert!(
        !log.iter()
            .any(|row| row.starts_with("REN ") || row.starts_with("EXIT "))
    );
    drop(iv);
    drop(trkr);
    fixture.shutdown();
}

#[test]
fn configured_source_rejects_instrument_policy_downgrade_without_hardware_access() {
    let directory = TempDir::new().unwrap();
    let journal = directory.path().join("sources.json");
    let mut fixture = Fixture::with_journal(
        directory,
        journal,
        FakeController::default(),
        source_at_three_devices(),
    );
    let mut iv = fixture.client("KohdaLab IV");
    let mut trkr = fixture.client("KohdaLab TRKR");
    claim(&mut iv, 7, SafetyPolicy::Instrument);

    let before = fixture.fake.logged();
    let error = trkr
        .request(
            Request::Claim {
                address: 3,
                policy: SafetyPolicy::Instrument,
            },
            TIMEOUT,
        )
        .unwrap_err();
    assert_eq!(error.code, ErrorCode::InvalidRequest);
    assert_eq!(
        fixture.fake.logged(),
        before,
        "rejected policy must not touch hardware"
    );
    assert_eq!(query(&mut iv, 7), "reply-7");

    drop(iv);
    drop(trkr);
    fixture.shutdown();
}

#[test]
fn dropped_source_is_verified_off_before_the_address_can_be_reclaimed() {
    let directory = TempDir::new().unwrap();
    let journal = directory.path().join("sources.json");
    let mut fixture = Fixture::with_journal(
        directory,
        journal,
        FakeController::default(),
        source_at_three_devices(),
    );
    let mut iv = fixture.client("KohdaLab IV");
    claim(
        &mut iv,
        3,
        SafetyPolicy::Source {
            model: SOURCE_MODEL.into(),
        },
    );
    let off_before_drop = fixture
        .fake
        .logged()
        .iter()
        .filter(|row| row.as_str() == format!("SOURCE_OFF 3 {SOURCE_MODEL}"))
        .count();
    assert_eq!(
        off_before_drop, 1,
        "source claim must verify OFF before ownership"
    );
    drop(iv);
    wait_for_log_count(
        &fixture.fake,
        &format!("SOURCE_OFF 3 {SOURCE_MODEL}"),
        off_before_drop + 1,
    );

    let mut trkr = fixture.client("KohdaLab TRKR");
    claim(
        &mut trkr,
        3,
        SafetyPolicy::Source {
            model: SOURCE_MODEL.into(),
        },
    );
    assert_eq!(query(&mut trkr, 3), "reply-3");
    drop(trkr);
    fixture.shutdown();
}

#[test]
fn durable_source_journal_fences_restart_until_explicit_recovery() {
    let directory = TempDir::new().unwrap();
    let journal = directory.path().join("sources.json");
    let fake = FakeController::default();
    {
        let mut core = Core::new(fake.clone(), journal.clone(), source_at_three_devices()).unwrap();
        core.request(
            "crashed-iv",
            "KohdaLab IV",
            Request::Claim {
                address: 3,
                policy: SafetyPolicy::Source {
                    model: SOURCE_MODEL.into(),
                },
            },
            TIMEOUT,
        )
        .unwrap();
    }
    let mut fixture = Fixture::with_journal(directory, journal, fake, source_at_three_devices());
    let mut trkr = fixture.client("KohdaLab TRKR");
    let error = trkr
        .request(
            Request::Claim {
                address: 3,
                policy: SafetyPolicy::Source {
                    model: SOURCE_MODEL.into(),
                },
            },
            TIMEOUT,
        )
        .unwrap_err();
    assert_eq!(error.code, ErrorCode::StateUnknown);
    trkr.request(
        Request::Recover {
            address: 3,
            model: SOURCE_MODEL.into(),
        },
        TIMEOUT,
    )
    .unwrap();
    claim(
        &mut trkr,
        3,
        SafetyPolicy::Source {
            model: SOURCE_MODEL.into(),
        },
    );
    drop(trkr);
    fixture.shutdown();
}

#[test]
fn expired_queued_request_is_not_executed_or_reused_after_a_slow_peer_query() {
    let mut fixture = Fixture::slow();
    let mut iv = fixture.client("KohdaLab IV");
    claim(&mut iv, 3, SafetyPolicy::Instrument);
    let (mut trkr_stream, mut trkr_reader, mut nonce) =
        raw_client(&fixture.endpoint, "KohdaLab TRKR");
    assert!(
        raw_request(
            &mut trkr_stream,
            &mut trkr_reader,
            &mut nonce,
            1,
            1_000,
            Request::Claim {
                address: 7,
                policy: SafetyPolicy::Instrument,
            },
        )
        .error
        .is_none()
    );

    thread::scope(|scope| {
        let slow = scope.spawn(|| query(&mut iv, 3));
        wait_for_log(&fixture.fake, "QUERY 3 *IDN?\n");
        let expired = raw_request(
            &mut trkr_stream,
            &mut trkr_reader,
            &mut nonce,
            2,
            1,
            Request::Transfer {
                operation: "QUERY".into(),
                address: 7,
                timeout_ms: 1,
                payload: b"*IDN?\n".to_vec(),
            },
        );
        assert_eq!(expired.id, 2);
        assert_eq!(expired.error.unwrap().code, ErrorCode::Deadline);
        assert_eq!(slow.join().unwrap(), "reply-3");
    });
    // The late reply belongs only to address 3. The next address-7 request
    // receives a fresh response, proving it was neither executed nor stolen.
    let fresh = raw_request(
        &mut trkr_stream,
        &mut trkr_reader,
        &mut nonce,
        3,
        1_000,
        Request::Transfer {
            operation: "QUERY".into(),
            address: 7,
            timeout_ms: 1_000,
            payload: b"*IDN?\n".to_vec(),
        },
    );
    assert_eq!(fresh.payload, "reply-7");
    assert!(fresh.error.is_none());
    assert_eq!(
        fixture
            .fake
            .logged()
            .iter()
            .filter(|row| row.as_str() == "QUERY 7 *IDN?\n")
            .count(),
        1,
        "the expired address-7 request must never reach the controller"
    );
    let _ = trkr_stream.shutdown(Shutdown::Both);
    drop(iv);
    fixture.shutdown();
}

#[test]
fn invalid_handshakes_never_touch_the_controller() {
    let mut fixture = Fixture::new();
    let wrong_token = Endpoint {
        token: "wrong".into(),
        ..fixture.endpoint.clone()
    };
    let error = match Client::connect(&wrong_token, "intruder") {
        Ok(_) => panic!("wrong token unexpectedly authenticated"),
        Err(error) => error,
    };
    assert_eq!(error.code, ErrorCode::InvalidRequest);

    let mut stream = TcpStream::connect(fixture.endpoint.address).unwrap();
    stream.set_read_timeout(Some(TIMEOUT)).unwrap();
    let read_stream = stream.try_clone().unwrap();
    let mut reader = BufReader::new(read_stream);
    write_json(
        &mut stream,
        &Hello {
            protocol: PROTOCOL_VERSION + 1,
            token: fixture.endpoint.token.clone(),
            app: "intruder".into(),
        },
    )
    .unwrap();
    let reply: HelloReply = read_json(&mut reader).unwrap();
    assert_eq!(reply.error.unwrap().code, ErrorCode::IncompatibleVersion);
    let _ = stream.shutdown(Shutdown::Both);
    thread::sleep(Duration::from_millis(50));
    assert!(fixture.fake.logged().is_empty());
    fixture.shutdown();
}
