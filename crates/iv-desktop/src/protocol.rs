use crate::compatibility::ProtocolInfo;
use crate::state::{Activity, Failure, OutputState, Phase};
use iv_core::{Mode, Record, RunConfig};
use iv_transport::ResourceInfo;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case")]
// Commands are low-frequency control messages. Keep RunConfig inline to avoid
// changing all command producers for a small, bounded queue-size difference.
#[allow(clippy::large_enum_variant)]
pub enum Command {
    /// Correlates one finite, non-safety command with the state published
    /// after that command has completed.
    GuiJob {
        id: String,
        #[serde(rename = "payload")]
        command: Box<Command>,
    },
    Heartbeat,
    Discover,
    SetLiveMode {
        mode: Mode,
    },
    /// Idle acquisition uses the same independent range choices as sweeps.
    SetMeterRanges {
        meter_voltage_range: Option<f64>,
        meter_current_range: Option<f64>,
    },
    Connect {
        simulation: bool,
        source: String,
        meter: String,
        #[serde(default = "default_source_model")]
        source_model: String,
        #[serde(default = "default_meter_model")]
        meter_model: String,
    },
    ConnectSource {
        #[serde(default = "default_source_model")]
        model: String,
        simulation: bool,
        resource: String,
    },
    ConnectSource2 {
        #[serde(default = "default_source_model")]
        model: String,
        simulation: bool,
        resource: String,
    },
    ConnectGate {
        #[serde(default = "default_source_model")]
        model: String,
        simulation: bool,
        resource: String,
    },
    SetGateVoltage {
        voltage: f64,
        compliance: f64,
        ramp_step: f64,
        /// Native source range command selected by the caller. `None` keeps
        /// the existing automatic/manual planner behaviour.
        #[serde(default)]
        range: Option<f64>,
    },
    GateOutputOff,
    /// Enable one of the ordinary source roles at a fixed, verified level.
    /// `secondary` selects Source 2; it is false for the primary Source.
    SetSourceDc {
        #[serde(default)]
        secondary: bool,
        mode: Mode,
        value: f64,
        compliance: f64,
        ramp_step: f64,
        /// Native source range command selected by the caller. `None` keeps
        /// the existing automatic/manual planner behaviour.
        #[serde(default)]
        range: Option<f64>,
    },
    /// Immediate, source-specific emergency OFF. This is intentionally not a
    /// GUI job because it must bypass the bounded normal command queue.
    SourceDcOff {
        #[serde(default)]
        secondary: bool,
    },
    ConnectMeter {
        #[serde(default = "default_meter_model")]
        model: String,
        simulation: bool,
        resource: String,
    },
    ConnectReferenceMeter {
        #[serde(default = "default_meter_model")]
        model: String,
        simulation: bool,
        resource: String,
    },
    DisconnectReferenceMeter,
    DisconnectSource,
    DisconnectSource2,
    DisconnectGate,
    DisconnectMeter,
    Run {
        config: RunConfig,
        path: String,
    },
    Stop,
    OutputOff,
    Disconnect,
    Shutdown,
}

/// Wrap a command whose final engine state should be correlated with `id`.
///
/// Safety and lease commands deliberately cannot be jobs: they must remain
/// immediate, even if the normal command queue is full.
pub fn gui_job(id: String, command: Command) -> Result<Command, String> {
    validate_gui_job(&id, &command).map_err(|_| {
        "GUI jobs support only finite measurement and connection commands.".to_string()
    })?;
    Ok(Command::GuiJob {
        id,
        command: Box::new(command),
    })
}

/// Removes a single, valid GUI-job envelope before the engine's normal
/// admission and safety handling.  Invalid envelopes are rejected rather
/// than allowing a safety command to be delayed behind the normal queue.
pub fn unwrap_gui_job(command: Command) -> Result<(Option<String>, Command), String> {
    let Command::GuiJob { id, command } = command else {
        return Ok((None, command));
    };
    validate_gui_job(&id, &command)?;
    Ok((Some(id), *command))
}

pub fn validate_gui_job(id: &str, command: &Command) -> Result<(), String> {
    if id.trim().is_empty() || !is_gui_job_command(command) {
        return Err("Invalid GUI job envelope.".into());
    }
    Ok(())
}

