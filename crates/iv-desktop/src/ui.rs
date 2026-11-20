//! Application state, event synchronization and three-panel layout.
//! Settings, display formatting, drawing history and shared widgets live in
//! private modules so UI work does not require changes to the device engine.

use crate::state::{OutputState, Phase};

mod cards;
mod control;
mod dc;
mod digits;
mod formatting;
mod history;
mod reference_analysis;
mod reference_view;
mod settings;
mod widgets;

use formatting::{AxisScale, display_reading, display_reading_at_scale, fmt_quantity};
use history::{LiveSample, append_live_sample, append_plot_point, extend_plot_extent, plot_series};
use settings::{Settings, save_settings, settings_path};
use widgets::{
    BG, BLUE, BORDER, BUTTON, INPUT, MUTED, ORANGE, PURPLE, RED, TEAL, button, compact_field,
    form_label, number, path_summary, quantity, resource_summary, section, style,
};

use crate::client::Client;
use crate::protocol::{Command, ProtectionAlert, ProtectionKind, Snapshot};
use eframe::egui::{self, Color32, RichText, Stroke, Vec2};
use egui_plot::{
    AxisHints, Legend, Line, LineStyle, MarkerShape, Plot, PlotMemory, PlotPoint, PlotPoints,
    PlotUi, Points, VPlacement,
};
use iv_core::{
    InstrumentRole, Mode, Pattern, RunConfig, instrument_catalog, instrument_descriptor,
    measurement_definition, sweep_definitions,
};
use std::collections::VecDeque;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::pin;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc::{self, Receiver, TryRecvError},
};
use std::task::{Context, Poll, Wake, Waker};
use std::thread;
use std::time::{Duration, Instant};

const SNAPSHOT_FIELDS: &[&str] = iv_core::CSV_FIELDS;

/// Cached, display-only description of the exact plan accepted by `iv_core`.
///
/// `iv_core::plan` can legitimately contain up to one million points. Build
/// this only when the configuration changes, never while rendering a frame.
#[derive(Clone, Debug, PartialEq)]
struct SweepPreview {
    effective_step: f64,
    effective_ramp: f64,
    requested_step: f64,
    planned_points: usize,
    source_range: f64,
    hardware_compliance: f64,
    route: Vec<f64>,
    unit: &'static str,
    custom: bool,
}

/// A single file chooser or export running outside the egui update thread.
///
/// Explorer/portal dialogs and copies on a network share can wait for seconds.
/// Keeping one operation in flight makes its result unambiguous while leaving
/// the measurement engine and Stop button responsive.
enum FileTaskResult {
    Config(Option<PathBuf>),
    OutputDirectory(Option<PathBuf>),
    SaveDestination {
        source: PathBuf,
        destination: Option<PathBuf>,
        points: usize,
    },
    SaveCopy {
        destination: PathBuf,
        result: Result<(), String>,
        points: usize,
    },
}

struct PendingFileTask {
    description: &'static str,
    result: Receiver<FileTaskResult>,
    /// Only a copy worker has created (or may create) measurement files.
    /// Pickers are intentionally still safe to close normally.
    blocks_close: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CloseAction {
    Stop,
    FinishFileCopy,
}

fn close_action(active: bool, file_copy_in_progress: bool) -> Option<CloseAction> {
    if active {
        Some(CloseAction::Stop)
    } else if file_copy_in_progress {
        Some(CloseAction::FinishFileCopy)
    } else {
        None
    }
}

fn file_task_blocks_close(pending: Option<&PendingFileTask>) -> bool {
    pending.is_some_and(|pending| pending.blocks_close)
}

#[derive(Debug, PartialEq, Eq)]
enum SaveCopyCompletion {
    Cancelled,
    Saved(String),
    Failed { log: String, notice: String },
}

fn save_copy_completion(
    destination: &Path,
    result: Result<(), String>,
    points: usize,
) -> SaveCopyCompletion {
    match result {
        Ok(()) => SaveCopyCompletion::Saved(format!(
            "Saved {points} points to {}\nThe companion .log records lifecycle status; .run.json contains settings and .raw.csv retains acquisition evidence.",
            destination.display()
        )),
        Err(error) => SaveCopyCompletion::Failed {
            log: format!("Save failed: {error}"),
            notice: format!(
                "Could not save the measurement copy.\n\n{error}\n\nThe original measurement files are unchanged."
            ),
        },
    }
}

fn selected_save_destination(
    source: PathBuf,
    destination: Option<PathBuf>,
    points: usize,
) -> Result<(PathBuf, PathBuf, usize), SaveCopyCompletion> {
    destination
        .map(|mut destination| {
            if !destination
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("csv"))
            {
                destination.add_extension("csv");
            }
            (source, destination, points)
        })
        .ok_or(SaveCopyCompletion::Cancelled)
}

struct DialogWaker(thread::Thread);

impl Wake for DialogWaker {
    fn wake(self: Arc<Self>) {
        self.0.unpark();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.0.unpark();
    }
}

/// Await rfd's native async sheet from a worker without a runtime dependency.
/// The sheet itself is created on the egui main thread before this runs.
fn wait_for_dialog<T>(future: impl Future<Output = T>) -> T {
    let waker = Waker::from(Arc::new(DialogWaker(thread::current())));
    let mut context = Context::from_waker(&waker);
    let mut future = pin!(future);
    loop {
        match future.as_mut().poll(&mut context) {
            Poll::Ready(value) => return value,
            Poll::Pending => thread::park(),
        }
    }
}

fn labelled_model(label: &str, model: &str) -> String {
    let label = label.trim();
    if label.is_empty() {
        model.to_owned()
    } else {
        format!("{label} · {model}")
    }
}

fn inventory_connection(
    source: bool,
    entry: &settings::InstrumentEntry,
    mode: Mode,
) -> Option<crate::protocol::InventoryConnection> {
    let simulation = entry.model.starts_with("SIMULATED_");
    if !model_supported(&entry.model) || (!simulation && entry.resource.trim().is_empty()) {
        return None;
    }
    Some(crate::protocol::InventoryConnection {
        id: entry.id,
        source,
        simulation,
        model: entry.model.clone(),
        resource: entry.resource.clone(),
        mode: if source { entry.dc.mode } else { mode },
        name: format!(
            "{} [{}]",
            labelled_model(&entry.label, &entry.model),
            entry.id
        ),
    })
}

fn inventory_connect_command(
    source: bool,
    entry: &settings::InstrumentEntry,
    mode: Mode,
) -> Option<Command> {
    let connection = inventory_connection(source, entry, mode)?;
    Some(Command::ConnectInventory {
        id: connection.id,
        source: connection.source,
        assign: false,
        simulation: connection.simulation,
        model: connection.model,
        resource: connection.resource,
        mode: connection.mode,
    })
}

fn dual_uses_meter_axis(
    meter_connected: bool,
    has_source_points: bool,
    has_meter_points: bool,
) -> bool {
    has_meter_points || (meter_connected && !has_source_points)
}

fn inventory_entry_name(role: &str, index: usize, entry: &settings::InstrumentEntry) -> String {
    let summary = labelled_model(&entry.label, device_summary(&entry.model));
    format!("{role} {} · {summary}", index + 1)
}

use cards::{AddDevice, DeleteDevice};

pub struct IvApp {
    plot_context: egui::Context,
    client: Result<Client, String>,
    control: Option<crate::gui_control::GuiControl>,
    settings: Settings,
    state: Snapshot,
    previous_phase: Option<Phase>,
    previous_log: String,
    state_revision: u64,
    run_id: u64,
    /// Lower plot: commanded/read-back source quantity vs the mandatory
    /// opposite-quantity meter.
    samples: Vec<(f64, f64, String, [Option<f64>; 2])>,
    plot_series: [Vec<PlotPoint>; 3],
    plot_extent: [f64; 2],
    /// Upper x axis: optional source-quantity reference DMM vs the mandatory
    /// opposite-quantity meter.  It stays empty for legacy one-meter runs.
    reference_samples: Vec<(f64, f64, String, [Option<f64>; 2])>,
    reference_plot_series: [Vec<PlotPoint>; 3],
    reference_plot_extent: [f64; 2],
    separate: reference_view::SeparateHistory,
    gate_summaries: Vec<reference_analysis::GateSummary>,
    gate_summary_revision: u64,
    gate_metric: reference_view::GateMetric,
    reference_replay: bool,
    separate_smoke_first: Option<[usize; 2]>,
    live_samples: VecDeque<LiveSample>,
    reference_live_samples: VecDeque<[f64; 2]>,
    idle_samples: VecDeque<[f64; 2]>,
    inventory_samples: std::collections::BTreeMap<u64, VecDeque<[f64; 2]>>,
    pending_inventory_selection: Option<(String, bool, u64)>,
    pending_bindings: Option<(String, Settings)>,
    device_change_notice: String,
    source2_samples: VecDeque<LiveSample>,
    live_reading: Option<iv_core::Record>,
    last_live_timestamp: String,
    table_values: Vec<String>,
    table_run_id: u64,
    table_points: usize,
    details: bool,
    left_visible: bool,
    table_visible: bool,
    add_device: Option<AddDevice>,
    delete_device: Option<DeleteDevice>,
    /// Newly added card to bring into view once it has been laid out.
    scroll_to_card: Option<u64>,
    /// Card and control rectangles from the latest frame (smoke geometry).
    card_geometry: std::collections::BTreeMap<String, egui::Rect>,
    /// Smoke only: keep Device and Settings open on every card.
    open_all_cards: bool,
    timing_open: bool,
    timing_before: Option<(Settings, String)>,
    config_path: String,
    notice: String,
    start_error: Option<String>,
    protection_dialog: Option<ProtectionAlert>,
    save_notice: Option<String>,
    pending_file_task: Option<PendingFileTask>,
    logs: Vec<String>,
    started: Instant,
    phase_start: Instant,
    smoke: bool,
    smoke_started: bool,
    footer_smoke_offset: Option<f32>,
    footer_geometry: [f32; 3],
    screenshot: Option<PathBuf>,
    screenshot_requested: bool,
    screenshot_saved: bool,
    closing: bool,
    custom_text: String,
    validated_config: RunConfig,
    preview: Result<SweepPreview, String>,
    success: Arc<AtomicBool>,
    boot: Instant,
    first_frame_s: Option<f64>,
    ui_cpu_s: f64,
    ui_frames: usize,
    pending_run: bool,
    pending_base: u64,
    pending_safety: u64,
    dc_open: std::collections::HashSet<u64>,
    dc_preview_mode: Option<Mode>,
    pending_dc: Option<(String, u64)>,
    pending_inventory_dc: std::collections::BTreeMap<u64, (String, u64)>,
    dc_smoke_stage: u8,
}

