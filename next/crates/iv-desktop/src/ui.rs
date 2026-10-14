use crate::client::Client;
use crate::protocol::{Command, Snapshot};
use eframe::egui::{self, Color32, RichText, Stroke, Vec2};
use egui_plot::{Line, Plot, PlotPoint, PlotPoints, Points};
use iv_core::{IV_FIELDS, Mode, Pattern, RunConfig};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};

const BG: Color32 = Color32::from_rgb(5, 5, 5);
const PANEL: Color32 = Color32::from_rgb(11, 11, 11);
const INPUT: Color32 = Color32::from_rgb(17, 17, 17);
const BUTTON: Color32 = Color32::from_rgb(27, 27, 27);
const BORDER: Color32 = Color32::from_rgb(51, 51, 51);
const TEXT: Color32 = Color32::from_rgb(232, 232, 232);
const BLUE: Color32 = Color32::from_rgb(90, 169, 255);
const ORANGE: Color32 = Color32::from_rgb(255, 159, 67);
const PURPLE: Color32 = Color32::from_rgb(197, 163, 255);
const RED: Color32 = Color32::from_rgb(255, 123, 130);
const SOURCE_MODELS: &[&str] = &["YOKOGAWA_GS210", "YOKOGAWA_7651", "SIMULATED_SOURCE"];
const METER_MODELS: &[&str] = &[
    "AGILENT_34401A",
    "AGILENT_34411A",
    "KEYSIGHT_34411A",
    "KEYSIGHT_34465A",
    "ADCMT_7461A",
    "SIMULATED_METER",
];
const SNAPSHOT_FIELDS: &[&str] = &[
    "timestamp",
    "elapsed_s",
    "point_index",
    "total_points",
    "direction",
    "source_target_value",
    "source_target_unit",
    "source_readback_value",
    "source_readback_unit",
    "voltage_V",
    "voltage_origin",
    "current_A",
    "current_origin",
    "resistance_Ohm",
    "conductance_S",
];

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
struct Settings {
    config: RunConfig,
    source: String,
    meter: String,
    output_dir: String,
    filename: String,
    simulation: bool,
    source_model: String,
    meter_model: String,
    auto_suffix: bool,
    units: [usize; 3],
    current_compliance: f64,
    voltage_compliance: f64,
    compliance_units: [usize; 2],
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            config: RunConfig::default(),
            source: String::new(),
            meter: String::new(),
            output_dir: user_dir().join("results").display().to_string(),
            filename: "iv_run".into(),
            simulation: false,
            source_model: "YOKOGAWA_GS210".into(),
            meter_model: "AGILENT_34411A".into(),
            auto_suffix: true,
            units: [1; 3],
            current_compliance: 0.001,
            voltage_compliance: 1.0,
            compliance_units: [1, 0],
        }
    }
}

fn user_dir() -> PathBuf {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join(".kohdalab")
}

fn settings_path() -> PathBuf {
    user_dir().join("config").join("iv-next.json")
}

fn save_settings(settings: &Settings, path: &Path) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let bytes = serde_json::to_vec_pretty(settings).map_err(|e| e.to_string())?;
    std::fs::write(path, bytes).map_err(|e| e.to_string())
}

// Draw a compact group box with its title interrupting the top border, as in
// the original Qt interface. The frame contributes its real measured height.
fn section(ui: &mut egui::Ui, title: &str, add: impl FnOnce(&mut egui::Ui)) {
    ui.add_space(10.0);
    let frame = egui::Frame::new()
        .fill(PANEL)
        .corner_radius(4)
        .stroke(Stroke::new(1.0_f32, BORDER))
        .inner_margin(egui::Margin {
            left: 10,
            right: 10,
            top: 14,
            bottom: 12,
        })
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            add(ui);
        });
    let galley = ui
        .painter()
        .layout_no_wrap(title.into(), egui::FontId::proportional(12.0), TEXT);
    let position = frame.response.rect.left_top() + Vec2::new(12.0, -7.0);
    let background = egui::Rect::from_min_size(
        position - Vec2::new(4.0, 0.0),
        galley.size() + Vec2::new(8.0, 0.0),
    );
    ui.painter().rect_filled(background, 0, BG);
    ui.painter().galley(position, galley, TEXT);
}

