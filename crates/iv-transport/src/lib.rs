//! Bounded, process-isolated transport for the existing KohdaLab IV helpers.
//!
//! This crate deliberately has no native bindings and contains no `unsafe` code.
//! The USB and GPIB helpers remain separate processes so a transport failure cannot
//! corrupt the measurement engine's memory.

#![forbid(unsafe_code)]

mod drivers;
mod instruments;
#[cfg(all(test, unix))]
mod legacy_models;
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::io::{BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::{Arc, Mutex, mpsc};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const MAX_LINE: usize = 64 * 1024;
const MAX_PAYLOAD: usize = 16 * 1024;
const STARTUP_LIMIT: usize = 64 * 1024;
const STARTUP_TIMEOUT: Duration = Duration::from_secs(10);
const SCPI_TIMEOUT_MS: u64 = 5_000;

#[derive(Clone, Debug)]
pub struct HelperPaths {
    pub gpib: PathBuf,
    pub usbtmc: PathBuf,
    pub setup: Option<PathBuf>,
}

impl HelperPaths {
    /// Finds a co-located portable helper set.  An explicit directory is useful
    /// for development and avoids accepting a helper from an unrelated PATH.
    pub fn discover() -> Result<Self, String> {
        let mut roots = Vec::new();
        if let Some(value) = std::env::var_os("KOHDALAB_IV_HELPER_DIR") {
            roots.push(PathBuf::from(value));
        }
        if let Ok(exe) = std::env::current_exe()
            && let Some(parent) = exe.parent()
        {
            roots.push(parent.join("support"));
            roots.push(parent.to_path_buf());
            if let Some(grandparent) = parent.parent() {
                roots.push(grandparent.join("support"));
            }
        }
        if cfg!(windows) {
            roots.push(PathBuf::from("packaging/windows/native"));
            // Keep old source-tree layouts usable for existing local rebuild kits.
            roots.push(PathBuf::from("portable/windows/native"));
            roots.push(PathBuf::from("build/native-windows"));
        } else {
            roots.push(PathBuf::from("build/native-gpib"));
            roots.push(PathBuf::from("build/usbtmc-probe"));
        }

        let gpib_name = executable_name("kohdalab-gpib-helper");
        let usb_name = executable_name("kohdalab-usbtmc-helper");
        for root in roots {
            let gpib = root.join(&gpib_name);
            let usbtmc = root.join(&usb_name);
            if gpib.is_file() && usbtmc.is_file() {
                let setup = if cfg!(windows) {
                    let candidate = root.join(executable_name("USB-Setup"));
                    candidate.is_file().then_some(candidate)
                } else {
                    None
                };
                return Ok(Self {
                    gpib,
                    usbtmc,
                    setup,
                });
            }
        }
        // Local macOS development builds intentionally keep the differently
        // licensed helpers in separate build directories.
        if !cfg!(windows) {
            let gpib = PathBuf::from("build/native-gpib").join(&gpib_name);
            let usbtmc = PathBuf::from("build/usbtmc-probe").join(&usb_name);
            if gpib.is_file() && usbtmc.is_file() {
                return Ok(Self {
                    gpib,
                    usbtmc,
                    setup: None,
                });
            }
        }
        Err("KohdaLab USB/GPIB helpers were not found. Reinstall the complete portable application or set KOHDALAB_IV_HELPER_DIR.".into())
    }
}

fn executable_name(stem: &str) -> String {
    if cfg!(windows) {
        format!("{stem}.exe")
    } else {
        stem.to_owned()
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct ResourceInfo {
    pub resource: String,
    pub label: String,
}

#[derive(Clone, Debug, Default)]
pub struct Discovery {
    pub resources: Vec<ResourceInfo>,
    pub warnings: Vec<String>,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Role {
    Source,
    Meter,
}

fn other_role(role: Role) -> Role {
    match role {
        Role::Source => Role::Meter,
        Role::Meter => Role::Source,
    }
}

fn role_name(role: Role) -> &'static str {
    match role {
        Role::Source => "source",
        Role::Meter => "meter",
    }
}

fn gpib_address(kind: &ResourceKind) -> Result<u8, String> {
    match kind {
        ResourceKind::Gpib { address, .. } => Ok(*address),
        ResourceKind::Usb { .. } | ResourceKind::Visa { .. } => {
            Err("Internal transport resource mismatch.".into())
        }
    }
}

/// Result of an emergency source shutdown. A zeroing failure is reported
/// separately because output-off was already positively verified and must not
/// be downgraded to an unknown state or retried as though output were still on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutputOffReport {
    pub output_off_verified: bool,
    pub zeroing_warning: Option<String>,
}

/// Lists direct USBTMC descriptors and GPIB listeners.  A failed backend does
/// not discard results from the other backend; callers can display `warnings`.
pub fn discover_resources(paths: &HelperPaths) -> Discovery {
    let started = Instant::now();
    let mut result = discover_preferred(paths);
    discover_gpib_with_new_session(paths, &mut result, started);
    result
}

// VISA stays isolated in a helper process; no vendor DLL enters the Rust engine.
fn visa_helper(paths: &HelperPaths) -> Option<PathBuf> {
    if !cfg!(any(windows, test)) {
        return None;
    }
    let helper = paths
        .gpib
        .parent()?
        .join(executable_name("kohdalab-visa-helper"));
    helper.is_file().then_some(helper)
}

fn discover_visa(paths: &HelperPaths) -> Discovery {
    let mut result = Discovery::default();
    let Some(helper) = visa_helper(paths) else {
        return result;
    };
    match run_oneshot(&helper, &["--list"], Duration::from_secs(8))
        .and_then(|line| parse_kiv(&line)?.into_payload())
        .and_then(|payload| {
            payload
                .lines()
                .filter(|line| !line.trim().is_empty())
                .map(|line| {
                    let native = line.split('\t').next().unwrap_or_default().trim();
                    let resource = format!("VISA::{native}");
                    ResourceKind::parse(&resource)?;
                    Ok(ResourceInfo {
                        label: format!("Official VISA · {native}"),
                        resource,
                    })
                })
                .collect::<Result<Vec<_>, String>>()
        }) {
        Ok(resources) => result.resources = resources,
        Err(error) => result
            .warnings
            .push(format!("Official VISA discovery: {error}")),
    }
    result
}

fn discover_preferred(paths: &HelperPaths) -> Discovery {
    let mut result = discover_visa(paths);
    let native = discover_usb(paths);
    result.warnings.extend(native.warnings);
    for resource in native.resources {
        let duplicate = result.resources.iter().any(|official| {
            match (
                ResourceKind::parse(&official.resource),
                ResourceKind::parse(&resource.resource),
            ) {
                (Ok(left), Ok(right)) => left.same_physical(&right),
                _ => false,
            }
        });
        if !duplicate {
            result.resources.push(resource);
        }
    }
    result
}

fn preferred_kind(paths: &HelperPaths, resource: &str) -> Result<ResourceKind, String> {
    let native = ResourceKind::parse(resource)?;
    if matches!(native, ResourceKind::Visa { .. }) {
        return Ok(native);
    }
    // Also route previously saved native resource strings through official VISA
    // when it now owns that device. Once selected, never retry I/O on another backend.
    for official in discover_visa(paths).resources {
        let official = ResourceKind::parse(&official.resource)?;
        if official.same_physical(&native) {
            return Ok(official);
        }
        if native.is_gpib() && official.is_gpib() {
            return Ok(ResourceKind::Visa {
                resource: format!("VISA::{}", native.resource()),
                native: Box::new(native),
            });
        }
    }
    Ok(native)
}

fn discover_usb(paths: &HelperPaths) -> Discovery {
    let mut result = Discovery::default();
    match run_oneshot(&paths.usbtmc, &["--list"], Duration::from_secs(8)) {
        Ok(line) => match parse_kiv(&line).and_then(|frame| frame.into_payload()) {
            Ok(payload) => match parse_resources(&payload, "USB") {
                Ok(resources) => result.resources.extend(resources),
                Err(error) => result
                    .warnings
                    .push(format!("Direct USB discovery: {error}")),
            },
            Err(error) => result
                .warnings
                .push(format!("Direct USB discovery: {error}")),
        },
        Err(error) => result
            .warnings
            .push(format!("Direct USB discovery: {error}")),
    }
    result
}

fn discover_gpib_with_new_session(paths: &HelperPaths, result: &mut Discovery, started: Instant) {
    // Do not open/reset a native GPIB bridge when VISA already exposes its bus.
    if result
        .resources
        .iter()
        .any(|r| r.resource.starts_with("VISA::GPIB"))
    {
        result.warnings.push("GPIB0 is owned by official VISA. Native USB-GPIB probing was skipped to avoid resetting another controller. Only one GPIB controller/backend is supported at a time.".into());
        return;
    }
    let remaining = Duration::from_secs(30).saturating_sub(started.elapsed());
    if remaining.is_zero() {
        result.warnings.push(
            "GPIB discovery was skipped because the 30 second discovery deadline expired.".into(),
        );
        return;
    }
    match run_oneshot(
        &paths.gpib,
        &["--available"],
        remaining.min(Duration::from_secs(8)),
    ) {
        Ok(line) if line.trim() == "0" => {}
        Ok(line) if line.trim() == "1" => {
            let remaining = Duration::from_secs(30).saturating_sub(started.elapsed());
            // `--available` is descriptor-only. Acquire the same adapter lease as
            // a live session before opening the helper, otherwise two processes
            // could both reset/assert REN on the same physical bridge.
            let session = DeviceLease::acquire("gpib-adapter").and_then(|lease| {
                HelperSession::spawn_with_lease(
                    &paths.gpib,
                    remaining.min(Duration::from_secs(12)),
                    lease,
                )
            });
            match session.and_then(|mut session| {
                let reply = session.request(
                    "LIST",
                    1,
                    10_000,
                    &[],
                    remaining.min(Duration::from_secs(12)),
                )?;
                session.close()?;
                Ok(reply)
            }) {
                Ok(payload) => match parse_resources(&payload, "GPIB") {
                    Ok(resources) => result.resources.extend(resources),
                    Err(error) => result.warnings.push(format!("GPIB discovery: {error}")),
                },
                Err(error) => result.warnings.push(format!("GPIB discovery: {error}")),
            }
        }
        Ok(line) => result.warnings.push(format!(
            "GPIB adapter discovery returned an invalid response: {line:?}"
        )),
        Err(error) => result
            .warnings
            .push(format!("GPIB adapter discovery: {error}")),
    }
}

pub fn list_resources(paths: &HelperPaths) -> Result<Vec<ResourceInfo>, String> {
    let found = discover_resources(paths);
    if found.resources.is_empty() && !found.warnings.is_empty() {
        return Err(found.warnings.join(" "));
    }
    Ok(found.resources)
}

pub struct DevicePair {
    pub source_identity: String,
    pub meter_identity: String,
    pub source_model: String,
    pub meter_model: String,
    meter_discard: bool,
    paths: HelperPaths,
    source_kind: Option<ResourceKind>,
    meter_kind: Option<ResourceKind>,
    source: Option<Endpoint>,
    meter: Option<Endpoint>,
    usable: bool,
}

/// One physical helper exchange performed while translating a logical query.
/// `response` is the exact helper text before any driver parsing or header
/// stripping.  `error` is retained when the helper did not return text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransportExchange {
    pub operation: String,
    pub command: String,
    pub response: Option<String>,
    pub error: Option<String>,
}

/// The ordinary query result plus the physical exchanges that produced it.
/// This is opt-in so normal/live queries do not allocate provenance records.
#[derive(Clone, Debug)]
pub struct RecordedQuery {
    pub result: Result<String, String>,
    pub exchanges: Vec<TransportExchange>,
}

#[derive(Clone)]
struct Endpoint {
    resource: String,
    address: u8,
    session: Arc<Mutex<HelperSession>>,
}

impl DevicePair {
    /// Creates an empty instrument set. Roles may be connected independently.
    pub fn new(paths: &HelperPaths) -> Self {
        Self {
            source_identity: String::new(),
            meter_identity: String::new(),
            source_model: instruments::default_model(Role::Source).into(),
            meter_model: instruments::default_model(Role::Meter).into(),
            meter_discard: false,
            paths: paths.clone(),
            source_kind: None,
            meter_kind: None,
            source: None,
            meter: None,
            usable: true,
        }
    }

    pub fn open(paths: &HelperPaths, source: &str, meter: &str) -> Result<Self, String> {
        let mut pair = Self::new(paths);
        pair.connect(Role::Source, source)?;
        if let Err(error) = pair.connect(Role::Meter, meter) {
            let _ = pair.disconnect(Role::Source);
            return Err(error);
        }
        Ok(pair)
    }

    pub fn is_connected(&self, role: Role) -> bool {
        self.endpoint_option(role).is_some()
    }

    /// Connects one role, validating its identity before it becomes usable.
    pub fn connect(&mut self, role: Role, resource: &str) -> Result<(), String> {
        self.connect_model(role, resource, instruments::default_model(role))
    }

    pub fn connect_model(&mut self, role: Role, resource: &str, model: &str) -> Result<(), String> {
        if !instruments::valid_model(role, model) {
            return Err(format!("Unsupported model: {model}"));
        }
        if self.is_connected(role) && self.model(role) != model {
            return Err("Disconnect before changing instrument model.".into());
        }
        let kind = preferred_kind(&self.paths, resource)?;
        if drivers::is_adc_meter(model) && matches!(kind, ResourceKind::Usb { .. }) {
            return Err("ADCMT 7461A USB requires the official Windows 64-bit VISA driver/runtime and a VISA USB resource. Native WinUSB ADCMT USB is not implemented; GPIB is available.".into());
        }
        if let Some(current) = self.kind_option(role) {
            if current.same_physical(&kind) {
                return Ok(());
            }
            return Err(
                "Disconnect the selected instrument before connecting a different resource.".into(),
            );
        }
        if let Some(other) = self.kind_option(other_role(role))
            && kind.same_physical(other)
        {
            return Err("Source and meter must use different physical resources.".into());
        }
        if let Some(other) = self.kind_option(other_role(role))
            && kind.is_gpib()
            && other.is_gpib()
            && matches!(&kind, ResourceKind::Visa { .. })
                != matches!(other, ResourceKind::Visa { .. })
        {
            return Err("GPIB0 cannot mix official VISA and native WinUSB sessions. Disconnect both instruments, configure one GPIB backend, then click Refresh and reconnect.".into());
        }
        let shared = self
            .endpoint_option(other_role(role))
            .filter(|_| matches!(&kind, ResourceKind::Gpib { .. }))
            .filter(|_| {
                matches!(
                    self.kind_option(other_role(role)),
                    Some(ResourceKind::Gpib { .. })
                )
            })
            .map(|endpoint| endpoint.session.clone());
        let endpoint = if let Some(session) = shared {
            Endpoint {
                resource: resource.to_owned(),
                address: gpib_address(&kind)?,
                session,
            }
        } else {
            self.open_single_endpoint(&kind)?
        };
        let identity = match identify_endpoint(&endpoint, role, model).and_then(|id| {
            instruments::verify_model(role, model, &id)?;
            Ok(id)
        }) {
            Ok(identity) => identity,
            Err(error) => {
                if self.session_is_attached_elsewhere(&endpoint.session) {
                    if let Err(local_error) =
                        lock_session(&endpoint.session).and_then(|mut session| {
                            session.request(
                                "LOCAL",
                                endpoint.address,
                                2_000,
                                &[],
                                Duration::from_secs(2),
                            )
                        })
                    {
                        return Err(format!(
                            "{error} Could not return rejected {} to local control: {local_error}",
                            endpoint.resource
                        ));
                    }
                } else {
                    let _ = lock_session(&endpoint.session).and_then(|mut session| session.close());
                }
                return Err(error);
            }
        };
        match role {
            Role::Source => {
                self.source_identity = identity;
                self.source_model = model.into();
                self.source_kind = Some(kind);
                self.source = Some(endpoint);
            }
            Role::Meter => {
                self.meter_identity = identity;
                self.meter_model = model.into();
                self.meter_discard = drivers::is_adc_meter(model);
                self.meter_kind = Some(kind);
                self.meter = Some(endpoint);
            }
        }
        self.usable = true;
        Ok(())
    }

    /// Returns one role to local control. A shared GPIB helper remains alive
    /// until its final attached role is detached.
    pub fn disconnect(&mut self, role: Role) -> Result<(), String> {
        let Some(endpoint) = self.endpoint_option(role).cloned() else {
            return Ok(());
        };
        if role == Role::Meter && drivers::is_adc_meter(&self.meter_model) {
            let mut session = lock_session(&endpoint.session)?;
            session.request(
                "CLEAR",
                endpoint.address,
                2_000,
                &[],
                Duration::from_secs(2),
            )?;
            let stop = if self.meter_usb() { "H0\n" } else { ":ABORt\n" };
            session.request(
                "WRITE",
                endpoint.address,
                2_000,
                stop.as_bytes(),
                Duration::from_secs(2),
            )?;
            session.request(
                "WRITE",
                endpoint.address,
                2_000,
                b"*CLS\n",
                Duration::from_secs(2),
            )?;
        }
        if let Err(error) = lock_session(&endpoint.session).and_then(|mut session| {
            session.request(
                "LOCAL",
                endpoint.address,
                2_000,
                &[],
                Duration::from_secs(2),
            )
        }) {
            return Err(format!("LOCAL {}: {error}", endpoint.resource));
        }
        let shared_with_other = self
            .endpoint_option(other_role(role))
            .is_some_and(|other| Arc::ptr_eq(&other.session, &endpoint.session));
        if !shared_with_other {
            if let Err(error) = lock_session(&endpoint.session).and_then(|mut session| {
                session.request("REN", endpoint.address, 2_000, b"0", Duration::from_secs(2))
            }) {
                return Err(format!("REN release: {error}"));
            }
            let close_error = lock_session(&endpoint.session)
                .and_then(|mut session| session.close())
                .err();
            self.clear_role(role);
            return close_error.map_or(Ok(()), Err);
        }
        self.clear_role(role);
        Ok(())
    }

    fn clear_role(&mut self, role: Role) {
        match role {
            Role::Source => {
                self.source = None;
                self.source_kind = None;
                self.source_identity.clear();
            }
            Role::Meter => {
                self.meter = None;
                self.meter_kind = None;
                self.meter_identity.clear();
            }
        }
    }

    /// Refreshes passive USB descriptors and GPIB listeners while this pair is
    /// idle. If the pair owns a GPIB bridge, LIST is sent through that existing
    /// helper, avoiding a second open/IFC cycle. The measurement engine must not
    /// call this while a run is active.
    pub fn discover(&mut self, paths: &HelperPaths) -> Discovery {
        let started = Instant::now();
        let mut result = discover_preferred(paths);
        let bridge = self
            .source
            .as_ref()
            .filter(|endpoint| endpoint.address != 0)
            .or_else(|| self.meter.as_ref().filter(|endpoint| endpoint.address != 0))
            .map(|endpoint| (&endpoint.session, endpoint.address));
        match bridge {
            Some((session, address)) => match lock_session(session).and_then(|mut session| {
                session.request("LIST", address, 10_000, &[], Duration::from_secs(12))
            }) {
                Ok(payload) => match parse_resources(&payload, "GPIB") {
                    Ok(resources) => result.resources.extend(resources),
                    Err(error) => result.warnings.push(format!("GPIB discovery: {error}")),
                },
                Err(error) => result
                    .warnings
                    .push(format!("GPIB discovery through the active bridge: {error}")),
            },
            None => discover_gpib_with_new_session(paths, &mut result, started),
        }
        result
    }

    pub fn model(&self, role: Role) -> &str {
        match role {
            Role::Source => &self.source_model,
            Role::Meter => &self.meter_model,
        }
    }

    /// The selected transport resource, retained verbatim for run provenance.
    /// This only reads the connection state; it never sends an instrument query.
    pub fn resource(&self, role: Role) -> Option<&str> {
        self.endpoint_option(role)
            .map(|endpoint| endpoint.resource.as_str())
    }

    fn meter_usb(&self) -> bool {
        self.meter_kind.as_ref().is_some_and(|kind| match kind {
            ResourceKind::Usb { .. } => true,
            ResourceKind::Visa { native, .. } => {
                matches!(native.as_ref(), ResourceKind::Usb { .. })
            }
            _ => false,
        })
    }

    pub fn write(&mut self, role: Role, command: &str) -> Result<(), String> {
        self.ensure_usable()?;
        let model = self.model(role).to_owned();
        let commands = drivers::write_commands(role, &model, self.meter_usb(), command)?;
        for command in commands {
            let terminated = drivers::terminated(&command);
            self.endpoint(role)?.request("WRITE", &terminated)?;
        }
        if drivers::source_write_marks_adc_discard(role, &self.meter_model, command) {
            self.meter_discard = true;
        }
        Ok(())
    }

    pub fn query(&mut self, role: Role, command: &str) -> Result<String, String> {
        self.ensure_usable()?;
        let endpoint = self.endpoint(role)?.clone();
        let model = self.model(role).to_owned();
        let usb = self.meter_usb();
        let mut io = EndpointIo {
            endpoint: &endpoint,
        };
        drivers::query(&mut io, role, &model, usb, &mut self.meter_discard, command)
    }

    /// Runs the same driver path as [`Self::query`] while retaining the exact
    /// helper text for each physical request. This does not add any transport
    /// I/O; callers should use it only for persisted acquisition evidence.
    pub fn query_recorded(&mut self, role: Role, command: &str) -> RecordedQuery {
        if let Err(error) = self.ensure_usable() {
            return RecordedQuery {
                result: Err(error),
                exchanges: Vec::new(),
            };
        }
        let endpoint = match self.endpoint(role) {
            Ok(endpoint) => endpoint.clone(),
            Err(error) => {
                return RecordedQuery {
                    result: Err(error),
                    exchanges: Vec::new(),
                };
            }
        };
        let model = self.model(role).to_owned();
        let usb = self.meter_usb();
        let mut io = RecordingEndpointIo {
            endpoint: &endpoint,
            exchanges: Vec::new(),
        };
        let result = drivers::query(&mut io, role, &model, usb, &mut self.meter_discard, command);
        RecordedQuery {
            result,
            exchanges: io.exchanges,
        }
    }

    /// Attempts emergency source shutdown. It sends output-off first, verifies
    /// that output is off, then returns the fixed source level to zero. If the
    /// source session is poisoned (including by a meter timeout on shared GPIB),
    /// it kills that helper, transfers its exclusive lease to a fresh helper,
    /// re-verifies the exact GS210 identity, and retries at most twice.
    pub fn force_output_off(&mut self) -> Result<OutputOffReport, String> {
        if !self.usable {
            return Err("This instrument pair is unusable after a failed recovery. Reconnect before measuring.".into());
        }
        if self.source.is_none() {
            return Err("Cannot turn output off because the source is not connected.".into());
        }
        let deadline = Instant::now() + Duration::from_secs(28);
        let initial_error = match self.output_off_once(deadline) {
            Ok(report) => return Ok(report),
            Err(error) => error,
        };
        let mut errors = vec![format!("Initial output-off attempt: {initial_error}")];
        for _attempt in 0..2 {
            if Instant::now() >= deadline {
                break;
            }
            match self
                .replace_source_helper(deadline)
                .and_then(|_| self.output_off_once(deadline))
            {
                Ok(report) => return Ok(report),
                Err(error) => errors.push(error),
            }
        }
        self.usable = false;
        Err(format!(
            "Could not confirm GS210 output off after transport recovery. Use the instrument front panel to turn output off, then reconnect. {}",
            errors.join(" ")
        ))
    }

    /// Releases remote control and terminates helpers. This does not claim that
    /// an instrument output is off: output cleanup belongs to the measurement core.
    pub fn close(&mut self) -> Result<(), String> {
        let mut errors = Vec::new();
        for role in [Role::Source, Role::Meter] {
            if let Err(error) = self.disconnect(role) {
                errors.push(error);
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors.join(" "))
        }
    }

    fn endpoint(&self, role: Role) -> Result<&Endpoint, String> {
        self.endpoint_option(role)
            .ok_or_else(|| format!("The selected {} is not connected.", role_name(role)))
    }

    fn endpoint_option(&self, role: Role) -> Option<&Endpoint> {
        match role {
            Role::Source => self.source.as_ref(),
            Role::Meter => self.meter.as_ref(),
        }
    }

    fn kind_option(&self, role: Role) -> Option<&ResourceKind> {
        match role {
            Role::Source => self.source_kind.as_ref(),
            Role::Meter => self.meter_kind.as_ref(),
        }
    }
    fn session_is_attached_elsewhere(&self, session: &Arc<Mutex<HelperSession>>) -> bool {
        self.source
            .as_ref()
            .is_some_and(|endpoint| Arc::ptr_eq(&endpoint.session, session))
            || self
                .meter
                .as_ref()
                .is_some_and(|endpoint| Arc::ptr_eq(&endpoint.session, session))
    }
    fn open_single_endpoint(&self, kind: &ResourceKind) -> Result<Endpoint, String> {
        let lease = if kind.is_gpib() {
            let existing = self
                .source
                .as_ref()
                .filter(|_| self.source_kind.as_ref().is_some_and(ResourceKind::is_gpib))
                .or_else(|| {
                    self.meter
                        .as_ref()
                        .filter(|_| self.meter_kind.as_ref().is_some_and(ResourceKind::is_gpib))
                });
            if let Some(existing) = existing {
                lock_session(&existing.session)?
                    .lease
                    .clone()
                    .ok_or_else(|| {
                        "GPIB adapter lease is missing; disconnect and reconnect.".to_owned()
                    })?
            } else {
                DeviceLease::acquire("gpib-adapter")?
            }
        } else {
            DeviceLease::acquire(&kind.lock_key())?
        };
        Ok(Endpoint {
            resource: kind.resource().to_owned(),
            address: match kind {
                ResourceKind::Gpib { address, .. } => *address,
                _ => 0,
            },
            session: Arc::new(Mutex::new(
                spawn_for_resource(&self.paths, kind, lease, Instant::now() + STARTUP_TIMEOUT)
                    .map_err(|error| {
                        if cfg!(windows) && !matches!(kind, ResourceKind::Visa { .. }) {
                            format!("{error} If this device uses a manufacturer driver, install/configure its official 64-bit VISA runtime and click Refresh. Existing manufacturer drivers are preserved; WinUSB setup is only for unassigned devices.")
                        } else { error }
                    })?,
            )),
        })
    }

    fn ensure_usable(&self) -> Result<(), String> {
        if self.usable {
            Ok(())
        } else {
            Err("This instrument pair is unusable after a failed recovery. Reconnect before measuring.".into())
        }
    }

    fn output_off_once(&mut self, deadline: Instant) -> Result<OutputOffReport, String> {
        let source = self.source.as_ref().ok_or_else(|| {
            "Cannot turn output off because the source is not connected.".to_owned()
        })?;
        let legacy = drivers::is_legacy_source(&self.source_model);
        source.request_before(
            "WRITE",
            if legacy { "O0;E\n" } else { ":OUTP OFF\n" },
            deadline,
        )?;
        let output = source.request_before(
            "QUERY",
            if legacy { "OC;E\n" } else { ":OUTP?\n" },
            deadline,
        )?;
        let output = if legacy {
            if instruments::legacy_output(&output)? {
                "1"
            } else {
                "0"
            }
            .to_owned()
        } else {
            output
        };
        if !matches!(output.trim(), "0" | "OFF") {
            return Err(format!(
                "{} did not confirm output off (returned {output:?}).",
                self.source_model
            ));
        }
        let zeroing_warning = source
            .request_before(
                "WRITE",
                if legacy {
                    "S0;E\n"
                } else {
                    ":SOUR:LEV:FIX 0\n"
                },
                deadline,
            )
            .err();
        Ok(OutputOffReport {
            output_off_verified: true,
            zeroing_warning,
        })
    }

    fn replace_source_helper(&mut self, deadline: Instant) -> Result<(), String> {
        let (source_resource, source_address, source_session) = self
            .source
            .as_ref()
            .map(|source| {
                (
                    source.resource.clone(),
                    source.address,
                    source.session.clone(),
                )
            })
            .ok_or_else(|| {
                "Cannot recover output control because the source is not connected.".to_owned()
            })?;
        let source_kind = self
            .source_kind
            .as_ref()
            .ok_or_else(|| "Source connection state is incomplete.".to_owned())?
            .clone();
        let lease = {
            let mut old = lock_session(&source_session)?;
            old.abandon_for_recovery().ok_or_else(|| {
                "The failed source helper no longer owns its device lease.".to_owned()
            })?
        };
        let fresh = spawn_for_resource(&self.paths, &source_kind, lease, deadline)?;
        let fresh = Arc::new(Mutex::new(fresh));
        let candidate = Endpoint {
            resource: source_resource,
            address: source_address,
            session: fresh.clone(),
        };
        // Install the fresh source session before validation. If validation or
        // shutdown fails, the next bounded recovery attempt can take its lease
        // and terminate it; no lease is silently dropped between attempts.
        self.source.as_mut().expect("source checked").session = fresh.clone();
        if let Some(meter) = self
            .meter
            .as_mut()
            .filter(|meter| Arc::ptr_eq(&meter.session, &source_session))
        {
            meter.session = fresh.clone();
        }
        let identity = if drivers::is_legacy_source(&self.source_model) {
            // Re-identification shares the same emergency deadline, including all OS lines.
            let panel = legacy_panel_before(&candidate, deadline)?;
            panel.lines().next().unwrap_or_default().trim().to_owned()
        } else {
            candidate.request_before("QUERY", "*IDN?\n", deadline)?
        };
        instruments::verify_model(Role::Source, &self.source_model, &identity)?;
        if normalize_identity(&identity) != normalize_identity(&self.source_identity) {
            return Err(format!(
                "Recovered source identity {identity:?} does not match the previously validated source {}.",
                self.source_identity
            ));
        }
        Ok(())
    }
}

impl Drop for DevicePair {
    fn drop(&mut self) {
        let _ = self.close();
    }
}

impl Endpoint {
    fn request(&self, operation: &str, command: &str) -> Result<String, String> {
        let mut session = lock_session(&self.session)?;
        session.request(
            operation,
            self.address,
            SCPI_TIMEOUT_MS,
            command.as_bytes(),
            Duration::from_millis(SCPI_TIMEOUT_MS + 1_000),
        )
    }

    fn request_before(
        &self,
        operation: &str,
        command: &str,
        deadline: Instant,
    ) -> Result<String, String> {
        let remaining = deadline.saturating_duration_since(Instant::now());
        let timeout_ms = remaining.as_millis().min(u128::from(SCPI_TIMEOUT_MS)) as u64;
        if timeout_ms == 0 {
            return Err(
                "Emergency output-off deadline expired before the next instrument command.".into(),
            );
        }
        let mut session = lock_session(&self.session)?;
        session.request(
            operation,
            self.address,
            timeout_ms,
            command.as_bytes(),
            remaining,
        )
    }
}

/// Adapter used only by the private dialect layer. DevicePair keeps ownership
/// of sessions, deadlines, recovery and leases; drivers only sequence protocol
/// operations through this narrow interface.
struct EndpointIo<'a> {
    endpoint: &'a Endpoint,
}