impl IvApp {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        smoke: bool,
        screenshot: Option<PathBuf>,
        success: Arc<AtomicBool>,
        boot: Instant,
        hardware: Option<(String, String)>,
        startup_profile: Option<PathBuf>,
    ) -> Result<Self, String> {
        if std::env::args().any(|arg| {
            matches!(
                arg.as_str(),
                "--gui-dc-smoke"
                    | "--gui-inventory-smoke"
                    | "--gui-current-source-smoke"
                    | "--gui-separate-smoke"
            )
        }) && (hardware.is_some() || startup_profile.is_some())
        {
            return Err("GUI output-control smoke requires its isolated simulation profile; hardware/config overrides are not allowed.".into());
        }
        style(&cc.egui_ctx);
        let ctx = cc.egui_ctx.clone();
        let client = Client::start(move || ctx.request_repaint());
        let (mut settings, mut settings_notice) = if smoke || startup_profile.is_some() {
            (Settings::default(), String::new())
        } else {
            match settings::load_startup_settings() {
                Ok(loaded) => loaded,
                Err(error) => (
                    Settings::default(),
                    format!(
                        "Configuration could not be loaded or created: {error}\nExisting files were not replaced. Check the Config path and folder permissions."
                    ),
                ),
            }
        };
        let startup_error = (!settings_notice.is_empty()).then(|| settings_notice.clone());
        if smoke {
            settings = Settings {
                simulation: true,
                ..Default::default()
            };
            settings.config.start_settle_s = 0.03;
            settings.config.settle_s = 0.02;
            if hardware.is_none() && std::env::args().any(|arg| arg == "--gui-settings-smoke") {
                settings.config.meter_current_range = Some(0.001);
                settings.config.meter_voltage_range = Some(1.0);
            }
            if std::env::args()
                .any(|arg| arg == "--gui-balanced-smoke" || arg == "--gui-control-smoke")
            {
                settings.config.balanced_source = Some(iv_core::BalancedSourceConfig::default());
                settings.config.source_compliance = Some(0.001);
                settings.source2_model = "SIMULATED_SOURCE".into();
                // Dual V-I needs a second device; fresh profiles have one.
                let id = settings.add_source("");
                settings.source2_id = id;
                settings.sources.last_mut().expect("Source 2 added").model =
                    "SIMULATED_SOURCE".into();
            }
            if std::env::args().any(|arg| arg == "--gui-limit-smoke") {
                settings.config.compliance = 0.5e-6;
                settings.config.source_compliance = Some(0.001);
            }
            settings.output_dir = std::env::temp_dir()
                .join("kohdalab-iv-smoke")
                .display()
                .to_string();
            if let Some((source, meter)) = hardware {
                settings.simulation = false;
                settings.source = source;
                settings.meter = meter;
                settings.config = crate::open_circuit_config();
                crate::validate_open_circuit(&settings.config)?;
            }
        }
        if smoke
            && std::env::args()
                .any(|arg| arg == "--gui-current-source-smoke" || arg == "--gui-separate-smoke")
        {
            settings.config = measurement_definition(Mode::Vi).default_config();
            settings.config.start = -0.0005;
            settings.config.stop = 0.0005;
            settings.config.step = 0.0001;
            settings.config.source_limit = 0.001;
            settings.config.compliance = 3.0;
            settings.config.ramp_step = 0.0001;
            settings.config.start_settle_s = 0.03;
            settings.config.settle_s = 0.02;
            settings.config.pattern = Pattern::Linear;
        }
        settings.migrate_inventory();
        if settings.simulation {
            settings.source_model = "SIMULATED_SOURCE".into();
            settings.meter_model = "SIMULATED_METER".into();
            if let Some(id) = settings.reference_meter_id
                && let Some(entry) = settings.meters.iter_mut().find(|entry| entry.id == id)
            {
                entry.model = "SIMULATED_METER".into();
            }
            if settings.config.balanced_source.is_some() {
                settings.source2_model = "SIMULATED_SOURCE".into();
            }
            // Legacy simulation settings have only scalar device fields.
            settings.store_active_entries();
        }
        let footer_smoke_offset =
            (smoke && std::env::args().any(|arg| arg == "--gui-footer-smoke")).then(|| {
                if std::env::args().any(|arg| arg == "--footer-scrolled") {
                    430.0
                } else {
                    0.0
                }
            });
        // `--footer-cards`: the default Source and Meter plus one added Source
        // and Meter, all with Device and Settings open.
        let open_all_cards =
            footer_smoke_offset.is_some() && std::env::args().any(|arg| arg == "--footer-cards");
        if footer_smoke_offset.is_some() {
            settings.two_card_columns = std::env::args().any(|arg| arg == "--footer-two-columns");
            let added = if open_all_cards { 2..=2 } else { 2..=7 };
            for index in added.clone() {
                settings.add_source(&format!("Demo source {index}"));
            }
            for index in added {
                settings.add_meter(&format!("Demo meter {index}"));
            }
            for source in &mut settings.sources {
                source.model = "SIMULATED_SOURCE".into();
            }
            for meter in &mut settings.meters {
                meter.model = "SIMULATED_METER".into();
            }
        }
        let column_smoke = smoke && std::env::args().any(|arg| arg == "--gui-two-column-smoke");
        if column_smoke {
            settings.two_card_columns = true;
            for label in ["Bias 2", "Gate"] {
                settings.add_source(label);
                settings.sources.last_mut().expect("source added").model =
                    "SIMULATED_SOURCE".into();
            }
        }
        if smoke
            && (column_smoke
                || std::env::args()
                    .any(|arg| arg == "--gui-reference-smoke" || arg == "--gui-separate-smoke"))
        {
            // Give the source-quantity observer its own simulated physical
            // device.  This exercises the optional DMM data path rather than
            // merely drawing an assigned-but-empty Meter axis.
            settings.add_meter("");
            let reference = settings.meters.last_mut().expect("meter added");
            reference.model = "SIMULATED_METER".into();
            reference.resource = "SIMULATED_REFERENCE_METER".into();
            settings.reference_meter_id = Some(reference.id);
            settings.bind_active_entries();
        }
        if smoke && std::env::args().any(|arg| arg == "--gui-separate-smoke") {
            settings.reference_display = settings::ReferenceDisplay::Separate;
            settings.reference_purpose = settings::ReferencePurpose::Gate;
            settings
                .meters
                .iter_mut()
                .find(|m| m.id == settings.meter_id)
                .expect("primary meter")
                .label = "Channel".into();
            settings
                .meters
                .iter_mut()
                .find(|m| Some(m.id) == settings.reference_meter_id)
                .expect("reference meter")
                .label = "Gate".into();
        }
        let dc_smoke = smoke && std::env::args().any(|arg| arg == "--gui-dc-smoke");
        if dc_smoke {
            // The fixed bias is an explicitly added second source.
            settings.dc_gate_id = settings.add_source("Gate");
            settings.sources[0].dc.value = 0.001;
            let gate = settings.sources.last_mut().expect("gate source added");
            gate.model = "SIMULATED_SOURCE".into();
            gate.resource = "SIM_GATE".into();
            gate.dc.value = 0.01;
        }
        if smoke && std::env::args().any(|arg| arg == "--gui-inventory-smoke") {
            settings.config.start_settle_s = 3.0;
            settings.add_source("Spare Source");
            let spare = settings.sources.last_mut().expect("spare source added");
            spare.model = "SIMULATED_SOURCE".into();
            spare.dc.value = 0.002;
            spare.dc.compliance = 0.2;
        }
        let mut config_path = settings_path().display().to_string();
        if let Some(path) = startup_profile {
            settings =
                load_gui_profile(&std::fs::read(&path).map_err(|e| format!("GUI profile: {e}"))?)?;
            config_path = path.display().to_string();
        }
        // Keep an existing source-protection request paired with its mode.
        // Fresh defaults deliberately remain `None` until the user edits
        // a protection field in Settings, retaining the legacy hardware-follows-meter-stop policy.
        settings.store_active_protection();
        settings.activate_mode_protection();
        let preview = sweep_preview(&settings.config);
        let validated_config = settings.config.clone();
        let custom_text = settings
            .config
            .custom_points
            .iter()
            .map(|v| v.to_string())
            .collect::<Vec<_>>()
            .join(", ");
        let control = match crate::gui_control::GuiControl::start() {
            Ok(control) => Some(control),
            Err(error) => {
                if std::env::args().any(|arg| arg == "--gui-api") {
                    return Err(format!("GUI API unavailable: {error}"));
                }
                let message = format!("GUI external control unavailable: {error}");
                eprintln!("{message}");
                if !settings_notice.is_empty() {
                    settings_notice.push_str(" · ");
                }
                settings_notice.push_str(&message);
                None
            }
        };
        let dc_open = if dc_smoke {
            [settings.source1_id, settings.dc_gate_id]
                .into_iter()
                .collect()
        } else {
            Default::default()
        };
        let mut app = Self {
            plot_context: cc.egui_ctx.clone(),
            client,
            control,
            settings,
            state: Snapshot::default(),
            previous_phase: None,
            previous_log: String::new(),
            state_revision: 0,
            run_id: 0,
            samples: vec![],
            plot_series: std::array::from_fn(|_| Vec::new()),
            plot_extent: [0.0; 2],
            reference_samples: vec![],
            reference_plot_series: std::array::from_fn(|_| Vec::new()),
            reference_plot_extent: [0.0; 2],
            separate: Default::default(),
            gate_summaries: Vec::new(),
            gate_summary_revision: 0,
            gate_metric: reference_view::GateMetric::Sweep,
            reference_replay: false,
            separate_smoke_first: None,
            live_samples: VecDeque::new(),
            reference_live_samples: VecDeque::new(),
            idle_samples: VecDeque::new(),
            inventory_samples: Default::default(),
            pending_inventory_selection: None,
            pending_bindings: None,
            device_change_notice: String::new(),
            source2_samples: VecDeque::new(),
            live_reading: None,
            last_live_timestamp: String::new(),
            table_values: vec![],
            table_run_id: u64::MAX,
            table_points: usize::MAX,
            details: true,
            left_visible: true,
            table_visible: true,
            add_device: None,
            delete_device: None,
            scroll_to_card: None,
            card_geometry: Default::default(),
            open_all_cards,
            timing_open: false,
            timing_before: None,
            config_path,
            notice: settings_notice,
            start_error: startup_error,
            protection_dialog: None,
            save_notice: None,
            pending_file_task: None,
            logs: vec![],
            started: Instant::now(),
            phase_start: Instant::now(),
            smoke,
            smoke_started: false,
            footer_smoke_offset,
            footer_geometry: [0.0; 3],
            screenshot,
            screenshot_requested: false,
            screenshot_saved: false,
            closing: false,
            custom_text,
            validated_config,
            preview,
            success,
            boot,
            first_frame_s: None,
            ui_cpu_s: 0.0,
            ui_frames: 0,
            pending_run: false,
            pending_base: 0,
            pending_safety: 0,
            dc_open,
            dc_preview_mode: None,
            pending_dc: None,
            pending_inventory_dc: Default::default(),
            dc_smoke_stage: 0,
        };
        if let Some(path) = crate::argument(
            &std::env::args().collect::<Vec<_>>(),
            "--gui-reference-replay",
        ) {
            if !smoke || !std::env::args().any(|arg| arg == "--gui-separate-smoke") {
                return Err("Read-only reference replay requires --gui-separate-smoke".into());
            }
            app.load_reference_replay(std::path::Path::new(&path))?;
        }
        Ok(app)
    }

    fn send(&mut self, command: Command) -> bool {
        if self.reference_replay {
            return false;
        }
        let result = self
            .client
            .as_ref()
            .map_err(Clone::clone)
            .and_then(|c| c.send(command));
        if let Err(error) = result {
            self.report(error);
            false
        } else {
            true
        }
    }

    fn busy(&self) -> bool {
        self.pending_dc.is_some()
            || self.pending_inventory_selection.is_some()
            || self.pending_bindings.is_some()
            || matches!(
                self.state.phase,
                Phase::Connecting
                    | Phase::Scanning
                    | Phase::SwitchingMode
                    | Phase::Preparing
                    | Phase::Running
                    | Phase::CleaningUp
            )
    }

    fn active(&self) -> bool {
        self.pending_dc.is_some()
            || matches!(
                self.state.phase,
                Phase::Preparing | Phase::Running | Phase::CleaningUp
            )
    }

    fn sync(&mut self) {
        self.settings.config.source_model = if self.settings.source_model == "SIMULATED_SOURCE" {
            "YOKOGAWA_GS210".into()
        } else {
            self.settings.source_model.clone()
        };
        self.settings.config.meter_model = if self.settings.meter_model == "SIMULATED_METER" {
            "AGILENT_34411A".into()
        } else {
            self.settings.meter_model.clone()
        };
        if let Some(dual) = &mut self.settings.config.balanced_source {
            dual.source_model = if self.settings.source2_model == "SIMULATED_SOURCE" {
                "YOKOGAWA_GS210".into()
            } else {
                self.settings.source2_model.clone()
            };
        }
        let config_changed = self.settings.config != self.validated_config;
        if config_changed {
            self.preview = sweep_preview(&self.settings.config);
            self.validated_config = self.settings.config.clone();
        }
        let mut state_changed = false;
        if let Ok(client) = &self.client {
            let Some(next) = client.snapshot_since(&mut self.state_revision) else {
                self.refresh_table_values(config_changed, false);
                return;
            };
            state_changed = true;
            dc::settle_job(&mut self.pending_dc, &mut self.start_error, &next);
            self.pending_inventory_dc.retain(|device, (id, safety)| {
                next.safety_revision == *safety
                    && next
                        .inventory_devices
                        .iter()
                        .any(|entry| entry.id == *device && entry.dc_job.as_ref() != Some(id))
                    && !next.gui_jobs.iter().any(|job| &job.id == id)
            });
            if next.balanced_active != self.state.balanced_active {
                self.live_samples.clear();
                self.source2_samples.clear();
                self.last_live_timestamp.clear();
            }
            match (&self.state.idle_reading, &next.idle_reading) {
                (_, None) => self.idle_samples.clear(),
                (previous, Some(reading)) => {
                    if previous
                        .as_ref()
                        .is_none_or(|old| old.mode != reading.mode || old.source != reading.source)
                    {
                        self.idle_samples.clear();
                    }
                    if previous
                        .as_ref()
                        .is_none_or(|old| old.timestamp != reading.timestamp)
                    {
                        let time = self.boot.elapsed().as_secs_f64();
                        self.idle_samples.push_back([time, reading.value]);
                        while self.idle_samples.len() > 1024
                            || self
                                .idle_samples
                                .front()
                                .is_some_and(|sample| time - sample[0] > 30.0)
                        {
                            self.idle_samples.pop_front();
                        }
                    }
                }
            }
            if next.source_connected != self.state.source_connected
                || next.meter_connected != self.state.meter_connected
            {
                self.live_samples.clear();
                self.live_reading = None;
            }
            if next.reference_meter_connected != self.state.reference_meter_connected
                || next.reference_meter_resource != self.state.reference_meter_resource
                || next
                    .live
                    .as_ref()
                    .zip(self.live_reading.as_ref())
                    .is_some_and(|(new, old)| new.mode != old.mode)
            {
                self.reference_live_samples.clear();
            }
            if let Some(record) = &next.live
                && self.last_live_timestamp != record.timestamp
            {
                if self
                    .live_reading
                    .as_ref()
                    .is_some_and(|old| old.mode != record.mode)
                {
                    self.live_samples.clear();
                }
                let mut sample = record.clone();
                sample.elapsed_s = self.boot.elapsed().as_secs_f64();
                append_live_sample(&mut self.live_samples, &sample);
                if let Some(value) = record.reference_meter_value {
                    self.reference_live_samples
                        .push_back([sample.elapsed_s, value]);
                    if self.reference_live_samples.len() > 1024 {
                        self.reference_live_samples.pop_front();
                    }
                }
                if let Some(level) = next.source2_level {
                    let mut secondary = sample.clone();
                    secondary.source_set = level;
                    secondary.source_readback = Some(level);
                    secondary.mode = Mode::Iv;
                    append_live_sample(&mut self.source2_samples, &secondary);
                }
                self.last_live_timestamp.clone_from(&record.timestamp);
                self.live_reading = Some(record.clone());
            }
            let new_run = next.run_id > self.run_id;
            if new_run {
                self.run_id = next.run_id;
                self.samples.clear();
                self.plot_series.iter_mut().for_each(Vec::clear);
                self.plot_extent = [0.0; 2];
                self.reference_samples.clear();
                self.separate = Default::default();
                self.gate_metric = reference_view::GateMetric::Sweep;
                self.reference_plot_series.iter_mut().for_each(Vec::clear);
                self.reference_plot_extent = [0.0; 2];
                self.notice.clear();
            }
            let new_points = client.drain_points();
            self.sync_inventory(&next);
            for (record_run, point) in new_points {
                if record_run < self.run_id {
                    continue;
                }
                if record_run > self.run_id {
                    self.samples.clear();
                    self.plot_series.iter_mut().for_each(Vec::clear);
                    self.plot_extent = [0.0; 2];
                    self.reference_samples.clear();
                    self.separate = Default::default();
                    self.gate_metric = reference_view::GateMetric::Sweep;
                    self.reference_plot_series.iter_mut().for_each(Vec::clear);
                    self.reference_plot_extent = [0.0; 2];
                    self.run_id = record_run;
                }
                self.separate.push(&point);
                let source = point.source_readback.unwrap_or(point.source_set);
                let xy = (source, point.meter_value);
                extend_plot_extent(&mut self.plot_extent, xy.0, xy.1);
                append_plot_point(
                    &mut self.plot_series,
                    self.samples.last().map(|sample| sample.2.as_str()),
                    xy.0,
                    xy.1,
                    &point.point.direction,
                );
                self.samples.push((
                    xy.0,
                    xy.1,
                    point.point.direction.clone(),
                    [point.source_resolution, point.meter_resolution],
                ));
                if let Some(reference) = point.reference_meter_value {
                    extend_plot_extent(
                        &mut self.reference_plot_extent,
                        reference,
                        point.meter_value,
                    );
                    append_plot_point(
                        &mut self.reference_plot_series,
                        self.reference_samples
                            .last()
                            .map(|sample| sample.2.as_str()),
                        reference,
                        point.meter_value,
                        &point.point.direction,
                    );
                    self.reference_samples.push((
                        reference,
                        point.meter_value,
                        point.point.direction.clone(),
                        [point.reference_meter_resolution, point.meter_resolution],
                    ));
                }
            }
            if self.samples.len() > 50000 {
                let mut reduced = Vec::with_capacity(16000);
                for chunk in self.samples.chunks(8) {
                    let low = chunk
                        .iter()
                        .enumerate()
                        .min_by(|a, b| a.1.1.total_cmp(&b.1.1))
                        .map(|x| x.0)
                        .unwrap_or(0);
                    let high = chunk
                        .iter()
                        .enumerate()
                        .max_by(|a, b| a.1.1.total_cmp(&b.1.1))
                        .map(|x| x.0)
                        .unwrap_or(0);
                    for index in 0..chunk.len() {
                        if index == low
                            || index == high
                            || (index > 0 && chunk[index].2 != chunk[index - 1].2)
                        {
                            reduced.push(chunk[index].clone());
                        }
                    }
                }
                if reduced.len() >= self.samples.len() {
                    reduced = self.samples.iter().step_by(2).cloned().collect();
                }
                self.samples = reduced;
                self.rebuild_plot_series();
            }
            if self.reference_samples.len() > 50000 {
                self.reference_samples =
                    self.reference_samples.iter().step_by(2).cloned().collect();
                self.rebuild_reference_plot_series();
            }
            if Some(next.phase) != self.previous_phase {
                self.phase_start = Instant::now();
                self.previous_phase = Some(next.phase);
            }
            if !next.error.is_empty()
                && next.error != self.state.error
                && (self.pending_run
                    || self.active()
                    || self.state.source_dc.is_some()
                    || self.state.source2_dc.is_some()
                    || self.state.gate_output == OutputState::On)
            {
                self.start_error = Some(format!(
                    "{}\n\nCheck the log and instrument connections. Verify output is off before trying again.",
                    next.error
                ));
            }
            if next.protection_alert.is_some()
                && (next.protection_alert != self.state.protection_alert
                    || next.run_id != self.state.run_id)
            {
                self.start_error = None;
                self.protection_dialog = next.protection_alert.clone();
            }
            let log = if next.error.is_empty() {
                next.message.clone()
            } else {
                format!("{}\n{}", next.message, next.error)
            };
            if log != self.previous_log {
                self.previous_log = log.clone();
                self.logs.push(log);
                if self.logs.len() > 200 {
                    self.logs.remove(0);
                }
            }
            if self.pending_run
                && next.run_id <= self.pending_base
                && next.phase != Phase::Failed
                && next.safety_revision <= self.pending_safety
            {
                self.state.phase = Phase::Preparing;
                self.state.message = "Sending the validated measurement plan…".into();
            } else if next.run_id >= self.run_id {
                self.pending_run = false;
                self.state = next;
            }
        }
        self.refresh_table_values(config_changed, state_changed);
    }

    fn rebuild_plot_series(&mut self) {
        self.plot_series = plot_series(&self.samples);
        self.plot_extent = [0.0; 2];
        for (x, y, _, _) in &self.samples {
            extend_plot_extent(&mut self.plot_extent, *x, *y);
        }
    }

    fn rebuild_reference_plot_series(&mut self) {
        self.reference_plot_series = plot_series(&self.reference_samples);
        self.reference_plot_extent = [0.0; 2];
        for (x, y, _, _) in &self.reference_samples {
            extend_plot_extent(&mut self.reference_plot_extent, *x, *y);
        }
    }

    fn refresh_table_values(&mut self, config_changed: bool, state_changed: bool) {
        if !config_changed
            && (!state_changed
                || (self.table_run_id == self.state.run_id
                    && self.table_points == self.state.points))
        {
            return;
        }
        self.table_values = self
            .state
            .last
            .as_ref()
            .map(|record| {
                let mut values = Vec::from(record.native_csv_values());
                if let Some(levels) = record.dual_source_levels {
                    values.extend(levels.csv_values());
                }
                values
            })
            .unwrap_or_default();
        self.table_run_id = self.state.run_id;
        self.table_points = self.state.points;
    }

    fn file_task_pending(&self) -> bool {
        self.pending_file_task.is_some()
    }

    fn start_file_task(
        &mut self,
        ctx: &egui::Context,
        description: &'static str,
        blocks_close: bool,
        task: impl FnOnce() -> FileTaskResult + Send + 'static,
    ) {
        debug_assert!(self.pending_file_task.is_none());
        let (send, receive) = mpsc::sync_channel(1);
        let repaint = ctx.clone();
        thread::spawn(move || {
            let outcome = task();
            let _ = send.send(outcome);
            repaint.request_repaint();
        });
        self.pending_file_task = Some(PendingFileTask {
            description,
            result: receive,
            blocks_close,
        });
        self.report(format!("{description}…"));
    }

    fn file_copy_in_progress(&self) -> bool {
        file_task_blocks_close(self.pending_file_task.as_ref())
    }

    fn poll_file_task(&mut self, ctx: &egui::Context) {
        let Some(pending) = self.pending_file_task.as_ref() else {
            return;
        };
        let description = pending.description;
        let outcome = match pending.result.try_recv() {
            Ok(outcome) => outcome,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => {
                self.pending_file_task = None;
                self.report(format!("{description} did not complete."));
                return;
            }
        };
        self.pending_file_task = None;
        match outcome {
            FileTaskResult::Config(Some(path)) => {
                if self.busy() || self.python_locked() {
                    self.report("Configuration selection ignored because the application is busy or controlled externally.".into());
                } else {
                    self.config_path = path.display().to_string();
                    self.report("Configuration file selected.".into());
                }
            }
            FileTaskResult::OutputDirectory(Some(path)) => {
                if self.busy() || self.python_locked() {
                    self.report("Output-directory selection ignored because the application is busy or controlled externally.".into());
                } else {
                    self.settings.output_dir = path.display().to_string();
                    self.report("Output directory selected.".into());
                }
            }
            FileTaskResult::SaveDestination {
                source,
                destination,
                points,
            } => match selected_save_destination(source, destination, points) {
                Ok((source, destination, points)) => {
                    self.start_save_copy(ctx, source, destination, points)
                }
                Err(SaveCopyCompletion::Cancelled) => {
                    self.report("Save measurement copy cancelled.".into())
                }
                Err(_) => unreachable!("only cancellation is possible before copying"),
            },
            FileTaskResult::SaveCopy {
                destination,
                result,
                points,
            } => match save_copy_completion(&destination, result, points) {
                SaveCopyCompletion::Cancelled => {
                    self.report("Save measurement copy cancelled.".into())
                }
                SaveCopyCompletion::Saved(message) => {
                    self.report(message.clone());
                    self.save_notice = Some(message);
                }
                SaveCopyCompletion::Failed { log, notice } => {
                    self.report(log);
                    self.save_notice = Some(notice);
                }
            },
            FileTaskResult::Config(None) | FileTaskResult::OutputDirectory(None) => {
                self.report("Selection cancelled.".into());
            }
        }
    }

    fn choose_config(&mut self, ctx: &egui::Context) {
        let initial = PathBuf::from(&self.config_path)
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));
        // Start the native sheet on the main thread, then await it on a
        // worker. rfd's synchronous macOS picker calls run_modal on the
        // main thread even when invoked from a worker.
        let dialog = rfd::AsyncFileDialog::new()
            .set_title("Configuration file")
            .add_filter("JSON", &["json"])
            .set_directory(initial)
            .pick_file();
        self.start_file_task(ctx, "Configuration picker", false, move || {
            FileTaskResult::Config(wait_for_dialog(dialog).map(|file| file.path().to_path_buf()))
        });
    }

    fn choose_output_directory(&mut self, ctx: &egui::Context) {
        let initial = PathBuf::from(&self.settings.output_dir);
        let dialog = rfd::AsyncFileDialog::new()
            .set_title("Output directory")
            .set_directory(initial)
            .pick_folder();
        self.start_file_task(ctx, "Output-directory picker", false, move || {
            FileTaskResult::OutputDirectory(
                wait_for_dialog(dialog).map(|folder| folder.path().to_path_buf()),
            )
        });
    }

    fn start_save_copy(
        &mut self,
        ctx: &egui::Context,
        source: PathBuf,
        destination: PathBuf,
        points: usize,
    ) {
        // Register the copy as close-blocking before its worker starts. This
        // leaves no interval where a selected destination can be written as
        // the process exits.
        self.start_file_task(ctx, "Saving measurement copy", true, move || {
            let result = crate::run_export::save_run_copy(&source, &destination);
            FileTaskResult::SaveCopy {
                destination,
                result,
                points,
            }
        });
    }

    fn start_run(&mut self) {
        self.start_run_impl(true);
    }

    fn start_run_impl(&mut self, save_preferences: bool) {
        if self.pending_run || self.file_task_pending() {
            return;
        }
        if !self.state.inventory_outputs_managed() {
            let reason = crate::protocol::UNASSIGNED_OUTPUT_BLOCK.to_owned();
            self.report(reason.clone());
            self.start_error = Some(reason);
            return;
        }
        if let Some(reason) = start_block_reason(
            &self.settings.config,
            self.state.connected
                && (self.settings.config.balanced_source.is_none() || self.state.source2_connected),
            self.state.source_simulation == self.state.meter_simulation
                && (self.settings.config.balanced_source.is_none()
                    || self.state.source_simulation == self.state.source2_simulation),
        ) {
            self.report(reason.clone());
            self.start_error = Some(reason);
            return;
        }
        let basename = match control::api_filename(&self.settings.filename) {
            Ok(basename) => basename,
            Err(error) => {
                self.report(error.clone());
                self.start_error = Some(error);
                return;
            }
        };
        let directory = PathBuf::from(&self.settings.output_dir);
        let stamp = time::OffsetDateTime::now_utc().unix_timestamp_nanos();
        let path = directory.join(if self.settings.auto_suffix {
            format!("{basename}_{stamp}.csv")
        } else {
            format!("{basename}.csv")
        });
        if !self.smoke && save_preferences {
            self.settings.store_active_entries();
            self.settings.simulation = self.both_simulated();
            if let Err(error) = save_settings(&self.settings, Path::new(&self.config_path)) {
                self.report(format!("Settings were not saved: {error}"));
            }
        }
        if !self.send(Command::Run {
            config: self.settings.config.clone(),
            path: path.display().to_string(),
        }) {
            self.start_error = Some("The measurement request could not be sent. Check the log and reconnect the instruments before trying again.".into());
            return;
        }
        self.protection_dialog = None;
        self.start_error = None;
        self.state.error.clear();
        self.pending_run = true;
        self.pending_base = self.state.run_id;
        self.pending_safety = self.state.safety_revision;
        self.state.phase = Phase::Preparing;
    }

    fn both_simulated(&self) -> bool {
        self.settings.source_model == "SIMULATED_SOURCE"
            && self.settings.meter_model == "SIMULATED_METER"
            && (self.settings.config.balanced_source.is_none()
                || self.settings.source2_model == "SIMULATED_SOURCE")
    }

    fn report(&mut self, message: String) {
        self.notice = message.clone();
        self.logs.push(message);
        if self.logs.len() > 200 {
            self.logs.remove(0);
        }
    }

    fn sidebar(&mut self, root: &mut egui::Ui) {
        let wide = root.ctx().content_rect().width() >= 1380.0;
        let two_columns = self.settings.two_card_columns && wide;
        let width = if !self.left_visible {
            40.0
        } else if two_columns {
            588.0
        } else {
            384.0
        };
        egui::Panel::left("setup")
            .exact_size(width)
            .resizable(false)
            .frame(egui::Frame::new().fill(BG).inner_margin(8))
            .show_inside(root, |ui| {
                let rect = ui.available_rect_before_wrap();
                let toggle = egui::Rect::from_min_max(
                    egui::pos2(rect.right() - 24.0, rect.top()),
                    egui::pos2(rect.right(), rect.top() + 28.0),
                );
                if self.left_visible {
                    let content = egui::Rect::from_min_max(
                        rect.left_top(),
                        egui::pos2(toggle.left() - 4.0, rect.bottom()),
                    );
                    ui.scope_builder(
                        egui::UiBuilder::new()
                            .max_rect(content)
                            .layout(egui::Layout::top_down(egui::Align::Min)),
                        |ui| {
                            ui.set_clip_rect(content);
                            if self.python_locked() {
                                ui.disable();
                            }
                            self.profile(ui);
                            ui.add_space(4.0);
                            ui.add_enabled(
                                wide,
                                egui::Checkbox::new(
                                    &mut self.settings.two_card_columns,
                                    "2 columns",
                                ),
                            )
                            .on_disabled_hover_text(
                                "Widen the IV window to use two device columns.",
                            );
                            let card_height = (ui.available_height() - 32.0).max(0.0);
                            let mut scroll = egui::ScrollArea::vertical()
                                .id_salt("settings_scroll")
                                .max_height(card_height)
                                .auto_shrink([false, false]);
                            if let Some(offset) = self.footer_smoke_offset {
                                scroll = scroll.vertical_scroll_offset(offset);
                            }
                            let offset = ui
                                .scope(|ui| {
                                    // Reserve the scrollbar gutter before calculating column widths.
                                    ui.spacing_mut().scroll = egui::style::ScrollStyle::solid();
                                    scroll
                                        .scroll_bar_visibility(
                                            egui::scroll_area::ScrollBarVisibility::AlwaysVisible,
                                        )
                                        .show(ui, |ui| self.device_cards(ui, two_columns))
                                        .state
                                        .offset
                                        .y
                                })
                                .inner;
                            ui.add_space(4.0);
                            let footer = self.device_add_footer(ui);
                            if self.footer_smoke_offset.is_some() {
                                self.footer_geometry = [offset, footer.top(), footer.bottom()];
                            }
                        },
                    );
                }
                if ui
                    .put(
                        toggle,
                        egui::Button::new(if self.left_visible { "<" } else { ">" }),
                    )
                    .clicked()
                {
                    self.left_visible = !self.left_visible;
                }
            });
    }

    fn profile(&mut self, ui: &mut egui::Ui) {
        section(ui, "Config", |ui| {
            let enabled = !self.busy() && !self.file_task_pending();
            ui.add_enabled_ui(enabled, |ui| {
                ui.horizontal(|ui| {
                    let field_width = (ui.available_width() - 70.0 - ui.spacing().item_spacing.x).max(40.0);
                    let summary = path_summary(&self.config_path);
                    compact_field(
                        ui,
                        "config_path",
                        &mut self.config_path,
                        field_width,
                        summary,
                    );
                    if button(ui, "Browse", 70.0, true).clicked() {
                        self.choose_config(ui.ctx());
                    }
                });
                ui.add_space(6.0);
                let row_width = (ui.available_width() - ui.spacing().item_spacing.x) / 2.0;
                ui.horizontal(|ui| {
                    if button(
                        ui,
                        "Load",
                        row_width,
                        settings::profile_load_allowed(&self.state),
                    )
                    .on_disabled_hover_text("Disconnect all instruments, including the fixed-bias source, before loading another profile.")
                    .clicked()
                    {
                        let loaded = std::fs::read(&self.config_path)
                            .map_err(|e| e.to_string())
                            .and_then(|bytes| settings::load_settings(&bytes));
                        match loaded {
                            Ok(mut settings) => {
                                if settings.simulation {
                                    settings.source_model = "SIMULATED_SOURCE".into();
                                    settings.meter_model = "SIMULATED_METER".into();
            if let Some(id)=settings.reference_meter_id && let Some(entry)=settings.meters.iter_mut().find(|entry|entry.id==id){entry.model="SIMULATED_METER".into();}
                                }
                                settings.migrate_inventory();
                                self.settings = settings;
                                self.custom_text = self
                                    .settings
                                    .config
                                    .custom_points
                                    .iter()
                                    .map(|v| v.to_string())
                                    .collect::<Vec<_>>()
                                    .join(", ");
                                self.report(format!("Loaded config: {}", self.config_path));
                            }
                            Err(error) => self.report(format!("Load config: {error}")),
                        }
                    }
                    if button(ui, "Save", row_width, true).clicked() {
                        self.settings.store_active_entries();
                        self.settings.simulation = self.both_simulated();
                        let result = save_settings(&self.settings, Path::new(&self.config_path));
                        self.report(match result {
                            Ok(()) => format!("Saved config: {}", self.config_path),
                            Err(e) => format!("Save config: {e}"),
                        });
                    }
                });
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if button(ui, "All Connect", row_width, self.inventory_editable()).clicked() {
                        self.connect_all();
                    }
                    if button(
                        ui,
                        "All Disconnect",
                        row_width,
                        self.state.source_connected
                            || self.state.source2_connected
                            || self.state.meter_connected
                            || self.state.reference_meter_connected
                            || self.state.gate_connected
                            || !self.state.inventory_devices.is_empty(),
                    )
                    .clicked()
                    {
                        self.send(Command::Disconnect);
                    }
                });
            });
        });
    }

    fn sync_inventory(&mut self, next: &Snapshot) {
        if let Some((job, _)) = &self.pending_bindings
            && let Some(completion) = next
                .gui_jobs
                .iter()
                .find(|completion| completion.id == *job)
        {
            let (_, settings) = self.pending_bindings.take().expect("binding job pending");
            if completion.snapshot["inventory_error"]
                .as_str()
                .unwrap_or_default()
                .is_empty()
            {
                let previous = std::mem::replace(&mut self.settings, settings);
                let mode_changed = previous.config.mode != self.settings.config.mode;
                self.report_device_bindings(&previous, "Applied");
                let primary_changed = mode_changed
                    || previous.source1_id != self.settings.source1_id
                    || previous.meter_id != self.settings.meter_id;
                if primary_changed {
                    self.live_reading = None;
                    self.live_samples.clear();
                    self.idle_samples.clear();
                    self.last_live_timestamp.clear();
                }
                if primary_changed
                    || previous.reference_meter_id != self.settings.reference_meter_id
                {
                    self.reference_live_samples.clear();
                }
                if mode_changed
                    || previous.source2_id != self.settings.source2_id
                    || previous.config.balanced_source.is_some()
                        != self.settings.config.balanced_source.is_some()
                {
                    self.source2_samples.clear();
                }
                if mode_changed
                    || previous.config.balanced_source.is_some()
                        != self.settings.config.balanced_source.is_some()
                {
                    self.samples.clear();
                    self.reference_samples.clear();
                    self.separate = Default::default();
                    self.gate_metric = reference_view::GateMetric::Sweep;
                    self.rebuild_plot_series();
                    self.rebuild_reference_plot_series();
                    self.custom_text.clear();
                }
                if mode_changed {
                    self.send(Command::SetLiveMode {
                        mode: self.settings.config.mode,
                    });
                }
                self.send_meter_ranges();
            } else if let Some(error) = completion.snapshot["inventory_error"].as_str() {
                self.device_change_notice = format!("Device selection rejected. {error}");
                self.report(self.device_change_notice.clone());
            }
        }
        if let Some((job, source, id)) = &self.pending_inventory_selection
            && next.gui_jobs.iter().any(|completion| completion.id == *job)
        {
            if next
                .inventory_devices
                .iter()
                .any(|entry| entry.id == *id && entry.assigned)
            {
                let previous = self.settings.clone();
                self.select_inventory_entry(*source, *id);
                self.report_device_bindings(&previous, "Applied");
                self.live_reading = None;
                self.live_samples.clear();
                self.idle_samples.clear();
                self.last_live_timestamp.clear();
            }
            self.pending_inventory_selection = None;
        }
        if !next.inventory_error.is_empty() && next.inventory_error != self.state.inventory_error {
            self.report(next.inventory_error.clone());
        }
        self.inventory_samples.retain(|id, _| {
            next.inventory_devices
                .iter()
                .any(|entry| entry.id == *id && entry.reading.is_some())
        });
        let time = self.boot.elapsed().as_secs_f64();
        for entry in &next.inventory_devices {
            let Some(reading) = &entry.reading else {
                continue;
            };
            let previous = self
                .state
                .inventory_devices
                .iter()
                .find(|old| old.id == entry.id)
                .and_then(|old| old.reading.as_ref());
            if previous.is_some_and(|old| old.timestamp == reading.timestamp) {
                continue;
            }
            let samples = self.inventory_samples.entry(entry.id).or_default();
            if previous.is_some_and(|old| old.mode != reading.mode) {
                samples.clear();
            }
            samples.push_back([time, reading.value]);
            while samples.len() > 1024
                || samples.front().is_some_and(|point| time - point[0] > 30.0)
            {
                samples.pop_front();
            }
        }
    }

    fn inventory_connected(&self, id: u64) -> bool {
        self.state
            .inventory_devices
            .iter()
            .any(|entry| entry.id == id)
    }

    fn device_cards(&mut self, ui: &mut egui::Ui, two_columns: bool) {
        let cards = self.settings.ordered_cards();
        self.card_geometry.clear();
        let mut drop = None;
        let mut render_card = |ui: &mut egui::Ui, (source, id): (bool, u64)| {
            let card = ui
                .push_id(("device_card", id), |ui| {
                    self.unified_device_card(ui, source, id)
                })
                .response;
            self.card_geometry.insert(format!("{id}.card"), card.rect);
            if self.scroll_to_card == Some(id) {
                card.scroll_to_me(Some(egui::Align::Max));
                self.scroll_to_card = None;
            }
            if self.inventory_editable()
                && let Some(dragged) = card.dnd_hover_payload::<u64>()
                && *dragged != id
                && (!two_columns || cards.contains(&(source, *dragged)))
                && let Some(pointer) = ui.ctx().pointer_interact_pos()
            {
                let after = pointer.y >= card.rect.center().y;
                let y = if after {
                    card.rect.bottom() + 4.0
                } else {
                    card.rect.top() - 4.0
                };
                ui.painter().hline(
                    card.rect.x_range(),
                    y,
                    egui::Stroke::new(2.0_f32, widgets::DROP_LINE),
                );
                if card.dnd_release_payload::<u64>().is_some() {
                    drop = Some((*dragged, id, after));
                }
            }
            ui.add_space(8.0);
        };
        if two_columns {
            let column_area_width = ui.available_width();
            let mut sources = cards.iter().copied().filter(|card| card.0);
            let mut meters = cards.iter().copied().filter(|card| !card.0);
            loop {
                let row = [sources.next(), meters.next()];
                if row == [None, None] {
                    break;
                }
                let mut row_rect = ui.available_rect_before_wrap();
                row_rect.max.x = row_rect.min.x + column_area_width;
                ui.scope_builder(egui::UiBuilder::new().max_rect(row_rect), |ui| {
                    ui.columns_const(|columns: &mut [egui::Ui; 2]| {
                        for (column, card) in columns.iter_mut().zip(row) {
                            if let Some(card) = card {
                                render_card(column, card);
                            }
                        }
                    });
                });
            }
        } else {
            for &card in &cards {
                render_card(ui, card);
            }
        }
        if let Some(&dragged) = egui::DragAndDrop::payload::<u64>(ui.ctx()).as_deref()
            && let Some(pointer) = ui.ctx().pointer_interact_pos()
        {
            if self.inventory_editable() {
                let clip = ui.clip_rect();
                if clip.contains(pointer) {
                    let delta = if pointer.y < clip.top() + 28.0 {
                        10.0
                    } else if pointer.y > clip.bottom() - 28.0 {
                        -10.0
                    } else {
                        0.0
                    };
                    if delta != 0.0 {
                        ui.scroll_with_delta(egui::vec2(0.0, delta));
                        ui.ctx().request_repaint();
                    }
                }
            }
            // TRKR drag preview: "Kind: name" beside the pointer.
            if let Some(&(source, _)) = cards.iter().find(|&&(_, id)| id == dragged) {
                let title = format!(
                    "{}: {}",
                    self.device_card_kind(source, dragged),
                    self.card_display_name(source, dragged)
                );
                let painter = ui.ctx().layer_painter(egui::LayerId::new(
                    egui::Order::Tooltip,
                    egui::Id::new("device-drag-preview"),
                ));
                let galley =
                    painter.layout_no_wrap(title, egui::FontId::proportional(12.0), widgets::TEXT);
                let preview = egui::Rect::from_min_size(
                    pointer + Vec2::new(14.0, 14.0),
                    galley.size() + Vec2::new(16.0, 10.0),
                );
                painter.rect(
                    preview,
                    6.0,
                    widgets::PANEL,
                    Stroke::new(1.0_f32, BORDER),
                    egui::StrokeKind::Inside,
                );
                painter.galley(preview.min + Vec2::new(8.0, 5.0), galley, widgets::TEXT);
            }
        }
        if let Some((dragged, target, after)) = drop
            && self.settings.move_card(dragged, target, after)
            && let Some(&(source, _)) = cards.iter().find(|&&(_, id)| id == dragged)
        {
            self.device_change_notice = format!("Moved {}.", self.inventory_name(source, dragged));
            self.report(self.device_change_notice.clone());
        }
    }

    fn connect_all(&mut self) {
        self.settings.store_active_entries();
        let settings = self.settings.clone();
        let mut connections = Vec::new();
        for (source, entries) in [(true, &settings.sources), (false, &settings.meters)] {
            for entry in entries {
                if !self.inventory_connected(entry.id)
                    && !(source && self.dc_gate_matches(entry.id))
                    && let Some(connection) =
                        inventory_connection(source, entry, settings.config.mode)
                {
                    connections.push(connection);
                }
            }
        }
        self.request_bindings(settings, connections);
    }

    fn request_bindings(
        &mut self,
        settings: Settings,
        connections: Vec<crate::protocol::InventoryConnection>,
    ) {
        if self.pending_bindings.is_some() {
            return;
        }
        let available = |id| {
            self.inventory_connected(id) || connections.iter().any(|connection| connection.id == id)
        };
        let command = Command::BindInventory {
            source: available(settings.source1_id).then_some(settings.source1_id),
            secondary: (settings.config.balanced_source.is_some()
                && available(settings.source2_id))
            .then_some(settings.source2_id),
            meter: available(settings.meter_id).then_some(settings.meter_id),
            reference: settings.reference_meter_id.filter(|&id| available(id)),
            mode: settings.config.mode,
            connections,
        };
        let id = format!("bindings-{}", uuid::Uuid::new_v4());
        if self.send(crate::protocol::gui_job(id.clone(), command).expect("valid binding job")) {
            self.pending_bindings = Some((id, settings));
        }
    }

    fn connect_instrument(&mut self, source: bool) {
        let id = if source {
            self.settings.source1_id
        } else {
            self.settings.meter_id
        };
        if self.inventory_connected(id) {
            self.request_inventory_selection(source, id);
            return;
        }
        let model = if source {
            &self.settings.source_model
        } else {
            &self.settings.meter_model
        };
        if !model_supported(model) {
            self.report(format!("{model} is not yet supported in the Rust preview."));
            return;
        }
        let simulation = model.starts_with("SIMULATED_");
        let resource = if source {
            self.settings.source.clone()
        } else {
            self.settings.meter.clone()
        };
        self.send(Command::ConnectInventory {
            id,
            source,
            assign: true,
            simulation,
            model: model.clone(),
            resource,
            mode: self.settings.config.mode,
        });
        self.send_meter_ranges();
    }

    fn connect_reference_meter(&mut self) {
        let Some(id) = self.settings.reference_meter_id else {
            return;
        };
        let settings = self.settings.clone();
        let mut connections = Vec::new();
        if !self.inventory_connected(id) {
            let Some(connection) = settings
                .meters
                .iter()
                .find(|entry| entry.id == id)
                .and_then(|entry| inventory_connection(false, entry, settings.config.mode))
            else {
                return;
            };
            connections.push(connection);
        }
        self.request_bindings(settings, connections);
    }

    fn send_meter_ranges(&mut self) {
        self.send(Command::SetMeterRanges {
            meter_voltage_range: self.settings.config.meter_voltage_range,
            meter_current_range: self.settings.config.meter_current_range,
        });
    }

    fn connect_secondary(&mut self) {
        self.settings.store_active_entries();
        let id = self.settings.source2_id;
        let settings = self.settings.clone();
        let mut connections = Vec::new();
        if !self.inventory_connected(id) {
            let Some(connection) = settings
                .sources
                .iter()
                .find(|entry| entry.id == id)
                .and_then(|entry| inventory_connection(true, entry, settings.config.mode))
            else {
                return;
            };
            connections.push(connection);
        }
        self.request_bindings(settings, connections);
    }

    fn device_card_kind(&self, source: bool, id: u64) -> &'static str {
        let mode = self.settings.config.mode;
        let unassigned_mode = self
            .state
            .inventory_devices
            .iter()
            .find(|entry| entry.id == id && !entry.assigned)
            .map(|entry| entry.mode);
        if source {
            let source_mode = if let Some(mode) = unassigned_mode {
                mode
            } else if id == self.settings.source1_id {
                self.card_control_mode()
            } else if self.settings.config.balanced_source.is_some()
                && id == self.settings.source2_id
            {
                Mode::Iv
            } else {
                self.settings
                    .sources
                    .iter()
                    .find(|entry| entry.id == id)
                    .map_or(mode, |entry| entry.dc.mode)
            };
            match source_mode {
                Mode::Iv => "Source(V)",
                Mode::Vi => "Source(I)",
            }
        } else {
            let observes_source = self.settings.reference_meter_id == Some(id);
            let mode = if id == self.settings.meter_id {
                self.meter_live_mode()
            } else {
                mode
            };
            if unassigned_mode.map_or((mode == Mode::Iv) == observes_source, |mode| {
                mode == Mode::Vi
            }) {
                "Meter(V)"
            } else {
                "Meter(I)"
            }
        }
    }

    fn sweep(&mut self, ui: &mut egui::Ui) {
        let old_mode = self.settings.config.mode;
        let was_dual = self.settings.config.balanced_source.is_some();
        let width = (ui.available_width() - 57.0).max(90.0);
        egui::Grid::new("sweep_conditions")
            .num_columns(2)
            .spacing([6.0, 8.0])
            .show(ui, |ui| {
                form_label(ui, "Mode", 45.0);
                let mut choice = if was_dual {
                    2_u8
                } else if old_mode == Mode::Iv {
                    0
                } else {
                    1
                };
                let mode_editable = !self.busy()
                    && (!self.state.source2_connected || self.inventory_connected(self.settings.source2_id));
                let meter_role_locked = (self.state.meter_connected || self.state.reference_meter_connected)
                    && self.settings.reference_meter_id.is_some()
                    && ![Some(self.settings.meter_id), self.settings.reference_meter_id]
                        .into_iter().flatten().all(|id| self.inventory_connected(id));
                let response = ui.add_enabled_ui(mode_editable, |ui| {
                    egui::ComboBox::from_id_salt("mode")
                        .width(width)
                    .truncate()
                        .selected_text(match choice {
                            0 => mode_name(Mode::Iv),
                            1 => mode_name(Mode::Vi),
                            _ => "Dual V-I",
                        })
                        .show_ui(ui, |ui| {
                            ui.set_min_width(250.0);
                            // V-I and Dual share the same source quantity, so
                            // they do not require swapping Meter(V/I) roles.
                            ui.add_enabled_ui(!meter_role_locked || old_mode == Mode::Iv, |ui| {
                                ui.selectable_value(&mut choice, 0, mode_name(Mode::Iv));
                                ui.selectable_value(
                                    &mut choice,
                                    2,
                                    "Dual V-I · differential voltage",
                                );
                            })
                            .response
                            .on_disabled_hover_text("Disconnect both Meters before changing measurement function.");
                            ui.add_enabled_ui(!meter_role_locked || old_mode == Mode::Vi, |ui| {
                                ui.selectable_value(&mut choice, 1, mode_name(Mode::Vi));
                            })
                            .response
                            .on_disabled_hover_text("Disconnect both Meters before changing measurement function.");
                        })
                });
                response.response.on_disabled_hover_text(
                    if meter_role_locked {
                        "Disconnect both Meters before changing measurement function."
                    } else {
                        "Disconnect Source 2 before leaving Dual mode. Wait for the current operation to finish."
                    },
                );
                ui.end_row();
                if choice == 2 && !was_dual && self.dc_gate_matches(self.settings.source2_id) {
                    self.start_error = Some("Source 2 is holding a fixed bias. Disconnect it or assign a different Source 2 before selecting Dual V-I.".into());
                    choice = if old_mode == Mode::Iv { 0 } else { 1 };
                }
                if choice
                    != if was_dual {
                        2
                    } else if old_mode == Mode::Iv {
                        0
                    } else {
                        1
                    }
                {
                    let mut candidate = self.settings.clone();
                    candidate.store_active_protection();
                    let next_mode = if choice == 1 { Mode::Vi } else { Mode::Iv };
                    candidate
                        .swap_meter_roles_for_mode_change(old_mode, next_mode);
                    if old_mode != next_mode {
                        let definition = measurement_definition(next_mode);
                        let ranges = (candidate.config.meter_voltage_range, candidate.config.meter_current_range);
                        candidate.config = definition.default_config();
                        (candidate.config.meter_voltage_range, candidate.config.meter_current_range) = ranges;
                        candidate.units = [definition.preferred_unit_index; 3];
                    }
                    candidate.config.mode = next_mode;
                    self.dc_preview_mode = None;
                    candidate.config.balanced_source =
                        (choice == 2).then(iv_core::BalancedSourceConfig::default);
                    // Source 2 is chosen from existing devices; never invent one.
                    if choice == 2 && candidate.source2_id == settings::NO_DEVICE {
                        candidate.source2_id = candidate
                            .sources
                            .iter()
                            .find(|entry| entry.id != candidate.source1_id)
                            .map_or(settings::NO_DEVICE, |entry| entry.id);
                    }
                    // `default_config` above intentionally rebuilds the run
                    // settings, so reapply both active DMM bindings here.
                    candidate.bind_active_entries();
                    candidate.activate_mode_protection();
                    if (was_dual != (choice == 2) && self.inventory_connected(candidate.source2_id))
                        || (old_mode != next_mode && candidate.reference_meter_id.is_some()
                            && (self.state.meter_connected || self.state.reference_meter_connected))
                    {
                        self.request_bindings(candidate, Vec::new());
                        return;
                    }
                    let previous = std::mem::replace(&mut self.settings, candidate);
                    self.report_device_bindings(&previous, "Selected");
                    if self.state.source_dc.is_none() && self.dc_open.contains(&self.settings.source1_id)
                        && let Some(entry) = self.settings.sources.iter_mut().find(|entry| entry.id == self.settings.source1_id)
                        && entry.dc.mode != next_mode
                    {
                        entry.dc.reset_function(next_mode);
                    }
                    self.samples.clear();
                    self.reference_samples.clear();
                self.separate = Default::default();
                self.gate_metric = reference_view::GateMetric::Sweep;
                    self.rebuild_plot_series();
                    self.rebuild_reference_plot_series();
                    self.custom_text.clear();
                    if old_mode != next_mode {
                        self.live_samples.clear();
                        self.live_reading = None;
                    }
                    if old_mode != next_mode && (self.state.source_connected || self.state.meter_connected) && self.state.source_dc.is_none() {
                        self.send(Command::SetLiveMode {
                            mode: self.settings.config.mode,
                        });
                    }
                }
                form_label(ui, "Sweep", 45.0);
                egui::ComboBox::from_id_salt("pattern")
                    .width(width)
                    .truncate()
                    .selected_text(pattern_name(self.settings.config.pattern))
                    .show_ui(ui, |ui| {
                        ui.set_min_width(230.0);
                        for definition in sweep_definitions() {
                            let pattern = definition.pattern;
                            ui.selectable_value(
                                &mut self.settings.config.pattern,
                                pattern,
                                pattern_name(pattern),
                            )
                            .on_hover_text(definition.description());
                        }
                    })
                    .response
                    .on_hover_text(
                        sweep_definitions()
                            .iter()
                            .find(|definition| definition.pattern == self.settings.config.pattern)
                            .expect("every built-in pattern has a definition")
                            .description(),
                    );
                ui.end_row();
                let unit = if self.settings.config.mode == Mode::Iv {
                    "V"
                } else {
                    "A"
                };
                for (index, (label, value)) in [
                    (
                        iv_core::parameter_definition("start").unwrap().label,
                        &mut self.settings.config.start,
                    ),
                    (
                        iv_core::parameter_definition("stop").unwrap().label,
                        &mut self.settings.config.stop,
                    ),
                    (
                        iv_core::parameter_definition("step").unwrap().label,
                        &mut self.settings.config.step,
                    ),
                ]
                .into_iter()
                .enumerate()
                {
                    form_label(ui, label, 45.0);
                    quantity(
                        ui,
                        ("sweep_quantity", index),
                        value,
                        &mut self.settings.units[index],
                        unit,
                        width,
                        false,
                    );
                    ui.end_row();
                }
                form_label(ui, "", 45.0);
                if button(ui, "Settings…", width.min(196.0), true).clicked() {
                    self.timing_before = Some((self.settings.clone(), self.custom_text.clone()));
                    self.settings.store_active_protection();
                    self.timing_open = true;
                }
                ui.end_row();
            });
    }

    fn sweep_owns_live_mode(&self) -> bool {
        self.pending_run
            || matches!(
                self.state.phase,
                Phase::Preparing | Phase::Running | Phase::CleaningUp
            ) && self.pending_dc.is_none()
    }

    fn verified_dc_mode(&self) -> Option<Mode> {
        self.state
            .source_dc
            .as_ref()
            .filter(|_| self.state.output == OutputState::On)
            .map(|dc| dc.mode)
    }

    fn card_control_mode(&self) -> Mode {
        card_mode(
            self.settings.config.mode,
            self.sweep_owns_live_mode(),
            self.verified_dc_mode(),
            self.dc_preview_mode,
        )
    }

    fn meter_live_mode(&self) -> Mode {
        card_mode(
            self.settings.config.mode,
            self.sweep_owns_live_mode(),
            self.verified_dc_mode(),
            None,
        )
    }

    /// Show the sweep protection alongside the manual DC compliance.
    fn source_compliance_control(&mut self, ui: &mut egui::Ui) {
        let mode = self.settings.config.mode;
        let unit = if mode == Mode::Iv { "A" } else { "V" };
        let effective = self
            .preview
            .as_ref()
            .ok()
            .map(|preview| preview.hardware_compliance)
            .unwrap_or(self.settings.config.compliance);
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            form_label(ui, "Compliance", 85.0);
            let width = ui.available_width();
            ui.add_enabled_ui(self.inventory_editable(), |ui| {
                let (saved, units) = match mode {
                    Mode::Iv => (
                        &mut self.settings.source_current_protection,
                        &mut self.settings.source_protection_units[0],
                    ),
                    Mode::Vi => (
                        &mut self.settings.source_voltage_protection,
                        &mut self.settings.source_protection_units[1],
                    ),
                };
                let mut value = saved.unwrap_or(effective);
                let before = value;
                quantity(
                    ui,
                    "source_protection",
                    &mut value,
                    units,
                    unit,
                    width,
                    true,
                );
                if value != before {
                    *saved = Some(value);
                }
            });
        })
        .response
        .on_hover_ui(|ui| self.source_protection_effective_note(ui, unit));
        self.settings.activate_mode_protection();
    }

    /// Meter stop and range controls are intentionally separate: the former
    /// stops a run while the latter selects the meter input path.
    fn meter_measurement_controls(&mut self, ui: &mut egui::Ui) {
        let mode = self.settings.config.mode;
        // Sweep protection belongs to the planned run. Range describes the
        // live meter function, which may instead follow a held manual DC.
        let range_mode = self.meter_live_mode();
        let protection_unit = if mode == Mode::Iv { "A" } else { "V" };
        let effective_source = self
            .preview
            .as_ref()
            .ok()
            .map(|preview| preview.hardware_compliance)
            .unwrap_or(self.settings.config.compliance);
        let previous_meter_limit = match mode {
            Mode::Iv => self.settings.current_compliance,
            Mode::Vi => self.settings.voltage_compliance,
        };
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            form_label(ui, "Sweep limit", 85.0);
            let width = ui.available_width();
            ui.add_enabled_ui(!self.busy() && !self.python_locked(), |ui| {
                let (meter_value, meter_unit) = match mode {
                    Mode::Iv => (
                        &mut self.settings.current_compliance,
                        &mut self.settings.compliance_units[0],
                    ),
                    Mode::Vi => (
                        &mut self.settings.voltage_compliance,
                        &mut self.settings.compliance_units[1],
                    ),
                };
                quantity(
                    ui,
                    "meter_stop",
                    meter_value,
                    meter_unit,
                    protection_unit,
                    width,
                    true,
                );
            });
        })
        .response
        .on_hover_text("Sweep stop threshold for the measured magnitude.");
        preserve_source_on_meter_edit(&mut self.settings, previous_meter_limit, effective_source);
        ui.add_space(4.0);

        let meter_model = self.settings.meter_model.clone();
        let range_locked_by_operation = self.busy() || self.python_locked();
        let range_edit = ui.add_enabled_ui(
            !range_locked_by_operation && crate::gui_control::outputs_off(&self.state),
            |ui| {
                ui.horizontal(|ui| {
                    form_label(ui, "Range", 85.0);
                    match range_mode {
                        Mode::Iv => widgets::meter_range_choice(
                            ui,
                            "active_meter_current_range",
                            &meter_model,
                            range_mode,
                            &mut self.settings.config.meter_current_range,
                        ),
                        Mode::Vi => widgets::meter_range_choice(
                            ui,
                            "active_meter_voltage_range",
                            &meter_model,
                            range_mode,
                            &mut self.settings.config.meter_voltage_range,
                        ),
                    }
                })
                .inner
            },
        );
        range_edit
            .response
            .on_disabled_hover_text(if range_locked_by_operation {
                "Wait for the current operation or external control to finish."
            } else {
                "Turn all source outputs OFF before changing the meter range."
            });
        let range_changed = range_edit.inner;
        if range_changed {
            self.send_meter_ranges();
        }
        let fixed_range = match range_mode {
            Mode::Iv => self.settings.config.meter_current_range,
            Mode::Vi => self.settings.config.meter_voltage_range,
        };
        let meter_limit = match mode {
            Mode::Iv => self.settings.current_compliance,
            Mode::Vi => self.settings.voltage_compliance,
        };
        if mode == range_mode && fixed_range.is_some_and(|range| meter_limit > range) {
            ui.response().on_hover_text("Limit exceeds the fixed range. Instrument overload may stop first; changing Limit does not change Range.");
        }
        ui.horizontal(|ui| {
            form_label(ui, "NPLC", 85.0);
            ui.add_enabled_ui(!self.busy() && !self.python_locked(), |ui| {
                ui.add_sized(
                    [ui.available_width(), 24.0],
                    egui::DragValue::new(&mut self.settings.config.nplc)
                        .speed(0.01)
                        .range(0.001..=100.0)
                        .max_decimals(3),
                );
            });
        });
        self.settings.activate_mode_protection();
    }

    fn measurement_controls(&mut self, ui: &mut egui::Ui) {
        let clip = ui.clip_rect();
        let response = ui.scope(|ui| {
            section(ui, "Measurement", |ui| {
                ui.columns(2, |columns| {
                    columns[0].add_enabled_ui(
                        !self.busy() && !self.python_locked() && !self.file_task_pending(),
                        |ui| self.sweep(ui),
                    );
                    section(&mut columns[1], "Output", |ui| {
                        ui.add_enabled_ui(
                            !self.busy() && !self.python_locked() && !self.file_task_pending(),
                            |ui| {
                                let compact = ui.available_width() < 230.0;
                                let label_width = if compact { 30.0 } else { 54.0 };
                                let action_width = if compact { 64.0 } else { 104.0 };
                                let field_width = (ui.available_width()
                                    - label_width
                                    - 6.0
                                    - action_width
                                    - ui.spacing().item_spacing.x)
                                    .max(24.0);
                                egui::Grid::new("output_files")
                                    .min_col_width(0.0)
                                    .num_columns(2)
                                    .spacing([6.0, 8.0])
                                    .show(ui, |ui| {
                                        form_label(
                                            ui,
                                            if compact { "Dir." } else { "Directory" },
                                            label_width,
                                        );
                                        ui.horizontal(|ui| {
                                            let summary = path_summary(&self.settings.output_dir);
                                            compact_field(
                                                ui,
                                                "output_dir",
                                                &mut self.settings.output_dir,
                                                field_width,
                                                summary,
                                            );
                                            if button(ui, "Browse", action_width, true).clicked() {
                                                self.choose_output_directory(ui.ctx());
                                            }
                                        });
                                        ui.end_row();
                                        form_label(ui, "File", label_width);
                                        ui.horizontal(|ui| {
                                            ui.add_sized(
                                                [field_width, 24.0],
                                                egui::TextEdit::singleline(
                                                    &mut self.settings.filename,
                                                )
                                                .margin(egui::Margin::symmetric(3, 3)),
                                            ).on_hover_text(self.settings.filename.as_str());
                                            ui.add_sized(
                                                [action_width, 24.0],
                                                egui::Checkbox::new(
                                                    &mut self.settings.auto_suffix,
                                                    if compact { "Suffix" } else { "Auto suffix" },
                                                ),
                                            ).on_hover_text("Append a timestamp to each new measurement filename.");
                                        });
                                        ui.end_row();
                                    });
                            },
                        );
                    });
                    columns[1].add_space(8.0);
                    section(&mut columns[1], "Run", |ui| self.run_controls(ui));
                });
            })
        });
        ui.ctx().data_mut(|data| {
            data.insert_temp(
                egui::Id::new("measurement_controls_visible"),
                clip.expand(0.5).contains_rect(response.response.rect),
            );
            let rect = response.response.rect;
            data.insert_temp(
                egui::Id::new("measurement_controls_bounds"),
                [rect.min.x, rect.min.y, rect.max.x, rect.max.y],
            );
        });
    }

    /// Inventory management stays beside the device cards; Settings only
    /// assigns their measurement roles and the dual-source ratio.
    fn device_add_footer(&mut self, ui: &mut egui::Ui) -> egui::Rect {
        let response = ui.add_enabled(
            self.inventory_editable(),
            egui::Button::new("+ Add device")
                .fill(BUTTON)
                .corner_radius(6)
                .min_size(Vec2::new(ui.available_width(), 24.0)),
        );
        if response.clicked() {
            self.add_device = Some(AddDevice {
                role: InstrumentRole::Source,
                label: String::new(),
            });
        }
        response.rect
    }

    fn inventory_editable(&self) -> bool {
        !self.busy() && !self.pending_run && !self.python_locked()
    }

    fn request_inventory_selection(&mut self, source: bool, id: u64) {
        if self.pending_inventory_selection.is_some() {
            return;
        }
        let job = format!("inventory-{}", uuid::Uuid::new_v4());
        let command =
            crate::protocol::gui_job(job.clone(), Command::SelectInventory { id, source })
                .expect("selection is a finite inventory command");
        if self.send(command) {
            self.pending_inventory_selection = Some((job, source, id));
        }
    }

    fn select_inventory_entry(&mut self, source: bool, id: u64) {
        self.settings.store_active_entries();
        if source {
            self.settings.source1_id = id;
        } else {
            self.settings.meter_id = id;
        }
        self.settings.bind_active_entries();
    }

    fn report_device_bindings(&mut self, previous: &Settings, action: &str) {
        let settings = &self.settings;
        let voltage = settings.config.mode == Mode::Iv;
        let mut changes = Vec::new();
        let was_dual = previous.config.balanced_source.is_some();
        let dual = settings.config.balanced_source.is_some();
        if settings.config.mode != previous.config.mode || was_dual != dual {
            changes.push(format!(
                "Mode: {}",
                if dual {
                    "Dual V-I"
                } else {
                    mode_name(settings.config.mode)
                }
            ));
        }
        for (label, old, new, source) in [
            (
                "Source 1",
                Some(previous.source1_id),
                Some(settings.source1_id),
                true,
            ),
            (
                "Source 2",
                (previous.source2_id != settings::NO_DEVICE).then_some(previous.source2_id),
                (settings.source2_id != settings::NO_DEVICE).then_some(settings.source2_id),
                true,
            ),
            (
                if voltage { "Meter (I)" } else { "Meter (V)" },
                Some(previous.meter_id),
                Some(settings.meter_id),
                false,
            ),
            (
                if voltage { "Meter (V)" } else { "Meter (I)" },
                previous.reference_meter_id,
                settings.reference_meter_id,
                false,
            ),
        ] {
            if old != new {
                let device =
                    new.map_or_else(|| "Unassigned".into(), |id| self.inventory_name(source, id));
                changes.push(format!("{label}: {device}"));
            }
        }
        self.device_change_notice = if changes.is_empty() {
            "Device assignments confirmed.".into()
        } else {
            format!("{action} · {}", changes.join(" · "))
        };
        self.report(self.device_change_notice.clone());
    }

    fn device_bindings(&mut self, ui: &mut egui::Ui) {
        self.settings.store_active_entries();
        section(ui, "Devices", |ui| {
            ui.label(
                RichText::new(
                    "Choose the saved instruments used by this IV run. Other cards remain stored choices.",
                )
                .small()
                .color(MUTED),
            );
            if self.pending_bindings.is_some() || self.pending_inventory_selection.is_some() {
                ui.label(
                    RichText::new("Applying device selection…")
                        .small()
                        .color(MUTED),
                );
            } else if !self.device_change_notice.is_empty() {
                ui.label(RichText::new(&self.device_change_notice).small());
            }
            ui.add_space(6.0);
            let locked = self.busy()
                || self.state.source_connected
                || self.state.source2_connected
                || self.state.meter_connected
                || self.state.reference_meter_connected
                || self.state.gate_connected;
            let source_choices = self
                .settings
                .sources
                .iter()
                .enumerate()
                .map(|(index, entry)| (entry.id, inventory_entry_name("Source", index, entry)))
                .collect::<Vec<_>>();
            let meter_choices = self
                .settings
                .meters
                .iter()
                .enumerate()
                .map(|(index, entry)| (entry.id, inventory_entry_name("Meter", index, entry)))
                .collect::<Vec<_>>();
            if !self.state.inventory_devices.is_empty() {
                let mut candidate = self.settings.clone();
                let current = [
                    Some(candidate.source1_id),
                    (candidate.source2_id != settings::NO_DEVICE).then_some(candidate.source2_id),
                    Some(candidate.meter_id),
                    candidate.reference_meter_id,
                ];
                let mut selected = current;
                let voltage_source = candidate.config.mode == Mode::Iv;
                ui.add_enabled_ui(self.inventory_editable(), |ui| {
                    egui::Grid::new("connected_device_bindings")
                        .num_columns(2)
                        .show(ui, |ui| {
                            for (index, label) in [
                                "Source 1",
                                "Source 2",
                                if voltage_source {
                                    "Meter (I)"
                                } else {
                                    "Meter (V)"
                                },
                                if voltage_source {
                                    "Meter (V)"
                                } else {
                                    "Meter (I)"
                                },
                            ]
                            .into_iter()
                            .enumerate()
                            {
                                if index == 1 && candidate.config.balanced_source.is_none() {
                                    continue;
                                }
                                let source = index < 2;
                                form_label(ui, label, 65.0);
                                egui::ComboBox::from_id_salt(("connected_binding", index))
                                    .truncate()
                                    .width(ui.available_width())
                                    .selected_text(
                                        current[index]
                                            .map(|id| self.inventory_name(source, id))
                                            .unwrap_or_else(|| {
                                                if index == 1 {
                                                    "Unassigned"
                                                } else {
                                                    "Source value only"
                                                }
                                                .into()
                                            }),
                                    )
                                    .show_ui(ui, |ui| {
                                        if index == 3 {
                                            ui.selectable_value(
                                                &mut selected[index],
                                                None,
                                                "Source value only",
                                            );
                                        }
                                        for (id, name) in if source {
                                            &source_choices
                                        } else {
                                            &meter_choices
                                        } {
                                            ui.add_enabled_ui(
                                                self.inventory_connected(*id)
                                                    && !self.dc_gate_matches(*id),
                                                |ui| {
                                                    ui.selectable_value(
                                                        &mut selected[index],
                                                        Some(*id),
                                                        name,
                                                    );
                                                },
                                            );
                                        }
                                    });
                                ui.end_row();
                            }
                        });
                });
                if let Some(index) =
                    (0..selected.len()).find(|&index| selected[index] != current[index])
                {
                    if let Some(id) = selected[index]
                        && let Some(other) = (0..current.len())
                            .find(|&other| other != index && current[other] == Some(id))
                    {
                        selected[other] = current[index];
                    }
                    candidate.source1_id = selected[0].unwrap_or(candidate.source1_id);
                    candidate.source2_id = selected[1].unwrap_or(candidate.source2_id);
                    candidate.meter_id = selected[2].unwrap_or(candidate.meter_id);
                    candidate.reference_meter_id = selected[3];
                    if selected[0].is_some()
                        && selected[2].is_some()
                        && (candidate.config.balanced_source.is_none() || selected[1].is_some())
                    {
                        candidate.bind_active_entries();
                        self.request_bindings(candidate, Vec::new());
                    }
                }
                return;
            }
            let previous = self.settings.clone();
            let mut changed = false;
            let response = ui.add_enabled_ui(!locked, |ui| {
                egui::Grid::new("device_role_bindings")
                    .num_columns(2)
                    .spacing([6.0, 6.0])
                    .show(ui, |ui| {
                        form_label(ui, "Source 1", 65.0);
                        let before = self.settings.source1_id;
                        egui::ComboBox::from_id_salt("source1_binding")
                            .truncate()
                            .width(ui.available_width())
                            .selected_text(self.inventory_name(true, before))
                            .show_ui(ui, |ui| {
                                for (id, label) in &source_choices {
                                    ui.add_enabled_ui(
                                        self.settings.config.balanced_source.is_none()
                                            || *id != self.settings.source2_id,
                                        |ui| {
                                            ui.selectable_value(
                                                &mut self.settings.source1_id,
                                                *id,
                                                label,
                                            );
                                        },
                                    );
                                }
                            });
                        changed |= before != self.settings.source1_id;
                        ui.end_row();

                        if self.settings.config.balanced_source.is_some() {
                            form_label(ui, "Source 2", 65.0);
                            let before = self.settings.source2_id;
                            egui::ComboBox::from_id_salt("source2_binding")
                                .truncate()
                                .width(ui.available_width())
                                .selected_text(if before == settings::NO_DEVICE {
                                    "Unassigned · add a second Source".to_owned()
                                } else {
                                    self.inventory_name(true, before)
                                })
                                .show_ui(ui, |ui| {
                                    for (id, label) in &source_choices {
                                        ui.add_enabled_ui(*id != self.settings.source1_id, |ui| {
                                            ui.selectable_value(
                                                &mut self.settings.source2_id,
                                                *id,
                                                label,
                                            );
                                        });
                                    }
                                });
                            changed |= before != self.settings.source2_id;
                            ui.end_row();
                        }

                        let source_quantity = if self.settings.config.mode == Mode::Iv {
                            "Meter (V)"
                        } else {
                            "Meter (I)"
                        };
                        let measured_quantity = if self.settings.config.mode == Mode::Iv {
                            "Meter (I)"
                        } else {
                            "Meter (V)"
                        };
                        form_label(ui, measured_quantity, 65.0);
                        let before = self.settings.meter_id;
                        egui::ComboBox::from_id_salt("meter_binding")
                            .truncate()
                            .width(ui.available_width())
                            .selected_text(self.inventory_name(false, before))
                            .show_ui(ui, |ui| {
                                for (id, label) in &meter_choices {
                                    ui.add_enabled_ui(
                                        self.settings.reference_meter_id != Some(*id),
                                        |ui| {
                                            ui.selectable_value(
                                                &mut self.settings.meter_id,
                                                *id,
                                                label,
                                            );
                                        },
                                    );
                                }
                            });
                        changed |= before != self.settings.meter_id;
                        ui.end_row();

                        form_label(ui, source_quantity, 65.0);
                        let before = self.settings.reference_meter_id;
                        egui::ComboBox::from_id_salt("reference_meter_binding")
                            .truncate()
                            .width(ui.available_width())
                            .selected_text(
                                before
                                    .map(|id| self.inventory_name(false, id))
                                    .unwrap_or_else(|| "Source value only".into()),
                            )
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut self.settings.reference_meter_id,
                                    None,
                                    "Source value only",
                                );
                                for (id, label) in &meter_choices {
                                    ui.add_enabled_ui(*id != self.settings.meter_id, |ui| {
                                        ui.selectable_value(
                                            &mut self.settings.reference_meter_id,
                                            Some(*id),
                                            label,
                                        );
                                    });
                                }
                            });
                        changed |= before != self.settings.reference_meter_id;
                        ui.end_row();
                    });
                if let Some(dual) = &mut self.settings.config.balanced_source {
                    ui.add_space(6.0);
                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.label("Voltage ratio");
                        ui.add(
                            egui::DragValue::new(&mut dual.ratio[0])
                                .range(1..=1000)
                                .speed(1),
                        );
                        ui.label(":");
                        ui.add(
                            egui::DragValue::new(&mut dual.ratio[1])
                                .range(1..=1000)
                                .speed(1),
                        );
                    });
                    ui.label(
                        RichText::new(format!(
                            "S1 = +{}/{} · V    S2 = −{}/{} · V",
                            dual.ratio[0],
                            dual.ratio[0] + dual.ratio[1],
                            dual.ratio[1],
                            dual.ratio[0] + dual.ratio[1],
                        ))
                        .small()
                        .color(MUTED),
                    );
                }
            });
            response.response.on_disabled_hover_text(
                "Disconnect the active instruments before changing device bindings or the dual-source split.",
            );
            if changed {
                if self.settings.source1_id == self.settings.source2_id {
                    self.settings.source2_id = self
                        .settings
                        .sources
                        .iter()
                        .find(|entry| entry.id != self.settings.source1_id)
                        .map_or(settings::NO_DEVICE, |entry| entry.id);
                }
                if self.settings.reference_meter_id == Some(self.settings.meter_id) {
                    self.settings.reference_meter_id = None;
                }
                if let Some(reference_id) = self.settings.reference_meter_id {
                    let primary_resource = self
                        .settings
                        .meters
                        .iter()
                        .find(|entry| entry.id == self.settings.meter_id)
                        .map(|entry| entry.resource.as_str());
                    let reference_resource = self
                        .settings
                        .meters
                        .iter()
                        .find(|entry| entry.id == reference_id)
                        .map(|entry| entry.resource.as_str());
                    if primary_resource.zip(reference_resource).is_some_and(
                        |(primary, reference)| !primary.is_empty() && primary == reference,
                    ) {
                        self.settings.reference_meter_id = None;
                        self.report("The same physical resource cannot be assigned to both Meter (V) and Meter (I).".into());
                    }
                }
                self.settings.bind_active_entries();
                self.report_device_bindings(&previous, "Selected");
            }
        });
    }

    fn inventory_name(&self, source: bool, id: u64) -> String {
        let entries = if source {
            &self.settings.sources
        } else {
            &self.settings.meters
        };
        let role = if source { "Source" } else { "Meter" };
        entries
            .iter()
            .enumerate()
            .find(|(_, entry)| entry.id == id)
            .map(|(index, entry)| inventory_entry_name(role, index, entry))
            .unwrap_or_else(|| format!("{role} unavailable"))
    }

    fn save_measurement(&mut self, ctx: &egui::Context) {
        if self.file_task_pending() {
            self.save_notice =
                Some("Wait for the current file operation to finish before saving a copy.".into());
            return;
        }
        if self.busy() || self.pending_run {
            self.save_notice = Some(
                "Stop the measurement and wait for output cleanup before saving a copy.".into(),
            );
            return;
        }
        if self.state.points == 0 || self.state.path.is_empty() {
            self.save_notice = Some("No measurement data to save. Start a measurement first. Live monitor readings are not saved as sweep data.".into());
            return;
        }
        let source = PathBuf::from(&self.state.path);
        let stem = source
            .file_stem()
            .and_then(|name| name.to_str())
            .unwrap_or("iv_run");
        let output_directory = PathBuf::from(&self.settings.output_dir);
        let default_name = format!("{stem}_copy.csv");
        let points = self.state.points;
        let dialog = rfd::AsyncFileDialog::new()
            .set_title("Save measurement copy")
            .add_filter("CSV measurement", &["csv"])
            .set_directory(output_directory)
            .set_file_name(default_name)
            .save_file();
        self.start_file_task(ctx, "Save measurement location", false, move || {
            let destination = wait_for_dialog(dialog).map(|file| file.path().to_path_buf());
            FileTaskResult::SaveDestination {
                source,
                destination,
                points,
            }
        });
    }

    fn save_dialog(&mut self, ctx: &egui::Context) {
        let Some(message) = self.save_notice.clone() else {
            return;
        };
        let response = egui::Modal::new(egui::Id::new("save_measurement")).show(ctx, |ui| {
            ui.set_width(400.0);
            ui.heading("Save measurement");
            ui.add_space(8.0);
            ui.label(message);
            ui.add_space(12.0);
            if button(ui, "OK", 80.0, true).clicked() {
                ui.close();
            }
        });
        if response.should_close() {
            self.save_notice = None;
        }
    }

    fn run_controls(&mut self, ui: &mut egui::Ui) {
        let width = (ui.available_width() - 16.0) / 3.0;
        ui.horizontal(|ui| {
            let outputs_ready = self.state.inventory_outputs_managed();
            let start_ready = self.state.connected && self.preview.is_ok() && outputs_ready;
            let start_button = egui::Button::new(RichText::new("Start").strong())
                .min_size(Vec2::new(width, 28.0))
                .fill(if start_ready {
                    Color32::from_rgb(45, 112, 224)
                } else {
                    BUTTON
                });
            if ui
                .add_enabled(
                    !self.busy()
                        && !self.pending_run
                        && !self.python_locked()
                        && !self.file_task_pending()
                        && outputs_ready,
                    start_button,
                )
                .on_disabled_hover_text(if outputs_ready {
                    "Wait for the current operation to finish, or use Stop."
                } else {
                    crate::protocol::UNASSIGNED_OUTPUT_BLOCK
                })
                .clicked()
            {
                self.start_run();
            }
            let stop_button = egui::Button::new(RichText::new("Stop").strong())
                .min_size(Vec2::new(width, 28.0))
                .fill(if self.active() {
                    Color32::from_rgb(137, 53, 63)
                } else {
                    BUTTON
                });
            let stop = ui.add_enabled(self.active(), stop_button);
            ui.ctx().data_mut(|data| {
                data.insert_temp(
                    egui::Id::new("run_stop_visible"),
                    ui.clip_rect().contains_rect(stop.rect),
                )
            });
            if stop.clicked() && self.send(Command::Stop) {
                self.report(
                    "Stop requested. Waiting for the current I/O and output cleanup.".into(),
                );
            }
            if button(
                ui,
                "Save",
                width,
                !self.busy()
                    && !self.pending_run
                    && !self.python_locked()
                    && !self.file_task_pending(),
            )
            .on_hover_text("Save a complete copy of the acquired CSV and run settings.")
            .on_disabled_hover_text(
                "Stop the measurement and wait for cleanup before saving a copy.",
            )
            .clicked()
            {
                self.save_measurement(ui.ctx());
            }
        });
    }

    fn protection_dialog(&mut self, ctx: &egui::Context) {
        let Some(alert) = self.protection_dialog.clone() else {
            return;
        };
        let off = self.state.output == OutputState::Off;
        let emergency = alert.indicator.contains("EMR");
        let (heading, action) = protection_summary(&alert);
        let accent = if !off || emergency { RED } else { ORANGE };
        let mut open_settings = false;
        let response = egui::Modal::new(egui::Id::new("protection_stop"))
            .frame(
                egui::Frame::popup(&ctx.global_style())
                    .fill(BG)
                    .inner_margin(18),
            )
            .show(ctx, |ui| {
                ui.set_width(380.0);
                ui.label(RichText::new(heading).size(20.0).strong().color(accent));
                ui.add_space(6.0);
                ui.label(
                    RichText::new(if off {
                        "Stopped · Source output OFF verified"
                    } else {
                        "Stopped · Output OFF not confirmed"
                    })
                    .color(if off {
                        Color32::from_rgb(75, 192, 135)
                    } else {
                        RED
                    }),
                );
                ui.add_space(14.0);
                egui::Frame::new()
                    .fill(INPUT)
                    .corner_radius(8)
                    .inner_margin(12)
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        egui::Grid::new("protection_values")
                            .num_columns(2)
                            .spacing([16.0, 8.0])
                            .show(ui, |ui| {
                                ui.label(RichText::new("Limit").color(MUTED));
                                ui.label(
                                    RichText::new(fmt_quantity(alert.limit, &alert.unit))
                                        .size(20.0)
                                        .strong()
                                        .color(accent),
                                );
                                ui.end_row();
                                if let Some(value) = alert.observed {
                                    ui.label(RichText::new("Measured").color(MUTED));
                                    ui.label(
                                        RichText::new(fmt_quantity(value, &alert.unit)).size(18.0),
                                    );
                                    ui.end_row();
                                } else if !alert.indicator.is_empty() {
                                    ui.label(RichText::new("Instrument").color(MUTED));
                                    ui.label(&alert.indicator);
                                    ui.end_row();
                                }
                            });
                    });
                ui.add_space(14.0);
                if !off {
                    ui.label(
                        RichText::new("Turn output OFF on the instrument first.")
                            .strong()
                            .color(RED),
                    );
                    ui.add_space(5.0);
                }
                ui.label(action);
                ui.add_space(8.0);
                ui.label(
                    RichText::new("Raise limits only after checking the sample ratings.")
                        .small()
                        .color(MUTED),
                );
                ui.add_space(8.0);
                egui::CollapsingHeader::new("Details").show(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .max_height((ctx.content_rect().height() - 500.0).clamp(70.0, 180.0))
                        .show(ui, |ui| {
                            ui.label(protection_message(&alert, self.state.output));
                            if !self.state.error.is_empty() {
                                ui.label(self.state.error.as_str());
                            }
                        });
                });
                ui.add_space(14.0);
                ui.horizontal(|ui| {
                    if button(ui, "Close", 90.0, true).clicked() {
                        ui.close();
                    }
                    let review = ui.add_enabled(
                        off && !self.busy() && !emergency,
                        egui::Button::new(RichText::new("Review settings").color(Color32::WHITE))
                            .fill(BLUE)
                            .min_size(egui::vec2(145.0, 28.0)),
                    );
                    if review.clicked() {
                        open_settings = true;
                        ui.close();
                    }
                });
            });
        if response.should_close() {
            self.protection_dialog = None;
            if open_settings {
                self.timing_before = Some((self.settings.clone(), self.custom_text.clone()));
                self.settings.store_active_protection();
                self.timing_open = true;
            }
        }
    }

    fn start_error_dialog(&mut self, ctx: &egui::Context) {
        let Some(reason) = self.start_error.clone() else {
            return;
        };
        let meter_range_changed = reason.contains("Meter range transition detected")
            || reason.contains("Meter range changed");
        let response = egui::Modal::new(egui::Id::new("start_error")).show(ctx, |ui| {
            ui.set_width(420.0);
            ui.heading(if meter_range_changed {
                "Meter range changed"
            } else {
                "Operation needs attention"
            });
            ui.add_space(8.0);
            egui::ScrollArea::vertical()
                .max_height((ctx.content_rect().height() - 200.0).max(160.0))
                .show(ui, |ui| {
                    ui.label(RichText::new(reason).color(if meter_range_changed {
                        ORANGE
                    } else {
                        Color32::WHITE
                    }));
                });
            ui.add_space(8.0);
            let next_step = if meter_range_changed {
                "Use a fixed Meter range in the Meter card, then repeat the complete sweep."
            } else if crate::gui_control::outputs_off(&self.state) {
                "Review the settings before trying again."
            } else {
                "Verify all connected source outputs are OFF before continuing."
            };
            ui.label(RichText::new(next_step).small().color(MUTED));
            ui.add_space(12.0);
            if button(ui, "OK", 80.0, true).clicked() {
                ui.close();
            }
        });
        if response.should_close() {
            self.start_error = None;
        }
    }

    fn timing_dialog(&mut self, ctx: &egui::Context) {
        if !self.timing_open {
            return;
        }
        let mut accepted = false;
        let mut custom_error = if self.settings.config.pattern == Pattern::CustomList {
            parse_custom_points(&self.custom_text).err()
        } else {
            None
        };
        let response = egui::Modal::new(egui::Id::new("measurement_settings"))
            .frame(egui::Frame::popup(&ctx.global_style()).fill(BG).inner_margin(16))
            .show(ctx, |ui| {
                ui.set_width(360.0);
                ui.strong("Measurement Settings");
                ui.add_space(8.0);
                egui::ScrollArea::vertical().max_height((ctx.content_rect().height() - 150.0).clamp(200.0, 600.0)).show(ui, |ui| {
                    ui.add_enabled_ui(!self.busy(), |ui| {
                        self.device_bindings(ui);
                        ui.add_space(8.0);
                        section(ui, "Timing", |ui| {
                            number(
                                ui,
                                "Wait time",
                                &mut self.settings.config.settle_s,
                                1.0,
                                "s",
                                0.0..=iv_core::MAX_SECONDS,
                            );
                        });
                        ui.add_space(8.0);
                        egui::CollapsingHeader::new("Advanced").show(ui, |ui| {
                            number(ui, iv_core::parameter_definition("start_settle_s").unwrap().label, &mut self.settings.config.start_settle_s, 1.0, "", 0.0..=iv_core::MAX_SECONDS);
                            ui.label("Ramp step: automatic (at least the sweep step)");
                            number(ui, iv_core::parameter_definition("ramp_wait_s").unwrap().label, &mut self.settings.config.ramp_wait_s, 1.0, "", 0.0..=iv_core::MAX_SECONDS);
                            ui.horizontal(|ui| {
                                ui.label(iv_core::parameter_definition("repeat").unwrap().label);
                                let mut count = iv_core::readings_per_point(&self.settings.config);
                                if ui.add(egui::DragValue::new(&mut count).range(1..=10000)).changed() {
                                    self.settings.config.repeat = count;
                                    self.settings.config.average_count = 1;
                                }
                            });
                            ui.label("Hold the source constant, average N meter readings, then save one point. The sweep runs once.");
                        });
                        if self.settings.config.pattern == Pattern::CustomList {
                            ui.add_space(8.0);
                            ui.label(if self.settings.config.mode == Mode::Iv { "Custom points (V, comma separated)" } else { "Custom points (A, comma separated)" });
                            if ui.add(egui::TextEdit::multiline(&mut self.custom_text).desired_rows(2).desired_width(340.0)).changed() {
                                match parse_custom_points(&self.custom_text) {
                                    Ok(points) => {
                                        self.settings.config.custom_points = points;
                                        custom_error = None;
                                    }
                                    Err(error) => custom_error = Some(error),
                                }
                            }
                            if let Some(error) = &custom_error {
                                ui.label(RichText::new(error).small().color(RED));
                            }
                        }
                    });
                });
                ui.add_space(10.0);
                ui.label("Use Config -> Save to save these settings to the file.");
                ui.add_space(8.0);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if button(ui, "Cancel", 76.0, true).clicked() { ui.close(); }
                    if button(ui, "OK", 76.0, !self.busy() && custom_error.is_none()).clicked() { accepted = true; ui.close(); }
                });
            });
        if response.should_close() {
            if !accepted && let Some((settings, custom)) = self.timing_before.take() {
                self.settings = settings;
                self.custom_text = custom;
            }
            self.timing_before = None;
            self.timing_open = false;
            if accepted {
                self.send_meter_ranges();
            }
        }
    }

    fn source_target_limit_control(&mut self, ui: &mut egui::Ui) {
        let unit = if self.settings.config.mode == Mode::Iv {
            "mV"
        } else {
            "mA"
        };
        ui.horizontal(|ui| {
            form_label(ui, "Max. target", 85.0);
            let width = ui.available_width();
            ui.add_enabled_ui(self.inventory_editable(), |ui| {
                let mut value = self.settings.config.source_limit / 0.001;
                let response = ui.add_sized(
                    [(width - 54.0 - ui.spacing().item_spacing.x).max(65.0), 24.0],
                    egui::DragValue::new(&mut value)
                        .range(0.0..=f64::MAX)
                        .speed(0.01)
                        .max_decimals(6),
                );
                ui.add_sized([54.0, 24.0], egui::Label::new(unit));
                if response.changed() {
                    self.settings.config.source_limit = value * 0.001;
                }
            });
        })
        .response
        .on_hover_text("Maximum source target. Software bound for the sweep target.");
    }

    fn source_protection_effective_note(&self, ui: &mut egui::Ui, unit: &str) {
        let Ok(preview) = &self.preview else {
            return;
        };
        let requested = self
            .settings
            .config
            .source_compliance
            .unwrap_or(self.settings.config.compliance);
        let differs = (preview.hardware_compliance - requested).abs()
            > preview.hardware_compliance.abs().max(requested.abs()) * f64::EPSILON * 8.0;
        let gs210_low_voltage_range = self.settings.source_model == "YOKOGAWA_GS210"
            && self.settings.config.mode == Mode::Iv
            && preview.source_range <= 0.1;
        let note = if gs210_low_voltage_range {
            format!(
                "GS210 10 mV / 100 mV range: fixed {} source current protection. The requested value cannot lower this hardware limiter.",
                fmt_quantity(preview.hardware_compliance, unit),
            )
        } else {
            format!(
                "Effective source protection: {}. Source range: {}.",
                fmt_quantity(preview.hardware_compliance, unit),
                fmt_quantity(
                    preview.source_range,
                    if self.settings.config.mode == Mode::Iv {
                        "V"
                    } else {
                        "A"
                    }
                ),
            )
        };
        ui.label(
            RichText::new(note)
                .small()
                .color(if differs || gs210_low_voltage_range {
                    ORANGE
                } else {
                    MUTED
                }),
        );
    }

    fn detail_panel(&mut self, root: &mut egui::Ui) {
        let width = if self.details { 288.0 } else { 40.0 };
        egui::Panel::right("details")
            .exact_size(width)
            .resizable(false)
            .frame(egui::Frame::new().fill(BG).inner_margin(8))
            .show_inside(root, |ui| {
                let height = ui.available_height();
                ui.horizontal_top(|ui| {
                    if ui
                        .add_sized(
                            [24.0, 28.0],
                            egui::Button::new(if self.details { ">" } else { "<" }),
                        )
                        .clicked()
                    {
                        self.details = !self.details;
                    }
                    if self.details {
                        ui.allocate_ui_with_layout(
                            Vec2::new(244.0, height),
                            egui::Layout::top_down(egui::Align::Min),
                            |ui| {
                                egui::Frame::new()
                                    .fill(INPUT)
                                    .stroke(Stroke::new(1.0_f32, BORDER))
                                    .corner_radius(3)
                                    .inner_margin(5)
                                    .show(ui, |ui| {
                                        egui::ScrollArea::vertical()
                                            .id_salt("activity_log")
                                            .auto_shrink([false, false])
                                            .max_height(216.0)
                                            .min_scrolled_height(216.0)
                                            .stick_to_bottom(true)
                                            .show(ui, |ui| {
                                                ui.set_width(232.0);
                                                for log in &self.logs {
                                                    ui.add(egui::Label::new(log).wrap());
                                                }
                                                if !self.state.error.is_empty() {
                                                    ui.add(
                                                        egui::Label::new(
                                                            RichText::new(
                                                                self.state.error.as_str(),
                                                            )
                                                            .color(RED),
                                                        )
                                                        .wrap(),
                                                    );
                                                }
                                                for warning in &self.state.warnings {
                                                    ui.add(egui::Label::new(warning).wrap());
                                                }
                                            });
                                    });
                                ui.add_space(8.0);
                                if ui
                                    .add_sized([110.0, 24.0], egui::Button::new("Field / Value"))
                                    .clicked()
                                {
                                    self.table_visible = !self.table_visible;
                                }
                                if self.table_visible {
                                    ui.add_space(8.0);
                                    let remaining = ui.available_height();
                                    egui::Frame::new()
                                        .fill(INPUT)
                                        .stroke(Stroke::new(1.0_f32, BORDER))
                                        .show(ui, |ui| {
                                            ui.set_min_height(remaining - 2.0);
                                            egui::ScrollArea::both()
                                                .id_salt("snapshot_fields")
                                                .auto_shrink([false, false])
                                                .show(ui, |ui| {
                                                    egui::Grid::new("snapshot_table")
                                                        .num_columns(2)
                                                        .min_row_height(24.0)
                                                        .spacing([8.0, 0.0])
                                                        .show(ui, |ui| {
                                                            ui.add_sized(
                                                                [111.0, 24.0],
                                                                egui::Label::new(
                                                                    RichText::new("Field").strong(),
                                                                ),
                                                            );
                                                            ui.add_sized(
                                                                [111.0, 24.0],
                                                                egui::Label::new(
                                                                    RichText::new("Value").strong(),
                                                                ),
                                                            );
                                                            ui.end_row();
                                                            for (index, field) in SNAPSHOT_FIELDS
                                                                .iter()
                                                                .chain(
                                                                    iv_core::DUAL_CSV_SOURCE_FIELDS,
                                                                )
                                                                .take(
                                                                    self.table_values
                                                                        .len()
                                                                        .max(SNAPSHOT_FIELDS.len()),
                                                                )
                                                                .enumerate()
                                                            {
                                                                ui.add_sized(
                                                                    [111.0, 24.0],
                                                                    egui::Label::new(*field)
                                                                        .truncate(),
                                                                )
                                                                .on_hover_text(*field);
                                                                let value = self
                                                                    .table_values
                                                                    .get(index)
                                                                    .map(String::as_str)
                                                                    .unwrap_or("—");
                                                                ui.add_sized(
                                                                    [111.0, 24.0],
                                                                    egui::Label::new(value)
                                                                        .truncate(),
                                                                )
                                                                .on_hover_text(value);
                                                                ui.end_row();
                                                            }
                                                        });
                                                });
                                        });
                                }
                            },
                        );
                    }
                });
            });
    }

    fn main_view(&mut self, ui: &mut egui::Ui) {
        ui.set_max_width((ui.clip_rect().right() - ui.cursor().left()).max(100.0));
        self.measurement_controls(ui);
        ui.add_space(8.0);
        let status = if self.busy() {
            format!(
                "{} ({:.0}s)",
                self.state.phase.to_lowercase(),
                self.phase_start.elapsed().as_secs_f64()
            )
        } else {
            self.state.phase.to_lowercase()
        };
        let point = if self.state.total == 0 {
            "—".into()
        } else {
            format!("{}/{}", self.state.points, self.state.total)
        };
        let fit = |value: Option<f64>| {
            value
                .filter(|r| r.is_finite() && *r != 0.0)
                .map(|r| (r, 1.0 / r))
        };
        let separate = self.separate_enabled();
        let channel_fit = (separate && self.state.phase == Phase::Completed)
            .then(|| self.separate.fit(self.settings.config.mode))
            .flatten();
        let source = fit(self.state.resistance);
        let meter = fit(self.state.reference_resistance);
        let detail = |name: &str, values: Option<(f64, f64)>| match values {
            Some((r, g)) => format!(
                "{name} sweep fit\nR: {r:.12e} Ω\nG: {g:.12e} S\nCSV R/G columns are per-point ratios."
            ),
            None => format!("{name} sweep fit: no valid fit available."),
        };
        // Preserve status and point counts on narrow panes instead of
        // truncating all four summary cells into one row.
        let cells = [
            (
                0.19,
                format!("Status {status}"),
                format!("{}\nOutput: {}", self.state.message, self.state.output),
            ),
            (
                0.12,
                format!("Point {point}"),
                format!("Sweep point {point}"),
            ),
            (
                0.345,
                format!(
                    "{} R/G {}",
                    if separate { "Channel" } else { "Source" },
                    if separate {
                        reference_view::channel_fit_text(channel_fit.as_ref())
                    } else {
                        format_resistance_conductance(source)
                    }
                ),
                if separate {
                    channel_fit
                        .as_ref()
                        .map(|f| {
                            format!(
                                "Channel free-intercept fit; {} points\nOffset: {:.12e} {}",
                                f.points,
                                f.offset,
                                if self.settings.config.mode == Mode::Vi {
                                    "V"
                                } else {
                                    "A"
                                }
                            )
                        })
                        .unwrap_or_else(|| "Channel fit is available after completion.".into())
                } else {
                    detail("Source", source)
                },
            ),
            (
                0.345,
                if separate {
                    self.reference_quantity()
                } else {
                    format!("Meter R/G {}", format_resistance_conductance(meter))
                },
                if separate {
                    "Separate circuit reading; no resistance or conductance is derived from it."
                        .into()
                } else {
                    detail("Meter", meter)
                },
            ),
        ];
        let narrow_summary = ui.available_width() < 520.0;
        for row in cells.chunks(if narrow_summary { 2 } else { 4 }) {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                let available = (ui.available_width() - 6.0 * (row.len() - 1) as f32).max(0.0);
                for (fraction, text, hint) in row {
                    let fraction = if narrow_summary { 0.5 } else { *fraction };
                    ui.add_sized(
                        [available * fraction, 20.0],
                        egui::Label::new(RichText::new(text.as_str()).size(11.0)).truncate(),
                    )
                    .on_hover_text(hint.as_str());
                }
            });
        }
        let feedback =
            if self.pending_bindings.is_some() || self.pending_inventory_selection.is_some() {
                "Applying device selection…"
            } else {
                self.device_change_notice.as_str()
            };
        ui.add_sized(
            [ui.available_width(), 17.0],
            egui::Label::new(RichText::new(feedback).size(11.0).color(MUTED)).truncate(),
        )
        .on_hover_text(feedback);
        ui.add_space(8.0);
        let [x_axis, y_axis] = measurement_definition(self.settings.config.mode).axes;
        let (x_unit, y_name, y_unit) = (x_axis.unit, y_axis.name, y_axis.unit);
        if self.settings.config.balanced_source.is_some() {
            let measured = dual_uses_meter_axis(
                self.state.reference_meter_connected,
                !self.samples.is_empty(),
                !self.reference_samples.is_empty(),
            );
            let height = shared_plot_height(ui.clip_rect().bottom() - ui.cursor().top() - 2.0);
            let (id, axis, samples, series, extent) = if measured {
                (
                    "dual_meter_plot",
                    "Meter voltage",
                    &self.reference_samples,
                    &self.reference_plot_series,
                    self.reference_plot_extent,
                )
            } else {
                (
                    "dual_source_plot",
                    "Source voltage (S1 − S2)",
                    &self.samples,
                    &self.plot_series,
                    self.plot_extent,
                )
            };
            self.render_sweep_plot(
                ui,
                id,
                axis,
                "",
                x_unit,
                y_name,
                y_unit,
                samples,
                series,
                extent,
                &[],
                &Default::default(),
                [0.0; 2],
                height,
                !measured,
            );
            return;
        }
        self.reference_header(ui);
        if self.separate_enabled() {
            self.separate_view(ui);
            return;
        }
        let available = shared_plot_height(ui.clip_rect().bottom() - ui.cursor().top() - 2.0);
        let (source_x_name, meter_x_name) = if self.settings.config.mode == Mode::Iv {
            ("V_source", "V_meter")
        } else {
            ("I_source", "I_meter")
        };
        // Overlay retains its real top Meter axis. The display header uses
        // the same reserved height with and without an assigned reference.
        self.render_sweep_plot(
            ui,
            "iv_plot",
            source_x_name,
            meter_x_name,
            x_unit,
            y_name,
            y_unit,
            &self.samples,
            &self.plot_series,
            self.plot_extent,
            &self.reference_samples,
            &self.reference_plot_series,
            self.reference_plot_extent,
            available,
            true,
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn render_sweep_plot(
        &self,
        ui: &mut egui::Ui,
        id: &'static str,
        source_x_name: &str,
        meter_x_name: &str,
        x_unit: &str,
        y_name: &str,
        y_unit: &str,
        samples: &[(f64, f64, String, [Option<f64>; 2])],
        series: &[Vec<PlotPoint>; 3],
        mut extent: [f64; 2],
        reference_samples: &[(f64, f64, String, [Option<f64>; 2])],
        reference_series: &[Vec<PlotPoint>; 3],
        reference_extent: [f64; 2],
        height: f32,
        use_sweep_extent_when_empty: bool,
    ) {
        if samples.is_empty() && use_sweep_extent_when_empty {
            extent[0] = self
                .settings
                .config
                .start
                .abs()
                .max(self.settings.config.stop.abs());
        }
        for (axis, value) in reference_extent.into_iter().enumerate() {
            extent[axis] = extent[axis].max(value);
        }
        let plot_id = ui.make_persistent_id(egui::Id::new(id));
        let view = (
            plot_id,
            self.settings.config.mode,
            self.run_id,
            self.gate_metric,
            self.gate_summary_revision,
        );
        ui.ctx().data_mut(|data| {
            let key = ui.make_persistent_id(("sweep_view_context", id));
            let active_key = ui.make_persistent_id("active_sweep_chart");
            let chart = if id.starts_with("separate_") {
                "separate"
            } else {
                id
            };
            if data.get_temp::<&'static str>(active_key) != Some(chart)
                || data.get_temp::<(egui::Id, Mode, u64, reference_view::GateMetric, u64)>(key)
                    != Some(view)
            {
                // Plot::auto_bounds sets the default, but retained manual zoom
                // overrides it. A new mode, chart or run starts a fresh view.
                data.remove::<PlotMemory>(plot_id);
                data.insert_temp(key, view);
                data.insert_temp(active_key, chart);
            }
        });
        if let Some(memory) = PlotMemory::load(ui.ctx(), plot_id) {
            for (axis, maximum) in extent.iter_mut().enumerate() {
                if !memory.auto_bounds[axis] {
                    let visible = memory.bounds().min()[axis]
                        .abs()
                        .max(memory.bounds().max()[axis].abs());
                    if visible.is_finite() && visible > 0.0 {
                        *maximum = visible;
                    }
                }
            }
        }
        let x_scale = AxisScale::for_magnitude(extent[0]);
        let y_scale = AxisScale::for_magnitude(extent[1]);
        let top_scale = x_scale;
        let meter_label = match (
            self.settings.reference_meter_id,
            reference_samples.is_empty(),
        ) {
            (None, _) => format!("{meter_x_name} (not assigned)"),
            (Some(_), true) => format!("{meter_x_name} (waiting)"),
            (Some(_), false) => meter_x_name.to_owned(),
        };
        let mut x_axes = vec![
            AxisHints::new_x()
                .label(format!("{source_x_name} ({}{x_unit})", x_scale.prefix))
                .placement(VPlacement::Bottom)
                .formatter(move |mark, _| x_scale.tick(mark.value, mark.step_size)),
        ];
        if !meter_x_name.is_empty() {
            x_axes.push(
                AxisHints::new_x()
                    .label(format!("{meter_label} ({}{x_unit})", top_scale.prefix))
                    .placement(VPlacement::Top)
                    .formatter(move |mark, _| top_scale.tick(mark.value, mark.step_size)),
            );
        }
        let mut plot = Plot::new(id)
            .width((ui.clip_rect().right() - ui.cursor().left()).max(100.0))
            .id(plot_id)
            .height(height)
            .show_background(false)
            .grid_spacing(16.0..=12000.0)
            .custom_x_axes(x_axes)
            .y_axis_label(format!("{y_name} ({}{y_unit})", y_scale.prefix))
            .y_axis_formatter(move |mark, _| y_scale.tick(mark.value, mark.step_size))
            .label_formatter(move |name, point| {
                // Legend names group by direction. Identify the actual point
                // from each dataset so tooltip quantities keep their meaning.
                let source = samples
                    .iter()
                    .rev()
                    .find(|sample| sample.0 == point.x && sample.1 == point.y);
                let measured = reference_samples
                    .iter()
                    .rev()
                    .find(|sample| sample.0 == point.x && sample.1 == point.y);
                let mut text = name.to_owned();
                if let Some(sample) = source {
                    if id.starts_with("separate_gate_") {
                        text.push_str(&format!("\nRun: {}", sample.2));
                    }
                    text.push_str(&format!(
                        "\n{source_x_name}: {}",
                        display_reading(point.x, sample.3[0], x_unit)
                    ));
                }
                if let Some(sample) = measured {
                    text.push_str(&format!(
                        "\n{meter_x_name}: {}",
                        display_reading(point.x, sample.3[0], x_unit)
                    ));
                }
                if source.is_none() && measured.is_none() {
                    text.push_str(&format!(
                        "\n{x_unit}: {}",
                        display_reading(point.x, None, x_unit)
                    ));
                }
                let resolution = measured.or(source).and_then(|sample| sample.3[1]);
                text.push_str(&format!(
                    "\n{y_name}: {}",
                    display_reading(point.y, resolution, y_unit)
                ));
                text
            })
            .legend(Legend::default().background_alpha(0.45))
            .allow_boxed_zoom(true)
            .allow_scroll(false)
            .allow_drag(true)
            .include_x(0.0)
            .auto_bounds(true);
        if id != "separate_reference" && id != "separate_gate_reference" {
            plot = plot.include_y(0.0);
        }
        if id.starts_with("separate_") {
            let group = ui.make_persistent_id((
                "separate_x",
                self.run_id,
                self.settings.config.mode == Mode::Vi,
                self.gate_metric,
                self.gate_summary_revision,
            ));
            plot = plot
                .include_x(-extent[0])
                .include_x(extent[0])
                .link_axis(group, [true, false])
                .link_cursor(group, [true, false]);
        }
        let plotted = plot.show(ui, |plot_ui| {
            if self.smoke
                && self.run_id >= 2
                && id == "separate_primary"
                && std::env::args().any(|arg| arg == "--gui-linked-axes-smoke")
            {
                // Exercise manual x bounds through the production link group.
                plot_ui.set_plot_bounds_x(-2e-6..=4e-6);
            }
            draw_sweep_series(
                plot_ui,
                series,
                false,
                if id.starts_with("separate_") {
                    y_name
                } else {
                    ""
                },
                !id.starts_with("separate_gate_"),
            );
            // The two x quantities intentionally share raw SI bounds. Do
            // not remap or normalize the reference curve: differences are
            // scientific signal, not UI decoration.
            if !reference_samples.is_empty() {
                draw_sweep_series(plot_ui, reference_series, true, "", true);
            }
        });
        if self.smoke || id.starts_with("separate_") {
            let bounds = plotted.transform.bounds();
            ui.ctx().data_mut(|data| {
                data.insert_temp(
                    egui::Id::new(("plot_bounds", id)),
                    [
                        bounds.min()[0],
                        bounds.min()[1],
                        bounds.max()[0],
                        bounds.max()[1],
                    ],
                )
            });
            let frame = plotted.transform.frame();
            ui.ctx().data_mut(|data| {
                data.insert_temp(
                    egui::Id::new("reference_plot_bounds"),
                    [frame.min.x, frame.min.y, frame.max.x, frame.max.y],
                )
            });
        }
        if self.smoke || id.starts_with("separate_") {
            let bounds = plotted.transform.bounds();
            ui.ctx().data_mut(|data| {
                data.insert_temp(
                    egui::Id::new("sweep_data_bounds"),
                    [
                        bounds.min()[0],
                        bounds.min()[1],
                        bounds.max()[0],
                        bounds.max()[1],
                    ],
                );
            });
        }
        if let Some(memory) = PlotMemory::load(ui.ctx(), plot_id) {
            for (axis, scale) in [x_scale, y_scale].into_iter().enumerate() {
                if !memory.auto_bounds[axis] {
                    let bounds = plotted.transform.bounds();
                    let visible = bounds.min()[axis].abs().max(bounds.max()[axis].abs());
                    if AxisScale::for_magnitude(visible) != scale {
                        ui.ctx().request_repaint();
                    }
                }
            }
        }
    }
}