fn is_gui_job_command(command: &Command) -> bool {
    matches!(
        command,
        Command::Discover
            | Command::SetLiveMode { .. }
            | Command::Connect { .. }
            | Command::ConnectSource { .. }
            | Command::ConnectSource2 { .. }
            | Command::ConnectGate { .. }
            | Command::ConnectMeter { .. }
            | Command::ConnectReferenceMeter { .. }
            | Command::DisconnectReferenceMeter
            | Command::DisconnectSource
            | Command::DisconnectSource2
            | Command::DisconnectGate
            | Command::DisconnectMeter
            | Command::Disconnect
            | Command::Run { .. }
            | Command::SetGateVoltage { .. }
            | Command::SetSourceDc { .. }
    )
}

/// Latched safety event from the run's frozen configuration, not GUI edits.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProtectionKind {
    SourceCompliance,
    MeasurementLimit,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProtectionAlert {
    pub kind: ProtectionKind,
    pub limit: f64,
    pub unit: String,
    pub observed: Option<f64>,
    pub indicator: String,
    #[serde(default)]
    pub source_index: Option<u8>,
}

/// Last successfully verified manual DC setting for an ordinary source role.
/// It is state only: reconnecting or editing the mode never re-applies it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DcOutputState {
    pub mode: Mode,
    pub value: f64,
    pub compliance: f64,
    pub ramp_step: f64,
    /// Explicit native source range, when the caller selected one.
    #[serde(default)]
    pub range: Option<f64>,
}

/// The verified hardware range held by an enabled fixed DC output.  The
/// command value intentionally uses the instrument's native range units
/// (including 7651 range codes); maximum and resolution are SI values.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DcLimits {
    pub range: f64,
    pub maximum: f64,
    pub resolution: f64,
}

