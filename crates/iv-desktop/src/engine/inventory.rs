//! Connections which persist independently of the sweep role assignment.
use super::*;
use crate::protocol::{DcOutputState, InventoryDevice};
use iv_transport::OutputOffReport;
use std::collections::VecDeque;

fn role(source: bool) -> Role {
    if source { Role::Source } else { Role::Meter }
}

pub(super) fn assigned_role(entry: &InventoryDevice) -> Option<Role> {
    entry
        .assigned
        .then_some(match (entry.source, entry.auxiliary) {
            (true, false) => Role::Source,
            (true, true) => Role::Source2,
            (false, false) => Role::Meter,
            (false, true) => Role::ReferenceMeter,
        })
}

/// Validate the complete role change before parking any endpoint.
pub(super) fn bind(
    pair: &mut Option<DevicePair>,
    state: &mut Snapshot,
    selection: [(Role, Option<u64>); 4],
    mode: Mode,
) -> Result<(), String> {
    let mut selected = std::collections::BTreeSet::new();
    let mut physical = Vec::new();
    let mut devices = Vec::new();
    for (role, id) in selection {
        let source = matches!(role, Role::Source | Role::Source2);
        let previous = state
            .inventory_devices
            .iter()
            .find(|entry| assigned_role(entry) == Some(role));
        if role_state(state, role).0 && previous.is_none() {
            return Err(format!(
                "Disconnect the directly connected {} before using inventory bindings.",
                role_label(role)
            ));
        }
        let device = if let Some(id) = id {
            if !selected.insert(id) {
                return Err("Choose a different instrument for each measurement role.".into());
            }
            let entry = state
                .inventory_devices
                .iter()
                .find(|entry| entry.id == id && entry.source == source)
                .ok_or_else(|| format!("Connect the selected {} first.", role_label(role)))?;
            if source && entry.simulation && entry.output != OutputState::Off {
                return Err(
                    "Turn the selected source output OFF before changing its assignment.".into(),
                );
            }
            let role_mode = if role == Role::ReferenceMeter {
                if mode == Mode::Iv { Mode::Vi } else { Mode::Iv }
            } else {
                mode
            };
            if !entry.simulation
                && !iv_core::instrument_descriptor(&entry.model)
                    .is_some_and(|descriptor| descriptor.supports_mode(role_mode))
            {
                return Err(format!(
                    "{} does not support this measurement function.",
                    entry.model
                ));
            }
            Some(entry.clone())
        } else {
            None
        };
        if previous.is_some_and(|entry| entry.source)
            && match role {
                Role::Source => state.output,
                _ => state.source2_output,
            } == OutputState::On
        {
            return Err("Turn the measurement sources OFF before changing assignments.".into());
        }
        if previous.is_some_and(|entry| !entry.simulation)
            || device.as_ref().is_some_and(|entry| !entry.simulation)
        {
            physical.push((
                role,
                device
                    .as_ref()
                    .filter(|entry| !entry.simulation)
                    .map(|entry| entry.id),
            ));
        }
        devices.push((role, device));
    }
    if !physical.is_empty() {
        pair.as_mut()
            .ok_or("Instrument transport unavailable.")?
            .select_registered(&physical)?;
    }
    for entry in &mut state.inventory_devices {
        let next_role = devices
            .iter()
            .find(|(_, device)| device.as_ref().is_some_and(|item| item.id == entry.id))
            .map(|(role, _)| *role);
        if next_role != assigned_role(entry) {
            if entry.source {
                entry.output = OutputState::Off;
                entry.dc = None;
                entry.dc_limits = None;
            }
            entry.reading = None;
            entry.error.clear();
        }
        entry.assigned = next_role.is_some();
        entry.auxiliary = matches!(next_role, Some(Role::Source2 | Role::ReferenceMeter));
        if let Some(role) = next_role {
            entry.mode = if role == Role::ReferenceMeter {
                if mode == Mode::Iv { Mode::Vi } else { Mode::Iv }
            } else {
                mode
            };
        }
    }
    for (role, device) in devices {
        let connected = device.is_some();
        let simulation = device.as_ref().is_some_and(|entry| entry.simulation);
        let (identity, resource) = device
            .map(|entry| (entry.identity, entry.resource))
            .unwrap_or_default();
        // Both the parked and currently assigned sources have passed an OFF
        // check before the transport changes its assignments.
        let output = OutputState::Off;
        match role {
            Role::Source => {
                state.source_connected = connected;
                state.source_simulation = simulation;
                state.source_identity = identity;
                state.source_resource = resource;
                state.output = output;
                state.source_dc = None;
                state.source_dc_limits = None;
            }
            Role::Source2 => {
                state.source2_connected = connected;
                state.source2_simulation = simulation;
                state.source2_identity = identity;
                state.source2_resource = resource;
                state.source2_output = output;
                state.source2_dc = None;
                state.source2_dc_limits = None;
                state.source2_level = None;
            }
            Role::Meter => {
                state.meter_connected = connected;
                state.meter_simulation = simulation;
                state.meter_identity = identity;
                state.meter_resource = resource;
            }
            Role::ReferenceMeter => {
                state.reference_meter_connected = connected;
                state.reference_meter_simulation = simulation;
                state.reference_meter_identity = identity;
                state.reference_meter_resource = resource;
            }
            Role::Gate => unreachable!(),
        }
    }
    let idle_reading = state.idle_reading.take();
    state.live_error.clear();
    refresh_connections(state);
    state.idle_reading = idle_reading;
    state.phase = if selected.is_empty() {
        Phase::Idle
    } else {
        Phase::Ready
    };
    state.message = "Updated measurement assignments using existing connections.".into();
    Ok(())
}