impl IvApp {
    fn footer_smoke_capture(&mut self, ctx: &egui::Context) {
        let elapsed = self.started.elapsed();
        let Some(path) = &self.screenshot else {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        };
        if elapsed > Duration::from_secs(1) && !self.screenshot_requested {
            self.screenshot_requested = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
        }
        if let Some(image) = ctx.input(|input| {
            input.events.iter().find_map(|event| {
                if let egui::Event::Screenshot { image, .. } = event {
                    Some(image.clone())
                } else {
                    None
                }
            })
        }) {
            let bytes = image
                .pixels
                .iter()
                .flat_map(|pixel| pixel.to_array())
                .collect();
            self.screenshot_saved =
                image::RgbaImage::from_raw(image.width() as u32, image.height() as u32, bytes)
                    .is_some_and(|buffer| buffer.save(path).is_ok());
            if self.screenshot_saved && !self.success.swap(true, Ordering::AcqRel) {
                println!(
                    "{}",
                    serde_json::json!({
                        "footer_geometry": self.footer_geometry,
                        "cards": self.card_geometry_report(),
                        "two_columns": self.settings.two_card_columns && ctx.content_rect().width() >= 1380.0,
                        "window_bottom": ctx.content_rect().bottom(),
                        "points": self.state.points,
                    })
                );
            }
        }
        if self.screenshot_saved || elapsed > Duration::from_secs(12) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        } else {
            ctx.request_repaint_after(Duration::from_millis(30));
        }
    }

    fn logic_tick(&mut self, ctx: &egui::Context) {
        if self.reference_replay {
            ctx.request_repaint_after(Duration::from_millis(30));
            return;
        }
        if self.smoke {
            let width = ctx.content_rect().width();
            let minimum = if self.settings.two_card_columns {
                1500.0
            } else {
                1080.0
            };
            if width > 0.0 && width < minimum - 0.5 {
                // Hosted Windows desktops can clamp native windows below the
                // supported minimum. Preserve the tested logical layout.
                ctx.set_zoom_factor(ctx.zoom_factor() * width / minimum);
            }
        }
        self.poll_file_task(ctx);
        self.sync();
        // Restore the measurement live mode after manual output ends.
        let control_mode = self.settings.config.mode;
        if !self.busy()
            && !self.python_locked()
            && self.state.error.is_empty()
            && self.state.source_connected
            && self.state.output == OutputState::Off
            && self
                .live_reading
                .as_ref()
                .is_some_and(|r| r.mode != control_mode)
        {
            self.live_samples.clear();
            self.live_reading = None;
            self.send(Command::SetLiveMode { mode: control_mode });
        }

        self.external_control();
        let close_requested = ctx.input(|input| input.viewport().close_requested());
        if close_requested
            && let Some(action) = close_action(self.active(), self.file_copy_in_progress())
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            match action {
                CloseAction::Stop if self.send(Command::Stop) => {
                    self.closing = true;
                    self.notice = "Stopping and cleaning up before closing…".into();
                }
                CloseAction::FinishFileCopy => {
                    self.closing = true;
                    self.notice = "Finishing the measurement copy before closing…".into();
                }
                CloseAction::Stop => {}
            }
        }
        if self.closing && !self.active() && !self.file_copy_in_progress() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        ctx.request_repaint_after(Duration::from_millis(100));
        if self.busy() {
            ctx.request_repaint_after(Duration::from_millis(33));
        }
    }
}