impl drivers::InstrumentIo for EndpointIo<'_> {
    fn request(&mut self, operation: &str, command: &str) -> Result<String, String> {
        self.endpoint.request(operation, command)
    }

    fn request_before(
        &mut self,
        operation: &str,
        command: &str,
        deadline: Instant,
    ) -> Result<String, String> {
        self.endpoint.request_before(operation, command, deadline)
    }

    fn wait(&mut self, duration: Duration) {
        thread::sleep(duration);
    }
}

/// Instrument-I/O adapter used by `query_recorded`. It deliberately delegates
/// to the same endpoint methods as `EndpointIo`, including shared deadlines.
struct RecordingEndpointIo<'a> {
    endpoint: &'a Endpoint,
    exchanges: Vec<TransportExchange>,
}

impl RecordingEndpointIo<'_> {
    fn record(&mut self, operation: &str, command: &str, result: &Result<String, String>) {
        let (response, error) = match result {
            Ok(response) => (Some(response.clone()), None),
            Err(error) => (None, Some(error.clone())),
        };
        self.exchanges.push(TransportExchange {
            operation: operation.into(),
            command: command.into(),
            response,
            error,
        });
    }
}

impl drivers::InstrumentIo for RecordingEndpointIo<'_> {
    fn request(&mut self, operation: &str, command: &str) -> Result<String, String> {
        let result = self.endpoint.request(operation, command);
        self.record(operation, command, &result);
        result
    }

    fn request_before(
        &mut self,
        operation: &str,
        command: &str,
        deadline: Instant,
    ) -> Result<String, String> {
        let result = self.endpoint.request_before(operation, command, deadline);
        self.record(operation, command, &result);
        result
    }

    fn wait(&mut self, duration: Duration) {
        self.exchanges.push(TransportExchange {
            operation: "WAIT".into(),
            command: format!("{}ms", duration.as_millis()),
            response: None,
            error: None,
        });
        thread::sleep(duration);
    }
}

