//! Single-owner controller state. The server invokes this on one worker only.
use instrument_protocol::{ErrorCode, HubError, Request, SafetyPolicy};
use iv_transport::shared_controller::NativeController;
use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::time::Duration;

pub trait Controller: Send {
    /// Initialization must finish before a lease is granted. An output WRITE
    /// must never be the operation that lazily opens/resets a physical bus.
    fn prepare(&mut self) -> Result<(), String> {
        Ok(())
    }
    fn transfer(
        &mut self,
        op: &str,
        address: u8,
        timeout: Duration,
        payload: &[u8],
    ) -> Result<String, String>;
    fn verify_source(&mut self, address: u8, model: &str) -> Result<(), String>;
    fn source_off(&mut self, address: u8, model: &str) -> Result<(), String>;
}

/// Lazy open: failed authentication/version negotiation never touches hardware.
pub struct NativeBackend {
    pub helper: PathBuf,
    controller: Option<NativeController>,
}
impl NativeBackend {
    pub fn new(helper: PathBuf) -> Self {
        Self {
            helper,
            controller: None,
        }
    }
    fn get(&mut self) -> Result<&mut NativeController, String> {
        if self.controller.is_none() {
            self.controller = Some(NativeController::open(&self.helper)?);
        }
        Ok(self.controller.as_mut().expect("opened controller"))
    }
}
impl Controller for NativeBackend {
    fn prepare(&mut self) -> Result<(), String> {
        self.get().map(|_| ())
    }
    fn transfer(
        &mut self,
        op: &str,
        address: u8,
        timeout: Duration,
        payload: &[u8],
    ) -> Result<String, String> {
        self.get()?.transfer(op, address, timeout, payload)
    }
    fn verify_source(&mut self, address: u8, model: &str) -> Result<(), String> {
        self.get()?.verify_source(address, model)
    }
    fn source_off(&mut self, address: u8, model: &str) -> Result<(), String> {
        self.get()?.source_off(address, model)
    }
}

struct Owner {
    session: String,
    app: String,
    policy: SafetyPolicy,
}

pub struct Core<B: Controller> {
    backend: B,
    owners: BTreeMap<u8, Owner>,
    /// Source claims persist before clients may write. A service crash leaves
    /// them quarantined on restart until explicit verified OFF recovery.
    unknown: BTreeMap<u8, String>,
    journal: PathBuf,
    inventory: Vec<u8>,
    fault: Option<String>,
    devices: BTreeMap<u8, SafetyPolicy>,
}