impl eframe::App for IvApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.logic_tick(ctx);
    }

    fn ui(&mut self, root: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = root.ctx().clone();
        let ui_start = Instant::now();
        if self.first_frame_s.is_none() {
            self.first_frame_s = Some(self.boot.elapsed().as_secs_f64());
        }
        self.python_banner(root);
        self.sidebar(root);
        self.detail_panel(root);
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(BG).inner_margin(8))
            .show_inside(root, |ui| self.main_view(ui));
        // Reproducible Settings screenshots use the existing simulator smoke path.
        if self.smoke
            && self.state.phase == Phase::Completed
            && !self.timing_open
            && !self.screenshot_requested
            && std::env::args().any(|arg| arg == "--gui-settings-smoke")
        {
            self.timing_before = Some((self.settings.clone(), self.custom_text.clone()));
            self.timing_open = true;
        }
        self.timing_dialog(&ctx);
        self.add_device_dialog(&ctx);
        self.delete_device_dialog(&ctx);
        if self.protection_dialog.is_some() {
            self.protection_dialog(&ctx);
        } else {
            self.start_error_dialog(&ctx);
        }
        self.save_dialog(&ctx);
        if self.reference_replay {
            self.reference_replay_capture(&ctx);
            return;
        }
        if self.footer_smoke_offset.is_some() {
            self.footer_smoke_capture(&ctx);
            return;
        }
        if self.smoke && !self.smoke_started && self.started.elapsed() > Duration::from_millis(100)
        {
            self.smoke_started = true;
            if self.settings.simulation
                && (self.settings.config.balanced_source.is_some()
                    || self.settings.reference_meter_id.is_some())
            {
                self.connect_all();
            } else {
                self.send(Command::Connect {
                    source_model: self.settings.config.source_model.clone(),
                    meter_model: self.settings.config.meter_model.clone(),
                    simulation: self.settings.simulation,
                    source: self.settings.source.clone(),
                    meter: self.settings.meter.clone(),
                });
            }
        }
        let control_smoke = self.smoke && std::env::args().any(|arg| arg == "--gui-control-smoke");
        let dc_smoke = self.smoke && std::env::args().any(|arg| arg == "--gui-dc-smoke");
        let inventory_smoke =
            self.smoke && std::env::args().any(|arg| arg == "--gui-inventory-smoke");
        if dc_smoke {
            self.dc_smoke_step();
        } else if inventory_smoke {
            self.inventory_smoke_step();
        }

        if self.smoke
            && !control_smoke
            && !dc_smoke
            && !inventory_smoke
            && self.state.connected
            && (self.settings.config.reference_meter.is_none()
                || self.state.reference_meter_connected)
            && (self.settings.config.balanced_source.is_none() || self.state.source2_connected)
            && self.state.run_id == 0
            && !self.pending_run
        {
            self.start_run();
        }
        let separate_smoke =
            self.smoke && std::env::args().any(|arg| arg == "--gui-separate-smoke");
        if separate_smoke
            && self.state.phase == Phase::Completed
            && self.separate_smoke_first.is_none()
        {
            self.separate_smoke_first = Some([self.samples.len(), self.separate.samples.len()]);
            // A rejected GUI Start must retain both acquired histories.
            let step = self.settings.config.step;
            self.settings.config.step = 0.0;
            self.start_run();
            assert_eq!(self.samples.len(), self.separate_smoke_first.unwrap()[0]);
            assert_eq!(
                self.separate.samples.len(),
                self.separate_smoke_first.unwrap()[1]
            );
            assert!(!self.pending_run);
            self.start_error = None;
            self.settings.config.step = step / 100.0;
            self.settings.config.start /= 100.0;
            self.settings.config.stop /= 100.0;
            self.start_run();
        }
        let limit_smoke = self.smoke && std::env::args().any(|arg| arg == "--gui-limit-smoke");
        let smoke_outcome = if dc_smoke {
            self.dc_smoke_stage == 10
                && self.state.phase == Phase::Completed
                && self.state.source_dc.is_none()
                && self.state.gate_output == OutputState::On
                && self.state.points == self.state.total
        } else if inventory_smoke {
            self.dc_smoke_stage == 5
                && self.state.phase == Phase::Completed
                && self.state.points == self.state.total
        } else if control_smoke {
            self.state.run_id >= 2
                && self.state.points == self.state.total
                && self.state.gate_connected
                && self.state.gate_output == OutputState::Off
                && !self
                    .control
                    .as_ref()
                    .is_some_and(|control| control.has_lease())
        } else if limit_smoke {
            self.state.phase == Phase::ComplianceStop
                && self
                    .state
                    .protection_alert
                    .as_ref()
                    .is_some_and(|alert| alert.kind == ProtectionKind::MeasurementLimit)
        } else {
            self.state.phase == Phase::Completed
                && (!separate_smoke || self.state.run_id >= 2)
                && self.state.points == self.state.total
                && (!std::env::args().any(|arg| arg == "--gui-reference-smoke")
                    || self
                        .state
                        .last
                        .as_ref()
                        .and_then(|record| record.reference_meter_value)
                        .is_some())
        };
        if self.screenshot.is_some()
            && !self.screenshot_requested
            && self.started.elapsed() > Duration::from_secs(2)
            && (!self.smoke
                || smoke_outcome
                || (dc_smoke
                    && self.dc_smoke_stage == 6
                    && self.state.output == OutputState::On
                    && std::env::args().any(|arg| arg == "--screenshot-dc-on")))
        {
            self.screenshot_requested = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
        }
        if let Some(path) = &self.screenshot {
            for event in ctx.input(|i| i.raw.events.clone()) {
                if let egui::Event::Screenshot { image, .. } = event {
                    let bytes: Vec<u8> = image.pixels.iter().flat_map(|p| p.to_array()).collect();
                    if let Some(buffer) = image::RgbaImage::from_raw(
                        image.width() as u32,
                        image.height() as u32,
                        bytes,
                    ) {
                        if let Some(parent) = path.parent() {
                            let _ = std::fs::create_dir_all(parent);
                        }
                        self.screenshot_saved = buffer.save(path).is_ok();
                    }
                }
            }
        }
        if self.smoke {
            ctx.request_repaint_after(Duration::from_millis(30));
            if smoke_outcome
                && self.state.output == OutputState::Off
                && (self.settings.config.balanced_source.is_none()
                    || self.state.source2_output == OutputState::Off)
                && self.state.error.is_empty()
                && self.started.elapsed() > Duration::from_secs(4)
                && (self.screenshot.is_none() || self.screenshot_saved)
            {
                if !self.success.swap(true, Ordering::AcqRel) {
                    println!(
                        "{}",
                        serde_json::json!({"gui_smoke":"passed","points":self.state.points,"output":self.state.output,"elapsed_s":self.started.elapsed().as_secs_f64(),"csv":self.state.path,
                    "plot_data_bounds":ctx.data(|data|data.get_temp::<[f64;4]>(egui::Id::new("sweep_data_bounds"))),
                    "mode":self.settings.config.mode,
                    "reference_display":self.settings.reference_display,
                    "run_id":self.run_id,"history_points":self.samples.len(),"separate_points":self.separate.samples.iter().filter(|p|p.1.is_finite()).count(),"first_run_histories":self.separate_smoke_first,
                    "stop_visible":ctx.data(|data|data.get_temp::<bool>(egui::Id::new("run_stop_visible"))),
                    "measurement_controls_visible":ctx.data(|data|data.get_temp::<bool>(egui::Id::new("measurement_controls_visible"))),
                    "measurement_controls_bounds":ctx.data(|data|data.get_temp::<[f32;4]>(egui::Id::new("measurement_controls_bounds"))),
                    "separate_primary_bounds":ctx.data(|data|data.get_temp::<[f64;4]>(egui::Id::new(("plot_bounds", "separate_primary")))),
                    "separate_reference_bounds":ctx.data(|data|data.get_temp::<[f64;4]>(egui::Id::new(("plot_bounds", "separate_reference")))),
                    "two_card_columns":self.settings.two_card_columns && ctx.content_rect().width() >= 1380.0,"plot_bounds":ctx.data(|data|data.get_temp::<[f32;4]>(egui::Id::new("reference_plot_bounds"))),"reference_meter_assigned":self.settings.reference_meter_id.is_some(),"first_ui_update_s":self.first_frame_s,"mean_ui_cpu_ms":self.ui_cpu_s*1000.0/self.ui_frames.max(1) as f64})
                    );
                }
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            if self.started.elapsed() > Duration::from_secs(if control_smoke { 60 } else { 20 }) {
                eprintln!("GUI smoke failed: {}", self.state.error);
                self.send(Command::Stop);
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
        // Active cards edit the legacy role fields directly; retain those
        // edits in the selected inventory entries before the next frame.
        self.settings.store_active_entries();
        self.ui_cpu_s += ui_start.elapsed().as_secs_f64();
        self.ui_frames += 1;
    }
}

/// Three display digits keep the summary readable; CSV and fit values are untouched.
fn compact_fit_quantity(value: f64, unit: &str) -> String {
    if !value.is_finite() {
        return "—".into();
    }
    if value == 0.0 {
        return format!("0 {unit}");
    }
    let mut scale = AxisScale::for_magnitude(value.abs());
    if (value / scale.factor).abs() >= 999.5 {
        scale = AxisScale::for_magnitude(value.abs() * 1.001);
    }
    let scaled = value / scale.factor;
    let decimals = (2.0 - (scaled.abs().log10() + 1e-12).floor()).clamp(0.0, 6.0) as usize;
    format!("{scaled:.decimals$} {}{unit}", scale.prefix)
}

fn format_resistance_conductance(values: Option<(f64, f64)>) -> String {
    values
        .map(|(resistance, conductance)| {
            format!(
                "{} / {}",
                compact_fit_quantity(resistance, "Ω"),
                compact_fit_quantity(conductance, "S")
            )
        })
        .unwrap_or_else(|| "—".into())
}

/// Draw source and reference measurements with direction encoded by color.
/// The reference line is dotted so it remains distinguishable without adding
/// a second, potentially misleading axis scale.
fn draw_sweep_series<'a>(
    plot_ui: &mut PlotUi<'a>,
    series: &'a [Vec<PlotPoint>; 3],
    reference: bool,
    quantity: &str,
    show_direction: bool,
) {
    for (values, (direction, color)) in
        series
            .iter()
            .zip([("forward", BLUE), ("backward", PURPLE), ("custom", TEAL)])
    {
        for segment in values
            .split(|point| !point.x.is_finite() || !point.y.is_finite())
            .filter(|segment| !segment.is_empty())
        {
            let name = if quantity.is_empty() {
                direction.to_owned()
            } else if show_direction {
                format!("{quantity} · {direction}")
            } else {
                quantity.to_owned()
            };
            let style = if reference {
                LineStyle::dotted_dense()
            } else if direction == "backward" {
                LineStyle::dashed_dense()
            } else {
                LineStyle::Solid
            };
            plot_ui.line(
                Line::new(name.clone(), PlotPoints::Borrowed(segment))
                    .color(color)
                    .width(if direction == "forward" {
                        3.0_f32
                    } else {
                        2.0_f32
                    })
                    .style(style),
            );
            if values.len() <= 2_000 {
                plot_ui.points(
                    Points::new(name, PlotPoints::Borrowed(segment))
                        .color(color)
                        .radius(if direction == "backward" {
                            4.0_f32
                        } else {
                            2.5_f32
                        })
                        .shape(if reference {
                            MarkerShape::Square
                        } else if direction == "backward" {
                            MarkerShape::Diamond
                        } else {
                            MarkerShape::Circle
                        })
                        .filled(!reference && direction != "backward"),
                );
            }
        }
    }
}