fn lock_session(
    session: &Arc<Mutex<HelperSession>>,
) -> Result<std::sync::MutexGuard<'_, HelperSession>, String> {
    session
        .lock()
        .map_err(|_| "The instrument transport worker stopped unexpectedly.".into())
}

fn legacy_panel_before(endpoint: &Endpoint, deadline: Instant) -> Result<String, String> {
    let mut panel = endpoint.request_before("QUERY", "OS;E\n", deadline)?;
    for _ in 0..5 {
        if panel.lines().any(|line| line.trim() == "END") {
            return Ok(panel);
        }
        if panel.len() > 4096 {
            return Err("Oversized 7651 panel response".into());
        }
        panel.push('\n');
        panel.push_str(&endpoint.request_before("READ", "", deadline)?);
    }
    Err("Incomplete 7651 panel response: END was not received".into())
}
fn identify_endpoint(endpoint: &Endpoint, role: Role, model: &str) -> Result<String, String> {
    drivers::identify(&mut EndpointIo { endpoint }, role, model)
}

fn spawn_for_resource(
    paths: &HelperPaths,
    kind: &ResourceKind,
    lease: DeviceLease,
    deadline: Instant,
) -> Result<HelperSession, String> {
    let remaining = deadline.saturating_duration_since(Instant::now());
    if remaining.is_zero() {
        return Err("Transport recovery deadline expired before helper restart.".into());
    }
    match kind {
        ResourceKind::Visa { native, .. } => {
            let helper = visa_helper(paths).ok_or_else(|| {
                "Official VISA helper was not found. Reinstall the complete application.".to_owned()
            })?;
            HelperSession::spawn_usb_with_lease(
                &helper,
                &[native.resource().to_owned()],
                remaining.min(STARTUP_TIMEOUT),
                lease,
            )
        }
        ResourceKind::Gpib { .. } => {
            HelperSession::spawn_with_lease(&paths.gpib, remaining.min(STARTUP_TIMEOUT), lease)
        }
        ResourceKind::Usb {
            serial, vid, pid, ..
        } => {
            let args = vec![
                serial.clone(),
                format!("0x{vid:04x}"),
                format!("0x{pid:04x}"),
            ];
            HelperSession::spawn_usb_with_lease(
                &paths.usbtmc,
                &args,
                remaining.min(STARTUP_TIMEOUT),
                lease,
            )
        }
    }
}