fn button(ui: &mut egui::Ui, text: &str, width: f32, enabled: bool) -> egui::Response {
    ui.add_enabled(
        enabled,
        egui::Button::new(text).min_size(Vec2::new(width, 24.0)),
    )
}

fn form_label(ui: &mut egui::Ui, text: &str, width: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 24.0), egui::Sense::hover());
    ui.painter().text(
        rect.right_center(),
        egui::Align2::RIGHT_CENTER,
        text,
        egui::FontId::proportional(12.0),
        TEXT,
    );
}

fn fmt_quantity(value: f64, unit: &str) -> String {
    if value == 0.0 {
        return format!("0 {unit}");
    }
    let mag = value.abs();
    let (scale, prefix) = if mag >= 1e6 {
        (1e6, "M")
    } else if mag >= 1e3 {
        (1e3, "k")
    } else if mag >= 1.0 {
        (1.0, "")
    } else if mag >= 1e-3 {
        (1e-3, "m")
    } else if mag >= 1e-6 {
        (1e-6, "µ")
    } else if mag >= 1e-9 {
        (1e-9, "n")
    } else {
        (1e-12, "p")
    };
    format!("{:.4} {prefix}{unit}", value / scale)
}

fn plot_series(samples: &[(f64, f64, String)]) -> [Vec<PlotPoint>; 3] {
    let mut series = std::array::from_fn(|_| Vec::new());
    let mut previous = None;
    for (x, y, direction) in samples {
        append_plot_point(&mut series, previous, *x, *y, direction);
        previous = Some(direction.as_str());
    }
    series
}

fn append_plot_point(
    series: &mut [Vec<PlotPoint>; 3],
    previous: Option<&str>,
    x: f64,
    y: f64,
    direction: &str,
) {
    let index = |direction: &str| match direction {
        "forward" => 0,
        "backward" => 1,
        _ => 2,
    };
    let current = index(direction);
    if let Some(last) = previous.map(index).filter(|last| *last != current) {
        series[last].push(PlotPoint::new(f64::NAN, f64::NAN));
    }
    series[current].push(PlotPoint::new(x, y));
}

pub struct IvApp {
    client: Result<Client, String>,
    settings: Settings,
    state: Snapshot,
    previous_phase: String,
    previous_log: String,
    state_revision: u64,
    run_id: u64,
    samples: Vec<(f64, f64, String)>,
    plot_series: [Vec<PlotPoint>; 3],
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
    preview: Result<usize, String>,
    success: Arc<AtomicBool>,
    boot: Instant,
    first_frame_s: Option<f64>,
    ui_cpu_s: f64,
    ui_frames: usize,
    pending_run: bool,
    pending_base: u64,
}