fn protection_role(alert: &ProtectionAlert) -> bool {
    alert.kind == ProtectionKind::SourceCompliance
}

fn protection_summary(alert: &ProtectionAlert) -> (&'static str, &'static str) {
    match alert.kind {
        ProtectionKind::SourceCompliance if alert.indicator.contains("EMR") => (
            "Source protection triggered",
            "Check the instrument and its temperature. Clear the emergency condition before restarting.",
        ),
        ProtectionKind::SourceCompliance if alert.source_index == Some(2) => (
            "Source 2 compliance reached",
            "Check wiring and contacts. Reduce the differential sweep range or review Source 2 compliance.",
        ),
        ProtectionKind::SourceCompliance => (
            "Source compliance reached",
            "Check wiring and contacts. Reduce the sweep range or review Source compliance.",
        ),
        ProtectionKind::MeasurementLimit => (
            "Meter limit reached",
            "Reduce the sweep range or review Meter limit.",
        ),
    }
}

fn protection_message(alert: &ProtectionAlert, output: OutputState) -> String {
    let output_status = if output == OutputState::Off {
        "Measurement stopped. Source output OFF verified."
    } else {
        "Measurement stopped. Source output OFF could not be verified. Turn output OFF on the instrument and check the connections before continuing."
    };
    let limit = fmt_quantity(alert.limit, &alert.unit);
    let detail = match alert.kind {
        ProtectionKind::SourceCompliance => {
            let reason = if alert.indicator.contains("EMR") {
                "Source emergency protection triggered. Check the instrument and its temperature before restarting."
            } else {
                "Source compliance reached. The source may no longer deliver the requested output."
            };
            format!(
                "{reason}\n\nCurrent source compliance: {limit}\nInstrument status: {}\n\nCheck the sample, current-path contacts and wiring. Four-terminal Meter voltage can differ from the source terminal voltage. Reduce the sweep range, or review Source card > Device > Compliance. Increase compliance only if the sample and wiring ratings permit it.",
                alert.indicator
            )
        }
        ProtectionKind::MeasurementLimit => {
            let reading = alert
                .observed
                .map(|value| fmt_quantity(value, &alert.unit))
                .unwrap_or_else(|| "Unavailable".into());
            format!(
                "Measurement limit reached.\n\nCurrent Meter limit: {limit}\nTrigger reading: {reading}\n\nCheck the sample and wiring. Reduce the sweep range, or review Meter card > Device > Limit. Increase the limit only if safe for the sample. Changing this limit does not change Source compliance."
            )
        }
    };
    format!(
        "{detail}\n\n{output_status}\n\nThe run will not resume automatically. Review the settings, then press Start to begin a new run."
    )
}