#[derive(Clone, Debug)]
enum ResourceKind {
    Visa {
        resource: String,
        native: Box<ResourceKind>,
    },
    Gpib {
        resource: String,
        address: u8,
    },
    Usb {
        resource: String,
        serial: String,
        vid: u16,
        pid: u16,
    },
}

impl ResourceKind {
    fn parse(value: &str) -> Result<Self, String> {
        if let Some(inner) = value.strip_prefix("VISA::") {
            if inner.starts_with("VISA::") {
                return Err("Nested VISA resource is invalid.".into());
            }
            let native = Box::new(Self::parse(inner)?);
            return Ok(Self::Visa {
                resource: value.to_owned(),
                native,
            });
        }
        let fields: Vec<_> = value.split("::").collect();
        if fields.len() == 3
            && fields[0].eq_ignore_ascii_case("GPIB0")
            && fields[2].eq_ignore_ascii_case("INSTR")
        {
            let address: u8 = fields[1]
                .parse()
                .map_err(|_| format!("Invalid GPIB resource {value:?}."))?;
            if !(1..=30).contains(&address) {
                return Err("GPIB addresses must be 1 through 30.".into());
            }
            return Ok(Self::Gpib {
                resource: value.to_owned(),
                address,
            });
        }
        if fields.len() == 5
            && fields[0].eq_ignore_ascii_case("USB0")
            && fields[4].eq_ignore_ascii_case("INSTR")
        {
            let vid = parse_hex_u16(fields[1], "USB vendor ID")?;
            let pid = parse_hex_u16(fields[2], "USB product ID")?;
            let serial = fields[3];
            if serial.is_empty()
                || !serial
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
            {
                return Err("USB resource has an invalid serial number.".into());
            }
            return Ok(Self::Usb {
                resource: value.to_owned(),
                serial: serial.to_owned(),
                vid,
                pid,
            });
        }
        Err(format!(
            "Unsupported resource {value:?}. Select a USB0 or GPIB0 resource returned by Refresh."
        ))
    }

