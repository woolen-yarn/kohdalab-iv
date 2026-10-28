//! Authenticated, bounded loopback API. Device I/O belongs to the GUI's engine.
use crate::protocol::{Command, Snapshot};
use crate::state::Phase;
use serde_json::{Value, json};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};
const MAX_REQUEST: usize = 1_048_576;
const LEASE_TIMEOUT: Duration = Duration::from_secs(10);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

pub fn error_code(message: &str) -> &'static str {
    let text = message.to_ascii_lowercase();
    if text.contains("request_id_conflict") {
        "request_id_conflict"
    } else if text.contains("unknown or expired run") {
        "unknown_job"
    } else if text.contains("authentication") {
        "authentication"
    } else if text.contains("expired")
        || text.contains("revoked")
        || text.contains("lease")
        || text.contains("control belongs")
    {
        "ownership_lost"
    } else if text.contains("busy")
        || text.contains("pending")
        || text.contains("disconnect") && text.contains("before")
    {
        "busy"
    } else if text.contains("timeout")
        || text.contains("timed out")
        || text.contains("not responding")
    {
        "timeout"
    } else if text.contains("engine") || text.contains("reconnect") {
        "disconnected"
    } else {
        "invalid_request"
    }
}
pub fn response(result: Result<Value, String>) -> Value {
    match result {
        Ok(data) => json!({"ok":true,"result":data,"data":data}),
        Err(error) => json!({"ok":false,"error_code":error_code(&error),"error":error}),
    }
}
pub struct Request {
    pub body: Value,
    pub method: String,
    reply: mpsc::SyncSender<Value>,
    expires: Instant,
}
impl Request {
    pub fn respond(self, result: Result<Value, String>) {
        let _ = self.reply.send(response(result));
    }
}
struct Owner {
    lease: String,
    client: String,
    label: String,
    seen: Instant,
}
struct Active {
    id: String,
    tag: Option<String>,
    commands: VecDeque<Command>,
    safety: u64,
}
pub struct GuiControl {
    endpoint: PathBuf,
    stop: Arc<AtomicBool>,
    requests: mpsc::Receiver<Request>,
    owner: Option<Owner>,
    revoked: HashSet<String>,
    jobs: VecDeque<Value>,
    used: HashSet<String>,
    fingerprints: HashMap<String, String>,
    active: Option<Active>,
    retiring: bool,
    retire_safety: u64,
}
impl GuiControl {
    pub fn start() -> Result<Self, String> {
        let home = std::env::var_os("USERPROFILE")
            .or_else(|| std::env::var_os("HOME"))
            .ok_or("No user profile for GUI control")?;
        Self::in_directory(PathBuf::from(home).join(".kohdalab/control"))
    }
    fn in_directory(directory: PathBuf) -> Result<Self, String> {
        std::fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
        let listener =
            TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).map_err(|e| e.to_string())?;
        listener.set_nonblocking(true).map_err(|e| e.to_string())?;
        let token = uuid::Uuid::new_v4().to_string();
        let instance = uuid::Uuid::new_v4().to_string();
        let endpoint = directory.join(format!("iv-{}-{instance}.json", std::process::id()));
        let declaration = json!({"protocol":2,"pid":std::process::id(),"address":listener.local_addr().map_err(|e|e.to_string())?.to_string(),"token":token,"instance_id":instance});
        // Publish atomically, with private permissions on Unix.
        let mut file = tempfile::NamedTempFile::new_in(&directory).map_err(|e| e.to_string())?;
        serde_json::to_writer(&mut file, &declaration).map_err(|e| e.to_string())?;
        file.flush().map_err(|e| e.to_string())?;
        file.persist_noclobber(&endpoint)
            .map_err(|e| e.to_string())?;
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = stop.clone();
        let (send, requests) = mpsc::sync_channel(16);
        std::thread::spawn(move || {
            let clients = Arc::new(AtomicUsize::new(0));
            while !thread_stop.load(Ordering::Acquire) {
                match listener.accept() {
                    Ok((stream, peer)) if peer.ip().is_loopback() => {
                        if clients.load(Ordering::Acquire) >= 4 {
                            drop(stream);
                            continue;
                        }
                        clients.fetch_add(1, Ordering::AcqRel);
                        let (clients, send, token) = (clients.clone(), send.clone(), token.clone());
                        std::thread::spawn(move || {
                            let _ = serve(stream, &token, send);
                            clients.fetch_sub(1, Ordering::AcqRel);
                        });
                    }
                    Ok(_) => {}
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
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
            owner: None,
            revoked: HashSet::new(),
            jobs: VecDeque::new(),
            used: HashSet::new(),
            fingerprints: HashMap::new(),
            active: None,
            retiring: false,
            retire_safety: 0,
        })
    }
    pub fn next(&self) -> Option<Request> {
        self.requests.try_recv().ok()
    }
    pub fn has_lease(&self) -> bool {
        self.owner.is_some() || self.retiring || self.active.is_some()
    }
    pub fn is_retiring(&self) -> bool {
        self.retiring
    }
    pub fn expired(&mut self, safety: u64) -> bool {
        if self
            .owner
            .as_ref()
            .is_some_and(|o| o.seen.elapsed() > LEASE_TIMEOUT)
            && !self.retiring
        {
            self.revoke(safety);
            true
        } else {
            false
        }
    }
    /// Revoke immediately, but retain GUI lock until engine safety cleanup acknowledges OFF.
    pub fn release(&mut self, safety: u64) {
        self.owner = None;
        self.retire_safety = safety;
        self.retiring = true;
    }
    pub fn revoke(&mut self, safety: u64) {
        self.retire_safety = safety;
        if let Some(o) = self.owner.take() {
            self.revoked.insert(o.client);
        }
        self.retiring = true;
        if let Some(active) = &mut self.active {
            active.commands.clear();
        }
    }
    /// A dead engine cannot provide another OFF acknowledgement. Unlock the
    /// GUI for diagnosis, never claim physical outputs were made safe.
    pub fn engine_lost(&mut self, error: String) {
        self.revoke(0);
        self.fail_dispatch(error);
        self.retiring = false;
    }
    pub fn settle_release(&mut self, state: &Snapshot) {
        if self.retiring
            && self.active.is_none()
            && !busy(state)
            && state.safety_revision > self.retire_safety
            && outputs_off(state)
        {
            self.retiring = false;
        }
    }
    pub fn control(&self) -> Value {
        json!({"owned":self.owner.is_some(),"client_id":self.owner.as_ref().map(|o|&o.client),"label":self.owner.as_ref().map(|o|&o.label),"status":if self.retiring {"detached"} else if self.owner.is_some(){"python"}else{"idle"},"active_run":self.active.as_ref().and_then(|a|self.result(&a.id).ok())})
    }
    pub fn authorize(
        &mut self,
        request: &Request,
        gui_idle: bool,
    ) -> Result<Option<Value>, String> {
        if Instant::now() >= request.expires {
            return Err("GUI request expired before execution. No action was taken.".into());
        }
        match request.method.as_str() {
            "status" | "config" | "configuration" | "plan" | "output" | "result"
            | "capabilities" => Ok(None),
            "acquire" | "claim" => {
                let client = request
                    .body
                    .get("client_id")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
                    .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
                uuid::Uuid::parse_str(&client).map_err(|_| "client_id must be a UUID")?;
                if self.revoked.contains(&client) {
                    return Err("GUI control was revoked. Create a new client.".into());
                }
                if let Some(owner) = &mut self.owner {
                    if owner.client != client {
                        return Err("Control belongs to another Python client.".into());
                    }
                    owner.seen = Instant::now();
                } else {
                    if !gui_idle || self.has_lease() {
                        return Err("GUI is busy. Finish operations and close dialogs before acquiring control.".into());
                    }
                    if self.revoked.len() >= 100_000 {
                        return Err("GUI API session limit reached; restart GUI.".into());
                    }
                    let label = request
                        .body
                        .get("label")
                        .and_then(Value::as_str)
                        .unwrap_or("Python");
                    if label.chars().count() > 48 {
                        return Err("Client label is limited to 48 characters".into());
                    }
                    self.owner = Some(Owner {
                        lease: uuid::Uuid::new_v4().to_string(),
                        client,
                        label: label.into(),
                        seen: Instant::now(),
                    });
                }
                let mut result = self.control();
                result["lease"] = json!(self.owner.as_ref().unwrap().lease);
                result["heartbeat_seconds"] = json!(1);
                result["timeout_seconds"] = json!(10);
                Ok(Some(result))
            }
            _ => {
                let owner = self
                    .owner
                    .as_mut()
                    .ok_or("GUI control lease is not held or was revoked")?;
                let lease = request.body.get("lease").and_then(Value::as_str);
                let client = request.body.get("client_id").and_then(Value::as_str);
                if lease != Some(owner.lease.as_str())
                    || client.is_some_and(|id| id != owner.client)
                {
                    return Err("Invalid GUI control lease".into());
                }
                owner.seen = Instant::now();
                if request.method == "heartbeat" {
                    let mut v = self.control();
                    v["alive"] = json!(true);
                    Ok(Some(v))
                } else {
                    Ok(None)
                }
            }
        }
    }
    pub fn result(&self, id: &str) -> Result<Value, String> {
        self.jobs
            .iter()
            .find(|j| j["id"] == id)
            .cloned()
            .ok_or_else(|| "Unknown or expired run ID".into())
    }
    pub fn job_id(&self, body: &Value) -> Result<String, String> {
        let id = body
            .get("request_id")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        uuid::Uuid::parse_str(&id).map_err(|_| "request_id must be a UUID")?;
        Ok(id)
    }
    pub fn active_id(&self) -> Option<&str> {
        self.active.as_ref().map(|a| a.id.as_str())
    }
    pub fn existing_job(
        &self,
        id: &str,
        kind: &str,
        body: &Value,
    ) -> Result<Option<Value>, String> {
        if let Some(expected) = self.fingerprints.get(id) {
            if expected != &request_fingerprint(kind, body) {
                return Err(
                    "request_id_conflict: request_id belongs to another operation or payload"
                        .into(),
                );
            }
            return self.result(id).map(Some);
        }
        Ok(None)
    }
    pub fn begin_job(
        &mut self,
        id: String,
        kind: &str,
        commands: Vec<Command>,
        state: &Snapshot,
        body: &Value,
    ) -> Result<Value, String> {
        if let Some(existing) = self.existing_job(&id, kind, body)? {
            return Ok(existing);
        }
        if self.used.contains(&id) {
            return Err("Run receipt expired; request_id cannot be reused".into());
        }
        if self.has_active() || busy(state) {
            return Err("GUI is busy".into());
        }
        if self.used.len() >= 100_000 {
            return Err("GUI API session limit reached; restart GUI".into());
        }
        let receipt = json!({"id":id,"status":"running","kind":kind,"rows":0,"output":null,"error":null,"error_code":null,"snapshot":null});
        if self.jobs.len() == 32 {
            self.jobs.pop_front();
        }
        self.jobs.push_back(receipt.clone());
        self.used.insert(id.clone());
        self.fingerprints
            .insert(id.clone(), request_fingerprint(kind, body));
        self.active = Some(Active {
            id,
            tag: None,
            commands: commands.into(),
            safety: state.safety_revision,
        });
        Ok(receipt)
    }
    pub fn has_active(&self) -> bool {
        self.active.is_some()
    }
    pub fn cancel_pending(&mut self) {
        if let Some(a) = &mut self.active {
            a.commands.clear();
        }
    }
    fn finish(&mut self, snapshot: Value, status: &str, code: Option<&str>, error: Option<String>) {
        let Some(active) = self.active.take() else {
            return;
        };
        if let Some(job) = self.jobs.iter_mut().find(|j| j["id"] == active.id) {
            job["status"] = json!(status);
            job["error"] = json!(error);
            job["error_code"] = json!(code);
            job["rows"] = snapshot.get("points").cloned().unwrap_or(json!(0));
            if job["kind"] == "start" {
                job["output"] = snapshot.get("path").cloned().unwrap_or(Value::Null);
            }
            job["snapshot"] = snapshot;
        }
    }
    pub fn fail_dispatch(&mut self, error: String) {
        self.finish(Value::Null, "error", Some("disconnected"), Some(error));
    }
    /// Consume only tagged completion, never an unrelated idle/live update.
    pub fn advance(&mut self, state: &Snapshot) -> Option<(String, Command)> {
        let active = self.active.as_ref()?;
        if state.safety_revision > active.safety && !busy(state) {
            self.finish(
                serde_json::to_value(state).unwrap_or(Value::Null),
                "stopped",
                Some("stopped"),
                Some("Operation cancelled by Stop or Output OFF".into()),
            );
            return None;
        }
        let mut completed_snapshot = None;
        if let Some(tag) = &active.tag
            && let Some(completion) = state
                .gui_jobs
                .iter()
                .chain(state.gui_job.iter())
                .find(|c| &c.id == tag)
        {
            let snapshot = completion.snapshot.clone();
            let phase = snapshot["phase"].as_str().unwrap_or("");
            let error = snapshot["error"].as_str().unwrap_or("");
            // Gate/connection jobs may retain the last measurement phase.
            // A successful Gate update after Stop is not a stopped new job.
            // Concurrent safety cancellation is handled by safety_revision above.
            let measurement = self
                .jobs
                .iter()
                .any(|job| job["id"] == active.id && job["kind"] == "start");
            if !error.is_empty()
                || phase == "Failed"
                || (measurement
                    && matches!(phase, "Compliance stop" | "Source limit stop" | "Stopped"))
            {
                let code = if matches!(phase, "Compliance stop" | "Source limit stop") {
                    "protection_stop"
                } else if phase == "Stopped" {
                    "stopped"
                } else {
                    snapshot["error_kind"]
                        .as_str()
                        .unwrap_or("operation_failed")
                };
                let message = if error.is_empty() {
                    phase.into()
                } else {
                    error.into()
                };
                self.finish(
                    snapshot.clone(),
                    if phase == "Stopped" {
                        "stopped"
                    } else {
                        "error"
                    },
                    Some(code),
                    Some(message),
                );
                return None;
            }
            self.active.as_mut().unwrap().tag = None;
            completed_snapshot = Some(snapshot);
        }
        let active = self.active.as_mut()?;
        if active.tag.is_some() {
            return None;
        }
        if let Some(command) = active.commands.pop_front() {
            let tag = uuid::Uuid::new_v4().to_string();
            active.tag = Some(tag.clone());
            Some((tag, command))
        } else {
            self.finish(
                completed_snapshot
                    .unwrap_or_else(|| serde_json::to_value(state).unwrap_or(Value::Null)),
                "completed",
                None,
                None,
            );
            None
        }
    }
}
fn request_fingerprint(kind: &str, body: &Value) -> String {
    use sha2::{Digest, Sha256};
    let mut body = body.clone();
    if let Some(object) = body.as_object_mut() {
        for field in [
            "token",
            "client_id",
            "lease",
            "request_id",
            "method",
            "action",
        ] {
            object.remove(field);
        }
    }
    format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&json!({"kind":kind,"body":body})).unwrap_or_default())
    )
}
pub fn busy(state: &Snapshot) -> bool {
    matches!(
        state.phase,
        Phase::Connecting
            | Phase::Scanning
            | Phase::SwitchingMode
            | Phase::Preparing
            | Phase::Running
            | Phase::CleaningUp
    )
}
pub fn outputs_off(state: &Snapshot) -> bool {
    (!state.source_connected || state.output == crate::state::OutputState::Off)
        && (!state.source2_connected || state.source2_output == crate::state::OutputState::Off)
        && (!state.gate_connected || state.gate_output == crate::state::OutputState::Off)
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
        .set_write_timeout(Some(Duration::from_secs(2)))
        .map_err(|e| e.to_string())?;
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut bytes = Vec::new();
    loop {
        stream
            .set_read_timeout(Some(
                deadline
                    .checked_duration_since(Instant::now())
                    .ok_or("Request timed out")?,
            ))
            .map_err(|e| e.to_string())?;
        let mut chunk = [0; 4096];
        let count = stream.read(&mut chunk).map_err(|e| e.to_string())?;
        if count == 0 {
            return Err("Incomplete GUI request".into());
        }
        let end = chunk[..count]
            .iter()
            .position(|b| *b == b'\n')
            .map(|i| i + 1);
        bytes.extend_from_slice(&chunk[..end.unwrap_or(count)]);
        if bytes.len() > MAX_REQUEST {
            return Err("Invalid GUI control request size".into());
        }
        if end.is_some() {
            break;
        }
    }
    let result = (|| {
        let body: Value =
            serde_json::from_slice(&bytes).map_err(|_| "Invalid GUI control request")?;
        if body["token"].as_str() != Some(token) {
            return Err("GUI control authentication failed".into());
        }
        let method = body
            .get("method")
            .or_else(|| body.get("action"))
            .and_then(Value::as_str)
            .ok_or("Missing method")?
            .to_owned();
        let (reply, receive) = mpsc::sync_channel(1);
        send.try_send(Request {
            body,
            method,
            reply,
            expires: Instant::now() + REQUEST_TIMEOUT,
        })
        .map_err(|_| "GUI control queue is full")?;
        receive
            .recv_timeout(REQUEST_TIMEOUT)
            .map_err(|_| "GUI is not responding; inspect its screen before retrying".to_string())
    })();
    let value = result.unwrap_or_else(|error| response(Err(error)));
    serde_json::to_writer(&mut stream, &value).map_err(|e| e.to_string())?;
    stream.write_all(b"\n").map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::GuiJobCompletion;
    use crate::state::OutputState;
    fn control() -> (tempfile::TempDir, GuiControl) {
        let d = tempfile::tempdir().unwrap();
        let c = GuiControl::in_directory(d.path().into()).unwrap();
        (d, c)
    }
    fn request(method: &str, body: Value) -> Request {
        let (reply, _) = mpsc::sync_channel(1);
        Request {
            body,
            method: method.into(),
            reply,
            expires: Instant::now() + REQUEST_TIMEOUT,
        }
    }
    fn client() -> String {
        uuid::Uuid::new_v4().to_string()
    }
    #[test]
    fn acquire_is_idle_only_exclusive_and_read_queries_do_not_claim() {
        let (_d, mut c) = control();
        let id = client();
        let acquire = request("acquire", json!({"client_id":id}));
        assert!(c.authorize(&acquire, false).is_err());
        assert!(!c.has_lease());
        assert!(
            c.authorize(&request("status", json!({})), false)
                .unwrap()
                .is_none()
        );
        assert!(!c.has_lease());
        let lease = c.authorize(&acquire, true).unwrap().unwrap()["lease"].clone();
        assert!(
            c.authorize(&request("acquire", json!({"client_id":client()})), true)
                .is_err()
        );
        assert!(
            c.authorize(
                &request("heartbeat", json!({"client_id":id,"lease":lease})),
                false
            )
            .is_ok()
        );
    }
    #[test]
    fn takeover_requires_fresh_off_ack_and_revoked_client_cannot_reclaim() {
        let (_d, mut c) = control();
        let id = client();
        c.authorize(&request("acquire", json!({"client_id":id})), true)
            .unwrap();
        let mut state = Snapshot {
            safety_revision: 7,
            source_connected: true,
            output: OutputState::Off,
            ..Default::default()
        };
        c.revoke(7);
        c.settle_release(&state);
        assert!(c.has_lease());
        state.safety_revision = 8;
        c.settle_release(&state);
        assert!(!c.has_lease());
        assert!(
            c.authorize(&request("acquire", json!({"client_id":id})), true)
                .unwrap_err()
                .contains("revoked")
        );
        assert!(
            c.authorize(&request("acquire", json!({"client_id":client()})), true)
                .is_ok()
        );
    }
    #[test]
    fn heartbeat_expiry_requests_safety_only_once_and_does_not_unlock_early() {
        let (_d, mut c) = control();
        c.authorize(&request("acquire", json!({"client_id":client()})), true)
            .unwrap();
        c.owner.as_mut().unwrap().seen = Instant::now() - LEASE_TIMEOUT - Duration::from_secs(1);
        assert!(c.expired(3));
        assert!(!c.expired(3));
        assert!(c.has_lease());
        c.settle_release(&Snapshot {
            safety_revision: 3,
            ..Default::default()
        });
        assert!(c.has_lease());
        c.settle_release(&Snapshot {
            safety_revision: 4,
            ..Default::default()
        });
        assert!(!c.has_lease());
    }
    #[test]
    fn jobs_wait_for_matching_completion_and_duplicate_is_not_dispatched() {
        let (_d, mut c) = control();
        let id = client();
        let mut state = Snapshot::default();
        let original = c
            .begin_job(
                id.clone(),
                "connect",
                vec![Command::Discover],
                &state,
                &json!({}),
            )
            .unwrap();
        assert_eq!(
            c.begin_job(
                id.clone(),
                "connect",
                vec![Command::Discover],
                &state,
                &json!({})
            )
            .unwrap(),
            original
        );
        let (tag, _) = c.advance(&state).unwrap();
        assert!(c.advance(&state).is_none());
        state.gui_job = Some(GuiJobCompletion {
            id: client(),
            snapshot: json!({"phase":"Completed","error":""}),
        });
        assert!(c.advance(&state).is_none());
        assert!(c.has_active());
        state.gui_job = Some(GuiJobCompletion {
            id: tag,
            snapshot: json!({"phase":"Completed","error":""}),
        });
        assert!(c.advance(&state).is_none());
        assert!(!c.has_active());
        assert_eq!(c.result(&id).unwrap()["status"], "completed");
    }
    #[test]
    fn stop_cancels_unsent_stages_and_protection_is_not_success() {
        let (_d, mut c) = control();
        let id = client();
        let mut state = Snapshot::default();
        c.begin_job(
            id.clone(),
            "connect",
            vec![Command::Discover, Command::Discover],
            &state,
            &json!({}),
        )
        .unwrap();
        c.advance(&state).unwrap();
        c.cancel_pending();
        state.safety_revision = 1;
        assert!(c.advance(&state).is_none());
        assert_eq!(c.result(&id).unwrap()["status"], "stopped");
        let id = client();
        c.begin_job(
            id.clone(),
            "start",
            vec![Command::Discover],
            &state,
            &json!({}),
        )
        .unwrap();
        let (tag, _) = c.advance(&state).unwrap();
        state.gui_job = Some(GuiJobCompletion {
            id: tag,
            snapshot: json!({"phase":"Source limit stop","error":""}),
        });
        c.advance(&state);
        assert_eq!(c.result(&id).unwrap()["error_code"], "protection_stop");
    }
    #[test]
    fn expired_and_wrong_lease_requests_never_dispatch() {
        let (_d, mut c) = control();
        let mut expired = request("acquire", json!({"client_id":client()}));
        expired.expires = Instant::now() - Duration::from_secs(1);
        assert!(c.authorize(&expired, true).is_err());
        c.authorize(&request("acquire", json!({"client_id":client()})), true)
            .unwrap();
        assert!(
            c.authorize(&request("start", json!({"lease":"wrong"})), true)
                .is_err()
        );
    }
    #[test]
    fn listener_authenticates_and_publishes_private_complete_descriptor() {
        use std::io::{BufRead, BufReader};
        let (d, c) = control();
        let path = std::fs::read_dir(d.path())
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        let declaration: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(declaration["protocol"], 2);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        let mut socket = TcpStream::connect(declaration["address"].as_str().unwrap()).unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        writeln!(socket, "{}", json!({"token":"wrong","method":"status"})).unwrap();
        let mut line = String::new();
        BufReader::new(socket).read_line(&mut line).unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&line).unwrap()["error_code"],
            "authentication"
        );
        assert!(c.next().is_none());
        drop(c);
        assert!(!path.exists());
    }
    #[test]
    fn voluntary_release_allows_reacquire_but_uuid_payload_conflicts_fail() {
        let (_d, mut c) = control();
        let client_id = client();
        let acquire = request("acquire", json!({"client_id":client_id}));
        c.authorize(&acquire, true).unwrap();
        c.release(0);
        c.settle_release(&Snapshot {
            safety_revision: 1,
            ..Default::default()
        });
        assert!(c.authorize(&acquire, true).is_ok());
        let id = client();
        let state = Snapshot::default();
        c.begin_job(
            id.clone(),
            "connect",
            vec![Command::Discover],
            &state,
            &json!({"devices":["source"]}),
        )
        .unwrap();
        assert!(
            c.existing_job(&id, "start", &json!({"devices":["source"]}))
                .unwrap_err()
                .contains("request_id_conflict")
        );
        assert!(
            c.existing_job(&id, "connect", &json!({"devices":["meter"]}))
                .is_err()
        );
        assert!(
            c.existing_job(
                &id,
                "connect",
                &json!({"devices":["source"],"token":"changed"})
            )
            .unwrap()
            .is_some()
        );
        assert_eq!(error_code("Unknown or expired run ID"), "unknown_job");
        assert_eq!(error_code("request_id_conflict"), "request_id_conflict");
    }
    #[test]
    fn coalesced_completion_history_keeps_earlier_job_observable() {
        let (_d, mut c) = control();
        let id = client();
        let mut state = Snapshot::default();
        c.begin_job(
            id.clone(),
            "connect",
            vec![Command::Discover],
            &state,
            &json!({}),
        )
        .unwrap();
        let (tag, _) = c.advance(&state).unwrap();
        state.gui_jobs = vec![GuiJobCompletion {
            id: tag,
            snapshot: json!({"phase":"Completed","error":""}),
        }];
        state.gui_job = Some(GuiJobCompletion {
            id: client(),
            snapshot: json!({"phase":"Failed","error":"different operation"}),
        });
        c.advance(&state);
        assert_eq!(c.result(&id).unwrap()["status"], "completed");
    }
    #[test]
    fn engine_loss_during_release_unlocks_for_diagnosis_without_claiming_off() {
        let (_d, mut c) = control();
        let id = client();
        let acquire = request("acquire", json!({"client_id":id}));
        c.authorize(&acquire, true).unwrap();
        c.release(5);
        assert!(c.has_lease());
        c.engine_lost("Engine disconnected; check outputs manually".into());
        assert!(!c.has_lease());
        // Real GUI admission additionally rejects any acquire when engine is dead.
        assert!(!c.is_retiring());
    }
    #[test]
    fn successful_gate_job_does_not_inherit_the_previous_sweep_stop() {
        for previous_phase in ["Stopped", "Source limit stop", "Compliance stop"] {
            let (_d, mut control) = control();
            let id = client();
            let mut state = Snapshot::default();
            control
                .begin_job(
                    id.clone(),
                    "set_gate_voltage",
                    vec![Command::SetGateVoltage {
                        voltage: 0.01,
                        compliance: 0.001,
                        ramp_step: 0.001,
                    }],
                    &state,
                    &json!({}),
                )
                .unwrap();
            let (tag, _) = control.advance(&state).unwrap();
            state.gui_job = Some(GuiJobCompletion {
                id: tag,
                snapshot: json!({"phase":previous_phase,"error":"","gate_output":"On","gate_voltage":0.01}),
            });
            control.advance(&state);
            assert_eq!(control.result(&id).unwrap()["status"], "completed");
            assert_eq!(
                control.result(&id).unwrap()["snapshot"]["gate_output"],
                "On"
            );
        }
    }
}