/// Preserve the previous hardware protection when editing a legacy profile's
/// coupled meter threshold. Editing Meter limit must never raise Source compliance.
fn card_mode(sweep: Mode, running: bool, dc: Option<Mode>, editor: Option<Mode>) -> Mode {
    if running {
        sweep
    } else {
        dc.or(editor).unwrap_or(sweep)
    }
}

fn preserve_source_on_meter_edit(settings: &mut Settings, previous: f64, effective_source: f64) {
    preserve_source_on_meter_edit_for_mode(
        settings,
        settings.config.mode,
        previous,
        effective_source,
    );
}

fn preserve_source_on_meter_edit_for_mode(
    settings: &mut Settings,
    mode: Mode,
    previous: f64,
    effective_source: f64,
) {
    let (meter, source) = match mode {
        Mode::Iv => (
            settings.current_compliance,
            &mut settings.source_current_protection,
        ),
        Mode::Vi => (
            settings.voltage_compliance,
            &mut settings.source_voltage_protection,
        ),
    };
    if meter != previous && source.is_none() {
        *source = Some(effective_source);
    }
}

fn parse_custom_points(text: &str) -> Result<Vec<f64>, String> {
    text.split(',')
        .enumerate()
        .map(|(index, value)| {
            value
                .trim()
                .parse::<f64>()
                .ok()
                .filter(|value| value.is_finite())
                .ok_or_else(|| {
                    format!("Point {}: enter a finite number between commas.", index + 1)
                })
        })
        .collect()
}