    fn is_gpib(&self) -> bool {
        match self {
            Self::Gpib { .. } => true,
            Self::Visa { native, .. } => native.is_gpib(),
            Self::Usb { .. } => false,
        }
    }

    fn resource(&self) -> &str {
        match self {
            Self::Visa { resource, .. }
            | Self::Gpib { resource, .. }
            | Self::Usb { resource, .. } => resource,
        }
    }

    fn lock_key(&self) -> String {
        match self {
            Self::Visa { native, .. } => native.lock_key(),
            Self::Gpib { .. } => "gpib-adapter".into(),
            Self::Usb {
                serial, vid, pid, ..
            } => format!("usb-{vid:04x}-{pid:04x}-{serial}"),
        }
    }

    fn same_physical(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Visa { native, .. }, other) => native.same_physical(other),
            (this, Self::Visa { native, .. }) => this.same_physical(native),
            (Self::Gpib { address: left, .. }, Self::Gpib { address: right, .. }) => left == right,
            (
                Self::Usb {
                    serial: left_serial,
                    vid: left_vid,
                    pid: left_pid,
                    ..
                },
                Self::Usb {
                    serial: right_serial,
                    vid: right_vid,
                    pid: right_pid,
                    ..
                },
            ) => left_serial == right_serial && left_vid == right_vid && left_pid == right_pid,
            _ => false,
        }
    }
}

fn parse_hex_u16(value: &str, label: &str) -> Result<u16, String> {
    let value = value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
        .unwrap_or(value);
    u16::from_str_radix(value, 16).map_err(|_| format!("Invalid {label}."))
}

#[cfg(test)]
fn verify_identity(role: Role, identity: &str) -> Result<(), String> {
    instruments::verify_model(role, instruments::default_model(role), identity)
}

fn normalize_identity(identity: &str) -> String {
    identity
        .trim()
        .split(',')
        .map(str::trim)
        .collect::<Vec<_>>()
        .join(",")
}

#[derive(Clone)]
struct DeviceLease {
    // One OS file handle owns the lock until the last helper lease is dropped.
    // The parent serializes all per-device VISA requests; another application
    // cannot acquire any address on this GPIB controller during the session.
    _file: Arc<File>,
}

impl DeviceLease {
    fn acquire(key: &str) -> Result<Self, String> {
        #[cfg(test)]
        let directory =
            std::env::temp_dir().join(format!("kohdalab-iv-lock-tests-{}", std::process::id()));
        #[cfg(not(test))]
        let directory = std::env::var_os("USERPROFILE")
            .or_else(|| std::env::var_os("HOME"))
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir)
            .join(".kohdalab")
            .join("iv-next-locks");
        fs::create_dir_all(&directory)
            .map_err(|error| format!("Cannot create transport lock directory: {error}"))?;
        if !key
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
        {
            return Err("Invalid internal device lock key.".into());
        }
        let path = directory.join(format!("{key}.lock"));
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(path)
            .map_err(|error| format!("Cannot create transport lock: {error}"))?;
        file.try_lock_exclusive().map_err(|_| {
            format!("Device is already in use: {key}. Close the other KohdaLab IV instance.")
        })?;
        Ok(Self {
            _file: Arc::new(file),
        })
    }
}

#[derive(Debug)]
enum LineEvent {
    Line(String),
    Error(String),
    Eof,
}

struct HelperSession {
    child: Child,
    input: ChildStdin,
    lines: Option<mpsc::Receiver<LineEvent>>,
    reader: Option<JoinHandle<()>>,
    lease: Option<DeviceLease>,
    exit_address: u8,
    poisoned: bool,
    closed: bool,
}

impl HelperSession {
    fn spawn_with_lease(
        path: &Path,
        timeout: Duration,
        lease: DeviceLease,
    ) -> Result<Self, String> {
        Self::spawn_with_args_and_lease(path, &[], timeout, lease, 1)
    }

    fn spawn_usb_with_lease(
        path: &Path,
        args: &[String],
        timeout: Duration,
        lease: DeviceLease,
    ) -> Result<Self, String> {
        Self::spawn_with_args_and_lease(path, args, timeout, lease, 0)
    }

    fn spawn_with_args_and_lease(
        path: &Path,
        args: &[String],
        timeout: Duration,
        lease: DeviceLease,
        exit_address: u8,
    ) -> Result<Self, String> {
        let mut command = Command::new(path);
        command
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        configure_process(&mut command);
        let mut child = command
            .spawn()
            .map_err(|error| format!("Cannot start helper {}: {error}", path.display()))?;
        let input = child
            .stdin
            .take()
            .ok_or_else(|| "Cannot open helper input pipe.".to_owned())?;
        let output = child
            .stdout
            .take()
            .ok_or_else(|| "Cannot open helper output pipe.".to_owned())?;
        let (sender, lines) = mpsc::sync_channel(16);
        let reader = thread::spawn(move || read_lines(output, sender));
        let mut session = Self {
            child,
            input,
            lines: Some(lines),
            reader: Some(reader),
            lease: Some(lease),
            exit_address,
            poisoned: false,
            closed: false,
        };
        session.wait_startup(timeout)?;
        Ok(session)
    }

    fn wait_startup(&mut self, timeout: Duration) -> Result<(), String> {
        let deadline = Instant::now() + timeout;
        let mut preamble = 0usize;
        loop {
            let line = self.next_line(deadline)?;
            preamble = preamble.saturating_add(line.len());
            if preamble > STARTUP_LIMIT {
                return Err("Helper startup output exceeded 64 KiB.".into());
            }
            if line.starts_with("KIV\t") {
                return parse_kiv(&line)
                    .and_then(|frame| frame.into_payload())
                    .map(|_| ());
            }
            // Old GPIB builds can print a bounded diagnostic preamble before KIV.
        }
    }

    fn request(
        &mut self,
        operation: &str,
        address: u8,
        timeout_ms: u64,
        payload: &[u8],
        deadline: Duration,
    ) -> Result<String, String> {
        if self.closed {
            return Err("The instrument transport is closed.".into());
        }
        if self.poisoned && matches!(operation, "WRITE" | "QUERY" | "READ") {
            return Err("A previous instrument transfer failed. Disconnect and reconnect the selected instrument before sending more SCPI commands.".into());
        }
        if payload.len() > MAX_PAYLOAD {
            return Err("Command payload exceeds the 16 KiB transport limit.".into());
        }
        let encoded = if payload.is_empty() {
            "-".to_owned()
        } else {
            hex(payload)
        };
        let line = format!("{operation} {address} {timeout_ms} {encoded}\n");
        self.input
            .write_all(line.as_bytes())
            .and_then(|_| self.input.flush())
            .map_err(|error| {
                self.poisoned = true;
                format!("Cannot send command to instrument helper: {error}")
            })?;
        let is_scpi = matches!(operation, "WRITE" | "QUERY" | "READ");
        let frame = match self.next_protocol_frame(Instant::now() + deadline) {
            Ok(frame) => frame,
            Err(error) => {
                if is_scpi {
                    self.poisoned = true;
                }
                return Err(error);
            }
        };
        match frame.into_payload() {
            Ok(value) => Ok(value),
            Err(error) => {
                if is_scpi {
                    self.poisoned = true;
                }
                Err(error)
            }
        }
    }

    fn next_line(&mut self, deadline: Instant) -> Result<String, String> {
        let wait = deadline.saturating_duration_since(Instant::now());
        let lines = self
            .lines
            .as_ref()
            .ok_or_else(|| "Instrument helper response pipe is closed.".to_owned())?;
        match lines.recv_timeout(wait) {
            Ok(LineEvent::Line(line)) => Ok(line),
            Ok(LineEvent::Error(error)) => Err(error),
            Ok(LineEvent::Eof) => {
                Err("Instrument helper closed its response pipe unexpectedly.".into())
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                self.poisoned = true;
                Err("Instrument helper timed out. Disconnect and reconnect the selected instrument before retrying.".into())
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                Err("Instrument helper reader stopped unexpectedly.".into())
            }
        }
    }

    fn next_protocol_frame(&mut self, deadline: Instant) -> Result<KivFrame, String> {
        let mut diagnostics = 0usize;
        loop {
            let line = self.next_line(deadline)?;
            if line.starts_with("gpib: ") {
                diagnostics = diagnostics.saturating_add(line.len());
                if diagnostics > STARTUP_LIMIT {
                    return Err("GPIB helper diagnostics exceeded 64 KiB during a command.".into());
                }
                continue;
            }
            return parse_kiv(&line);
        }
    }