pub(super) fn connect(
    pair: &mut Option<DevicePair>,
    state: &mut Snapshot,
    mut device: InventoryDevice,
) -> Result<(), String> {
    if device.id == 0
        || state
            .inventory_devices
            .iter()
            .any(|entry| entry.id == device.id)
    {
        return Err("Choose a valid, disconnected inventory entry.".into());
    }
    if device.simulation {
        let model = if device.source {
            "SIMULATED_SOURCE"
        } else {
            "SIMULATED_METER"
        };
        if device.model != model {
            return Err("The simulation model does not match the instrument type.".into());
        }
        device.identity = "Simulation".into();
        device.resource = model.into();
        if device.source {
            device.output = OutputState::Off;
        }
    } else {
        if pair.is_none() {
            *pair = Some(DevicePair::new(&HelperPaths::discover()?));
        }
        let pair = pair.as_mut().expect("transport initialized");
        pair.connect_registered(
            device.id,
            role(device.source),
            &device.resource,
            &device.model,
        )?;
        let info = pair
            .registered_devices()
            .into_iter()
            .find(|entry| entry.id == device.id)
            .expect("new connection registered");
        device.identity = info.identity;
    }
    state.inventory_devices.push(device);
    Ok(())
}

pub(super) fn select(
    pair: &mut Option<DevicePair>,
    state: &mut Snapshot,
    id: u64,
    source: bool,
) -> Result<(), String> {
    let device = state
        .inventory_devices
        .iter()
        .find(|entry| entry.id == id && entry.source == source)
        .cloned()
        .ok_or("Connect the selected inventory instrument first.")?;
    if let Some(assigned) = assigned_role(&device) {
        return if assigned == role(source) {
            Ok(())
        } else {
            Err("This instrument is assigned to another measurement role.".into())
        };
    }
    if source {
        let observed = if device.simulation {
            device.output
        } else {
            let pair = pair.as_mut().ok_or("Instrument transport unavailable.")?;
            safety::output_state(&pair.query_unassigned(id, ":OUTP?")?)?
        };
        if observed != OutputState::Off {
            return Err(
                "Verify the selected source output OFF before assigning it to the sweep.".into(),
            );
        }
    }
    let previous = state
        .inventory_devices
        .iter()
        .find(|entry| assigned_role(entry) == Some(role(source)));
    let selected_role = role(source);
    if role_state(state, selected_role).0 && previous.is_none() {
        return Err(
            "Disconnect the current instrument before switching to a registered device.".into(),
        );
    }
    if source && previous.is_some() && state.output != OutputState::Off {
        return Err("Verify Source output OFF before changing its assignment.".into());
    }
    if !device.simulation || previous.is_some_and(|entry| !entry.simulation) {
        let pair = pair.as_mut().ok_or("Instrument transport unavailable.")?;
        pair.select_registered(&[(selected_role, (!device.simulation).then_some(id))])?;
    }
    for entry in state
        .inventory_devices
        .iter_mut()
        .filter(|entry| entry.source == source)
    {
        if assigned_role(entry) == Some(selected_role) || entry.id == id {
            entry.assigned = entry.id == id;
            entry.auxiliary = false;
            if entry.source && !entry.assigned {
                entry.output = OutputState::Off;
            }
            entry.reading = None;
            entry.error.clear();
        }
    }
    if source {
        state.source_connected = true;
        state.source_simulation = device.simulation;
        state.source_identity = device.identity;
        state.source_resource = device.resource;
        state.output = if device.simulation {
            OutputState::Off
        } else {
            OutputState::Unknown
        };
        state.source_dc = None;
        state.source_dc_limits = None;
    } else {
        state.meter_connected = true;
        state.meter_simulation = device.simulation;
        state.meter_identity = device.identity;
        state.meter_resource = device.resource;
    }
    state.live = None;
    state.live_error.clear();
    refresh_connections(state);
    state.phase = Phase::Ready;
    state.message = format!(
        "Selected {} from the device inventory.",
        role_label(selected_role)
    );
    Ok(())
}