fn start_block_reason(config: &RunConfig, connected: bool, matching_modes: bool) -> Option<String> {
    if !connected {
        return Some("Connect both Source and Meter before starting. Click Refresh, select the correct resources, then click All Connect.".into());
    }
    if !matching_modes {
        return Some("Source and Meter must both use real instruments, or both use simulation. Check the Device selections and reconnect.".into());
    }
    iv_core::plan(config).err().map(|reason| {
        let help = if reason.contains("must include zero") {
            "For zero_centered, use endpoints on opposite sides of zero, or set one endpoint to zero. Otherwise choose linear or round_trip. The source will not be enabled."
        } else if reason.contains("exceeds effective limit") {
            "Reduce Start / End, or review Max. target in Source card > Device. Increase a limit only when it is safe for your setup."
        } else if reason.contains("must be a multiple") {
            "Set Start / End to whole multiples of the source resolution stated in the error. Step and ramp are adjusted automatically."
        } else if reason.starts_with("Source 1 compliance ") {
            "Review Source 1 > Device > Compliance. If hardware protection follows the Meter limit, set a separate Source compliance only if the instrument can provide the required protection. The Meter limit is a separate run-stop threshold."
        } else if reason.starts_with("Source 2 compliance ") {
            "Review Source 2 > Device > Compliance. If the instrument cannot provide the required protection, use a source that supports it."
        } else if reason.contains("points") {
            "Increase Step or reduce the sweep span."
        } else if reason.contains("GS210 range") {
            "Reduce Start / End to values supported by the selected source."
        } else {
            "Check Measurement Settings for an invalid value."
        };
        format!("{reason}\n\n{help}")
    })
}

fn sweep_preview(config: &RunConfig) -> Result<SweepPreview, String> {
    // Planning is the single source of truth for target order and exact
    // endpoints. This function is called only when settings change.
    let plan = iv_core::plan(config)?;
    let effective = iv_core::prepare_config(config)?;
    Ok(SweepPreview {
        effective_step: effective.step,
        effective_ramp: effective.ramp_step,
        requested_step: config.step,
        planned_points: plan.points.len(),
        source_range: plan.source_range,
        hardware_compliance: plan.hardware_compliance,
        route: route_vertices(&plan.points),
        unit: if effective.mode == Mode::Vi { "A" } else { "V" },
        custom: effective.pattern == Pattern::CustomList,
    })
}

/// Return the start, every direction-change target, and the final target.
/// The UI turns an arbitrarily long planned point list into a compact route,
/// while retaining the exact targets that are sent at each turning point.
fn route_vertices(points: &[iv_core::SweepPoint]) -> Vec<f64> {
    let Some(first) = points.first() else {
        return Vec::new();
    };
    let mut vertices = vec![first.target];
    let mut previous_sign = 0_i8;
    for index in 1..points.len() {
        let delta = points[index].target - points[index - 1].target;
        let sign = if delta > 0.0 {
            1
        } else if delta < 0.0 {
            -1
        } else {
            0
        };
        if sign != 0 {
            if previous_sign != 0 && sign != previous_sign {
                let turning = points[index - 1].target;
                if vertices.last().copied() != Some(turning) {
                    vertices.push(turning);
                }
            }
            previous_sign = sign;
        }
    }
    let last = points.last().expect("non-empty points").target;
    // Preserve an explicit return to the starting target after a turn, but
    // avoid displaying a duplicate for a static custom list.
    if vertices.len() > 1 || vertices[0] != last {
        vertices.push(last);
    }
    vertices
}

fn mode_name(mode: Mode) -> &'static str {
    measurement_definition(mode).label
}

fn pattern_name(pattern: Pattern) -> &'static str {
    sweep_definitions()
        .iter()
        .find(|definition| definition.pattern == pattern)
        .expect("every built-in pattern has a definition")
        .label
}

/// The optional reference curve shares the one scientific plot; assignment
/// changes axes and data only, never the central panel's plot allocation.
fn shared_plot_height(available: f32) -> f32 {
    available.max(180.0)
}

#[cfg(test)]
impl SweepPreview {
    fn route_text(&self) -> String {
        let label = if self.custom {
            format!("Custom route ({} targets)", self.planned_points)
        } else {
            "Route".to_owned()
        };
        let route = if self.route.len() <= 4 {
            self.route
                .iter()
                .map(|value| fmt_quantity(*value, self.unit))
                .collect::<Vec<_>>()
                .join(" -> ")
        } else {
            let first = fmt_quantity(self.route[0], self.unit);
            let last = fmt_quantity(*self.route.last().expect("non-empty route"), self.unit);
            format!("{first} -> … -> {last}")
        };
        format!("{label}: {route}")
    }
}

fn model_supported(model: &str) -> bool {
    instrument_descriptor(model).is_some()
}

fn device_summary(model: &str) -> &str {
    if model.starts_with("SIMULATED_") {
        "Simulation"
    } else {
        model.split_once('_').map_or(model, |(_, name)| name)
    }
}

/// Validate an explicit launch profile before creating any GUI/engine owner.
pub fn validate_startup_profile(path: &Path) -> Result<(), String> {
    settings::load_settings(
        &std::fs::read(path).map_err(|e| format!("Could not read GUI profile: {e}"))?,
    )
    .map(|_| ())
}