impl<B: Controller> Core<B> {
    pub fn new(
        backend: B,
        journal: PathBuf,
        devices: BTreeMap<u8, SafetyPolicy>,
    ) -> Result<Self, HubError> {
        for (address, policy) in &devices {
            valid_address(*address)?;
            if let SafetyPolicy::Source { model } = policy
                && !NativeController::valid_source(model)
            {
                return Err(error(
                    ErrorCode::InvalidRequest,
                    "Device map contains an unsupported source safety model",
                ));
            }
        }
        let bytes = (|| {
            let mut bytes = Vec::new();
            std::fs::File::open(&journal)?
                .take(8193)
                .read_to_end(&mut bytes)?;
            Ok::<_, std::io::Error>(bytes)
        })();
        let unknown = match bytes {
            Ok(bytes) if bytes.len() <= 8192 => serde_json::from_slice(&bytes).map_err(|e| {
                error(
                    ErrorCode::StateUnknown,
                    format!("Invalid output journal: {e}"),
                )
            })?,
            Ok(_) => {
                return Err(error(
                    ErrorCode::StateUnknown,
                    "Output journal is too large",
                ));
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => BTreeMap::new(),
            Err(e) => {
                return Err(error(
                    ErrorCode::StateUnknown,
                    format!("Read output journal: {e}"),
                ));
            }
        };
        Ok(Self {
            backend,
            owners: BTreeMap::new(),
            unknown,
            journal,
            inventory: Vec::new(),
            fault: None,
            devices,
        })
    }

    fn persist(&self) -> Result<(), HubError> {
        let bytes = serde_json::to_vec(&self.unknown).map_err(internal)?;
        let temporary = self
            .journal
            .with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
        let result = (|| {
            let mut options = OpenOptions::new();
            options.create_new(true).write(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            let mut file = options.open(&temporary)?;
            file.write_all(&bytes)?;
            file.sync_all()?;
            fs::rename(&temporary, &self.journal)?;
            #[cfg(unix)]
            std::fs::File::open(
                self.journal
                    .parent()
                    .ok_or_else(|| std::io::Error::other("Missing journal directory"))?,
            )?
            .sync_all()?;
            Ok::<_, std::io::Error>(())
        })();
        if let Err(e) = result {
            let _ = fs::remove_file(temporary);
            return Err(error(
                ErrorCode::StateUnknown,
                format!("Cannot persist output ownership: {e}"),
            ));
        }
        Ok(())
    }

    fn check_owner(&self, session: &str, address: u8) -> Result<(), HubError> {
        match self.owners.get(&address) {
            Some(owner) if owner.session == session => Ok(()),
            Some(owner) => Err(error(
                ErrorCode::ResourceInUse,
                format!("GPIB address {address} is in use by {}", owner.app),
            )),
            None => Err(error(
                ErrorCode::InvalidRequest,
                format!("Claim GPIB address {address} before sending commands"),
            )),
        }
    }

    fn healthy(&self) -> Result<(), HubError> {
        self.fault.as_ref().map_or(Ok(()), |e| Err(error(ErrorCode::ControllerFault,
            format!("GPIB controller faulted: {e}. No reset or write replay was attempted. Verify outputs at the front panel."))))
    }

    fn transfer(
        &mut self,
        op: &str,
        address: u8,
        timeout: Duration,
        payload: &[u8],
    ) -> Result<String, HubError> {
        self.healthy()?;
        match self.backend.transfer(op, address, timeout, payload) {
            Ok(reply) => Ok(reply),
            Err(e) => {
                // Fail the complete controller rather than interpreting any
                // possible stale reply as another client's response.
                self.fault = Some(e.clone());
                Err(error(ErrorCode::ControllerFault, e))
            }
        }
    }

    pub fn request(
        &mut self,
        session: &str,
        app: &str,
        request: Request,
        timeout: Duration,
    ) -> Result<String, HubError> {
        match request {
            Request::Status => Ok(serde_json::json!({
                "controller_fault": self.fault,
                "owners": self.owners.iter().map(|(a,o)| serde_json::json!({"address":a,"app":o.app})).collect::<Vec<_>>(),
                "unverified_sources": self.unknown.keys().copied().collect::<Vec<_>>()
            }).to_string()),
            Request::Claim { address, policy } => {
                valid_address(address)?;
                self.healthy()?;
                if let Some(owner) = self.owners.get(&address) {
                    self.check_owner(session, address)?;
                    return if owner.policy == policy { Ok(String::new()) }
                        else { Err(error(ErrorCode::InvalidRequest, "Release before changing instrument safety policy")) };
                }
                // The app cannot downgrade a source to an unprotected meter.
                if self.devices.get(&address) != Some(&policy) {
                    return Err(error(ErrorCode::InvalidRequest, "Instrument safety policy does not match the service device map. Register the correct device/model before connecting."));
                }
                if self.unknown.contains_key(&address) {
                    return Err(error(ErrorCode::StateUnknown, format!("GPIB address {address} has an unverified source output. Explicit recovery and verified OFF are required.")));
                }
                if let Err(e) = self.backend.prepare() {
                    self.fault = Some(e.clone());
                    return Err(error(ErrorCode::ControllerFault, e));
                }
                if let SafetyPolicy::Source { model } = &policy {
                    if !NativeController::valid_source(model) {
                        return Err(error(ErrorCode::InvalidRequest, "Unsupported source safety model"));
                    }
                    self.backend.verify_source(address, model).map_err(|e| error(ErrorCode::StateUnknown, e))?;
                    // Reconcile OFF even without a journal. A lost/stale
                    // directory entry after power loss cannot grant a live output.
                    self.backend.source_off(address, model).map_err(|e| error(ErrorCode::StateUnknown, e))?;
                    self.unknown.insert(address, model.clone());
                    self.persist()?;
                }
                self.owners.insert(address, Owner { session: session.into(), app: app.into(), policy });
                Ok(String::new())
            }
            Request::Transfer { operation, address, timeout_ms, payload } => {
                valid_address(address)?;
                self.check_owner(session, address)?;
                if timeout < Duration::from_millis(1) {
                    return Err(error(ErrorCode::Deadline, "Transaction expired before controller dispatch"));
                }
                if !matches!(operation.as_str(), "WRITE" | "QUERY" | "READ" | "LOCAL" | "CLEAR")
                    || payload.len() > instrument_protocol::MAX_PAYLOAD || !(1..=30_000).contains(&timeout_ms) {
                    return Err(error(ErrorCode::InvalidRequest, "Invalid addressed transaction"));
                }
                self.transfer(&operation, address, timeout.min(Duration::from_millis(timeout_ms)), &payload)
            }
            Request::Release { address } => {
                valid_address(address)?;
                self.check_owner(session, address)?;
                self.release(address, false)?;
                Ok(String::new())
            }
            Request::Close => { self.disconnect(session, false)?; Ok(String::new()) }
            Request::List => {
                self.healthy()?;
                // Listener probing is not performed under active leases. It
                // must not interfere with split WRITE/READ device protocols.
                if self.owners.is_empty() {
                    let list = self.transfer("LIST", 1, timeout, &[])?;
                    self.inventory = list.lines().filter_map(|line| line.strip_prefix("GPIB0::")?.strip_suffix("::INSTR")?.parse::<u8>().ok())
                        .filter(|a| (1..=30).contains(a)).collect();
                }
                let mut addresses = self.inventory.clone();
                addresses.extend(self.owners.keys().copied());
                addresses.sort_unstable(); addresses.dedup();
                Ok(addresses.into_iter().map(|a| format!("GPIB0::{a}::INSTR\n")).collect())
            }
            Request::Recover { address, model } => {
                valid_address(address)?;
                self.healthy()?;
                if self.owners.contains_key(&address) {
                    return Err(error(ErrorCode::ResourceInUse, "Recovery cannot interrupt an owned instrument"));
                }
                if self.unknown.get(&address) != Some(&model) {
                    return Err(error(ErrorCode::InvalidRequest, "Recovery requires the exact quarantined source model"));
                }
                self.backend.verify_source(address, &model).map_err(|e| error(ErrorCode::StateUnknown, e))?;
                self.backend.source_off(address, &model).map_err(|e| error(ErrorCode::StateUnknown, e))?;
                self.unknown.remove(&address);
                self.persist()?;
                Ok(String::new())
            }
        }
    }

    fn release(&mut self, address: u8, lost: bool) -> Result<(), HubError> {
        let Some(owner) = self.owners.remove(&address) else {
            return Ok(());
        };
        // Remove live ownership even when cleanup fails; durable quarantine
        // continues to prevent all future claims of an unverified source.
        if let SafetyPolicy::Source { model } = &owner.policy {
            self.healthy()?;
            self.backend
                .source_off(address, model)
                .map_err(|e| error(ErrorCode::StateUnknown, e))?;
        } else if lost {
            self.transfer("CLEAR", address, Duration::from_secs(2), &[])?;
        }
        self.transfer("LOCAL", address, Duration::from_secs(2), &[])?;
        if matches!(owner.policy, SafetyPolicy::Source { .. }) {
            let saved = self.unknown.remove(&address);
            if let Err(e) = self.persist() {
                if let Some(model) = saved {
                    self.unknown.insert(address, model);
                }
                return Err(e);
            }
        }
        Ok(())
    }

    pub fn disconnect(&mut self, session: &str, lost: bool) -> Result<(), HubError> {
        let addresses: Vec<_> = self
            .owners
            .iter()
            .filter(|(_, o)| o.session == session)
            .map(|(a, _)| *a)
            .collect();
        let mut failures = Vec::new();
        for address in addresses {
            if let Err(e) = self.release(address, lost) {
                failures.push(e.to_string());
            }
        }
        if failures.is_empty() {
            Ok(())
        } else {
            Err(error(ErrorCode::StateUnknown, failures.join("; ")))
        }
    }
}

fn valid_address(address: u8) -> Result<(), HubError> {
    if (1..=30).contains(&address) {
        Ok(())
    } else {
        Err(error(
            ErrorCode::InvalidRequest,
            "GPIB address must be 1..30",
        ))
    }
}
pub(crate) fn error(code: ErrorCode, message: impl Into<String>) -> HubError {
    HubError::new(code, message)
}
fn internal(e: impl std::fmt::Display) -> HubError {
    error(ErrorCode::Internal, e.to_string())
}