pub(super) fn disconnect(
    pair: &mut Option<DevicePair>,
    state: &mut Snapshot,
    id: u64,
    control: &Control,
) -> Result<(), String> {
    let Some(device) = state.inventory_devices.iter().find(|entry| entry.id == id) else {
        return Ok(());
    };
    if let Some(role) = assigned_role(device) {
        return disconnect_role(pair, state, role, control);
    }
    if device.source && device.output != OutputState::Off {
        output_off_one(pair.as_mut(), state, id)?;
    }
    let device = state
        .inventory_devices
        .iter()
        .find(|entry| entry.id == id)
        .expect("output off keeps the connection");
    if !device.simulation {
        pair.as_mut()
            .ok_or("Instrument transport unavailable.")?
            .disconnect_registered(id)?;
    }
    state.inventory_devices.retain(|entry| entry.id != id);
    Ok(())
}

struct InventorySource<'a> {
    pair: &'a mut DevicePair,
    id: u64,
    model: &'a str,
    identity: &'a str,
}

impl source_dc::SourcePort for InventorySource<'_> {
    fn model(&self, _: Role) -> &str {
        self.model
    }
    fn identity(&self, _: Role) -> &str {
        self.identity
    }
    fn is_connected(&self, _: Role) -> bool {
        true
    }
    fn query(&mut self, _: Role, command: &str) -> Result<String, String> {
        self.pair.query_unassigned(self.id, command)
    }
    fn write(&mut self, _: Role, command: &str) -> Result<(), String> {
        self.pair.write_unassigned_source(self.id, command)
    }
    fn force_output_off_role(&mut self, _: Role) -> Result<OutputOffReport, String> {
        self.pair.force_unassigned_output_off(self.id)
    }
}

