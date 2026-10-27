//! Authenticated loopback control of the existing GUI/device owner.
//! One external lease; GUI Stop always remains independent of the lease.
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    fs::OpenOptions,
    io::{BufRead, BufReader, Read, Write},
    net::{TcpListener, TcpStream},
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

const MAX_REQUEST: u64 = 1_048_576;
const LEASE_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Deserialize)]
#[serde(tag = "method", rename_all = "snake_case")]
pub enum Action {
    Status,
    Configuration,
    Claim,
    Heartbeat,
    Release,
    Start {
        filename: Option<String>,
        directory: Option<String>,
    },
    Stop,
    OutputOff,
    ConnectGate {
        resource: String,
        #[serde(default)]
        simulation: bool,
        #[serde(default = "gate_model")]
        model: String,
    },
    SetGateVoltage {
        voltage: f64,
        compliance: f64,
        ramp_step: f64,
    },
    GateOutputOff,
    DisconnectGate,
}
fn gate_model() -> String {
    "YOKOGAWA_GS210".into()
}
#[derive(Debug, Deserialize)]
struct WireRequest {
    token: String,
    lease: Option<String>,
    #[serde(flatten)]
    action: Action,
}
pub struct Request {
    pub action: Action,
    lease: Option<String>,
    reply: mpsc::SyncSender<Value>,
    expires: Instant,
}
impl Request {
    pub fn respond(self, result: Result<Value, String>) {
        let value = match result {
            Ok(data) => json!({"ok":true,"data":data}),
            Err(error) => json!({"ok":false,"error":error}),
        };
        let _ = self.reply.send(value);
    }
}
#[derive(Serialize, Deserialize)]
struct Endpoint {
    protocol: u32,
    pid: u32,
    address: String,
    token: String,
}