impl IvApp {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        smoke: bool,
        screenshot: Option<PathBuf>,
        success: Arc<AtomicBool>,
        boot: Instant,
        hardware: Option<(String, String)>,
    ) -> Self {
        style(&cc.egui_ctx);
        let ctx = cc.egui_ctx.clone();
        let client = Client::start(move || ctx.request_repaint());
        let mut settings = std::fs::read(settings_path())
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        if smoke {
            settings = Settings {
                simulation: true,
                ..Default::default()
            };
            settings.config.start_settle_s = 0.03;
            settings.config.settle_s = 0.02;
            settings.output_dir = std::env::temp_dir()
                .join("kohdalab-iv-next-smoke")
                .display()
                .to_string();
            if let Some((source, meter)) = hardware {
                settings.simulation = false;
                settings.source = source;
                settings.meter = meter;
                settings.config = RunConfig {
                    pattern: Pattern::Linear,
                    step: 0.001,
                    ..Default::default()
                };
            }
        }
        if settings.simulation {
            settings.source_model = "SIMULATED_SOURCE".into();
            settings.meter_model = "SIMULATED_METER".into();
        }
        let preview = iv_core::plan(&settings.config).map(|p| p.points.len());
        let validated_config = settings.config.clone();
        let custom_text = settings
            .config
            .custom_points
            .iter()
            .map(|v| v.to_string())
            .collect::<Vec<_>>()
            .join(", ");
        Self {
            client,
            settings,
            state: Snapshot::default(),
            previous_phase: String::new(),
            previous_log: String::new(),
            state_revision: 0,
            run_id: 0,
            samples: vec![],
            plot_series: std::array::from_fn(|_| Vec::new()),
            table_values: vec![],
            table_run_id: u64::MAX,
            table_points: usize::MAX,
            details: true,
            left_visible: true,
            table_visible: true,
            timing_open: false,
            timing_before: None,
            config_path: settings_path().display().to_string(),
            notice: String::new(),
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
        }
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
        matches!(
            self.state.phase.as_str(),
            "Connecting" | "Scanning" | "Preparing" | "Running" | "Cleaning up"
        )
    }

    fn active(&self) -> bool {
        matches!(
            self.state.phase.as_str(),
            "Preparing" | "Running" | "Cleaning up"
        )
    }

    fn sync(&mut self) {
        let config_changed = self.settings.config != self.validated_config;
        if config_changed {
            self.preview = iv_core::plan(&self.settings.config).map(|p| p.points.len());
            self.validated_config = self.settings.config.clone();
        }
        let mut state_changed = false;
        if let Ok(client) = &self.client {
            let Some(next) = client.snapshot_since(&mut self.state_revision) else {
                self.refresh_table_values(config_changed, false);
                return;
            };
            state_changed = true;
            let new_run = next.run_id > self.run_id;
            if new_run {
                self.run_id = next.run_id;
                self.samples.clear();
                self.plot_series.iter_mut().for_each(Vec::clear);
                self.notice.clear();
            }
            for (record_run, point) in client.drain_points() {
                if record_run < self.run_id {
                    continue;
                }
                if record_run > self.run_id {
                    self.samples.clear();
                    self.plot_series.iter_mut().for_each(Vec::clear);
                    self.run_id = record_run;
                }
                let source = point.source_readback.unwrap_or(point.source_set);
                let xy = (source, point.meter_value);
                append_plot_point(
                    &mut self.plot_series,
                    self.samples.last().map(|sample| sample.2.as_str()),
                    xy.0,
                    xy.1,
                    &point.point.direction,
                );
                self.samples.push((xy.0, xy.1, point.point.direction));
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
            if next.phase != self.previous_phase {
                self.phase_start = Instant::now();
                self.previous_phase = next.phase.clone();
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
            if self.pending_run && next.run_id <= self.pending_base && next.phase != "Failed" {
                self.state.phase = "Preparing".into();
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
            .map(|record| record.csv_values(&self.settings.config))
            .unwrap_or_default();
        self.table_run_id = self.state.run_id;
        self.table_points = self.state.points;
    }

    fn start_run(&mut self) {
        if self.pending_run {
            return;
        }
        if let Err(error) = iv_core::plan(&self.settings.config) {
            self.report(error);
            return;
        }
        let name = self.settings.filename.trim();
        if name.is_empty() || name.contains(['/', '\\']) {
            self.report("Use a file name without path separators.".into());
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
        if !self.smoke {
            self.settings.simulation = self.both_simulated();
            let _ = save_settings(&self.settings, Path::new(&self.config_path));
        }
        if !self.send(Command::Run {
            config: self.settings.config.clone(),
            path: path.display().to_string(),
        }) {
            return;
        }
        self.pending_run = true;
        self.pending_base = self.state.run_id;
        self.state.phase = "Preparing".into();
    }

    fn both_simulated(&self) -> bool {
        self.settings.source_model == "SIMULATED_SOURCE"
            && self.settings.meter_model == "SIMULATED_METER"
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
                    rect.right_bottom(),
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
                            egui::ScrollArea::vertical()
                                .id_salt("settings_scroll")
                                .auto_shrink([false, false])
                                .show(ui, |ui| {
                                    self.profile(ui);
                                    ui.add_space(8.0);
                                    self.instrument(ui, true);
                                    ui.add_space(8.0);
                                    self.instrument(ui, false);
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
                    ui.add(
                        egui::TextEdit::singleline(&mut self.config_path)
                            .desired_width(field_width),
                    )
                    .on_hover_text(&self.config_path);
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
                        !self.state.source_connected && !self.state.meter_connected,
                    )
                    .clicked()
                    {
                        let loaded = std::fs::read(&self.config_path)
                            .map_err(|e| e.to_string())
                            .and_then(|bytes| {
                                serde_json::from_slice::<Settings>(&bytes)
                                    .map_err(|e| e.to_string())
                            });
                        match loaded {
                            Ok(mut settings) => {
                                if settings.simulation {
                                    settings.source_model = "SIMULATED_SOURCE".into();
                                    settings.meter_model = "SIMULATED_METER".into();
                                }
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
                        self.connect_instrument(false);
                    }
                    if button(
                        ui,
                        "All Disconnect",
                        row_width,
                        self.state.source_connected || self.state.meter_connected,
                    )
                    .clicked()
                    {
                        self.send(Command::Disconnect);
                    }
                });
            });
        });
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
                simulation,
                resource,
            });
        } else {
            self.send(Command::ConnectMeter {
                simulation,
                resource,
            });
        }
    }

    fn instrument(&mut self, ui: &mut egui::Ui, source: bool) {
        let title = if source { "Source" } else { "Meter" };
        section(ui, title, |ui| {
            let connected = if source {
                self.state.source_connected
            } else {
                self.state.meter_connected
            };
            let idle = !self.busy();
            let width = (ui.available_width() - 65.0).max(140.0);
            egui::Grid::new(("instrument", source))
                .num_columns(2)
                .spacing([6.0, 12.0])
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
                                for candidate in if source { SOURCE_MODELS } else { METER_MODELS } {
                                    ui.add_enabled_ui(model_supported(candidate), |ui| {
                                        ui.selectable_value(
                                            model,
                                            candidate.to_string(),
                                            *candidate,
                                        );
                                    })
                                    .response
                                    .on_disabled_hover_text(
                                        "Not yet supported in the Rust preview",
                                    );
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
                        form_label(ui, "NPLC", 57.0);
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
                        let valid = model_supported(model)
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
                        if button(ui, "Disconnect", (width - 8.0) / 2.0, idle && connected)
                            .clicked()
                        {
                            self.send(if source {
                                Command::DisconnectSource
                            } else {
                                Command::DisconnectMeter
                            });
                        }
                    });
                    ui.end_row();
                });
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
            ui.add(
                egui::TextEdit::singleline(selection)
                    .margin(egui::Margin::symmetric(2, 3))
                    .desired_width((width - 28.0).max(30.0)),
            )
            .on_hover_text(selection.as_str());
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
        let width = (ui.available_width() - 57.0).max(150.0);
        egui::Grid::new("sweep_conditions")
            .num_columns(2)
            .spacing([6.0, 8.0])
            .show(ui, |ui| {
                form_label(ui, "Mode", 45.0);
                egui::ComboBox::from_id_salt("mode")
                    .width(width)
                    .selected_text(mode_name(old_mode))
                    .show_ui(ui, |ui| {
                        ui.set_min_width(250.0);
                        ui.selectable_value(
                            &mut self.settings.config.mode,
                            Mode::Iv,
                            mode_name(Mode::Iv),
                        );
                        ui.selectable_value(
                            &mut self.settings.config.mode,
                            Mode::Vi,
                            mode_name(Mode::Vi),
                        );
                    });
                ui.end_row();
                if self.settings.config.mode != old_mode {
                    if old_mode == Mode::Iv {
                        self.settings.current_compliance = self.settings.config.compliance;
                    } else {
                        self.settings.voltage_compliance = self.settings.config.compliance;
                    }
                    self.settings.config = RunConfig {
                        mode: self.settings.config.mode,
                        ..Default::default()
                    };
                    if self.settings.config.mode == Mode::Vi {
                        self.settings.config.start = -1e-6;
                        self.settings.config.stop = 1e-6;
                        self.settings.config.step = 1e-7;
                        self.settings.config.source_limit = 1e-6;
                        self.settings.config.compliance = 1.0;
                        self.settings.config.ramp_step = 1e-7;
                        self.settings.units = [2; 3];
                    } else {
                        self.settings.units = [1; 3];
                    }
                    self.settings.config.compliance = if self.settings.config.mode == Mode::Iv {
                        self.settings.current_compliance
                    } else {
                        self.settings.voltage_compliance
                    };
                    self.samples.clear();
                    self.rebuild_plot_series();
                    self.custom_text.clear();
                }
                form_label(ui, "Sweep", 45.0);
                egui::ComboBox::from_id_salt("pattern")
                    .width(width.min(150.0))
                    .selected_text(pattern_name(self.settings.config.pattern))
                    .show_ui(ui, |ui| {
                        ui.set_min_width(230.0);
                        for pattern in [
                            Pattern::Linear,
                            Pattern::RoundTrip,
                            Pattern::ZeroCentered,
                            Pattern::CustomList,
                        ] {
                            ui.selectable_value(
                                &mut self.settings.config.pattern,
                                pattern,
                                pattern_name(pattern),
                            );
                        }
                    });
                ui.end_row();
                let unit = if self.settings.config.mode == Mode::Iv {
                    "V"
                } else {
                    "A"
                };
                for (index, (label, value)) in [
                    ("Start", &mut self.settings.config.start),
                    ("End", &mut self.settings.config.stop),
                    ("Step", &mut self.settings.config.step),
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
                    );
                    ui.end_row();
                }
                form_label(ui, "", 45.0);
                if button(ui, "Timing / Compliance…", width.min(196.0), true).clicked() {
                    if self.settings.config.mode == Mode::Iv {
                        self.settings.current_compliance = self.settings.config.compliance;
                    } else {
                        self.settings.voltage_compliance = self.settings.config.compliance;
                    }
                    self.timing_before = Some((self.settings.clone(), self.custom_text.clone()));
                    self.timing_open = true;
                }
                ui.end_row();
            });
    }

    fn measurement_controls(&mut self, ui: &mut egui::Ui) {
        section(ui, "Measurement", |ui| {
            ui.columns(2, |columns| {
                columns[0].add_enabled_ui(!self.busy(), |ui| self.sweep(ui));
                section(&mut columns[1], "Output", |ui| {
                    ui.add_enabled_ui(!self.busy(), |ui| {
                        let width = (ui.available_width() - 62.0).max(150.0);
                        egui::Grid::new("output_files")
                            .num_columns(2)
                            .spacing([6.0, 8.0])
                            .show(ui, |ui| {
                                form_label(ui, "Directory", 54.0);
                                ui.horizontal(|ui| {
                                    ui.add(
                                        egui::TextEdit::singleline(&mut self.settings.output_dir)
                                            .margin(egui::Margin::symmetric(3, 3))
                                            .desired_width((width - 118.0).max(35.0)),
                                    )
                                    .on_hover_text(&self.settings.output_dir);
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

    fn run_controls(&mut self, ui: &mut egui::Ui) {
        let ready = self.state.connected
            && !self.busy()
            && self.preview.is_ok()
            && self.state.source_simulation == self.state.meter_simulation;
        let width = (ui.available_width() - 16.0) / 3.0;
        ui.horizontal(|ui| {
            if button(ui, "Start", width, ready)
                .on_disabled_hover_text(
                    self.preview.as_ref().err().map(String::as_str).unwrap_or(
                        "Connect two real instruments or two simulated instruments first.",
                    ),
                )
                .clicked()
            {
                self.start_run();
            }
            if button(ui, "Stop", width, self.active()).clicked() && self.send(Command::Stop) {
                self.report(
                    "Stop requested. Waiting for the current I/O and output cleanup.".into(),
                );
            }
            if button(ui, "Output Off", width, self.state.source_connected).clicked() {
                self.send(Command::OutputOff);
            }
        });
    }

    fn timing_dialog(&mut self, ctx: &egui::Context) {
        if !self.timing_open {
            return;
        }
        let mut accepted = false;
        let response = egui::Modal::new(egui::Id::new("timing_compliance"))
            .frame(egui::Frame::popup(&ctx.style()).fill(BG).inner_margin(16))
            .show(ctx, |ui| {
                ui.set_width(360.0);
                ui.strong("Timing / Compliance");
                ui.add_space(8.0);
                egui::ScrollArea::vertical().max_height((ctx.content_rect().height() - 150.0).clamp(200.0, 600.0)).show(ui, |ui| {
                    ui.add_enabled_ui(!self.busy(), |ui| {
                        section(ui, "Timing", |ui| {
                            number(ui, "Wait time (s)", &mut self.settings.config.settle_s, 1.0, "");
                            ui.horizontal(|ui| { ui.label("Average count"); ui.add(egui::DragValue::new(&mut self.settings.config.average_count).range(1..=1000)); });
                        });
                        ui.add_space(8.0);
                        section(ui, "Compliance", |ui| {
                            let width = ui.available_width();
                            ui.label("Current limit (V source)");
                            quantity(ui, "current_compliance", &mut self.settings.current_compliance,
                                &mut self.settings.compliance_units[0], "A", width);
                            ui.add_space(8.0);
                            ui.label("Voltage limit (I source)");
                            quantity(ui, "voltage_compliance", &mut self.settings.voltage_compliance,
                                &mut self.settings.compliance_units[1], "V", width);
                            self.settings.config.compliance = if self.settings.config.mode == Mode::Iv {
                                self.settings.current_compliance
                            } else { self.settings.voltage_compliance };
                            ui.label("Stop when |measured value| ≥ limit.\nApplied after each averaged reading.");
                        });
                        ui.add_space(8.0);
                        egui::CollapsingHeader::new("Advanced").show(ui, |ui| {
                            let unit = if self.settings.config.mode == Mode::Iv { "mV" } else { "mA" };
                            number(ui, "Source limit", &mut self.settings.config.source_limit, 0.001, unit);
                            number(ui, "First wait (s)", &mut self.settings.config.start_settle_s, 1.0, "");
                            number(ui, "Ramp step", &mut self.settings.config.ramp_step, 0.001, unit);
                            number(ui, "Ramp wait (s)", &mut self.settings.config.ramp_wait_s, 1.0, "");
                            ui.horizontal(|ui| { ui.label("Repeat count"); ui.add(egui::DragValue::new(&mut self.settings.config.repeat).range(1..=1000)); });
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
                ui.label("Use Config → Save to save these settings to the file.");
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
        }
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
                            [24.0, height],
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
                                                            RichText::new(&self.state.error)
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
                                                            if !self.table_values.is_empty() {
                                                                for field in SNAPSHOT_FIELDS {
                                                                    let index = IV_FIELDS
                                                                        .iter()
                                                                        .position(|name| {
                                                                            name == field
                                                                        })
                                                                        .expect(
                                                                            "Known snapshot field",
                                                                        );
                                                                    ui.add_sized(
                                                                        [111.0, 24.0],
                                                                        egui::Label::new(*field)
                                                                            .truncate(),
                                                                    )
                                                                    .on_hover_text(*field);
                                                                    let value = self.table_values
                                                                        [index]
                                                                        .as_str();
                                                                    ui.add_sized(
                                                                        [111.0, 24.0],
                                                                        egui::Label::new(value)
                                                                            .truncate(),
                                                                    )
                                                                    .on_hover_text(value);
                                                                    ui.end_row();
                                                                }
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
            ui.label("Status");
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
            ui.label("Point");
            ui.label(if self.state.total == 0 {
                "-".into()
            } else {
                format!("{}/{}", self.state.points, self.state.total)
            });
            ui.add_space(8.0);
            ui.label("Resistance");
            ui.label(
                self.state
                    .resistance
                    .map(|r| fmt_quantity(r, "Ohm"))
                    .unwrap_or_else(|| "-".into()),
            );
            ui.add_space(8.0);
            ui.label("Conductance");
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
        let (x_label, y_label) = if self.settings.config.mode == Mode::Iv {
            ("Voltage (V)", "Current (A)")
        } else {
            ("Current (A)", "Voltage (V)")
        };
        Plot::new("iv_plot")
            .height(graph_height)
            .show_background(false)
            .grid_spacing(8.0..=12000.0)
            .x_axis_label(x_label)
            .y_axis_label(y_label)
            .allow_boxed_zoom(true)
            .allow_scroll(true)
            .allow_drag(true)
            .include_x(0.0)
            .include_y(0.0)
            .auto_bounds(true)
            .show(ui, |plot_ui| {
                for (values, (name, color)) in self.plot_series.iter().zip([
                    ("forward", BLUE),
                    ("backward", ORANGE),
                    ("custom", PURPLE),
                ]) {
                    if !values.is_empty() {
                        plot_ui.line(
                            Line::new(name, PlotPoints::Borrowed(values))
                                .color(color)
                                .width(2.0_f32),
                        );
                        plot_ui.points(
                            Points::new(name, PlotPoints::Borrowed(values))
                                .color(color)
                                .radius(2.5_f32),
                        );
                    }
                }
            });
    }
}

impl eframe::App for IvApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let ui_start = Instant::now();
        if self.first_frame_s.is_none() {
            self.first_frame_s = Some(self.boot.elapsed().as_secs_f64());
        }
        self.sync();
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
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("measurement_view")
                    .auto_shrink([false, false])
                    .show(ui, |ui| self.main_view(ui));
            });
        self.timing_dialog(ctx);
        if self.busy() {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
        if self.smoke && !self.smoke_started && self.started.elapsed() > Duration::from_millis(100)
        {
            self.smoke_started = true;
            self.send(Command::Connect {
                simulation: self.settings.simulation,
                source: self.settings.source.clone(),
                meter: self.settings.meter.clone(),
            });
        }
        if self.smoke && self.state.connected && self.state.run_id == 0 && !self.pending_run {
            self.start_run();
        }
        if self.screenshot.is_some()
            && !self.screenshot_requested
            && self.started.elapsed() > Duration::from_secs(2)
            && (!self.smoke || self.state.phase == "Completed")
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
            if self.state.phase == "Completed"
                && self.state.points == self.state.total
                && self.state.output == "Off"
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
            if self.started.elapsed() > Duration::from_secs(20) {
                eprintln!("GUI smoke failed: {}", self.state.error);
                self.send(Command::Stop);
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
        self.ui_cpu_s += ui_start.elapsed().as_secs_f64();
        self.ui_frames += 1;
    }
}

fn style(ctx: &egui::Context) {
    ctx.set_theme(egui::ThemePreference::Dark);
    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = BG;
    visuals.window_fill = BG;
    visuals.extreme_bg_color = INPUT;
    visuals.faint_bg_color = PANEL;
    visuals.override_text_color = Some(TEXT);
    visuals.selection.bg_fill = Color32::from_rgb(37, 93, 143);
    visuals.selection.stroke = Stroke::new(1.0_f32, TEXT);
    for widget in [
        &mut visuals.widgets.inactive,
        &mut visuals.widgets.hovered,
        &mut visuals.widgets.active,
    ] {
        widget.bg_stroke = Stroke::new(1.0_f32, Color32::from_rgb(74, 74, 74));
        widget.corner_radius = egui::CornerRadius::same(3);
        widget.fg_stroke = Stroke::new(1.0_f32, TEXT);
    }
    visuals.widgets.inactive.bg_fill = BUTTON;
    visuals.widgets.inactive.weak_bg_fill = BUTTON;
    visuals.widgets.hovered.bg_fill = Color32::from_rgb(42, 42, 42);
    visuals.widgets.active.bg_fill = Color32::from_rgb(37, 93, 143);
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, Color32::from_rgb(58, 58, 58));
    ctx.set_visuals(visuals);
    ctx.style_mut(|style| {
        style.spacing.item_spacing = Vec2::new(8.0, 4.0);
        style.spacing.button_padding = Vec2::new(8.0, 4.0);
        style.spacing.interact_size = Vec2::new(36.0, 24.0);
        style.spacing.combo_width = 100.0;
        style.spacing.slider_width = 120.0;
        for text in [
            egui::TextStyle::Body,
            egui::TextStyle::Button,
            egui::TextStyle::Small,
        ] {
            style
                .text_styles
                .insert(text, egui::FontId::proportional(12.0));
        }
        style
            .text_styles
            .insert(egui::TextStyle::Heading, egui::FontId::proportional(12.0));
    });
}

fn number(ui: &mut egui::Ui, label: &str, value: &mut f64, scale: f64, unit: &str) {
    ui.horizontal(|ui| {
        ui.label(label);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if !unit.is_empty() {
                ui.label(unit);
            }
            let mut display = *value / scale;
            if ui
                .add_sized(
                    [100.0, 24.0],
                    egui::DragValue::new(&mut display)
                        .speed(0.01)
                        .max_decimals(6)
                        .min_decimals(3),
                )
                .changed()
            {
                *value = display * scale;
            }
        });
    });
}

fn quantity(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash,
    value: &mut f64,
    unit_index: &mut usize,
    base: &str,
    width: f32,
) {
    let scales = [1.0, 1e-3, 1e-6, 1e-9, 1e-12];
    let names = [
        base.to_string(),
        format!("m{base}"),
        format!("u{base}"),
        format!("n{base}"),
        format!("p{base}"),
    ];
    *unit_index = (*unit_index).min(4);
    let scale = scales[*unit_index];
    ui.horizontal(|ui| {
        let mut display = *value / scale;
        if ui
            .add_sized(
                [(width - 68.0).max(65.0), 24.0],
                egui::DragValue::new(&mut display)
                    .speed(0.01)
                    .min_decimals(3)
                    .max_decimals(6),
            )
            .changed()
        {
            *value = display * scale;
        }
        egui::ComboBox::from_id_salt(id)
            .width(54.0)
            .selected_text(&names[*unit_index])
            .show_ui(ui, |ui| {
                ui.set_min_width(100.0);
                for (index, name) in names.iter().enumerate() {
                    ui.selectable_value(unit_index, index, name);
                }
            });
    });
}

fn mode_name(mode: Mode) -> &'static str {
    if mode == Mode::Iv {
        "V-I (source V, measure I)"
    } else {
        "I-V (source I, measure V)"
    }
}

fn pattern_name(pattern: Pattern) -> &'static str {
    match pattern {
        Pattern::Linear => "linear",
        Pattern::RoundTrip => "round_trip",
        Pattern::ZeroCentered => "zero_centered",
        Pattern::CustomList => "custom_list",
    }
}

fn model_supported(model: &str) -> bool {
    matches!(
        model,
        "YOKOGAWA_GS210"
            | "AGILENT_34411A"
            | "KEYSIGHT_34411A"
            | "SIMULATED_SOURCE"
            | "SIMULATED_METER"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn earlier_preview_config_keeps_safe_defaults() {
        let settings: Settings =
            serde_json::from_str(r#"{"filename":"experiment","simulation":true}"#).unwrap();
        assert!(settings.auto_suffix);
        assert_eq!(settings.units, [1; 3]);
        assert_eq!(settings.config.compliance, 0.001);
    }

    #[test]
    fn only_implemented_instruments_are_enabled() {
        assert!(model_supported("KEYSIGHT_34411A"));
        assert!(!model_supported("AGILENT_34401A"));
        assert!(!model_supported("YOKOGAWA_7651"));
    }

    #[test]
    fn cached_plot_series_preserves_direction_breaks() {
        let samples = vec![
            (0.0, 1.0, "forward".into()),
            (1.0, 2.0, "backward".into()),
            (2.0, 3.0, "forward".into()),
            (3.0, 4.0, "custom-a".into()),
            (4.0, 5.0, "custom-b".into()),
        ];
        let series = plot_series(&samples);

        assert_eq!(series[0].len(), 4);
        assert_eq!(series[0][0], PlotPoint::new(0.0, 1.0));
        assert!(series[0][1].x.is_nan() && series[0][1].y.is_nan());
        assert_eq!(series[0][2], PlotPoint::new(2.0, 3.0));
        assert!(series[0][3].x.is_nan() && series[0][3].y.is_nan());
        assert_eq!(series[1].len(), 2);
        assert_eq!(series[1][0], PlotPoint::new(1.0, 2.0));
        assert!(series[1][1].x.is_nan() && series[1][1].y.is_nan());
        assert_eq!(
            series[2],
            vec![PlotPoint::new(3.0, 4.0), PlotPoint::new(4.0, 5.0)]
        );
    }
}