fn operate_dc(
    pair: Option<&mut DevicePair>,
    device: &mut InventoryDevice,
    warnings: &mut Vec<String>,
    operation: impl FnOnce(Option<&mut dyn source_dc::SourcePort>, &mut Snapshot) -> Result<(), String>,
) -> Result<(), String> {
    let mut local = Snapshot {
        source_connected: true,
        source_simulation: device.simulation,
        output: device.output,
        source_dc: device.dc.clone(),
        source_dc_limits: device.dc_limits.clone(),
        error: device.error.clone().into(),
        ..Default::default()
    };
    let mut port = pair
        .filter(|_| !device.simulation)
        .map(|pair| InventorySource {
            pair,
            id: device.id,
            model: &device.model,
            identity: &device.identity,
        });
    let result = operation(
        port.as_mut()
            .map(|port| port as &mut dyn source_dc::SourcePort),
        &mut local,
    );
    device.output = local.output;
    device.dc = local.source_dc;
    device.dc_limits = local.source_dc_limits;
    device.error = result.as_ref().err().cloned().unwrap_or_default();
    warnings.extend(
        local
            .warnings
            .into_iter()
            .map(|warning| format!("Inventory source {}: {warning}", device.id)),
    );
    result
}

pub(super) fn set_dc(
    pair: Option<&mut DevicePair>,
    state: &mut Snapshot,
    id: u64,
    request: DcOutputState,
    cancelled: impl Fn() -> bool,
) -> Result<(), String> {
    let device = state
        .inventory_devices
        .iter_mut()
        .find(|entry| entry.id == id && entry.source && !entry.assigned)
        .ok_or("Connect an unassigned Source before enabling its DC output.")?;
    operate_dc(pair, device, &mut state.warnings, |port, local| {
        source_dc::set_with(port, local, false, request, cancelled)
    })
}

pub(super) fn output_off_one(
    pair: Option<&mut DevicePair>,
    state: &mut Snapshot,
    id: u64,
) -> Result<(), String> {
    let device = state
        .inventory_devices
        .iter_mut()
        .find(|entry| entry.id == id && entry.source && !entry.assigned)
        .ok_or("Unassigned Source is not connected.")?;
    operate_dc(pair, device, &mut state.warnings, |port, local| {
        source_dc::output_off_with(port, local, false)
    })
    .map_err(|error| format!("Inventory source {id}: {error}"))
}

pub(super) fn output_off(
    mut pair: Option<&mut DevicePair>,
    state: &mut Snapshot,
) -> Result<(), String> {
    let mut errors = Vec::new();
    let ids: Vec<_> = state
        .inventory_devices
        .iter()
        .filter(|entry| entry.source && !entry.assigned)
        .map(|entry| entry.id)
        .collect();
    for id in ids {
        if let Err(error) = output_off_one(pair.as_deref_mut(), state, id) {
            errors.push(error);
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join(" · "))
    }
}

pub(super) fn check_dc(mut pair: Option<&mut DevicePair>, state: &mut Snapshot) -> bool {
    let mut changed = false;
    for device in state.inventory_devices.iter_mut().filter(|entry| {
        entry.source
            && !entry.assigned
            && entry.output == OutputState::On
            && entry.dc.is_some()
            && !entry.simulation
    }) {
        let result = operate_dc(
            pair.as_deref_mut(),
            device,
            &mut state.warnings,
            |port, local| source_dc::check_with(port, local, false),
        );
        if result.is_err() {
            changed = true;
        }
    }
    changed
}

