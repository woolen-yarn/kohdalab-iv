//! Application state, event synchronization and three-panel layout.
//! Settings, display formatting, drawing history and shared widgets live in
//! private modules so UI work does not require changes to the device engine.

use crate::state::{OutputState, Phase};

mod control;
mod dc;
mod digits;
mod formatting;
mod history;
mod settings;
mod widgets;

use formatting::{AxisScale, display_reading, display_reading_at_scale, fmt_quantity};
use history::{LiveSample, append_live_sample, append_plot_point, extend_plot_extent, plot_series};
use settings::{Settings, save_settings, settings_path};
use widgets::{
    BG, BLUE, BORDER, BUTTON, INPUT, MUTED, ORANGE, PURPLE, RED, TEAL, button, compact_field,
    form_label, number, path_summary, quantity, resource_summary, section, state_badge, style,
};

use crate::client::Client;
use crate::protocol::{Command, ProtectionAlert, ProtectionKind, Snapshot};
use eframe::egui::{self, Color32, RichText, Stroke, Vec2};
use egui_plot::{
    Legend, Line, LineStyle, MarkerShape, Plot, PlotMemory, PlotPoint, PlotPoints, Points,
};
use iv_core::{
    InstrumentRole, Mode, Pattern, RunConfig, instrument_catalog, instrument_descriptor,
    measurement_definition, sweep_definitions,
};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
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