/// Explicit profile launch and API loading follow the existing GUI's legacy
/// Simulation flag normalization, without opening any device.
fn load_gui_profile(bytes: &[u8]) -> Result<Settings, String> {
    let mut settings = settings::load_settings(bytes)?;
    if settings.simulation {
        settings.source_model = "SIMULATED_SOURCE".into();
        settings.meter_model = "SIMULATED_METER".into();
        if let Some(id) = settings.reference_meter_id
            && let Some(entry) = settings.meters.iter_mut().find(|entry| entry.id == id)
        {
            entry.model = "SIMULATED_METER".into();
        }
        if settings.config.balanced_source.is_some() {
            settings.source2_model = "SIMULATED_SOURCE".into();
        }
        settings.store_active_entries();
    }
    // Match the active role binding before validating the RunConfig. This
    // prevents a profile from being validated for one model and executed by
    // another when several requests arrive in the same GUI frame.
    settings.config.source_model = if settings.source_model == "SIMULATED_SOURCE" {
        "YOKOGAWA_GS210".into()
    } else {
        settings.source_model.clone()
    };
    settings.config.meter_model = if settings.meter_model == "SIMULATED_METER" {
        "AGILENT_34411A".into()
    } else {
        settings.meter_model.clone()
    };
    if let Some(dual) = &mut settings.config.balanced_source {
        dual.source_model = if settings.source2_model == "SIMULATED_SOURCE" {
            "YOKOGAWA_GS210".into()
        } else {
            settings.source2_model.clone()
        };
    }
    Ok(settings)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_copy_keeps_dots_and_recognizes_existing_csv_extension() {
        let source = PathBuf::from("run.csv");
        for (name, expected) in [
            ("sample_4.2K", "sample_4.2K.csv"),
            ("sample_4.2K.CSV", "sample_4.2K.CSV"),
            ("sample", "sample.csv"),
        ] {
            let (_, path, _) =
                selected_save_destination(source.clone(), Some(PathBuf::from(name)), 2).unwrap();
            assert_eq!(path, Path::new(expected));
        }
    }

    #[test]
    fn dual_axis_follows_meter_data_and_keeps_saved_points_visible() {
        assert!(!dual_uses_meter_axis(false, false, false));
        assert!(dual_uses_meter_axis(true, false, false));
        assert!(!dual_uses_meter_axis(false, true, false));
        assert!(!dual_uses_meter_axis(true, true, false));
        assert!(dual_uses_meter_axis(true, true, true));
        assert!(dual_uses_meter_axis(false, true, true));
    }

    #[test]
    fn measurement_controls_fit_narrow_and_wide_center_panes() {
        let context = egui::Context::default();
        let creation = eframe::CreationContext::_new_kittest(context.clone());
        let mut app = IvApp::new(
            &creation,
            true,
            None,
            Arc::new(AtomicBool::new(false)),
            Instant::now(),
            None,
            None,
        )
        .unwrap();
        app.settings.filename = "sample_4.2K_long_measurement_name".into();
        for mode in [Mode::Iv, Mode::Vi] {
            app.settings.config.mode = mode;
            for width in [384.0, 400.0, 560.0, 760.0] {
                // Two passes let Grid resolve its cached column sizes.
                for _ in 0..2 {
                    let _ = context.run_ui(egui::RawInput::default(), |ui| {
                        ui.set_width(width);
                        ui.set_clip_rect(egui::Rect::from_min_size(
                            ui.cursor().min,
                            Vec2::new(width, 650.0),
                        ));
                        app.measurement_controls(ui);
                    });
                }
                assert!(
                    context
                        .data(|d| d.get_temp::<bool>(egui::Id::new("measurement_controls_visible")))
                        .unwrap(),
                    "Measurement overflow at {width} px, mode {mode:?}"
                );
                assert!(
                    context
                        .data(|d| d.get_temp::<bool>(egui::Id::new("run_stop_visible")))
                        .unwrap()
                );
            }
        }
    }

    #[test]
    fn sweep_plot_refits_on_mode_run_and_dual_chart_changes_but_keeps_current_zoom() {
        let context = egui::Context::default();
        let creation_context = eframe::CreationContext::_new_kittest(context.clone());
        let mut app = IvApp::new(
            &creation_context,
            true,
            None,
            Arc::new(AtomicBool::new(false)),
            Instant::now(),
            None,
            None,
        )
        .unwrap();
        // Use the production renderer, including its plot identity and memory.
        let render = |app: &IvApp, id: &'static str, x: f64, y: f64| {
            let samples = vec![
                (-x, -y, "forward".into(), [None, None]),
                (x, y, "forward".into(), [None, None]),
            ];
            let series = plot_series(&samples);
            let mut actual_id = egui::Id::NULL;
            let _ = context.run_ui(egui::RawInput::default(), |ui| {
                actual_id = ui.make_persistent_id(egui::Id::new(id));
                app.render_sweep_plot(
                    ui,
                    id,
                    "Source",
                    "",
                    "A",
                    "Voltage",
                    "V",
                    &samples,
                    &series,
                    [x, y],
                    &[],
                    &Default::default(),
                    [0.0; 2],
                    300.0,
                    false,
                );
            });
            (actual_id, PlotMemory::load(&context, actual_id).unwrap())
        };
        let zoom = |id| {
            let mut memory = PlotMemory::load(&context, id).unwrap();
            memory.auto_bounds = false.into();
            memory.set_bounds(egui_plot::PlotBounds::from_min_max(
                [-1.0, -0.001],
                [1.0, 0.001],
            ));
            memory.store(&context, id);
        };
        let (id, _) = render(&app, "iv_plot", 0.0005, 1.026);
        zoom(id);
        let (_, retained) = render(&app, "iv_plot", 0.0005, 1.026);
        assert!(!retained.auto_bounds.x && !retained.auto_bounds.y);
        assert_eq!(retained.bounds().max(), [1.0, 0.001]);

        app.settings.config.mode = Mode::Vi;
        let (_, fitted) = render(&app, "iv_plot", 0.0005, 1.026);
        assert!(fitted.auto_bounds.x && fitted.auto_bounds.y);
        assert!(fitted.bounds().max()[0] < 0.001);
        assert!(fitted.bounds().max()[1] >= 1.026);

        zoom(id);
        app.run_id += 1;
        let (_, fitted) = render(&app, "iv_plot", 0.0005, 1.026);
        assert!(fitted.auto_bounds.x && fitted.auto_bounds.y);
        assert!(fitted.bounds().max()[0] < 0.001);

        app.settings.config.mode = Mode::Iv;
        let (dual_id, _) = render(&app, "dual_source_plot", 0.001, 1e-6);
        zoom(dual_id);
        render(&app, "iv_plot", 0.001, 1e-6);
        let (_, fitted) = render(&app, "dual_source_plot", 0.001, 1e-6);
        assert!(fitted.auto_bounds.x && fitted.auto_bounds.y);
        let (_, fitted) = render(&app, "dual_meter_plot", 0.0008, 1e-6);
        assert!(fitted.auto_bounds.x && fitted.auto_bounds.y);
        // Rendering the lower separate plot must not reset the upper plot's
        // independent y zoom on every frame. Linked x gestures are covered
        // through PlotUi in verify_reference_gui.py, not by editing memory
        // behind the linked group's back.
        let (upper, _) = render(&app, "separate_primary", 1e-5, 0.02);
        render(&app, "separate_reference", 1e-5, 35e-6);
        zoom(upper);
        render(&app, "separate_primary", 1e-5, 0.02);
        render(&app, "separate_reference", 1e-5, 35e-6);
        let (_, retained) = render(&app, "separate_primary", 1e-5, 0.02);
        assert!(!retained.auto_bounds.y);
        assert_eq!(retained.bounds().max()[1], 0.001);
    }

    #[test]
    fn eframe_logic_completes_file_task_without_drawing_ui() {
        let context = egui::Context::default();
        let creation_context = eframe::CreationContext::_new_kittest(context.clone());
        let mut app = IvApp::new(
            &creation_context,
            true,
            None,
            Arc::new(AtomicBool::new(false)),
            Instant::now(),
            None,
            None,
        )
        .unwrap();
        let (send, receive) = mpsc::sync_channel(1);
        send.send(FileTaskResult::OutputDirectory(Some(PathBuf::from(
            "logic-output",
        ))))
        .unwrap();
        app.pending_file_task = Some(PendingFileTask {
            description: "Select output directory",
            result: receive,
            blocks_close: false,
        });

        let mut frame = eframe::Frame::_new_kittest();
        eframe::App::logic(&mut app, &context, &mut frame);

        assert_eq!(app.settings.output_dir, "logic-output");
        assert!(app.first_frame_s.is_none());
    }

    #[test]
    fn eframe_logic_cancels_close_while_file_copy_is_pending_without_drawing_ui() {
        let context = egui::Context::default();
        let creation_context = eframe::CreationContext::_new_kittest(context.clone());
        let mut app = IvApp::new(
            &creation_context,
            true,
            None,
            Arc::new(AtomicBool::new(false)),
            Instant::now(),
            None,
            None,
        )
        .unwrap();
        let (_send, receive) = mpsc::sync_channel(1);
        app.pending_file_task = Some(PendingFileTask {
            description: "Saving measurement copy",
            result: receive,
            blocks_close: true,
        });
        let mut frame = eframe::Frame::_new_kittest();
        let mut input = egui::RawInput::default();
        input.viewports.insert(
            egui::ViewportId::ROOT,
            egui::ViewportInfo {
                events: vec![egui::ViewportEvent::Close],
                ..Default::default()
            },
        );

        let output = context.run_ui(input, |_| {
            eframe::App::logic(&mut app, &context, &mut frame)
        });

        assert!(app.closing);
        assert!(app.first_frame_s.is_none());
        assert!(
            output
                .viewport_output
                .get(&egui::ViewportId::ROOT)
                .is_some_and(|viewport| viewport
                    .commands
                    .contains(&egui::ViewportCommand::CancelClose))
        );
    }

    #[test]
    fn eframe_logic_stops_an_active_run_on_close_without_drawing_ui() {
        assert_eq!(close_action(true, false), Some(CloseAction::Stop));
        assert_eq!(close_action(true, true), Some(CloseAction::Stop));
    }

    #[test]
    fn file_task_states_keep_picker_cancellation_and_copy_close_protection_distinct() {
        let (_send, receive) = mpsc::sync_channel(1);
        let picker = PendingFileTask {
            description: "Save measurement location",
            result: receive,
            blocks_close: false,
        };
        assert!(!file_task_blocks_close(Some(&picker)));
        assert!(selected_save_destination(PathBuf::from("run.csv"), None, 4).is_err());

        let (_send, receive) = mpsc::sync_channel(1);
        let copy = PendingFileTask {
            description: "Saving measurement copy",
            result: receive,
            blocks_close: true,
        };
        assert!(file_task_blocks_close(Some(&copy)));
        assert!(!file_task_blocks_close(None));
    }

    #[test]
    fn save_copy_completion_preserves_success_and_failure_messages() {
        let destination = Path::new("export.csv");
        assert_eq!(
            save_copy_completion(destination, Ok(()), 7),
            SaveCopyCompletion::Saved(
                "Saved 7 points to export.csv\nThe companion .log records lifecycle status; .run.json contains settings and .raw.csv retains acquisition evidence."
                    .into()
            )
        );
        assert_eq!(
            save_copy_completion(destination, Err("disk full".into()), 7),
            SaveCopyCompletion::Failed {
                log: "Save failed: disk full".into(),
                notice: "Could not save the measurement copy.\n\ndisk full\n\nThe original measurement files are unchanged.".into(),
            }
        );
    }

    #[test]
    fn dialog_worker_can_complete_an_already_ready_native_future() {
        assert_eq!(wait_for_dialog(async { 17_u8 }), 17);
    }

    #[test]
    fn dialog_worker_does_not_lose_a_wakeup_before_parking() {
        // Native completion can arrive between poll returning Pending and the
        // worker parking. Exercise that transition without opening a dialog.
        struct WakesDuringPoll(bool);
        impl Future for WakesDuringPoll {
            type Output = u8;
            fn poll(mut self: std::pin::Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<u8> {
                if self.0 {
                    Poll::Ready(23)
                } else {
                    self.0 = true;
                    cx.waker().wake_by_ref();
                    Poll::Pending
                }
            }
        }
        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || {
            sender
                .send(wait_for_dialog(WakesDuringPoll(false)))
                .unwrap()
        });
        assert_eq!(receiver.recv_timeout(Duration::from_secs(2)).unwrap(), 23);
    }

    #[test]
    fn compact_cards_follow_sweep_and_dc_function_without_mixing_ranges() {
        // The meter supplies only a verified live quantity. An idle editor
        // selection is passed solely to the source card.
        let sweep = Mode::Iv;
        let dc = Some(Mode::Vi);
        assert_eq!(card_mode(sweep, false, None, dc), Mode::Vi);
        assert_eq!(card_mode(sweep, false, None, None), Mode::Iv);
        assert_eq!(card_mode(sweep, false, dc, None), Mode::Vi);
        assert_eq!(card_mode(sweep, true, dc, None), Mode::Iv);

        assert_eq!(card_mode(Mode::Iv, false, None, None), Mode::Iv);
        assert_eq!(card_mode(Mode::Vi, false, None, None), Mode::Vi);
        assert_eq!(card_mode(Mode::Iv, false, None, Some(Mode::Vi)), Mode::Vi);
        assert_eq!(
            card_mode(Mode::Iv, false, Some(Mode::Vi), Some(Mode::Iv)),
            Mode::Vi
        );
        assert_eq!(
            card_mode(Mode::Iv, true, Some(Mode::Vi), Some(Mode::Vi)),
            Mode::Iv
        );
        let mut settings = Settings::default();
        settings.config.meter_current_range = Some(0.001);
        settings.config.meter_voltage_range = Some(1.0);
        settings.current_compliance = 0.002;
        settings.voltage_compliance = 0.1;
        settings.source_voltage_protection = None;
        settings.voltage_compliance = 0.2;
        preserve_source_on_meter_edit_for_mode(&mut settings, Mode::Vi, 0.1, 0.1);
        settings.activate_mode_protection();
        assert_eq!(settings.config.mode, Mode::Iv);
        assert_eq!(settings.config.compliance, 0.002);
        assert_eq!(settings.source_voltage_protection, Some(0.1));
        assert_eq!(settings.config.meter_current_range, Some(0.001));
        assert_eq!(settings.config.meter_voltage_range, Some(1.0));
    }

    #[test]
    fn meter_limit_edits_preserve_source_protection_in_both_modes() {
        for mode in [Mode::Iv, Mode::Vi] {
            let mut settings = Settings::default();
            settings.config.mode = mode;
            settings.current_compliance = 0.005;
            settings.voltage_compliance = 0.005;
            settings.source_current_protection = None;
            settings.source_voltage_protection = None;
            preserve_source_on_meter_edit(&mut settings, 0.001, 0.2);
            settings.activate_mode_protection();
            assert_eq!(settings.config.source_compliance, Some(0.2));
            assert_eq!(settings.config.compliance, 0.005);
            // An explicit source setting is never replaced by a meter edit.
            preserve_source_on_meter_edit(&mut settings, 0.003, 0.5);
            settings.activate_mode_protection();
            assert_eq!(settings.config.source_compliance, Some(0.2));
        }
    }

    #[test]
    fn api_profile_uses_the_selected_models_before_plan_validation() {
        let mut profile = Settings::default();
        profile.sources[0].model = "YOKOGAWA_7651".into();
        let loaded = load_gui_profile(&serde_json::to_vec(&profile).unwrap()).unwrap();
        assert_eq!(loaded.config.source_model, "YOKOGAWA_7651");
        profile.simulation = true;
        let loaded = load_gui_profile(&serde_json::to_vec(&profile).unwrap()).unwrap();
        assert_eq!(loaded.source_model, "SIMULATED_SOURCE");
        assert_eq!(loaded.config.source_model, "YOKOGAWA_GS210");
    }

    #[test]
    fn saved_data_inspector_has_exact_native_csv_fields_and_values() {
        assert_eq!(SNAPSHOT_FIELDS, iv_core::CSV_FIELDS);
        let point = iv_core::SweepPoint {
            target: 0.001,
            direction: "backward".into(),
            repeat_index: 2,
            sweep_index: 3,
            point_index: 4,
            total_points: 5,
        };
        let mut record = iv_core::simulate_record(&RunConfig::default(), &point, 0.5);
        record.timestamp = "2026-10-14T00:00:00Z".into();
        record.status = "running".into();
        record.run_id = "run-test".into();
        record.readings_requested = 1;
        record.readings_completed = Some(1);
        record.quality_flags = "normal".into();
        let values = record.native_csv_values();
        assert_eq!(
            values,
            [
                "run-test",
                "4",
                "2026-10-14T00:00:00Z",
                "5e-1",
                "3",
                "backward",
                "1e-3",
                "V",
                "1e-3",
                "1e-6",
                "source_readback",
                "meter",
                "1",
                "1",
                "",
                "A",
                "false",
                "normal",
                "1e-3",
                "",
                "",
                "1e-6",
                "1.0000000000000001e3",
                "",
                "1e-3",
                ""
            ]
            .map(str::to_string)
        );
    }

    #[test]
    fn custom_point_drafts_accept_scientific_values_and_explain_invalid_entries() {
        assert_eq!(
            parse_custom_points(" -1e-3, 0,\n2.5E-3 ").unwrap(),
            vec![-0.001, 0.0, 0.0025]
        );
        for text in [
            "", "1,", "1,,2", "1,NaN", "1,inf", "1,-inf", "1,1e999", "1,text",
        ] {
            assert!(parse_custom_points(text).is_err(), "{text:?}");
        }
        assert!(
            parse_custom_points("1,")
                .unwrap_err()
                .starts_with("Point 2:")
        );
    }

    #[test]
    fn start_failures_explain_connection_limits_and_resolution() {
        let mut config = RunConfig::default();
        assert!(
            start_block_reason(&config, false, true)
                .unwrap()
                .contains("All Connect")
        );
        assert!(
            start_block_reason(&config, true, false)
                .unwrap()
                .contains("both use simulation")
        );
        config.source_limit = 0.0001;
        assert!(
            start_block_reason(&config, true, true)
                .unwrap()
                .contains("Increase a limit only when it is safe")
        );
        config.source_limit = 0.001;
        config.start = -0.00100005;
        assert!(
            start_block_reason(&config, true, true)
                .unwrap()
                .contains("whole multiples")
        );
        config.start = -0.001;
        assert!(start_block_reason(&config, true, true).is_none());
    }

    #[test]
    fn invalid_zero_centered_interval_explains_safe_alternatives_before_start() {
        let config = RunConfig {
            start: 0.0001,
            stop: 0.0002,
            ..RunConfig::default()
        };
        let reason = start_block_reason(&config, true, true).unwrap();
        assert!(reason.contains("must include zero"));
        assert!(reason.contains("linear or round_trip"));
        assert!(reason.contains("source will not be enabled"));
    }

    fn assert_route(actual: &[f64], expected: &[f64]) {
        assert_eq!(actual.len(), expected.len());
        for (actual, expected) in actual.iter().zip(expected) {
            assert!(
                (actual - expected).abs() < 1e-15,
                "{actual:e} != {expected:e}"
            );
        }
    }

    #[test]
    fn cached_preview_shows_the_safe_negative_only_zero_centered_route() {
        let preview = sweep_preview(&RunConfig {
            start: -0.0016,
            stop: 0.0,
            step: 0.0001,
            source_limit: 0.002,
            ..RunConfig::default()
        })
        .unwrap();
        assert_eq!(preview.planned_points, 33);
        assert_route(&preview.route, &[0.0, -0.0016, 0.0]);
        assert_eq!(preview.route_text(), "Route: 0 V -> -1.6000 mV -> 0 V");
    }

    #[test]
    fn cached_preview_preserves_asymmetric_reverse_zero_centered_order() {
        let preview = sweep_preview(&RunConfig {
            start: 0.001,
            stop: -0.002,
            step: 0.0001,
            source_limit: 0.003,
            ..RunConfig::default()
        })
        .unwrap();
        assert_eq!(preview.planned_points, 61);
        assert_route(&preview.route, &[0.0, -0.002, 0.001, 0.0]);
        assert_eq!(
            preview.route_text(),
            "Route: 0 V -> -2.0000 mV -> 1.0000 mV -> 0 V"
        );
    }

    #[test]
    fn cached_preview_reports_custom_target_count_and_effective_step() {
        let preview = sweep_preview(&RunConfig {
            pattern: Pattern::CustomList,
            custom_points: vec![0.0001, -0.0002, 0.0003],
            step: 0.00000015,
            source_limit: 0.001,
            ..RunConfig::default()
        })
        .unwrap();
        assert_eq!(preview.planned_points, 3);
        assert!((preview.effective_step - 0.0000002).abs() < 1e-20);
        assert_route(&preview.route, &[0.0001, -0.0002, 0.0003]);
        assert_eq!(
            preview.route_text(),
            "Custom route (3 targets): 100.0000 µV -> -200.0000 µV -> 300.0000 µV"
        );
    }

    #[test]
    fn only_implemented_instruments_are_enabled() {
        assert!(model_supported("KEYSIGHT_34411A"));
        assert!(model_supported("AGILENT_34401A"));
        assert!(model_supported("KEYSIGHT_34465A"));
        assert!(model_supported("ADCMT_7461A"));
        assert!(!model_supported("YOKOGAWA_GS200"));
        assert!(model_supported("YOKOGAWA_7651"));
    }
    #[test]
    fn protection_popups_use_frozen_limits_and_never_claim_unverified_off() {
        let source = ProtectionAlert {
            kind: ProtectionKind::SourceCompliance,
            limit: 5.0,
            unit: "V".into(),
            observed: None,
            indicator: "H".into(),
            source_index: Some(1),
        };
        assert!(protection_role(&source));
        assert_eq!(protection_summary(&source).0, "Source compliance reached");
        assert!(
            protection_summary(&source)
                .1
                .contains("wiring and contacts")
        );
        let mut emergency = source.clone();
        emergency.indicator = "EMR".into();
        assert_eq!(
            protection_summary(&emergency).0,
            "Source protection triggered"
        );
        assert!(
            protection_summary(&emergency)
                .1
                .contains("before restarting")
        );
        let text = protection_message(&source, OutputState::Off);
        assert!(
            text.contains("Current source compliance: 5.0000 V"),
            "{text}"
        );
        assert!(text.contains("Instrument status: H"));
        assert!(text.contains("OFF verified"));
        assert!(text.contains("Source card > Device > Compliance"));
        for output in [OutputState::Unknown, OutputState::On] {
            let text = protection_message(&source, output);
            assert!(!text.contains("OFF verified"));
            assert!(text.contains("could not be verified"));
        }
        let meter = ProtectionAlert::measurement(
            &RunConfig {
                mode: Mode::Vi,
                compliance: 0.2,
                ..Default::default()
            },
            -0.3,
        );
        assert!(!protection_role(&meter));
        assert_eq!(protection_summary(&meter).0, "Meter limit reached");
        assert!(protection_summary(&meter).1.contains("review Meter limit"));
        let text = protection_message(&meter, OutputState::Off);
        assert!(text.contains("Current Meter limit:"));
        assert!(text.contains("Trigger reading: -300.0000 mV"), "{text}");
        assert!(text.contains("Meter card > Device > Limit"));
        assert!(text.contains("will not resume automatically"));
    }
}