pub(super) fn poll(
    pair: Option<&mut DevicePair>,
    device: &mut InventoryDevice,
) -> Result<IdleReading, String> {
    let query = |pair: &mut DevicePair, command: &str| pair.query_unassigned(device.id, command);
    let (mode, value, resolution) = if device.simulation {
        let dc = device
            .dc
            .as_ref()
            .filter(|_| device.output == OutputState::On);
        (
            dc.map_or(device.mode, |dc| dc.mode),
            dc.map_or(0.0, |dc| dc.value),
            device.dc_limits.as_ref().map(|limits| limits.resolution),
        )
    } else {
        let pair = pair.ok_or("Instrument transport unavailable.")?;
        if device.source {
            device.output = safety::output_state(&query(pair, ":OUTP?")?)?;
            let mode = source_mode(&query(pair, ":SOUR:FUNC?")?)?;
            let value = live_number(&query(pair, ":SOUR:LEV?")?, "source")?;
            let range_query = iv_core::source_range_query(&device.model)?.unwrap_or(":SOUR:RANG?");
            let range = positive_reading(&query(pair, range_query)?);
            let resolution = range.and_then(|range| source_resolution(&device.model, mode, range));
            (mode, value, resolution)
        } else {
            let mode = device.mode;
            if device.reading.is_none() {
                let config = RunConfig {
                    mode,
                    meter_model: device.model.clone(),
                    ..Default::default()
                };
                let function = iv_core::measurement_definition(mode).meter_function;
                pair.configure_unassigned_meter(device.id, &format!("CONF:{function}:DC"))?;
                for command in meter::commands(&config)? {
                    pair.configure_unassigned_meter(device.id, &command)?;
                }
                pair.configure_unassigned_meter(device.id, &format!("SENS:{function}:DC:NPLC 1"))?;
                pair.configure_unassigned_meter(device.id, "TRIG:SOUR IMM")?;
                meter::verify(&config, |command| query(pair, command))?;
            }
            let value = live_number(&query(pair, "READ?")?, "meter")?;
            let resolution = match iv_core::meter_display_queries(&device.model, mode)? {
                Some((_, resolution_query)) => positive_reading(&query(pair, resolution_query)?),
                None => None,
            };
            (mode, value, resolution)
        }
    };
    Ok(IdleReading {
        timestamp: timestamp(),
        mode,
        source: device.source,
        value,
        resolution,
    })
}

#[derive(Default)]
pub(super) struct Commands {
    off: VecDeque<u64>,
    off_in_progress: Option<u64>,
    dc: VecDeque<DcCommand>,
}

struct DcCommand {
    id: u64,
    job: Option<String>,
    request: DcOutputState,
    cancelled: bool,
}

pub(super) fn enqueue(control: &Control, job: Option<String>, command: Command) -> bool {
    let Ok(mut commands) = control.inventory_commands.lock() else {
        return false;
    };
    match command {
        Command::InventoryDcOff { id } => {
            for pending in commands.dc.iter_mut().filter(|pending| pending.id == id) {
                pending.cancelled = true;
            }
            if !commands.off.contains(&id) {
                if commands.off.len() == 32 {
                    return false;
                }
                commands.off.push_back(id);
            }
        }
        Command::SetInventoryDc { id, request } => {
            if commands.dc.len() == 32 {
                return false;
            }
            let cancelled = commands.off.contains(&id)
                || commands.off_in_progress == Some(id)
                || stop_inventory(control);
            commands.dc.push_back(DcCommand {
                id,
                job,
                request,
                cancelled,
            });
        }
        _ => unreachable!("only per-device output commands use this queue"),
    }
    true
}

pub(super) fn cancel_queued_dc(control: &Control) {
    if let Ok(mut commands) = control.inventory_commands.lock() {
        for command in &mut commands.dc {
            command.cancelled = true;
        }
    }
}

fn stop_inventory(control: &Control) -> bool {
    control.closed.load(Ordering::Acquire)
        || control.inventory_abort.load(Ordering::Acquire)
        || control.pending_safety.load(Ordering::Acquire) & SAFETY_ALL_OFF != 0
        || control
            .heartbeat
            .lock()
            .map_or(true, |t| t.elapsed() > Duration::from_secs(2))
}

pub(super) fn off_requested(control: &Control) -> bool {
    control
        .inventory_commands
        .lock()
        .map_or(true, |queue| !queue.off.is_empty())
}