pub struct IvApp {
    client: Result<Client, String>,
    control: Option<crate::gui_control::GuiControl>,
    settings: Settings,
    state: Snapshot,
    previous_phase: Option<Phase>,
    previous_log: String,
    state_revision: u64,
    run_id: u64,
    samples: Vec<(f64, f64, String, [Option<f64>; 2])>,
    plot_series: [Vec<PlotPoint>; 3],
    plot_extent: [f64; 2],
    live_samples: VecDeque<LiveSample>,
    source2_samples: VecDeque<LiveSample>,
    live_reading: Option<iv_core::Record>,
    last_live_timestamp: String,
    table_values: Vec<String>,
    table_run_id: u64,
    table_points: usize,
    details: bool,
    left_visible: bool,
    table_visible: bool,
    timing_open: bool,
    timing_before: Option<(Settings, String)>,
    config_path: String,
    notice: String,
    start_error: Option<String>,
    protection_dialog: Option<ProtectionAlert>,
    save_notice: Option<String>,
    logs: Vec<String>,
    started: Instant,
    phase_start: Instant,
    smoke: bool,
    smoke_started: bool,
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
    pending_dc: Option<(String, u64)>,
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
        if std::env::args().any(|arg| arg == "--gui-dc-smoke")
            && (hardware.is_some() || startup_profile.is_some())
        {
            return Err("DC GUI smoke requires its isolated simulation profile; hardware/config overrides are not allowed.".into());
        }
        style(&cc.egui_ctx);
        let ctx = cc.egui_ctx.clone();
        let client = Client::start(move || ctx.request_repaint());
        let (mut settings, mut settings_notice) = match std::fs::read(settings_path()) {
            Ok(bytes) => match settings::load_settings(&bytes) {
                Ok(settings) => (settings, String::new()),
                Err(error) => (Settings::default(), error),
            },
            Err(_) => (Settings::default(), String::new()),
        };
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
                settings.config = RunConfig {
                    pattern: Pattern::Linear,
                    source_compliance: Some(0.001),
                    step: 0.001,
                    ..Default::default()
                };
            }
        }
        settings.migrate_inventory();
        if settings.simulation {
            settings.source_model = "SIMULATED_SOURCE".into();
            settings.meter_model = "SIMULATED_METER".into();
            if settings.config.balanced_source.is_some() {
                settings.source2_model = "SIMULATED_SOURCE".into();
            }
            // Legacy simulation settings have only scalar device fields.
            settings.store_active_entries();
        }
        let dc_smoke = smoke && std::env::args().any(|arg| arg == "--gui-dc-smoke");
        if dc_smoke {
            settings.dc_gate_id = settings.sources[1].id;
            settings.sources[0].dc.value = 0.001;
            settings.sources[1].model = "SIMULATED_SOURCE".into();
            settings.sources[1].resource = "SIM_GATE".into();
            settings.sources[1].dc.value = 0.01;
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
        Ok(Self {
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
            live_samples: VecDeque::new(),
            source2_samples: VecDeque::new(),
            live_reading: None,
            last_live_timestamp: String::new(),
            table_values: vec![],
            table_run_id: u64::MAX,
            table_points: usize::MAX,
            details: true,
            left_visible: true,
            table_visible: true,
            timing_open: false,
            timing_before: None,
            config_path,
            notice: settings_notice,
            start_error: None,
            protection_dialog: None,
            save_notice: None,
            logs: vec![],
            started: Instant::now(),
            phase_start: Instant::now(),
            smoke,
            smoke_started: false,
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
            pending_dc: None,
            dc_smoke_stage: 0,
        })
    }

    fn send(&mut self, command: Command) -> bool {
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
            if next.balanced_active != self.state.balanced_active {
                self.live_samples.clear();
                self.source2_samples.clear();
                self.last_live_timestamp.clear();
            }
            if let Some(record) = &next.live
                && self.last_live_timestamp != record.timestamp
            {
                let mut sample = record.clone();
                sample.elapsed_s = self.boot.elapsed().as_secs_f64();
                append_live_sample(&mut self.live_samples, &sample);
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
                self.notice.clear();
            }
            for (record_run, point) in client.drain_points() {
                if record_run < self.run_id {
                    continue;
                }
                if record_run > self.run_id {
                    self.samples.clear();
                    self.plot_series.iter_mut().for_each(Vec::clear);
                    self.plot_extent = [0.0; 2];
                    self.run_id = record_run;
                }
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
                    point.point.direction,
                    [point.source_resolution, point.meter_resolution],
                ));
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
            .map(|record| record.native_csv_values().into())
            .unwrap_or_default();
        self.table_run_id = self.state.run_id;
        self.table_points = self.state.points;
    }

    fn start_run(&mut self) {
        self.start_run_impl(true);
    }

    fn start_run_impl(&mut self, save_preferences: bool) {
        if self.pending_run {
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
        let name = self.settings.filename.trim();
        if name.is_empty() || name.contains(['/', '\\']) {
            let reason =
                "Enter a file name without / or \\ characters in Output > File.".to_owned();
            self.report(reason.clone());
            self.start_error = Some(reason);
            return;
        }
        let directory = PathBuf::from(&self.settings.output_dir);
        let stamp = time::OffsetDateTime::now_utc().unix_timestamp_nanos();
        let basename = name.strip_suffix(".csv").unwrap_or(name);
        let path = directory.join(if self.settings.auto_suffix {
            format!("{basename}_{stamp}.csv")
        } else {
            format!("{basename}.csv")
        });
        if !self.smoke && save_preferences {
            self.settings.store_active_entries();
            self.settings.simulation = self.both_simulated();
            let _ = save_settings(&self.settings, Path::new(&self.config_path));
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

    fn sidebar(&mut self, ctx: &egui::Context) {
        let width = if self.left_visible { 306.0 } else { 40.0 };
        egui::SidePanel::left("setup")
            .exact_width(width)
            .resizable(false)
            .frame(egui::Frame::new().fill(BG).inner_margin(8))
            .show(ctx, |ui| {
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
                            egui::ScrollArea::vertical()
                                .id_salt("settings_scroll")
                                .auto_shrink([false, false])
                                .show(ui, |ui| {
                                    self.device_inventory_actions(ui);
                                    ui.add_space(4.0);
                                    self.instrument(ui, true);
                                    if self.settings.config.balanced_source.is_some() {
                                        ui.add_space(4.0);
                                        self.secondary_source(ui);
                                    }
                                    let inactive_sources = self
                                        .settings
                                        .sources
                                        .iter()
                                        .filter(|entry| {
                                            entry.id != self.settings.source1_id
                                                && (self.settings.config.balanced_source.is_none()
                                                    || entry.id != self.settings.source2_id)
                                        })
                                        .map(|entry| entry.id)
                                        .collect::<Vec<_>>();
                                    // Keep the active meter next to the sweep
                                    // sources. An expanded fixed-bias card must
                                    // not displace its live monitor below gates.
                                    ui.add_space(4.0);
                                    self.instrument(ui, false);
                                    for id in inactive_sources {
                                        ui.add_space(4.0);
                                        self.inactive_instrument(ui, true, id);
                                    }
                                    let inactive_meters = self
                                        .settings
                                        .meters
                                        .iter()
                                        .filter(|entry| entry.id != self.settings.meter_id)
                                        .map(|entry| entry.id)
                                        .collect::<Vec<_>>();
                                    for id in inactive_meters {
                                        ui.add_space(4.0);
                                        self.inactive_instrument(ui, false, id);
                                    }
                                });
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
            let enabled = !self.busy();
            ui.add_enabled_ui(enabled, |ui| {
                ui.horizontal(|ui| {
                    let field_width = (ui.available_width() - 92.0).max(80.0);
                    let summary = path_summary(&self.config_path);
                    compact_field(
                        ui,
                        "config_path",
                        &mut self.config_path,
                        field_width,
                        summary,
                    );
                    if button(ui, "Browse", 70.0, true).clicked()
                        && let Some(path) = rfd::FileDialog::new()
                            .set_title("Configuration file")
                            .add_filter("JSON", &["json"])
                            .set_directory(
                                Path::new(&self.config_path)
                                    .parent()
                                    .unwrap_or(Path::new(".")),
                            )
                            .pick_file()
                    {
                        self.config_path = path.display().to_string();
                    }
                });
                ui.add_space(6.0);
                let row_width = (ui.available_width() - 8.0) / 2.0;
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
                    if button(ui, "All Connect", row_width, true).clicked() {
                        self.connect_instrument(true);
                        if self.settings.config.balanced_source.is_some() {
                            self.connect_secondary();
                        }
                        self.connect_instrument(false);
                    }
                    if button(
                        ui,
                        "All Disconnect",
                        row_width,
                        self.state.source_connected
                            || self.state.source2_connected
                            || self.state.meter_connected
                            || self.state.gate_connected,
                    )
                    .clicked()
                    {
                        self.send(Command::Disconnect);
                    }
                });
            });
        });
    }

    fn live_monitor(&self, ui: &mut egui::Ui, source: bool) {
        let mode = self.card_control_mode();
        let source_channel = if mode == Mode::Iv { 1 } else { 0 };
        let channel = if source {
            source_channel
        } else {
            1 - source_channel
        };
        let (unit, color) = if channel == 0 {
            ("A", TEAL)
        } else {
            ("V", ORANGE)
        };
        let running = self.state.connected && self.state.error.is_empty();
        let stale = self
            .live_samples
            .back()
            .is_some_and(|s| self.boot.elapsed().as_secs_f64() - s.time > 3.0);
        ui.add_space(4.0);
        ui.separator();
        let alert = self.state.protection_alert.as_ref().filter(|alert| {
            protection_role(alert) == source && (!source || alert.source_index != Some(2))
        });
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(if channel == 0 { "I" } else { "V" })
                    .color(color)
                    .strong(),
            );
            ui.label(
                RichText::new(if self.state.simulation {
                    "SIM"
                } else if source {
                    "SOURCE LEVEL"
                } else {
                    "MEASURED"
                })
                .size(9.0)
                .color(MUTED),
            )
            .on_hover_text(if source {
                "Programmed source level; not an independent output measurement."
            } else {
                "Measured by the meter."
            });
            ui.label(
                RichText::new(if source && self.state.source_dc.is_some() {
                    "DC SET"
                } else if running && stale {
                    "STALE"
                } else if running {
                    "LIVE"
                } else if self.live_samples.is_empty() {
                    "WAITING"
                } else {
                    "HELD"
                })
                .size(9.0)
                .color(if running { BLUE } else { MUTED }),
            );
            // Reserve the same small slot in both states: no extra row,
            // and neither plot height nor the output badge moves on a trip.
            let bright = (self.boot.elapsed().as_millis() / 500).is_multiple_of(2);
            let limit_label = if alert.is_some_and(|alert| alert.indicator.contains("EMR")) {
                "FAULT"
            } else if alert.is_some() {
                "LIMIT"
            } else {
                ""
            };
            let (badge_rect, response) =
                ui.allocate_exact_size(egui::vec2(34.0, 14.0), egui::Sense::hover());
            if alert.is_some() {
                ui.painter().rect_filled(
                    badge_rect,
                    3.0,
                    if bright {
                        Color32::from_rgb(220, 49, 66)
                    } else {
                        Color32::from_rgb(142, 39, 48)
                    },
                );
                ui.painter().text(
                    badge_rect.center(),
                    egui::Align2::CENTER_CENTER,
                    limit_label,
                    egui::FontId::proportional(9.0),
                    Color32::WHITE,
                );
            }
            if let Some(alert) = alert {
                ui.ctx().request_repaint_after(Duration::from_millis(100));
                let mut detail = format!(
                    "{}\nLimit: {}",
                    protection_summary(alert).0,
                    fmt_quantity(alert.limit, &alert.unit)
                );
                if let Some(value) = alert.observed {
                    detail.push_str(&format!("\nMeasured: {}", fmt_quantity(value, &alert.unit)));
                }
                if !alert.indicator.is_empty() {
                    detail.push_str(&format!("\nInstrument: {}", alert.indicator));
                }
                response.on_hover_text(detail);
            }
            if source {
                state_badge(
                    ui,
                    &format!("Output {}", self.state.output.to_lowercase()),
                    if self.state.output == OutputState::Off {
                        MUTED
                    } else {
                        ORANGE
                    },
                );
            }
        });
        let newest = if self.state.connected {
            self.boot.elapsed().as_secs_f64()
        } else {
            self.live_samples.back().map_or(0.0, |s| s.time)
        };
        let window = newest.clamp(2.0, 30.0);
        let source1_factor = if self.state.balanced_active {
            self.settings
                .config
                .balanced_source
                .as_ref()
                .map_or(0.5, |dual| dual.source1_fraction())
        } else {
            1.0
        };
        let extent = self.live_samples.iter().fold(0_f64, |m, s| {
            m.max(if channel == 0 {
                s.current.abs()
            } else {
                s.voltage.abs() * if source { source1_factor } else { 1.0 }
            })
        });
        let scale = AxisScale::for_magnitude(extent);
        let dc_text = if source {
            self.state
                .source_dc
                .as_ref()
                .map(|dc| fmt_quantity(dc.value, unit))
        } else {
            None
        };
        let text = dc_text.unwrap_or_else(|| {
            self.live_reading
                .as_ref()
                .map(|record| {
                    let source =
                        record.source_readback.unwrap_or(record.source_set) * source1_factor;
                    let source_channel = if record.mode == Mode::Iv { 1 } else { 0 };
                    let (value, resolution) = if channel == source_channel {
                        (
                            source,
                            record
                                .source_resolution
                                .map(|resolution| resolution * source1_factor),
                        )
                    } else {
                        (record.meter_value, record.meter_resolution)
                    };
                    display_reading_at_scale(value, resolution, unit, scale)
                })
                .unwrap_or_else(|| format!("— {unit}"))
        });
        let unknown = text.ends_with(" (resolution unknown)");
        ui.label(
            RichText::new(text.trim_end_matches(" (resolution unknown)"))
                .monospace()
                .size(22.0)
                .color(color),
        );
        if unknown {
            ui.label(RichText::new("Resolution unknown").size(9.0).color(MUTED));
        }
        let values: Vec<[f64; 2]> = self
            .live_samples
            .iter()
            .filter(|s| newest - s.time <= window)
            .map(|s| {
                [
                    s.time - newest,
                    if channel == 0 {
                        s.current
                    } else {
                        s.voltage * if source { source1_factor } else { 1.0 }
                    },
                ]
            })
            .collect();
        let current = values.last().map_or(0.0, |p| p[1]);
        let fraction = if extent > 0.0 {
            (current / extent).clamp(-1.0, 1.0) as f32
        } else {
            0.0
        };
        let (rect, _) =
            ui.allocate_exact_size(egui::vec2(ui.available_width(), 6.0), egui::Sense::hover());
        let painter = ui.painter();
        painter.rect_filled(rect, 2.0, BORDER);
        let center = rect.center().x;
        let end = center + fraction * rect.width() * 0.5;
        painter.rect_filled(
            egui::Rect::from_min_max(
                egui::pos2(center.min(end), rect.top()),
                egui::pos2(center.max(end), rect.bottom()),
            ),
            0.0,
            color,
        );
        painter.line_segment(
            [
                egui::pos2(center, rect.top()),
                egui::pos2(center, rect.bottom()),
            ],
            Stroke::new(1.0_f32, Color32::WHITE),
        );
        Plot::new(("live_channel", channel))
            .height(55.0)
            .show_background(false)
            .show_axes([true, true])
            .show_grid([true, true])
            .allow_drag(false)
            .allow_zoom(false)
            .allow_scroll(false)
            .include_x(-window)
            .include_x(0.0)
            .include_y(0.0)
            .y_axis_formatter(move |mark, _| scale.tick(mark.value, mark.step_size))
            .label_formatter(|_, _| String::new())
            .show(ui, |plot_ui| {
                plot_ui.line(
                    Line::new("trend", values)
                        .color(color)
                        .width(1.5_f32)
                        .fill(0.0_f32)
                        .fill_alpha(0.75_f32),
                );
            });
        ui.label(
            RichText::new(format!(
                "Time · s ago · {}{} · updated {:.1} s ago",
                scale.prefix,
                unit,
                self.live_samples
                    .back()
                    .map_or(0.0, |s| (self.boot.elapsed().as_secs_f64() - s.time)
                        .max(0.0))
            ))
            .size(9.0)
            .color(MUTED),
        );
    }

    fn connect_instrument(&mut self, source: bool) {
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
        if source {
            self.send(Command::ConnectSource {
                model: model.clone(),
                simulation,
                resource,
            });
            self.send(Command::SetLiveMode {
                mode: self.settings.config.mode,
            });
        } else {
            self.send(Command::ConnectMeter {
                model: model.clone(),
                simulation,
                resource,
            });
        }
        self.send_meter_ranges();
    }

    fn send_meter_ranges(&mut self) {
        self.send(Command::SetMeterRanges {
            meter_voltage_range: self.settings.config.meter_voltage_range,
            meter_current_range: self.settings.config.meter_current_range,
        });
    }

    fn connect_secondary(&mut self) {
        self.send(Command::ConnectSource2 {
            model: self.settings.source2_model.clone(),
            simulation: self.settings.source2_model == "SIMULATED_SOURCE",
            resource: self.settings.source2.clone(),
        });
    }

    fn secondary_source(&mut self, ui: &mut egui::Ui) {
        let index = self
            .settings
            .sources
            .iter()
            .position(|entry| entry.id == self.settings.source2_id)
            .unwrap_or(1)
            + 1;
        let title = if index == 2 {
            "Source 2".into()
        } else {
            format!("Source {index} · S2")
        };
        section(ui, &title, |ui| {
            let connected = self.state.source2_connected;
            let idle = !self.busy();
            ui.painter().text(
                ui.max_rect().right_top() + Vec2::new(-2.0, -18.0),
                egui::Align2::RIGHT_TOP,
                if connected {
                    "Connected"
                } else {
                    "Disconnected"
                },
                egui::FontId::proportional(10.0),
                if connected { TEAL } else { MUTED },
            );
            let summary = device_summary(&self.settings.source2_model).to_owned();
            let id = self.settings.source2_id;
            if self.source_card_header(ui, id, &summary) {
                ui.add_enabled_ui(idle && !connected, |ui| {
                    egui::ComboBox::from_id_salt("source2_model")
                        .width(ui.available_width())
                        .selected_text(&self.settings.source2_model)
                        .show_ui(ui, |ui| {
                            ui.set_min_width(230.0);
                            for descriptor in instrument_catalog(InstrumentRole::Source) {
                                if model_supported(descriptor.id) {
                                    ui.selectable_value(
                                        &mut self.settings.source2_model,
                                        descriptor.id.into(),
                                        descriptor.id,
                                    );
                                }
                            }
                        });
                    ui.horizontal(|ui| {
                        let summary = resource_summary(&self.settings.source2);
                        let width = (ui.available_width() - 30.0).max(65.0);
                        compact_field(
                            ui,
                            "source2_resource",
                            &mut self.settings.source2,
                            width,
                            summary,
                        );
                        egui::ComboBox::from_id_salt("source2_resources")
                            .width(20.0)
                            .selected_text("")
                            .show_ui(ui, |ui| {
                                ui.set_min_width(470.0);
                                for resource in &self.state.resources {
                                    ui.selectable_value(
                                        &mut self.settings.source2,
                                        resource.resource.clone(),
                                        &resource.label,
                                    );
                                }
                            });
                    });
                });
                ui.horizontal(|ui| {
                    if button(ui, "Refresh", 70.0, idle).clicked() {
                        self.send(Command::Discover);
                    }
                    if button(
                        ui,
                        "Connect",
                        70.0,
                        idle && !connected
                            && (self.settings.source2_model == "SIMULATED_SOURCE"
                                || !self.settings.source2.is_empty()),
                    )
                    .clicked()
                    {
                        self.connect_secondary();
                    }
                    if button(ui, "Disconnect", 80.0, idle && connected).clicked() {
                        self.send(Command::DisconnectSource2);
                    }
                });
            }
            self.dc_controls(ui, id);
            ui.add_space(4.0);
            ui.separator();
            ui.horizontal(|ui| {
                ui.label(RichText::new("V").color(ORANGE).strong());
                ui.label(
                    RichText::new(if self.state.source2_simulation {
                        "SIM"
                    } else {
                        "SOURCE LEVEL"
                    })
                    .size(9.0)
                    .color(MUTED),
                );
                ui.label(
                    RichText::new(if connected { "LIVE" } else { "WAITING" })
                        .size(9.0)
                        .color(if connected { BLUE } else { MUTED }),
                );
                let alert = self.state.protection_alert.as_ref().filter(|a| {
                    a.kind == ProtectionKind::SourceCompliance && a.source_index == Some(2)
                });
                let (rect, response) =
                    ui.allocate_exact_size(egui::vec2(34.0, 14.0), egui::Sense::hover());
                if let Some(alert) = alert {
                    let bright = (self.boot.elapsed().as_millis() / 500).is_multiple_of(2);
                    ui.ctx().request_repaint_after(Duration::from_millis(100));
                    ui.painter().rect_filled(
                        rect,
                        3.0,
                        if bright {
                            RED
                        } else {
                            Color32::from_rgb(142, 39, 48)
                        },
                    );
                    ui.painter().text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        "LIMIT",
                        egui::FontId::proportional(9.0),
                        Color32::WHITE,
                    );
                    response.on_hover_text(format!(
                        "Source 2 compliance: {}",
                        fmt_quantity(alert.limit, &alert.unit)
                    ));
                }
                state_badge(
                    ui,
                    &format!("Output {}", self.state.source2_output.to_lowercase()),
                    MUTED,
                );
            });
            let value = self.state.source2_level;
            let resolution = self
                .state
                .source2_resolution
                .or(if self.state.source2_simulation {
                    Some(1e-9)
                } else {
                    None
                });
            let text = value
                .map(|v| display_reading(v, resolution, "V"))
                .unwrap_or_else(|| "— V".into());
            ui.label(RichText::new(text).monospace().size(22.0).color(ORANGE));
            let now = self.boot.elapsed().as_secs_f64();
            let scale = AxisScale::for_magnitude(
                self.source2_samples
                    .iter()
                    .fold(0.0_f64, |m, s| m.max(s.voltage.abs())),
            );
            let points: Vec<[f64; 2]> = self
                .source2_samples
                .iter()
                .filter(|s| now - s.time <= 30.0)
                .map(|s| [s.time - now, s.voltage])
                .collect();
            Plot::new("source2_live")
                .height(55.0)
                .show_background(false)
                .allow_drag(false)
                .allow_zoom(false)
                .allow_scroll(false)
                .include_x(-now.clamp(2.0, 30.0))
                .include_x(0.0)
                .include_y(0.0)
                .y_axis_formatter(move |mark, _| scale.tick(mark.value, mark.step_size))
                .show(ui, |plot| {
                    plot.line(Line::new("source2", points).color(ORANGE).fill(0.0_f32));
                });
            ui.label(
                RichText::new("Time · s ago · programmed source level")
                    .size(9.0)
                    .color(MUTED),
            );
        });
    }

    /// Saved devices that are not assigned to an active measurement role stay
    /// visible and editable, but deliberately have no Connect control.  This
    /// prevents a configuration card from energising an unselected source.
    fn inactive_instrument(&mut self, ui: &mut egui::Ui, source: bool, id: u64) {
        if source {
            self.dc_stored_source_card(ui, id);
            return;
        }
        let entries = if source {
            &self.settings.sources
        } else {
            &self.settings.meters
        };
        let Some(position) = entries.iter().position(|entry| entry.id == id) else {
            return;
        };
        let title = if source {
            format!("Source {}", position + 1)
        } else {
            format!("Meter {}", position + 1)
        };
        let (model, resource) = {
            let entry = &entries[position];
            (entry.model.clone(), entry.resource.clone())
        };
        ui.add_space(4.0);
        let idle = !self.busy();
        let frame = egui::Frame::new()
            .fill(BG.gamma_multiply(0.9))
            .corner_radius(10)
            .stroke(Stroke::new(1.0_f32, BORDER.gamma_multiply(0.62)))
            .inner_margin(egui::Margin::symmetric(10, 8))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(&title).color(MUTED));
                    ui.label(RichText::new("INACTIVE").size(9.0).color(MUTED));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let can_remove = !self.busy()
                            && if source {
                                self.settings.sources.len() > 2
                            } else {
                                self.settings.meters.len() > 1
                            };
                        if button(ui, "Remove", 62.0, can_remove)
                            .on_hover_text("Choose another active device in Settings before removing this card.")
                            .clicked()
                        {
                            if source {
                                self.settings.remove_source(id);
                            } else {
                                self.settings.remove_meter(id);
                            }
                        }
                    });
                });
                ui.add_space(3.0);
                egui::CollapsingHeader::new(format!("Device · {}", device_summary(&model)))
                    .id_salt(("inactive_device", source, id))
                    .default_open(false)
                    .show(ui, |ui| {
                        ui.label(
                            RichText::new(
                                "Stored only. Select this device in Settings to connect or use it in a run.",
                            )
                            .small()
                            .color(MUTED),
                        );
                        let entries = if source {
                            &mut self.settings.sources
                        } else {
                            &mut self.settings.meters
                        };
                        let Some(entry) = entries.iter_mut().find(|entry| entry.id == id) else {
                            return;
                        };
                        ui.add_enabled_ui(idle, |ui| {
                            egui::ComboBox::from_id_salt(("inactive_model", source, id))
                                .width(ui.available_width())
                                .selected_text(&entry.model)
                                .show_ui(ui, |ui| {
                                    for descriptor in instrument_catalog(if source {
                                        InstrumentRole::Source
                                    } else {
                                        InstrumentRole::Meter
                                    }) {
                                        ui.add_enabled_ui(model_supported(descriptor.id), |ui| {
                                            ui.selectable_value(
                                                &mut entry.model,
                                                descriptor.id.to_owned(),
                                                descriptor.id,
                                            );
                                        });
                                    }
                                });
                            ui.horizontal(|ui| {
                                let width = (ui.available_width() - 28.0).max(55.0);
                                let summary = resource_summary(&entry.resource);
                                compact_field(
                                    ui,
                                    ("inactive_resource", source, id),
                                    &mut entry.resource,
                                    width,
                                    summary,
                                );
                                egui::ComboBox::from_id_salt(("inactive_resources", source, id))
                                    .width(20.0)
                                    .selected_text("")
                                    .show_ui(ui, |ui| {
                                        ui.set_min_width(470.0);
                                        for item in &self.state.resources {
                                            ui.selectable_value(
                                                &mut entry.resource,
                                                item.resource.clone(),
                                                &item.label,
                                            );
                                        }
                                    });
                            });
                        });
                    });
            });
        frame
            .response
            .on_hover_text(format!("{model}\n{}", resource_summary(&resource)));
    }

    fn instrument(&mut self, ui: &mut egui::Ui, source: bool) {
        let index = if source {
            self.settings
                .sources
                .iter()
                .position(|entry| entry.id == self.settings.source1_id)
        } else {
            self.settings
                .meters
                .iter()
                .position(|entry| entry.id == self.settings.meter_id)
        }
        .unwrap_or(0)
            + 1;
        let title = if source {
            if self.settings.config.balanced_source.is_some() && index != 1 {
                format!("Source {index} · S1")
            } else {
                format!("Source {index}")
            }
        } else {
            format!("Meter {index}")
        };
        section(ui, &title, |ui| {
            let connected = if source {
                self.state.source_connected
            } else {
                self.state.meter_connected
            };
            ui.painter().text(
                ui.max_rect().right_top() + Vec2::new(-2.0, -18.0),
                egui::Align2::RIGHT_TOP,
                if connected {
                    "Connected"
                } else {
                    "Disconnected"
                },
                egui::FontId::proportional(10.0),
                if connected { TEAL } else { MUTED },
            );
            let model = if source {
                &self.settings.source_model
            } else {
                &self.settings.meter_model
            };
            let summary = if model.starts_with("SIMULATED_") {
                "Simulation"
            } else {
                model
                    .split_once('_')
                    .map_or(model.as_str(), |(_, name)| name)
            };
            let resource = if source {
                &self.settings.source
            } else {
                &self.settings.meter
            };
            let hint = format!(
                "{model}\n{}",
                if resource.is_empty() {
                    "No resource selected"
                } else {
                    resource
                }
            );
            if source {
                let id = self.settings.source1_id;
                let summary = summary.to_owned();
                if self.source_card_header(ui, id, &summary) {
                    self.instrument_connection_controls(ui, true, connected);
                }
                self.source_compliance_control(ui);
                self.dc_controls(ui, id);
            } else {
                egui::CollapsingHeader::new(format!("Device  ·  {summary}"))
                    .id_salt(("instrument_device_settings", source))
                    .default_open(false)
                    .show_background(true)
                    .show_unindented(ui, |ui| {
                        self.instrument_connection_controls(ui, source, connected)
                    })
                    .header_response
                    .on_hover_text(hint);
                self.meter_measurement_controls(ui);
            }
            self.live_monitor(ui, source);
        });
    }

    fn instrument_connection_controls(&mut self, ui: &mut egui::Ui, source: bool, connected: bool) {
        let idle = !self.busy();
        let width = (ui.available_width() - 65.0).max(140.0);
        egui::Grid::new(("instrument", source))
            .num_columns(2)
            .spacing([6.0, 6.0])
            .show(ui, |ui| {
                form_label(ui, "Device", 57.0);
                let model = if source {
                    &mut self.settings.source_model
                } else {
                    &mut self.settings.meter_model
                };
                ui.add_enabled_ui(idle && !connected, |ui| {
                    egui::ComboBox::from_id_salt(("device", source))
                        .width(width)
                        .wrap_mode(egui::TextWrapMode::Truncate)
                        .selected_text(model.as_str())
                        .show_ui(ui, |ui| {
                            ui.set_min_width(230.0);
                            for descriptor in instrument_catalog(if source {
                                InstrumentRole::Source
                            } else {
                                InstrumentRole::Meter
                            }) {
                                let candidate = descriptor.id;
                                ui.add_enabled_ui(model_supported(candidate), |ui| {
                                    ui.selectable_value(model, candidate.to_string(), candidate);
                                })
                                .response
                                .on_disabled_hover_text("Not yet supported in the Rust preview");
                            }
                        });
                });
                ui.end_row();
                form_label(ui, "Resource", 57.0);
                ui.horizontal(|ui| {
                    let resource_width = (width - 76.0).max(65.0);
                    self.resource(ui, source, resource_width, idle && !connected);
                    if button(ui, "Refresh", 68.0, idle).clicked() {
                        self.send(Command::Discover);
                    }
                });
                ui.end_row();
                if !source {
                    form_label(
                        ui,
                        iv_core::parameter_definition("nplc").unwrap().label,
                        57.0,
                    );
                    ui.add_enabled_ui(idle, |ui| {
                        ui.add_sized(
                            [60.0, 24.0],
                            egui::DragValue::new(&mut self.settings.config.nplc)
                                .speed(0.01)
                                .range(0.001..=100.0)
                                .min_decimals(3)
                                .max_decimals(3),
                        );
                    });
                    ui.end_row();
                }
                form_label(ui, "", 57.0);
                ui.horizontal(|ui| {
                    let model = if source {
                        &self.settings.source_model
                    } else {
                        &self.settings.meter_model
                    };
                    let resource = if source {
                        &self.settings.source
                    } else {
                        &self.settings.meter
                    };
                    let valid = instrument_descriptor(model)
                        .is_some_and(|d| d.supports_mode(self.settings.config.mode))
                        && (model.starts_with("SIMULATED_") || !resource.is_empty());
                    if button(
                        ui,
                        "Connect",
                        (width - 8.0) / 2.0,
                        idle && !connected && valid,
                    )
                    .clicked()
                    {
                        self.connect_instrument(source);
                    }
                    if button(ui, "Disconnect", (width - 8.0) / 2.0, idle && connected).clicked() {
                        self.send(if source {
                            Command::DisconnectSource
                        } else {
                            Command::DisconnectMeter
                        });
                    }
                });
                ui.end_row();
            });
    }

    fn resource(&mut self, ui: &mut egui::Ui, source: bool, width: f32, enabled: bool) {
        let selection = if source {
            &mut self.settings.source
        } else {
            &mut self.settings.meter
        };
        ui.add_enabled_ui(enabled, |ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            ui.spacing_mut().button_padding.x = 3.0;
            ui.spacing_mut().icon_width = 14.0;
            ui.spacing_mut().icon_spacing = 0.0;
            let summary = resource_summary(selection);
            compact_field(
                ui,
                ("resource_edit", source),
                selection,
                (width - 28.0).max(30.0),
                summary,
            );
            egui::ComboBox::from_id_salt(("resources", source))
                .width(20.0)
                .selected_text("")
                .show_ui(ui, |ui| {
                    ui.set_min_width(470.0);
                    for resource in &self.state.resources {
                        ui.selectable_value(
                            selection,
                            resource.resource.clone(),
                            &resource.resource,
                        )
                        .on_hover_text(&resource.label);
                    }
                    if self.state.resources.is_empty() {
                        ui.label("Click Refresh to find connected instruments.");
                    }
                });
        });
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
                let mode_editable = !self.busy() && !self.state.source2_connected;
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
                            ui.selectable_value(&mut choice, 0, mode_name(Mode::Iv));
                            ui.selectable_value(&mut choice, 1, mode_name(Mode::Vi));
                            ui.selectable_value(&mut choice, 2, "Dual V-I · differential voltage");
                        })
                });
                response.response.on_disabled_hover_text(
                    "Disconnect Source 2 before leaving Dual mode. Wait for the current operation to finish.",
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
                    self.settings.store_active_protection();
                    let next_mode = if choice == 1 { Mode::Vi } else { Mode::Iv };
                    if old_mode != next_mode {
                        let definition = measurement_definition(next_mode);
                        let ranges = (self.settings.config.meter_voltage_range, self.settings.config.meter_current_range);
                        self.settings.config = definition.default_config();
                        (self.settings.config.meter_voltage_range, self.settings.config.meter_current_range) = ranges;
                        self.settings.units = [definition.preferred_unit_index; 3];
                    }
                    self.settings.config.mode = next_mode;
                    if self.state.source_dc.is_none() && self.dc_open.contains(&self.settings.source1_id)
                        && let Some(entry)=self.settings.sources.iter_mut().find(|e|e.id==self.settings.source1_id)
                        && entry.dc.mode != next_mode {
                        entry.dc.reset_function(next_mode);
                    }
                    self.settings.config.balanced_source =
                        (choice == 2).then(iv_core::BalancedSourceConfig::default);
                    self.settings.bind_active_entries();
                    self.settings.activate_mode_protection();
                    self.samples.clear();
                    self.rebuild_plot_series();
                    self.custom_text.clear();
                    self.live_samples.clear();
                    self.live_reading = None;
                    if self.state.source_connected && self.state.source_dc.is_none() {
                        self.send(Command::SetLiveMode {
                            mode: self.settings.config.mode,
                        });
                    }
                }
                if self.settings.config.balanced_source.is_some() {
                    form_label(ui, "", 45.0);
                    ui.label(
                        RichText::new("Differential V · split is configured in Settings")
                            .small()
                            .color(MUTED),
                    );
                    ui.end_row();
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

    /// Active sweep wins; otherwise the held DC mode or open DC editor wins.
    fn card_control_mode(&self) -> Mode {
        let dc = self
            .state
            .source_dc
            .as_ref()
            .filter(|_| self.state.output == OutputState::On)
            .map(|d| d.mode);
        let editor = self
            .dc_open
            .contains(&self.settings.source1_id)
            .then(|| {
                self.settings
                    .sources
                    .iter()
                    .find(|e| e.id == self.settings.source1_id)
                    .map(|e| e.dc.mode)
            })
            .flatten();
        card_mode(
            self.settings.config.mode,
            self.pending_run
                || matches!(
                    self.state.phase,
                    Phase::Preparing | Phase::Running | Phase::CleaningUp
                ) && self.pending_dc.is_none(),
            dc,
            editor,
        )
    }

    fn primary_dc_context(&self) -> bool {
        if self.pending_run
            || matches!(
                self.state.phase,
                Phase::Preparing | Phase::Running | Phase::CleaningUp
            ) && self.pending_dc.is_none()
        {
            return false;
        }
        self.card_control_mode() != self.settings.config.mode
            || self.dc_open.contains(&self.settings.source1_id)
            || self.state.source_dc.is_some() && self.state.output == OutputState::On
    }

    /// Keep source protection beside the selected source, rather than hiding it
    /// in a run-level dialog. This is the protection value for the active mode.
    fn source_compliance_control(&mut self, ui: &mut egui::Ui) {
        let mode = self.card_control_mode();
        let dc_context =
            self.primary_dc_context() && !self.pending_run && self.state.phase != Phase::Running;
        let unit = if mode == Mode::Iv { "A" } else { "V" };
        let effective = self
            .preview
            .as_ref()
            .ok()
            .map(|p| p.hardware_compliance)
            .unwrap_or(self.settings.config.compliance);
        let held = self
            .state
            .source_dc
            .as_ref()
            .filter(|_| self.state.output == OutputState::On)
            .cloned();
        ui.add_space(4.);
        ui.horizontal(|ui| {
            form_label(ui,"Compliance",72.);
            let width=ui.available_width();
            ui.add_enabled_ui(!self.busy() && !self.python_locked() && !(dc_context && held.is_some()),|ui| {
                if dc_context {
                    if let Some(entry)=self.settings.sources.iter_mut().find(|e|e.id==self.settings.source1_id) {
                        let mut value=held.as_ref().map_or(entry.dc.compliance,|d|d.compliance);
                        quantity(ui,"dc_card_compliance",&mut value,&mut entry.dc.units[1],unit,width,true);
                        if held.is_none() { entry.dc.compliance=value; }
                    }
                } else {
                    let (saved,units)=match mode {
                        Mode::Iv=>(&mut self.settings.source_current_protection,&mut self.settings.source_protection_units[0]),
                        Mode::Vi=>(&mut self.settings.source_voltage_protection,&mut self.settings.source_protection_units[1]),
                    };
                    let mut value=saved.unwrap_or(effective); let before=value;
                    quantity(ui,"source_protection",&mut value,units,unit,width,true);
                    if value!=before { *saved=Some(value); }
                }
            }).response.on_disabled_hover_text("Finish the current operation and turn DC output OFF before changing compliance.");
        }).response.on_hover_text(if dc_context {
            if mode==Mode::Iv { "DC voltage output · current compliance. Changes apply on the next explicit Output ON." } else { "DC current output · voltage compliance. Changes apply on the next explicit Output ON." }
        } else if mode==Mode::Iv { "Voltage source · current compliance" } else { "Current source · voltage compliance" })
        .on_hover_ui(|ui| { if !dc_context { self.source_protection_effective_note(ui,unit); } });
        self.settings.activate_mode_protection();
    }

    /// Meter stop and range controls are intentionally separate: the former
    /// stops a run while the latter selects the meter input path.
    fn meter_measurement_controls(&mut self, ui: &mut egui::Ui) {
        let width = (ui.available_width() - 78.0).max(125.0);
        let mode = self.card_control_mode();
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
            form_label(ui,"Limit",72.0);
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

        }).response.on_hover_text("Sweep stop threshold for the measured magnitude. It does not select a range or replace DC hardware compliance.");
        if mode == self.settings.config.mode {
            preserve_source_on_meter_edit(
                &mut self.settings,
                previous_meter_limit,
                effective_source,
            );
        } else {
            preserve_source_on_meter_edit_for_mode(
                &mut self.settings,
                mode,
                previous_meter_limit,
                previous_meter_limit,
            );
        }
        ui.add_space(4.0);

        let meter_model = self.settings.meter_model.clone();
        let range_locked_by_operation = self.busy() || self.python_locked();
        let range_edit = ui.add_enabled_ui(
            !range_locked_by_operation && crate::gui_control::outputs_off(&self.state),
            |ui| {
                ui.horizontal(|ui| {
                    form_label(ui, "Range", 72.0);
                    match mode {
                        Mode::Iv => widgets::meter_range_choice(
                            ui,
                            "active_meter_current_range",
                            &meter_model,
                            mode,
                            &mut self.settings.config.meter_current_range,
                        ),
                        Mode::Vi => widgets::meter_range_choice(
                            ui,
                            "active_meter_voltage_range",
                            &meter_model,
                            mode,
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
        let fixed_range = match mode {
            Mode::Iv => self.settings.config.meter_current_range,
            Mode::Vi => self.settings.config.meter_voltage_range,
        };
        if let Some(range) = fixed_range {
            if let Some(resistance) = iv_core::meter_input_resistance(&meter_model, mode, range) {
                let label = match resistance.kind {
                    iv_core::MeterResistanceKind::Shunt => "Shunt resistance",
                    iv_core::MeterResistanceKind::Terminal => "Input resistance",
                };
                ui.label(
                    RichText::new(format!("{label}: {}", fmt_quantity(resistance.ohms, "ohm")))
                        .small()
                        .color(MUTED),
                );
            } else {
                let note = if mode == Mode::Vi {
                    "Input impedance depends on instrument setting"
                } else if meter_model == "SIMULATED_METER" {
                    "Simulation · no physical input"
                } else if meter_model == "ADCMT_7461A" {
                    "Range monitoring unavailable for this model"
                } else {
                    "Input resistance: not verified"
                };
                ui.label(RichText::new(note).small().color(MUTED));
            }
            let meter_limit = match mode {
                Mode::Iv => self.settings.current_compliance,
                Mode::Vi => self.settings.voltage_compliance,
            };
            if meter_limit > range {
                ui.label(
                    RichText::new("Meter limit exceeds fixed range; overload may stop first.")
                        .small()
                        .color(ORANGE),
                );
            }
        } else {
            let auto_note = if meter_model == "ADCMT_7461A" {
                "Auto · range monitoring unavailable for this model"
            } else {
                "Auto · range monitoring on"
            };
            ui.label(RichText::new(auto_note).small().color(MUTED));
        }
        self.settings.activate_mode_protection();
    }

    fn measurement_controls(&mut self, ui: &mut egui::Ui) {
        section(ui, "Measurement", |ui| {
            ui.columns(2, |columns| {
                columns[0]
                    .add_enabled_ui(!self.busy() && !self.python_locked(), |ui| self.sweep(ui));
                section(&mut columns[1], "Output", |ui| {
                    ui.add_enabled_ui(!self.busy() && !self.python_locked(), |ui| {
                        let width = (ui.available_width() - 62.0).max(150.0);
                        egui::Grid::new("output_files")
                            .num_columns(2)
                            .spacing([6.0, 8.0])
                            .show(ui, |ui| {
                                form_label(ui, "Directory", 54.0);
                                ui.horizontal(|ui| {
                                    let summary = path_summary(&self.settings.output_dir);
                                    compact_field(
                                        ui,
                                        "output_dir",
                                        &mut self.settings.output_dir,
                                        (width - 118.0).max(35.0),
                                        summary,
                                    );
                                    if button(ui, "Browse", 104.0, true).clicked()
                                        && let Some(path) = rfd::FileDialog::new()
                                            .set_title("Output directory")
                                            .set_directory(&self.settings.output_dir)
                                            .pick_folder()
                                    {
                                        self.settings.output_dir = path.display().to_string();
                                    }
                                });
                                ui.end_row();
                                form_label(ui, "File", 54.0);
                                ui.horizontal(|ui| {
                                    ui.add(
                                        egui::TextEdit::singleline(&mut self.settings.filename)
                                            .margin(egui::Margin::symmetric(3, 3))
                                            .desired_width((width - 118.0).max(35.0)),
                                    );
                                    ui.add_sized(
                                        [104.0, 24.0],
                                        egui::Checkbox::new(
                                            &mut self.settings.auto_suffix,
                                            "Auto suffix",
                                        ),
                                    );
                                });
                                ui.end_row();
                            });
                    });
                });
                columns[1].add_space(8.0);
                section(&mut columns[1], "Run", |ui| self.run_controls(ui));
            });
        });
    }

    /// Inventory management stays beside the device cards; Settings only
    /// assigns their measurement roles and the dual-source ratio.
    fn device_inventory_actions(&mut self, ui: &mut egui::Ui) {
        let editable = !self.busy()
            && !self.state.source_connected
            && !self.state.source2_connected
            && !self.state.meter_connected
            && !self.state.gate_connected;
        ui.horizontal(|ui| {
            if button(ui, "+ Source", 82.0, editable)
                .on_disabled_hover_text("Disconnect the active instruments before adding a device.")
                .clicked()
            {
                self.settings.add_source();
            }
            if button(ui, "+ Meter", 78.0, editable)
                .on_disabled_hover_text("Disconnect the active instruments before adding a device.")
                .clicked()
            {
                self.settings.add_meter();
            }
        });
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
            ui.add_space(6.0);
            let locked = self.busy()
                || self.state.source_connected
                || self.state.source2_connected
                || self.state.meter_connected
                || self.state.gate_connected;
            let source_choices = self
                .settings
                .sources
                .iter()
                .enumerate()
                .map(|(index, entry)| {
                    (
                        entry.id,
                        format!("Source {} · {}", index + 1, device_summary(&entry.model)),
                    )
                })
                .collect::<Vec<_>>();
            let meter_choices = self
                .settings
                .meters
                .iter()
                .enumerate()
                .map(|(index, entry)| {
                    (
                        entry.id,
                        format!("Meter {} · {}", index + 1, device_summary(&entry.model)),
                    )
                })
                .collect::<Vec<_>>();
            let mut changed = false;
            let response = ui.add_enabled_ui(!locked, |ui| {
                egui::Grid::new("device_role_bindings")
                    .num_columns(2)
                    .spacing([6.0, 6.0])
                    .show(ui, |ui| {
                        form_label(ui, "Source 1", 65.0);
                        let before = self.settings.source1_id;
                        egui::ComboBox::from_id_salt("source1_binding")
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
                                .width(ui.available_width())
                                .selected_text(self.inventory_name(true, before))
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

                        form_label(ui, "Meter", 65.0);
                        let before = self.settings.meter_id;
                        egui::ComboBox::from_id_salt("meter_binding")
                            .width(ui.available_width())
                            .selected_text(self.inventory_name(false, before))
                            .show_ui(ui, |ui| {
                                for (id, label) in &meter_choices {
                                    ui.selectable_value(&mut self.settings.meter_id, *id, label);
                                }
                            });
                        changed |= before != self.settings.meter_id;
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
                        .map_or(self.settings.source1_id, |entry| entry.id);
                }
                self.settings.bind_active_entries();
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
            .position(|entry| entry.id == id)
            .map(|index| {
                format!(
                    "{role} {} · {}",
                    index + 1,
                    device_summary(&entries[index].model)
                )
            })
            .unwrap_or_else(|| format!("{role} unavailable"))
    }

    fn save_measurement(&mut self) {
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
        let Some(destination) = rfd::FileDialog::new()
            .set_title("Save measurement copy")
            .add_filter("CSV measurement", &["csv"])
            .set_directory(&self.settings.output_dir)
            .set_file_name(format!("{stem}_copy.csv"))
            .save_file()
        else {
            return;
        };
        let destination = if destination
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("csv"))
        {
            destination
        } else {
            destination.with_extension("csv")
        };
        match crate::run_export::save_run_copy(&source, &destination) {
            Ok(()) => {
                let message = format!(
                    "Saved {} points to {}\nThe companion .run.json contains settings and .readings.jsonl retains acquisition evidence.",
                    self.state.points,
                    destination.display()
                );
                self.report(message.clone());
                self.save_notice = Some(message);
            }
            Err(error) => {
                self.report(format!("Save failed: {error}"));
                self.save_notice = Some(format!(
                    "Could not save the measurement copy.\n\n{error}\n\nThe original measurement files are unchanged."
                ));
            }
        }
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
            let start_ready = self.state.connected && self.preview.is_ok();
            let start_button = egui::Button::new(RichText::new("Start").strong())
                .min_size(Vec2::new(width, 28.0))
                .fill(if start_ready {
                    Color32::from_rgb(45, 112, 224)
                } else {
                    BUTTON
                });
            if ui
                .add_enabled(
                    !self.busy() && !self.pending_run && !self.python_locked(),
                    start_button,
                )
                .on_disabled_hover_text("Wait for the current operation to finish, or use Stop.")
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
            if ui.add_enabled(self.active(), stop_button).clicked() && self.send(Command::Stop) {
                self.report(
                    "Stop requested. Waiting for the current I/O and output cleanup.".into(),
                );
            }
            if button(
                ui,
                "Save",
                width,
                !self.busy() && !self.pending_run && !self.python_locked(),
            )
            .on_hover_text("Save a complete copy of the acquired CSV and run settings.")
            .on_disabled_hover_text(
                "Stop the measurement and wait for cleanup before saving a copy.",
            )
            .clicked()
            {
                self.save_measurement();
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
            .frame(egui::Frame::popup(&ctx.style()).fill(BG).inner_margin(18))
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
        let response = egui::Modal::new(egui::Id::new("measurement_settings"))
            .frame(egui::Frame::popup(&ctx.style()).fill(BG).inner_margin(16))
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
                        if let Some(dual) = &mut self.settings.config.balanced_source {
                            section(ui, "Dual source", |ui| {
                                ui.label("Source 2 compliance");
                                quantity(
                                    ui,
                                    "source2_compliance",
                                    &mut dual.source_compliance,
                                    &mut self.settings.source2_compliance_unit,
                                    "A",
                                    ui.available_width(),
                                    true,
                                );
                            });
                            ui.add_space(8.0);
                        }
                        egui::CollapsingHeader::new("Advanced").show(ui, |ui| {
                            let unit = if self.settings.config.mode == Mode::Iv { "mV" } else { "mA" };
                            number(ui, "Maximum source target", &mut self.settings.config.source_limit, 0.001, unit, 0.0..=f64::MAX);
                            ui.label(RichText::new("Software planning bound for Start, End, and custom targets. It is separate from source hardware protection.").small().color(MUTED));
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
                                self.settings.config.custom_points = self.custom_text.split(',').map(|v| v.trim().parse::<f64>()).collect::<Result<Vec<_>, _>>().unwrap_or_default();
                            }
                        }
                    });
                });
                ui.add_space(10.0);
                ui.label("Use Config -> Save to save these settings to the file.");
                ui.add_space(8.0);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if button(ui, "Cancel", 76.0, true).clicked() { ui.close(); }
                    if button(ui, "OK", 76.0, !self.busy()).clicked() { accepted = true; ui.close(); }
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

    fn detail_panel(&mut self, ctx: &egui::Context) {
        let width = if self.details { 288.0 } else { 40.0 };
        egui::SidePanel::right("details")
            .exact_width(width)
            .resizable(false)
            .frame(egui::Frame::new().fill(BG).inner_margin(8))
            .show(ctx, |ui| {
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
                                                            for (index, field) in
                                                                SNAPSHOT_FIELDS.iter().enumerate()
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
        self.measurement_controls(ui);
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new("Status").color(MUTED));
            let status = if self.busy() {
                format!(
                    "{} ({:.0}s)",
                    self.state.phase.to_lowercase(),
                    self.phase_start.elapsed().as_secs_f64()
                )
            } else {
                self.state.phase.to_lowercase()
            };
            let width = ui.available_width() / 4.0;
            ui.add_sized([width * 0.75, 20.0], egui::Label::new(status).truncate())
                .on_hover_text(format!(
                    "{}\nOutput: {}",
                    self.state.message, self.state.output
                ));
            ui.label(RichText::new("Point").color(MUTED));
            ui.label(if self.state.total == 0 {
                "-".into()
            } else {
                format!("{}/{}", self.state.points, self.state.total)
            });
            ui.add_space(8.0);
            ui.label(RichText::new("Resistance").color(MUTED));
            ui.label(
                self.state
                    .resistance
                    .map(|r| fmt_quantity(r, "Ohm"))
                    .unwrap_or_else(|| "-".into()),
            );
            ui.add_space(8.0);
            ui.label(RichText::new("Conductance").color(MUTED));
            ui.label(
                self.state
                    .resistance
                    .filter(|r| *r != 0.0)
                    .map(|r| fmt_quantity(1.0 / r, "S"))
                    .unwrap_or_else(|| "-".into()),
            );
        });
        ui.add_space(8.0);
        let graph_height = (ui.clip_rect().bottom() - ui.cursor().top() - 2.0).max(320.0);
        let [x_axis, y_axis] = measurement_definition(self.settings.config.mode).axes;
        let (x_name, x_unit, y_name, y_unit) = (
            if self.settings.config.balanced_source.is_some() {
                "Differential voltage"
            } else {
                x_axis.name
            },
            x_axis.unit,
            y_axis.name,
            y_axis.unit,
        );
        let mut extent = self.plot_extent;
        if self.samples.is_empty() {
            extent[0] = self
                .settings
                .config
                .start
                .abs()
                .max(self.settings.config.stop.abs());
        }
        let plot_id = ui.make_persistent_id(egui::Id::new("iv_plot"));
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
        let samples = &self.samples;
        let plotted = Plot::new("iv_plot")
            .id(plot_id)
            .height(graph_height)
            .show_background(false)
            .grid_spacing(16.0..=12000.0)
            .x_axis_label(format!("{x_name} ({}{x_unit})", x_scale.prefix))
            .y_axis_label(format!("{y_name} ({}{y_unit})", y_scale.prefix))
            .x_axis_formatter(move |mark, _| x_scale.tick(mark.value, mark.step_size))
            .y_axis_formatter(move |mark, _| y_scale.tick(mark.value, mark.step_size))
            .label_formatter(move |name, point| {
                let resolution = samples
                    .iter()
                    .rev()
                    .find(|s| s.0 == point.x && s.1 == point.y)
                    .map(|s| s.3)
                    .unwrap_or([None; 2]);
                format!(
                    "{name}\n{x_name}: {}\n{y_name}: {}",
                    display_reading(point.x, resolution[0], x_unit),
                    display_reading(point.y, resolution[1], y_unit)
                )
            })
            .legend(Legend::default().background_alpha(0.45))
            .allow_boxed_zoom(true)
            .allow_scroll(false)
            .allow_drag(true)
            .include_x(0.0)
            .include_y(0.0)
            .auto_bounds(true)
            .show(ui, |plot_ui| {
                for (values, (name, color)) in self.plot_series.iter().zip([
                    ("forward", BLUE),
                    ("backward", PURPLE),
                    ("custom", TEAL),
                ]) {
                    // NaN is an internal segment delimiter, never a renderer
                    // coordinate: path tessellation otherwise drops end edges.
                    for segment in values
                        .split(|p| !p.x.is_finite() || !p.y.is_finite())
                        .filter(|s| !s.is_empty())
                    {
                        plot_ui.line(
                            Line::new(name, PlotPoints::Borrowed(segment))
                                .color(color)
                                .width(if name == "forward" { 3.0_f32 } else { 2.0_f32 })
                                .style(if name == "backward" {
                                    LineStyle::dashed_dense()
                                } else {
                                    LineStyle::Solid
                                }),
                        );
                        plot_ui.points(
                            Points::new(name, PlotPoints::Borrowed(segment))
                                .color(color)
                                .radius(if name == "backward" { 4.0_f32 } else { 2.5_f32 })
                                .shape(if name == "backward" {
                                    MarkerShape::Diamond
                                } else {
                                    MarkerShape::Circle
                                })
                                .filled(name != "backward"),
                        );
                    }
                }
            });
        // Zoom/pan bounds are applied during show. If a prefix changed, finish
        // the label transition on the next frame even after input becomes idle.
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

impl eframe::App for IvApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let ui_start = Instant::now();
        if self.first_frame_s.is_none() {
            self.first_frame_s = Some(self.boot.elapsed().as_secs_f64());
        }
        self.sync();
        // Reconcile the idle live channel after closing DC or finishing a
        // sweep. This changes only an already-OFF source function, never ON.
        let control_mode = self.card_control_mode();
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
        self.python_banner(ctx);
        ctx.request_repaint_after(Duration::from_millis(100));
        if ctx.input(|i| i.viewport().close_requested()) && self.active() {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            if self.send(Command::Stop) {
                self.closing = true;
                self.notice = "Stopping and cleaning up before closing…".into();
            }
        }
        if self.closing && !self.active() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        self.sidebar(ctx);
        self.detail_panel(ctx);
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(BG).inner_margin(8))
            .show(ctx, |ui| self.main_view(ui));
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
        self.timing_dialog(ctx);
        if self.protection_dialog.is_some() {
            self.protection_dialog(ctx);
        } else {
            self.start_error_dialog(ctx);
        }
        self.save_dialog(ctx);
        if self.busy() || self.state.connected || self.state.gate_connected {
            ctx.request_repaint_after(Duration::from_millis(33));
        }
        if self.smoke && !self.smoke_started && self.started.elapsed() > Duration::from_millis(100)
        {
            self.smoke_started = true;
            self.send(Command::Connect {
                source_model: self.settings.config.source_model.clone(),
                meter_model: self.settings.config.meter_model.clone(),
                simulation: self.settings.simulation,
                source: self.settings.source.clone(),
                meter: self.settings.meter.clone(),
            });
            if self.settings.config.balanced_source.is_some() {
                self.connect_secondary();
            }
        }
        let control_smoke = self.smoke && std::env::args().any(|arg| arg == "--gui-control-smoke");
        let dc_smoke = self.smoke && std::env::args().any(|arg| arg == "--gui-dc-smoke");
        if dc_smoke {
            self.dc_smoke_step();
        }

        if self.smoke
            && !control_smoke
            && !dc_smoke
            && self.state.connected
            && (self.settings.config.balanced_source.is_none() || self.state.source2_connected)
            && self.state.run_id == 0
            && !self.pending_run
        {
            self.start_run();
        }
        let limit_smoke = self.smoke && std::env::args().any(|arg| arg == "--gui-limit-smoke");
        let smoke_outcome = if dc_smoke {
            self.dc_smoke_stage == 8
                && self.state.phase == Phase::Completed
                && self.state.source_dc.is_none()
                && self.state.gate_output == OutputState::On
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
            self.state.phase == Phase::Completed && self.state.points == self.state.total
        };
        if self.screenshot.is_some()
            && !self.screenshot_requested
            && self.started.elapsed() > Duration::from_secs(2)
            && (!self.smoke || smoke_outcome)
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
                    "first_ui_update_s":self.first_frame_s,"mean_ui_cpu_ms":self.ui_cpu_s*1000.0/self.ui_frames.max(1) as f64})
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
                "{reason}\n\nCurrent source compliance: {limit}\nInstrument status: {}\n\nCheck the sample, current-path contacts and wiring. Four-terminal Meter voltage can differ from the source terminal voltage. Reduce the sweep range, or review Source card > Source compliance. Increase compliance only if the sample and wiring ratings permit it.",
                alert.indicator
            )
        }
        ProtectionKind::MeasurementLimit => {
            let reading = alert
                .observed
                .map(|value| fmt_quantity(value, &alert.unit))
                .unwrap_or_else(|| "Unavailable".into());
            format!(
                "Measurement limit reached.\n\nCurrent Meter limit: {limit}\nTrigger reading: {reading}\n\nCheck the sample and wiring. Reduce the sweep range, or review Meter card > Meter limit. Increase the limit only if safe for the sample. Changing this limit does not change Source compliance."
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
            "Reduce Start / End, or review Maximum source target in Settings > Advanced. Increase a limit only when it is safe for your setup."
        } else if reason.contains("must be a multiple") {
            "Set Start / End to whole multiples of the source resolution stated in the error. Step and ramp are adjusted automatically."
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
    fn compact_cards_follow_sweep_and_dc_function_without_mixing_ranges() {
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
                "normal"
            ]
            .map(str::to_string)
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
        assert!(text.contains("Source card > Source compliance"));
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
        assert!(text.contains("Meter card > Meter limit"));
        assert!(text.contains("will not resume automatically"));
    }
}