    fn close(&mut self) -> Result<(), String> {
        if self.closed {
            return Ok(());
        }
        let reply = self.request(
            "EXIT",
            self.exit_address,
            2_000,
            &[],
            Duration::from_secs(2),
        );
        self.closed = true;
        self.finish_process(true);
        reply.map(|_| ())
    }

    /// Terminates a poisoned helper without consuming any more protocol frames.
    /// Joining the reader before returning the lease prevents stale replies from
    /// being interpreted as responses from a replacement process.
    fn abandon_for_recovery(&mut self) -> Option<DeviceLease> {
        self.poisoned = true;
        self.closed = true;
        self.finish_process(false);
        self.lease.take()
    }

    fn finish_process(&mut self, release_lease: bool) {
        // Disconnect the receiver first. The reader uses try_send, so a full
        // bounded queue can never keep it alive while shutdown waits to join.
        self.lines.take();
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline {
            match self.child.try_wait() {
                Ok(Some(_)) => break,
                Ok(None) => thread::sleep(Duration::from_millis(20)),
                Err(_) => break,
            }
        }
        if matches!(self.child.try_wait(), Ok(None)) {
            let _ = self.child.kill();
        }
        let _ = self.child.wait();
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
        if release_lease {
            self.lease.take();
        }
    }
}

impl Drop for HelperSession {
    fn drop(&mut self) {
        if !self.closed {
            let _ = self.close();
        }
        self.finish_process(true);
    }
}

fn configure_process(command: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    #[cfg(not(windows))]
    {
        let _ = command;
    }
}

fn read_lines<R: Read>(output: R, sender: mpsc::SyncSender<LineEvent>) {
    let mut reader = BufReader::new(output);
    let mut byte = [0u8; 1];
    let mut line = Vec::new();
    loop {
        match reader.read(&mut byte) {
            Ok(0) => {
                let _ = sender.try_send(LineEvent::Eof);
                return;
            }
            Ok(_) if byte[0] == b'\n' => match String::from_utf8(std::mem::take(&mut line)) {
                Ok(value) => {
                    if sender
                        .try_send(LineEvent::Line(value.trim_end_matches('\r').to_owned()))
                        .is_err()
                    {
                        return;
                    }
                }
                Err(_) => {
                    let _ = sender.try_send(LineEvent::Error(
                        "Helper emitted non-UTF-8 protocol output.".into(),
                    ));
                    return;
                }
            },
            Ok(_) => {
                line.push(byte[0]);
                if line.len() > MAX_LINE {
                    let _ = sender.try_send(LineEvent::Error(
                        "Helper protocol line exceeds 64 KiB.".into(),
                    ));
                    return;
                }
            }
            Err(error) => {
                let _ = sender.try_send(LineEvent::Error(format!(
                    "Cannot read helper response: {error}"
                )));
                return;
            }
        }
    }
}

#[derive(Debug)]
struct KivFrame {
    ok: bool,
    payload: String,
}

impl KivFrame {
    fn into_payload(self) -> Result<String, String> {
        let decoded = decode_hex(&self.payload)?;
        if decoded.len() > MAX_PAYLOAD {
            return Err("Helper payload exceeds the 16 KiB transport limit.".into());
        }
        if self.ok {
            String::from_utf8(decoded).map_err(|_| "Helper returned non-UTF-8 text payload.".into())
        } else {
            Err(String::from_utf8_lossy(&decoded).into_owned())
        }
    }
}

fn parse_kiv(line: &str) -> Result<KivFrame, String> {
    let mut fields = line.splitn(3, '\t');
    if fields.next() != Some("KIV") {
        return Err("Malformed helper response (missing KIV frame).".into());
    }
    let ok = match fields.next() {
        Some("OK") => true,
        Some("ERR") => false,
        _ => return Err("Malformed helper response status.".into()),
    };
    let payload = fields
        .next()
        .ok_or_else(|| "Malformed helper response payload.".to_owned())?;
    if payload.len() > MAX_LINE * 2 {
        return Err("Helper encoded payload exceeds protocol limit.".into());
    }
    Ok(KivFrame {
        ok,
        payload: payload.to_owned(),
    })
}

fn hex(payload: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(payload.len() * 2);
    for byte in payload {
        output.push(DIGITS[(byte >> 4) as usize] as char);
        output.push(DIGITS[(byte & 15) as usize] as char);
    }
    output
}

fn decode_hex(value: &str) -> Result<Vec<u8>, String> {
    if value.is_empty() {
        return Ok(Vec::new());
    }
    if !value.len().is_multiple_of(2) {
        return Err("Helper returned invalid hexadecimal payload.".into());
    }
    if value.len() / 2 > MAX_PAYLOAD {
        return Err("Helper payload exceeds the 16 KiB transport limit.".into());
    }
    let mut output = Vec::with_capacity(value.len() / 2);
    for chunk in value.as_bytes().as_chunks::<2>().0 {
        let high = hex_nibble(chunk[0])
            .ok_or_else(|| "Helper returned invalid hexadecimal payload.".to_owned())?;
        let low = hex_nibble(chunk[1])
            .ok_or_else(|| "Helper returned invalid hexadecimal payload.".to_owned())?;
        output.push(high << 4 | low);
    }
    Ok(output)
}

fn hex_nibble(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        _ => None,
    }
}

fn parse_resources(payload: &str, kind: &str) -> Result<Vec<ResourceInfo>, String> {
    let mut resources = Vec::new();
    for raw in payload.lines() {
        let resource = raw.trim();
        if resource.is_empty() {
            continue;
        }
        ResourceKind::parse(resource)?;
        let label = match kind {
            "USB" => format!("Direct USB · {resource}"),
            _ => format!("GPIB · {resource}"),
        };
        resources.push(ResourceInfo {
            resource: resource.to_owned(),
            label,
        });
    }
    Ok(resources)
}

fn run_oneshot(path: &Path, args: &[&str], timeout: Duration) -> Result<String, String> {
    let mut command = Command::new(path);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    configure_process(&mut command);
    let mut child = command
        .spawn()
        .map_err(|error| format!("Cannot start helper {}: {error}", path.display()))?;
    let output = child
        .stdout
        .take()
        .ok_or_else(|| "Cannot open helper output pipe.".to_owned())?;
    let (sender, receiver) = mpsc::sync_channel(4);
    let reader = thread::spawn(move || read_lines(output, sender));
    let deadline = Instant::now() + timeout;
    let first = match receiver.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
        Ok(LineEvent::Line(line)) => Ok(line),
        Ok(LineEvent::Error(error)) => Err(error),
        Ok(LineEvent::Eof) => Err("Helper exited without a response.".into()),
        Err(_) => Err("Helper timed out during discovery.".into()),
    };
    if matches!(child.try_wait(), Ok(None)) {
        let _ = child.kill();
    }
    let _ = child.wait();
    let _ = reader.join();
    first
}

#[cfg(test)]
mod tests {
    use super::*;

    // These tests intentionally use the production GPIB adapter lease key, so
    // serialize them without weakening the production cross-process lock.
    #[cfg(unix)]
    pub(super) static GPIB_TEST_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn frame_handles_empty_and_fragment_independent_payloads() {
        let frame = parse_kiv("KIV\tOK\t").unwrap();
        assert_eq!(frame.into_payload().unwrap(), "");
        let frame = parse_kiv("KIV\tOK\t2a49444e3f0a").unwrap();
        assert_eq!(frame.into_payload().unwrap(), "*IDN?\n");
    }

    #[test]
    fn malformed_or_oversized_frames_are_rejected() {
        assert!(parse_kiv("KIV\tOK\t0").unwrap().into_payload().is_err());
        assert!(parse_kiv("KIV\tWHAT\t").is_err());
        assert!(decode_hex(&"00".repeat(MAX_PAYLOAD + 1)).is_err());
        assert_eq!(
            parse_kiv("KIV\tERR\t6361626c6520646973636f6e6e6563746564")
                .unwrap()
                .into_payload()
                .unwrap_err(),
            "cable disconnected"
        );
    }

    #[test]
    fn resources_are_strict_and_model_matching_is_explicit() {
        assert!(matches!(
            ResourceKind::parse("GPIB0::3::INSTR"),
            Ok(ResourceKind::Gpib { address: 3, .. })
        ));
        assert!(matches!(
            ResourceKind::parse("USB0::0x0957::0x0A07::MY48001051::INSTR"),
            Ok(ResourceKind::Usb { .. })
        ));
        assert!(ResourceKind::parse("GPIB0::0::INSTR").is_err());
        let source = ResourceKind::parse("GPIB0::3::INSTR").unwrap();
        let meter = ResourceKind::parse("GPIB0::7::INSTR").unwrap();
        assert!(!source.same_physical(&meter));
        assert!(source.same_physical(&ResourceKind::parse("GPIB0::3::INSTR").unwrap()));
        assert!(verify_identity(Role::Source, "YOKOGAWA,GS210,1,2").is_ok());
        assert!(verify_identity(Role::Meter, "Agilent Technologies,34411A,1,2").is_ok());
        assert!(verify_identity(Role::Meter, "YOKOGAWA,GS210,1,2").is_err());
        assert_ne!(
            normalize_identity("YOKOGAWA,GS210,91N1,2.01"),
            normalize_identity("YOKOGAWA,GS210,91N2,2.01")
        );
    }