pub struct GuiControl {
    endpoint: PathBuf,
    stop: Arc<AtomicBool>,
    requests: mpsc::Receiver<Request>,
    lease: Option<(String, Instant)>,
}
impl GuiControl {
    pub fn start() -> Result<Self, String> {
        let home = std::env::var_os("USERPROFILE")
            .or_else(|| std::env::var_os("HOME"))
            .ok_or("No user profile for GUI control")?;
        let directory = PathBuf::from(home).join(".kohdalab/control");
        std::fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
        Self::in_directory(directory)
    }
    fn in_directory(directory: PathBuf) -> Result<Self, String> {
        let listener =
            TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).map_err(|e| e.to_string())?;
        listener.set_nonblocking(true).map_err(|e| e.to_string())?;
        let token = uuid::Uuid::new_v4().to_string();
        let endpoint = directory.join(format!(
            "iv-{}-{}.json",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        let declaration = Endpoint {
            protocol: 1,
            pid: std::process::id(),
            address: listener
                .local_addr()
                .map_err(|e| e.to_string())?
                .to_string(),
            token: token.clone(),
        };
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&endpoint).map_err(|e| e.to_string())?;
        if let Err(error) = serde_json::to_writer(&mut file, &declaration)
            .and_then(|()| file.flush().map_err(serde_json::Error::io))
        {
            let _ = std::fs::remove_file(&endpoint);
            return Err(error.to_string());
        }
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = stop.clone();
        let (send, requests) = mpsc::sync_channel(16);
        std::thread::spawn(move || {
            let clients = Arc::new(AtomicUsize::new(0));
            while !thread_stop.load(Ordering::Acquire) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        if clients.load(Ordering::Acquire) >= 4 {
                            drop(stream);
                            continue;
                        }
                        clients.fetch_add(1, Ordering::AcqRel);
                        let clients = clients.clone();
                        let send = send.clone();
                        let token = token.clone();
                        std::thread::spawn(move || {
                            let _ = serve(stream, &token, send);
                            clients.fetch_sub(1, Ordering::AcqRel);
                        });
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(20))
                    }
                    Err(_) => break,
                }
            }
        });
        Ok(Self {
            endpoint,
            stop,
            requests,
            lease: None,
        })
    }
    /// Expiration is consumed once; the GUI routes this through OutputOff.
    pub fn expired(&mut self) -> bool {
        if self
            .lease
            .as_ref()
            .is_some_and(|(_, seen)| seen.elapsed() > LEASE_TIMEOUT)
        {
            self.lease = None;
            true
        } else {
            false
        }
    }
    pub fn next(&self) -> Option<Request> {
        self.requests.try_recv().ok()
    }
    /// Returns Some for requests fully handled by the lease layer.
    pub fn authorize(&mut self, request: &Request) -> Result<Option<Value>, String> {
        if Instant::now() >= request.expires {
            return Err("GUI request expired before execution. No action was taken.".into());
        }
        match request.action {
            Action::Status | Action::Configuration => Ok(None),
            Action::Claim => {
                if self.lease.is_some() {
                    return Err(
                        "Another Python client controls this GUI. Close it before attaching."
                            .into(),
                    );
                }
                let lease = uuid::Uuid::new_v4().to_string();
                self.lease = Some((lease.clone(), Instant::now()));
                Ok(Some(
                    json!({"lease":lease,"heartbeat_seconds":1,"timeout_seconds":LEASE_TIMEOUT.as_secs()}),
                ))
            }
            _ => {
                let Some((lease, seen)) = &mut self.lease else {
                    return Err("Claim a GUI control lease before changing devices or starting measurement.".into());
                };
                if request.lease.as_ref() != Some(lease) {
                    return Err("Invalid GUI control lease.".into());
                }
                *seen = Instant::now();
                if matches!(request.action, Action::Heartbeat) {
                    Ok(Some(json!({"alive":true})))
                } else {
                    Ok(None)
                }
            }
        }
    }
    pub fn has_lease(&self) -> bool {
        self.lease.is_some()
    }
    pub fn release(&mut self) {
        self.lease = None;
    }
}
impl Drop for GuiControl {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        let _ = std::fs::remove_file(&self.endpoint);
    }
}
fn serve(
    mut stream: TcpStream,
    token: &str,
    send: mpsc::SyncSender<Request>,
) -> Result<(), String> {
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .map_err(|e| e.to_string())?;
    stream
        .set_write_timeout(Some(Duration::from_secs(2)))
        .map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    BufReader::new(&stream)
        .take(MAX_REQUEST + 1)
        .read_until(b'\n', &mut bytes)
        .map_err(|e| e.to_string())?;
    let result = (|| {
        if bytes.len() as u64 > MAX_REQUEST || bytes.last() != Some(&b'\n') {
            return Err("Invalid GUI control request size".into());
        }
        let request: WireRequest = serde_json::from_slice(&bytes)
            .map_err(|_| "Invalid GUI control request".to_string())?;
        if request.token != token {
            return Err("GUI control authentication failed".into());
        }
        let (reply, receive) = mpsc::sync_channel(1);
        send.try_send(Request {
            action: request.action,
            lease: request.lease,
            reply,
            expires: Instant::now() + Duration::from_secs(10),
        })
        .map_err(|_| "GUI control queue is full".to_string())?;
        receive
            .recv_timeout(Duration::from_secs(10))
            .map_err(|_| "GUI is not responding; inspect its screen before retrying".to_string())
    })();
    let value = result.unwrap_or_else(|error| json!({"ok":false,"error":error}));
    serde_json::to_writer(&mut stream, &value).map_err(|e| e.to_string())?;
    stream.write_all(b"\n").map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn request(action: Action, lease: Option<String>) -> Request {
        Request {
            action,
            lease,
            reply: mpsc::sync_channel(1).0,
            expires: Instant::now() + Duration::from_secs(10),
        }
    }
    #[test]
    fn mutation_requires_exclusive_live_lease() {
        let dir = tempfile::tempdir().unwrap();
        let mut control = GuiControl::in_directory(dir.path().into()).unwrap();
        assert!(control.authorize(&request(Action::Stop, None)).is_err());
        let lease = control
            .authorize(&request(Action::Claim, None))
            .unwrap()
            .unwrap()["lease"]
            .as_str()
            .unwrap()
            .to_owned();
        assert!(control.authorize(&request(Action::Claim, None)).is_err());
        assert!(
            control
                .authorize(&request(Action::Stop, Some("wrong".into())))
                .is_err()
        );
        assert!(
            control
                .authorize(&request(Action::Stop, Some(lease)))
                .unwrap()
                .is_none()
        );
        control.lease.as_mut().unwrap().1 = Instant::now() - LEASE_TIMEOUT - Duration::from_secs(1);
        assert!(control.expired());
        assert!(!control.expired());
        assert!(
            control
                .authorize(&request(Action::OutputOff, None))
                .is_err()
        );
    }
    #[test]
    fn listener_rejects_wrong_token_and_removes_discovery_file() {
        let dir = tempfile::tempdir().unwrap();
        let control = GuiControl::in_directory(dir.path().into()).unwrap();
        let endpoint: Endpoint =
            serde_json::from_slice(&std::fs::read(&control.endpoint).unwrap()).unwrap();
        let mut stream = TcpStream::connect(&endpoint.address).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        writeln!(stream, "{{\"token\":\"wrong\",\"method\":\"status\"}}").unwrap();
        let mut line = String::new();
        BufReader::new(stream).read_line(&mut line).unwrap();
        assert_eq!(serde_json::from_str::<Value>(&line).unwrap()["ok"], false);
        let path = control.endpoint.clone();
        drop(control);
        assert!(!path.exists());
    }
}