/// The final state belonging to the last tagged GUI operation.  The embedded
/// snapshot deliberately has no `gui_job` field so JSON cannot recurse.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GuiJobCompletion {
    pub id: String,
    pub snapshot: serde_json::Value,
}
impl ProtectionAlert {
    pub(crate) fn measurement(config: &RunConfig, observed: f64) -> Self {
        Self {
            kind: ProtectionKind::MeasurementLimit,
            limit: config.compliance,
            unit: if config.mode == Mode::Iv { "A" } else { "V" }.into(),
            observed: Some(observed),
            indicator: String::new(),
            source_index: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    #[serde(default)]
    pub gui_job: Option<GuiJobCompletion>,
    /// Small bounded completion history survives coalesced state updates.
    #[serde(default)]
    pub gui_jobs: Vec<GuiJobCompletion>,
    /// Acknowledged explicit safety commands, including cancellation before Run starts.
    #[serde(default)]
    pub safety_revision: u64,
    #[serde(default)]
    pub source_dc: Option<DcOutputState>,
    #[serde(default)]
    pub source_dc_limits: Option<DcLimits>,
    #[serde(default)]
    pub source2_dc: Option<DcOutputState>,
    #[serde(default)]
    pub source2_dc_limits: Option<DcLimits>,
    #[serde(default)]
    pub source2_connected: bool,
    #[serde(default)]
    pub source2_simulation: bool,
    #[serde(default)]
    pub source2_identity: String,
    #[serde(default)]
    pub source2_resource: String,
    #[serde(default)]
    pub source2_output: OutputState,
    #[serde(default)]
    pub source2_level: Option<f64>,
    #[serde(default)]
    pub source2_resolution: Option<f64>,
    #[serde(default)]
    pub balanced_active: bool,
    #[serde(default)]
    pub gate_connected: bool,
    #[serde(default)]
    pub gate_simulation: bool,
    #[serde(default)]
    pub gate_identity: String,
    #[serde(default)]
    pub gate_resource: String,
    #[serde(default)]
    pub gate_output: OutputState,
    #[serde(default)]
    pub gate_voltage: Option<f64>,
    #[serde(default)]
    pub gate_compliance: Option<f64>,
    #[serde(default)]
    pub gate_revision: u64,
    #[serde(default)]
    pub gate_dc_limits: Option<DcLimits>,
    pub phase: Phase,
    pub message: String,
    pub connected: bool,
    pub source_connected: bool,
    pub meter_connected: bool,
    #[serde(default)]
    pub reference_meter_connected: bool,
    #[serde(default)]
    pub reference_meter_simulation: bool,
    #[serde(default)]
    pub reference_meter_identity: String,
    #[serde(default)]
    pub reference_meter_resource: String,
    pub source_simulation: bool,
    pub meter_simulation: bool,
    pub simulation: bool,
    pub source_identity: String,
    pub meter_identity: String,
    pub source_resource: String,
    pub meter_resource: String,
    pub output: OutputState,
    pub points: usize,
    pub total: usize,
    pub elapsed: f64,
    pub resistance: Option<f64>,
    #[serde(default)]
    pub reference_resistance: Option<f64>,
    pub last: Option<Record>,
    #[serde(default)]
    pub live: Option<Record>,
    #[serde(default)]
    pub live_error: String,
    #[serde(default)]
    pub protection_alert: Option<ProtectionAlert>,
    pub path: String,
    #[serde(flatten)]
    pub error: Failure,
    #[serde(default)]
    pub activity: Activity,
    #[serde(default)]
    pub protocol: ProtocolInfo,
    pub resources: Vec<ResourceInfo>,
    pub warnings: Vec<String>,
    pub run_id: u64,
}

impl Default for Snapshot {
    fn default() -> Self {
        Self {
            gui_job: None,
            gui_jobs: Vec::new(),
            safety_revision: 0,
            source_dc: None,
            source_dc_limits: None,
            source2_dc: None,
            source2_dc_limits: None,
            source2_connected: false,
            source2_simulation: false,
            source2_identity: String::new(),
            source2_resource: String::new(),
            source2_output: OutputState::Unknown,
            source2_level: None,
            source2_resolution: None,
            balanced_active: false,
            gate_connected: false,
            gate_simulation: false,
            gate_identity: String::new(),
            gate_resource: String::new(),
            gate_output: OutputState::Unknown,
            gate_voltage: None,
            gate_compliance: None,
            gate_revision: 0,
            gate_dc_limits: None,
            phase: Phase::Idle,
            message: "Choose your instruments or explore Simulation.".into(),
            connected: false,
            source_connected: false,
            meter_connected: false,
            reference_meter_connected: false,
            reference_meter_simulation: false,
            reference_meter_identity: String::new(),
            reference_meter_resource: String::new(),
            source_simulation: false,
            meter_simulation: false,
            simulation: false,
            source_identity: String::new(),
            meter_identity: String::new(),
            output: OutputState::Unknown,
            points: 0,
            total: 0,
            elapsed: 0.0,
            last: None,
            live: None,
            live_error: String::new(),
            protection_alert: None,
            path: String::new(),
            source_resource: String::new(),
            meter_resource: String::new(),
            resistance: None,
            reference_resistance: None,
            error: Failure::default(),
            activity: Activity::default(),
            protocol: ProtocolInfo::default(),
            resources: vec![],
            warnings: vec![],
            run_id: 0,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "event", content = "data", rename_all = "snake_case")]
pub enum Event {
    State(Box<Snapshot>),
    Point { run_id: u64, record: Box<Record> },
}

fn default_source_model() -> String {
    "YOKOGAWA_GS210".into()
}
fn default_meter_model() -> String {
    "AGILENT_34411A".into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gui_job_envelope_accepts_only_normal_finite_commands() {
        let command = gui_job("job-1".into(), Command::Discover).unwrap();
        let (id, inner) = unwrap_gui_job(command).unwrap();
        assert_eq!(id.as_deref(), Some("job-1"));
        assert!(matches!(inner, Command::Discover));

        assert!(
            gui_job(
                "dc-1".into(),
                Command::SetSourceDc {
                    secondary: false,
                    mode: Mode::Iv,
                    value: 1e-3,
                    compliance: 1e-3,
                    ramp_step: 1e-4,
                    range: None,
                },
            )
            .is_ok()
        );

        for command in [
            Command::Heartbeat,
            Command::Stop,
            Command::GateOutputOff,
            Command::SourceDcOff { secondary: false },
            Command::OutputOff,
            Command::Shutdown,
        ] {
            assert!(gui_job("job-1".into(), command).is_err());
        }
        let nested = Command::GuiJob {
            id: "outer".into(),
            command: Box::new(Command::GuiJob {
                id: "inner".into(),
                command: Box::new(Command::Discover),
            }),
        };
        assert!(unwrap_gui_job(nested).is_err());
    }

    #[test]
    fn legacy_dc_commands_deserialize_without_an_explicit_range() {
        let command: Command = serde_json::from_str(
            r#"{"command":"set_source_dc","mode":"dc_iv","value":0.001,"compliance":0.2,"ramp_step":0.0001}"#,
        )
        .unwrap();
        assert!(matches!(command, Command::SetSourceDc { range: None, .. }));
        let command: Command = serde_json::from_str(
            r#"{"command":"set_gate_voltage","voltage":0.001,"compliance":0.2,"ramp_step":0.0001}"#,
        )
        .unwrap();
        assert!(matches!(
            command,
            Command::SetGateVoltage { range: None, .. }
        ));
    }
}
