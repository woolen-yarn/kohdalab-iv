//! Application state, event synchronization and three-panel layout.
//! Settings, display formatting, drawing history and shared widgets live in
//! private modules so UI work does not require changes to the device engine.

use crate::state::{OutputState, Phase};

mod cards;
mod control;
mod dc;
mod devices;
mod dialogs;
mod digits;
mod file_tasks;
mod formatting;
mod history;
mod layout;
mod reference_analysis;
mod reference_view;
mod session;
mod settings;
mod sweep_controls;
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
    AxisHints, HoverPosition, Legend, Line, LineStyle, MarkerShape, Plot, PlotMemory, PlotPoint,
    PlotPoints, PlotUi, Points, VPlacement,
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
            .show(root, |ui| self.main_view(ui));
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
mod tests;