pub(super) fn service_off(
    pair: Option<&mut DevicePair>,
    state: &mut Snapshot,
    control: &Control,
) -> bool {
    let id = {
        let Ok(mut commands) = control.inventory_commands.lock() else {
            return false;
        };
        commands.off_in_progress = commands.off.pop_front();
        commands.off_in_progress
    };
    let Some(id) = id else {
        return false;
    };
    state.inventory_error = output_off_one(pair, state, id).err().unwrap_or_default();
    if let Ok(mut commands) = control.inventory_commands.lock() {
        commands.off_in_progress = None;
    }
    true
}

pub(super) fn service_command(
    mut pair: Option<&mut DevicePair>,
    state: &mut Snapshot,
    control: &Control,
) -> bool {
    if service_off(pair.as_deref_mut(), state, control) {
        return true;
    }
    let command = control
        .inventory_commands
        .lock()
        .ok()
        .and_then(|mut commands| commands.dc.pop_front());
    let Some(DcCommand {
        id,
        job,
        request,
        cancelled,
    }) = command
    else {
        return false;
    };
    let cancel = || cancelled || stop_inventory(control) || off_requested(control);
    state.inventory_error = if cancel() {
        "DC request cancelled before changing output.".into()
    } else {
        set_dc(pair, state, id, request, cancel)
            .err()
            .unwrap_or_default()
    };
    if let Some(job) = job {
        if let Some(entry) = state
            .inventory_devices
            .iter_mut()
            .find(|entry| entry.id == id)
        {
            entry.dc_job = Some(job.clone());
        }
        let sweep_error = std::mem::replace(&mut state.error, state.inventory_error.clone().into());
        complete_gui_job(state, job);
        state.error = sweep_error;
    }
    true
}

pub(super) fn poll_ready(entry: &InventoryDevice) -> bool {
    !entry.assigned && entry.error.is_empty()
}

pub(super) fn poll_next(
    pair: Option<&mut DevicePair>,
    state: &mut Snapshot,
    cursor: &mut usize,
) -> bool {
    let len = state.inventory_devices.len();
    for _ in 0..len {
        *cursor = (*cursor + 1) % len;
        let entry = &mut state.inventory_devices[*cursor];
        if !poll_ready(entry) {
            continue;
        }
        let id = entry.id;
        let expected_output = match entry.output {
            OutputState::On if entry.dc.is_some() => Some(OutputState::On),
            OutputState::Off => Some(OutputState::Off),
            _ => None,
        };
        let mut pair = pair;
        let reading = poll(pair.as_deref_mut(), entry);
        let unexpected_output =
            entry.source && expected_output.is_some_and(|expected| expected != entry.output);
        let error = match reading {
            Ok(reading) if !unexpected_output => {
                entry.reading = Some(reading);
                return true;
            }
            Ok(_) => "Unexpected Source output state during live monitoring.".into(),
            Err(error) => error,
        };
        entry.reading = None;
        if entry.source {
            let shutdown = output_off_one(pair, state, id).err();
            let entry = &mut state.inventory_devices[*cursor];
            entry.error = shutdown.map_or(error.clone(), |off| format!("{error} · {off}"));
            state.inventory_error.clone_from(&entry.error);
        } else {
            entry.error = error;
        }
        return true;
    }
    false
}

/// Merge only independent-device data. Run phase, output and records remain
/// owned by the acquisition thread even when an inventory command fails.
pub(super) fn apply_updates(state: &mut Snapshot, inventory: &Snapshot) {
    for entry in &mut state.inventory_devices {
        if !entry.assigned
            && let Some(updated) = inventory
                .inventory_devices
                .iter()
                .find(|other| other.id == entry.id)
        {
            entry.clone_from(updated);
        }
    }
    state.inventory_error.clone_from(&inventory.inventory_error);
    for warning in &inventory.warnings {
        if !state.warnings.contains(warning) {
            state.warnings.push(warning.clone());
        }
    }
    for job in &inventory.gui_jobs {
        if !state.gui_jobs.iter().any(|existing| existing.id == job.id) {
            if state.gui_jobs.len() == 4 {
                state.gui_jobs.remove(0);
            }
            state.gui_jobs.push(job.clone());
            state.gui_job = Some(job.clone());
        }
    }
}

