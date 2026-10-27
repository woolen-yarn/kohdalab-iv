use crate::compatibility::ProtocolInfo;
use crate::state::{Activity, Failure, OutputState, Phase};
use iv_core::{Mode, Record, RunConfig};
use iv_transport::ResourceInfo;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum Command {
    Heartbeat,
    Discover,
    SetLiveMode {
        mode: Mode,
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
    },
    GateOutputOff,
    ConnectMeter {
        #[serde(default = "default_meter_model")]
        model: String,
        simulation: bool,
        resource: String,
    },
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
    /// Acknowledged explicit safety commands, including cancellation before Run starts.
    #[serde(default)]
    pub safety_revision: u64,
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
    pub phase: Phase,
    pub message: String,
    pub connected: bool,
    pub source_connected: bool,
    pub meter_connected: bool,
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
            safety_revision: 0,
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
            phase: Phase::Idle,
            message: "Choose your instruments or explore Simulation.".into(),
            connected: false,
            source_connected: false,
            meter_connected: false,
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