    #[test]
    fn visa_resources_share_physical_identity_but_retain_the_backend() {
        let native = ResourceKind::parse("USB0::0x0957::0x0A07::SERIAL::INSTR").unwrap();
        let official = ResourceKind::parse("VISA::USB0::0x0957::0x0A07::SERIAL::INSTR").unwrap();
        assert!(official.same_physical(&native));
        assert!(native.same_physical(&official));
        assert_eq!(native.lock_key(), official.lock_key());
        assert!(ResourceKind::parse("VISA::VISA::GPIB0::3::INSTR").is_err());
        assert!(ResourceKind::parse("VISA::GPIB0::0::INSTR").is_err());
        assert!(ResourceKind::parse("VISA::TCPIP0::host::INSTR").is_err());
    }

    #[cfg(unix)]
    #[test]
    fn official_route_wins_deduplicates_and_never_retries_a_busy_device_on_native() {
        use std::os::unix::fs::PermissionsExt;
        let _guard = GPIB_TEST_LOCK.lock().unwrap();
        let directory =
            std::env::temp_dir().join(format!("iv-visa-routing-{}", std::process::id()));
        fs::create_dir_all(&directory).unwrap();
        let log = directory.join("calls.log");
        let busy = directory.join("busy");
        let _ = fs::remove_file(&log);
        let _ = fs::remove_file(&busy);
        let no_visa = directory.join("no-visa");
        let _ = fs::remove_file(&no_visa);
        let listing =
            "GPIB0::3::INSTR\tOfficial VISA\nUSB0::0x0957::0x0A07::METER::INSTR\tOfficial VISA\n";
        let visa = format!(
            r#"#!/bin/sh
if [ "$1" = --list ]; then
 if [ -f '{}' ]; then printf 'KIV\tOK\t\n'; else printf 'KIV\tOK\t{}\n'; fi
 exit
fi
printf 'VISA %s\n' "$1" >> '{}'
if [ -f '{}' ]; then printf 'KIV\tERR\t62757379\n'; exit 1; fi
resource="$1"
printf 'KIV\tOK\t\n'
while read op address timeout payload; do
 if [ "$op" = QUERY ] && [ "$payload" = 2a49444e3f0a ]; then
  if [ "$resource" = 'GPIB0::3::INSTR' ]; then printf 'KIV\tOK\t{}\n'; else printf 'KIV\tOK\t{}\n'; fi
 elif [ "$op" = QUERY ]; then printf 'KIV\tOK\t300a\n'
 else printf 'KIV\tOK\t\n'; fi
 if [ "$op" = EXIT ]; then exit; fi
done
"#,
            no_visa.display(),
            hex(listing.as_bytes()),
            log.display(),
            busy.display(),
            hex(b"YOKOGAWA,GS210,SOURCE,1\n"),
            hex(b"Agilent,34411A,METER,1\n")
        );
        let native = format!(
            r#"#!/bin/sh
if [ "$1" = --list ]; then printf 'KIV\tOK\t{}\n'; exit; fi
printf 'NATIVE\n' >> '{}'
exit 1
"#,
            hex(b"USB0::0x0957::0x0A07::METER::INSTR\n"),
            log.display()
        );
        let paths = HelperPaths {
            gpib: directory.join("native-gpib"),
            usbtmc: directory.join("native-usb"),
            setup: None,
        };
        let visa_path = directory.join(executable_name("kohdalab-visa-helper"));
        for (path, contents) in [
            (&visa_path, visa.as_str()),
            (&paths.gpib, native.as_str()),
            (&paths.usbtmc, native.as_str()),
        ] {
            fs::write(path, contents).unwrap();
            fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
        }
        let found = discover_resources(&paths);
        assert_eq!(found.resources.len(), 2);
        assert!(
            found
                .resources
                .iter()
                .all(|r| r.resource.starts_with("VISA::"))
        );
        assert_eq!(found.warnings.len(), 1);
        assert!(found.warnings[0].contains("GPIB0 is owned by official VISA"));
        let mut pair = DevicePair::new(&paths);
        // Old saved strings are routed automatically as well as explicit VISA selections.
        pair.connect(Role::Source, "GPIB0::3::INSTR").unwrap();
        pair.connect(Role::Meter, "GPIB0::7::INSTR").unwrap();
        assert!(DeviceLease::acquire("gpib-adapter").is_err());
        assert!(
            DevicePair::new(&paths)
                .connect(Role::Source, "VISA::GPIB0::8::INSTR")
                .is_err()
        );
        assert!(pair.force_output_off().unwrap().output_off_verified);
        pair.disconnect(Role::Meter).unwrap();
        assert!(DeviceLease::acquire("gpib-adapter").is_err());
        fs::write(&no_visa, b"runtime unavailable").unwrap();
        assert!(
            pair.connect(Role::Meter, "GPIB0::7::INSTR")
                .unwrap_err()
                .contains("cannot mix")
        );
        fs::remove_file(&no_visa).unwrap();
        pair.close().unwrap();
        drop(DeviceLease::acquire("gpib-adapter").unwrap());
        fs::write(&busy, b"busy").unwrap();
        assert!(
            DevicePair::new(&paths)
                .connect(Role::Source, "GPIB0::3::INSTR")
                .unwrap_err()
                .contains("busy")
        );
        let calls = fs::read_to_string(&log).unwrap();
        assert!(!calls.contains("NATIVE"), "{calls}");
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn source_only_shutdown_requires_a_source_connection() {
        let paths = HelperPaths {
            gpib: PathBuf::from("unused-gpib"),
            usbtmc: PathBuf::from("unused-usbtmc"),
            setup: None,
        };
        assert_eq!(
            DevicePair::new(&paths).force_output_off().unwrap_err(),
            "Cannot turn output off because the source is not connected."
        );
    }

    #[test]
    fn reader_reports_eof_and_oversize_without_collecting_unbounded_output() {
        let (sender, receiver) = mpsc::sync_channel(2);
        let reader = thread::spawn(move || read_lines(&b"KIV\tOK\t\n"[..], sender));
        assert!(matches!(receiver.recv().unwrap(), LineEvent::Line(_)));
        assert!(matches!(receiver.recv().unwrap(), LineEvent::Eof));
        reader.join().unwrap();
        let (sender, receiver) = mpsc::sync_channel(2);
        let bytes = vec![b'x'; MAX_LINE + 1];
        let reader = thread::spawn(move || read_lines(&bytes[..], sender));
        assert!(matches!(receiver.recv().unwrap(), LineEvent::Error(_)));
        reader.join().unwrap();
    }

    #[test]
    fn exclusive_lease_prevents_a_second_owner() {
        // A concurrent fork can briefly inherit the lock before exec closes it.
        // Keep this assertion isolated from the helper-spawning contract tests.
        #[cfg(unix)]
        let _guard = GPIB_TEST_LOCK.lock().unwrap();
        let key = format!("transport-test-{}", std::process::id());
        let first = DeviceLease::acquire(&key).unwrap();
        assert!(DeviceLease::acquire(&key).is_err());
        drop(first);
        assert!(DeviceLease::acquire(&key).is_ok());
    }

    #[cfg(unix)]
    #[test]
    fn poisoned_shared_gpib_recovers_source_before_output_off() {
        use std::os::unix::fs::PermissionsExt;
        let _guard = GPIB_TEST_LOCK.lock().unwrap();
        let unique = format!("kohdalab-transport-test-{}", std::process::id());
        let directory = std::env::temp_dir().join(unique);
        fs::create_dir_all(&directory).unwrap();
        let log = directory.join("requests.log");
        let helper = directory.join("fake-gpib-helper.sh");
        let script = format!(
            r#"#!/bin/sh
printf 'KIV\tOK\t\n'
while read op address timeout payload; do
  printf '%s %s %s\n' "$op" "$address" "$payload" >> '{}'
  if [ "$op" = QUERY ] && [ "$address" = 3 ]; then
    if [ "$payload" = 2a49444e3f0a ]; then printf 'KIV\tOK\t594f4b4f474157412c47533231302c53455249414c2d312c322e30310a\n'; else printf 'KIV\tOK\t300a\n'; fi
  elif [ "$op" = QUERY ] && [ "$address" = 7 ]; then
    if [ "$payload" = 2a49444e3f0a ]; then printf 'KIV\tOK\t4167696c656e7420546563686e6f6c6f676965732c3334343131412c4d455445522d312c322e300a\n'; else printf 'KIV\tERR\t6d6574657220726561642074696d656f7574\n'; fi
  elif [ "$op" = WRITE ] && [ "$address" = 3 ] && [ "$payload" = 3a534f55523a4c45563a46495820300a ]; then
    printf 'KIV\tERR\t7a65726f696e67206661696c6564\n'
  elif [ "$op" = EXIT ]; then
    printf 'gpib: ni_usb_gpib driver unloading\n'
    printf 'gpib: unregistering driver '\''ni_usb_b'\''\n'
    printf 'KIV\tOK\t\n'
  else
    printf 'KIV\tOK\t\n'
  fi
done
"#,
            log.display()
        );
        fs::write(&helper, script).unwrap();
        let mut permissions = fs::metadata(&helper).unwrap().permissions();
        permissions.set_mode(0o700);
        fs::set_permissions(&helper, permissions).unwrap();
        let paths = HelperPaths {
            gpib: helper.clone(),
            usbtmc: helper,
            setup: None,
        };
        let mut pair = DevicePair::open(&paths, "GPIB0::3::INSTR", "GPIB0::7::INSTR").unwrap();
        assert!(pair.query(Role::Meter, "READ?\n").is_err());
        let report = pair.force_output_off().unwrap();
        assert!(report.output_off_verified);
        assert_eq!(report.zeroing_warning.as_deref(), Some("zeroing failed"));
        let requests = fs::read_to_string(log).unwrap();
        assert_eq!(requests.matches("QUERY 3 2a49444e3f0a").count(), 2);
        assert!(requests.contains("WRITE 3 3a4f555450204f46460a"));
        assert!(requests.contains("QUERY 3 3a4f5554503f0a"));
        assert!(requests.contains("WRITE 3 3a534f55523a4c45563a46495820300a"));
        pair.close().unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn usb_helper_exit_uses_address_zero() {
        use std::os::unix::fs::PermissionsExt;
        // Share the helper-fixture lock with tests that fork while creating scripts.
        // A concurrent fork can temporarily retain a writable script descriptor.
        let _guard = GPIB_TEST_LOCK.lock().unwrap();
        let unique = format!("kohdalab-usb-exit-test-{}", std::process::id());
        let directory = std::env::temp_dir().join(unique);
        fs::create_dir_all(&directory).unwrap();
        let log = directory.join("requests.log");
        let helper = directory.join("fake-usbtmc-helper.sh");
        let script = format!(
            r#"#!/bin/sh
printf 'KIV\tOK\t\n'
read op address timeout payload
printf '%s %s %s\n' "$op" "$address" "$payload" >> '{}'
if [ "$op" = EXIT ] && [ "$address" = 0 ]; then printf 'KIV\tOK\t\n'; else printf 'KIV\tERR\t62616420657869742061646472657373\n'; fi
"#,
            log.display()
        );
        fs::write(&helper, script).unwrap();
        let mut permissions = fs::metadata(&helper).unwrap().permissions();
        permissions.set_mode(0o700);
        fs::set_permissions(&helper, permissions).unwrap();
        let lease = DeviceLease::acquire(&format!("usb-exit-test-{}", std::process::id())).unwrap();
        let mut session = HelperSession::spawn_usb_with_lease(
            &helper,
            &[
                "SERIAL-1".to_owned(),
                "0x0957".to_owned(),
                "0x0a07".to_owned(),
            ],
            Duration::from_secs(2),
            lease,
        )
        .unwrap();
        session.close().unwrap();
        assert_eq!(fs::read_to_string(log).unwrap().trim(), "EXIT 0 -");
    }

    #[cfg(unix)]
    #[test]
    fn roles_connect_independently_and_keep_a_shared_gpib_helper() {
        use std::os::unix::fs::PermissionsExt;
        let _guard = GPIB_TEST_LOCK.lock().unwrap();
        let directory = std::env::temp_dir().join(format!(
            "kohdalab-transport-roles-test-{}",
            std::process::id()
        ));
        fs::create_dir_all(&directory).unwrap();
        let log = directory.join("requests.log");
        let helper = directory.join("fake-gpib-helper.sh");
        let script = format!(
            r#"#!/bin/sh
printf 'START\n' >> '{}'
printf 'KIV\tOK\t\n'
while read op address timeout payload; do
  printf '%s %s %s\n' "$op" "$address" "$payload" >> '{}'
  if [ "$op" = QUERY ] && [ "$payload" = 2a49444e3f0a ]; then
    if [ "$address" = 3 ]; then printf 'KIV\tOK\t594f4b4f474157412c47533231302c53455249414c2d312c322e30310a\n'; elif [ "$address" = 7 ]; then printf 'KIV\tOK\t4167696c656e742c3334343131412c4d455445522d312c322e300a\n'; else printf 'KIV\tOK\t6e6f742d612d6d657465720a\n'; fi
  else
    printf 'KIV\tOK\t300a\n'
  fi
done
"#,
            log.display(),
            log.display()
        );
        fs::write(&helper, script).unwrap();
        let mut permissions = fs::metadata(&helper).unwrap().permissions();
        permissions.set_mode(0o700);
        fs::set_permissions(&helper, permissions).unwrap();
        let paths = HelperPaths {
            gpib: helper.clone(),
            usbtmc: helper,
            setup: None,
        };
        let mut pair = DevicePair::new(&paths);
        assert!(!pair.is_connected(Role::Source));
        pair.connect(Role::Source, "GPIB0::3::INSTR").unwrap();
        assert!(pair.is_connected(Role::Source));
        assert!(pair.query(Role::Source, "READ?").is_ok());
        assert!(pair.connect(Role::Meter, "GPIB0::8::INSTR").is_err());
        assert!(pair.is_connected(Role::Source));
        assert_eq!(
            fs::read_to_string(&log).unwrap().matches("START").count(),
            1
        );
        assert!(fs::read_to_string(&log).unwrap().contains("LOCAL 8 -"));
        pair.connect(Role::Meter, "GPIB0::7::INSTR").unwrap();
        pair.disconnect(Role::Source).unwrap();
        assert!(!pair.is_connected(Role::Source));
        let requests = fs::read_to_string(&log).unwrap();
        assert!(requests.contains("LOCAL 3 -"));
        assert!(!requests.contains("REN 3 30"));
        assert!(!requests.contains("EXIT 1 -"));
        pair.disconnect(Role::Meter).unwrap();
        let requests = fs::read_to_string(log).unwrap();
        assert!(requests.contains("REN 7 30"));
        assert!(requests.contains("EXIT 1 -"));
    }

    #[cfg(unix)]
    #[test]
    fn failed_local_keeps_role_connected_for_disconnect_retry() {
        use std::os::unix::fs::PermissionsExt;
        let _guard = GPIB_TEST_LOCK.lock().unwrap();
        let directory = std::env::temp_dir().join(format!(
            "kohdalab-transport-local-retry-test-{}",
            std::process::id()
        ));
        fs::create_dir_all(&directory).unwrap();
        let log = directory.join("requests.log");
        let failed_once = directory.join("local-failed-once");
        let helper = directory.join("fake-gpib-helper.sh");
        let script = format!(
            r#"#!/bin/sh
printf 'KIV\tOK\t\n'
while read op address timeout payload; do
  printf '%s %s %s\n' "$op" "$address" "$payload" >> '{}'
  if [ "$op" = QUERY ] && [ "$payload" = 2a49444e3f0a ]; then
    printf 'KIV\tOK\t594f4b4f474157412c47533231302c53455249414c2d312c322e30310a\n'
  elif [ "$op" = LOCAL ] && [ "$address" = 3 ] && [ ! -f '{}' ]; then
    touch '{}'
    printf 'KIV\tERR\t6c6f63616c206661696c6564\n'
  else
    printf 'KIV\tOK\t\n'
  fi
done
"#,
            log.display(),
            failed_once.display(),
            failed_once.display()
        );
        fs::write(&helper, script).unwrap();
        let mut permissions = fs::metadata(&helper).unwrap().permissions();
        permissions.set_mode(0o700);
        fs::set_permissions(&helper, permissions).unwrap();
        let paths = HelperPaths {
            gpib: helper.clone(),
            usbtmc: helper,
            setup: None,
        };
        let mut pair = DevicePair::new(&paths);
        pair.connect(Role::Source, "GPIB0::3::INSTR").unwrap();
        assert!(pair.disconnect(Role::Source).is_err());
        assert!(pair.is_connected(Role::Source));
        assert!(!pair.source_identity.is_empty());
        pair.disconnect(Role::Source).unwrap();
        assert!(!pair.is_connected(Role::Source));
        assert_eq!(
            fs::read_to_string(log)
                .unwrap()
                .matches("LOCAL 3 -")
                .count(),
            2
        );
    }

    #[cfg(unix)]
    #[test]
    fn close_acknowledgement_error_detaches_role_after_local_and_ren() {
        use std::os::unix::fs::PermissionsExt;
        let _guard = GPIB_TEST_LOCK.lock().unwrap();
        let directory = std::env::temp_dir().join(format!(
            "kohdalab-transport-close-error-test-{}",
            std::process::id()
        ));
        fs::create_dir_all(&directory).unwrap();
        let helper = directory.join("fake-gpib-helper.sh");
        let script = r#"#!/bin/sh
printf 'KIV\tOK\t\n'
while read op address timeout payload; do
  if [ "$op" = QUERY ] && [ "$payload" = 2a49444e3f0a ]; then
    printf 'KIV\tOK\t594f4b4f474157412c47533231302c53455249414c2d312c322e30310a\n'
  elif [ "$op" = EXIT ]; then
    printf 'KIV\tERR\t657869742061636b206c6f7374\n'
  else
    printf 'KIV\tOK\t\n'
  fi
done
"#;
        fs::write(&helper, script).unwrap();
        let mut permissions = fs::metadata(&helper).unwrap().permissions();
        permissions.set_mode(0o700);
        fs::set_permissions(&helper, permissions).unwrap();
        let paths = HelperPaths {
            gpib: helper.clone(),
            usbtmc: helper,
            setup: None,
        };
        let mut pair = DevicePair::new(&paths);
        pair.connect(Role::Source, "GPIB0::3::INSTR").unwrap();
        assert!(pair.disconnect(Role::Source).is_err());
        assert!(!pair.is_connected(Role::Source));
        assert!(pair.source_identity.is_empty());
    }
}