fn publish_inventory(latest: &StateFeed, state: &Snapshot) {
    if let Ok(mut pending) = latest.pending.lock()
        && let Some(mut snapshot) = pending.sweep.clone()
    {
        apply_updates(&mut snapshot, state);
        pending.inventory = Some(state.clone());
        pending.snapshot = Some(snapshot);
        latest.changed.notify_one();
    }
}

/// Keep the acquisition and independent controls responsive to each other
/// without sharing a mutable transport owner. Only physical helper requests
/// serialize when devices are connected through the same controller.
pub(super) fn during_sweep(
    pair: &mut Option<DevicePair>,
    state: &mut Snapshot,
    control: &Control,
    latest: &StateFeed,
    sweep: impl FnOnce(Option<&mut DevicePair>, &mut Snapshot) -> Result<(), String>,
) -> Result<(), String> {
    let mut parked = pair.as_mut().map(DevicePair::take_parked_for_sweep);
    let mut inventory = Snapshot {
        inventory_devices: state
            .inventory_devices
            .iter()
            .filter(|entry| !entry.assigned)
            .cloned()
            .collect(),
        inventory_error: state.inventory_error.clone(),
        ..Snapshot::default()
    };
    if let Ok(mut pending) = latest.pending.lock() {
        pending.sweep = Some(display_snapshot(state));
    }
    control
        .inventory_worker_active
        .store(true, Ordering::Release);
    let result = std::thread::scope(|scope| {
        let (sweep_running, sweep_finished) = mpsc::channel::<()>();
        let parked = &mut parked;
        let inventory = &mut inventory;
        let worker = scope.spawn(move || {
            let mut cursor = 0;
            let mut poll_due = Instant::now();
            let mut protection_due = Instant::now();
            while sweep_finished.try_recv() == Err(mpsc::TryRecvError::Empty) {
                if stop_inventory(control) {
                    if let Err(error) = output_off(parked.as_mut(), inventory) {
                        inventory.inventory_error = error;
                    }
                    publish_inventory(latest, inventory);
                    break;
                }
                let mut changed = service_command(parked.as_mut(), inventory, control);
                if Instant::now() >= protection_due {
                    protection_due = Instant::now() + Duration::from_millis(50);
                    changed |= check_dc(parked.as_mut(), inventory);
                }
                if Instant::now() >= poll_due {
                    poll_due = Instant::now() + Duration::from_millis(100);
                    changed |= poll_next(parked.as_mut(), inventory, &mut cursor);
                }
                if !inventory.inventory_outputs_managed() {
                    control.inventory_abort.store(true, Ordering::Release);
                    cancel_queued_dc(control);
                    publish_inventory(latest, inventory);
                    break;
                }
                if changed {
                    publish_inventory(latest, inventory);
                } else {
                    std::thread::sleep(Duration::from_millis(10));
                }
            }
        });
        let result = sweep(pair.as_mut().filter(|_| !state.simulation), state);
        drop(sweep_running);
        worker
            .join()
            .expect("independent instrument owner panicked");
        result
    });
    control
        .inventory_worker_active
        .store(false, Ordering::Release);
    if let (Some(pair), Some(parked)) = (pair.as_mut(), parked) {
        pair.restore_parked_after_sweep(parked);
    }
    let unsafe_output = !inventory.inventory_outputs_managed();
    apply_updates(state, &inventory);
    if let Ok(mut pending) = latest.pending.lock() {
        pending.sweep = None;
        pending.inventory = None;
    }
    control.inventory_abort.store(false, Ordering::Release);
    if unsafe_output {
        let error = "An independent source output could not be verified during the sweep.";
        let cleanup = output_off(pair.as_mut(), state).err();
        return Err(cleanup.map_or(error.to_owned(), |off| format!("{error} {off}")));
    }
    result
}
