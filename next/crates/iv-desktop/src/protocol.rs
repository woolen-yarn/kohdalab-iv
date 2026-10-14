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
    },
    ConnectSource {
        simulation: bool,
        resource: String,
    },
    ConnectMeter {
        simulation: bool,
        resource: String,
    },
    DisconnectSource,
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

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    pub phase: String,
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
    pub output: String,
    pub points: usize,
    pub total: usize,
    pub elapsed: f64,
    pub resistance: Option<f64>,
    pub last: Option<Record>,
    #[serde(default)]
    pub live: Option<Record>,
    #[serde(default)]
    pub live_error: String,
    pub path: String,
    pub error: String,
    pub resources: Vec<ResourceInfo>,
    pub warnings: Vec<String>,
    pub run_id: u64,
}

impl Default for Snapshot {
    fn default() -> Self {
        Self {
            phase: "Idle".into(),
            message: "Choose your instruments or explore Simulation.".into(),
            connected: false,
            source_connected: false,
            meter_connected: false,
            source_simulation: false,
            meter_simulation: false,
            simulation: false,
            source_identity: String::new(),
            meter_identity: String::new(),
            output: "Unknown".into(),
            points: 0,
            total: 0,
            elapsed: 0.0,
            last: None,
            live: None,
            live_error: String::new(),
            path: String::new(),
            source_resource: String::new(),
            meter_resource: String::new(),
            resistance: None,
            error: String::new(),
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
