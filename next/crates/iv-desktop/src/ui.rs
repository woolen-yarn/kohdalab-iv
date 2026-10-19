use crate::client::Client;
use crate::protocol::{Command, Snapshot};
use eframe::egui::{self, Color32, RichText, Stroke, Vec2};
use egui_plot::{
    Legend, Line, LineStyle, MarkerShape, Plot, PlotMemory, PlotPoint, PlotPoints, Points,
};
use iv_core::{Mode, Pattern, RunConfig};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};

const BG: Color32 = Color32::from_rgb(18, 19, 23);
const PANEL: Color32 = Color32::from_rgb(26, 27, 32);
const INPUT: Color32 = Color32::from_rgb(20, 21, 25);
const BUTTON: Color32 = Color32::from_rgb(45, 46, 53);
const BORDER: Color32 = Color32::from_rgb(56, 58, 67);
const TEXT: Color32 = Color32::from_rgb(235, 236, 241);
const BLUE: Color32 = Color32::from_rgb(102, 174, 255);
const ORANGE: Color32 = Color32::from_rgb(245, 183, 82);
const PURPLE: Color32 = Color32::from_rgb(179, 155, 248);
const TEAL: Color32 = Color32::from_rgb(63, 215, 194);
const MUTED: Color32 = Color32::from_rgb(160, 163, 176);
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
const SNAPSHOT_FIELDS: &[&str] = iv_core::CSV_FIELDS;

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

// Preserve the original density while placing headings inside quiet cards.
fn section(ui: &mut egui::Ui, title: &str, add: impl FnOnce(&mut egui::Ui)) {
    let nested = matches!(title, "Output" | "Run");
    ui.add_space(4.0);
    let frame = egui::Frame::new()
        .fill(if nested { Color32::TRANSPARENT } else { PANEL })
        .corner_radius(10)
        .stroke(Stroke::new(1.0_f32, BORDER))
        .inner_margin(egui::Margin {
            left: 10,
            right: 10,
            top: if nested { 22 } else { 24 },
            bottom: if nested { 8 } else { 6 },
        })
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            add(ui);
        });
    ui.painter().text(
        frame.response.rect.left_top() + Vec2::new(10.0, 6.0),
        egui::Align2::LEFT_TOP,
        title,
        egui::FontId::proportional(13.0),
        TEXT,
    );
}

fn button(ui: &mut egui::Ui, text: &str, width: f32, enabled: bool) -> egui::Response {
    ui.add_enabled(
        enabled,
        egui::Button::new(text)
            .fill(BUTTON)
            .min_size(Vec2::new(width, 24.0)),
    )
}

fn form_label(ui: &mut egui::Ui, text: &str, width: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 24.0), egui::Sense::hover());
    ui.painter().text(
        rect.right_center(),
        egui::Align2::RIGHT_CENTER,
        text,
        egui::FontId::proportional(12.0),
        MUTED,
    );
}

// Keep paths and resource identities readable at rest; preserve full manual editing.
fn compact_field(
    ui: &mut egui::Ui,
    salt: impl std::hash::Hash,
    value: &mut String,
    width: f32,
    summary: String,
) {
    let id = ui.make_persistent_id(salt);
    let editing = ui
        .ctx()
        .data_mut(|data| data.get_temp::<bool>(id).unwrap_or(false));
    if editing {
        let response = ui.add_sized(
            [width, 24.0],
            egui::TextEdit::singleline(value)
                .id(id)
                .margin(egui::Margin::symmetric(5, 4)),
        );
        if !response.has_focus() && !response.lost_focus() {
            response.request_focus();
        }
        if response.lost_focus() {
            ui.ctx().data_mut(|data| data.insert_temp(id, false));
        }
        response.on_hover_text(value.as_str());
    } else {
        let response = ui.add_sized(
            [width, 24.0],
            egui::Button::new(RichText::new(summary).color(TEXT))
                .fill(INPUT)
                .stroke(Stroke::new(0.7_f32, BORDER))
                .wrap_mode(egui::TextWrapMode::Truncate),
        );
        if response.clicked() {
            ui.ctx().data_mut(|data| data.insert_temp(id, true));
            ui.ctx().request_repaint();
        }
        response.on_hover_text(format!("{value}\nClick to edit the full value."));
    }
}

fn path_summary(value: &str) -> String {
    let trimmed = value.trim_end_matches(['/', '\\']);
    match trimmed.rsplit_once(['/', '\\']) {
        Some((_, tail)) if !tail.is_empty() => format!("…/{tail}"),
        _ => value.to_owned(),
    }
}

fn resource_summary(value: &str) -> String {
    if let Some(native) = value.strip_prefix("VISA::") {
        return format!("VISA · {}", resource_summary(native));
    }
    let parts: Vec<_> = value.split("::").collect();
    if value.starts_with("GPIB") && parts.len() >= 2 {
        format!("GPIB · {}", parts[1])
    } else if value.starts_with("USB") && parts.len() >= 4 {
        format!("USB · {}", parts[3])
    } else {
        value.to_owned()
    }
}

fn state_badge(ui: &mut egui::Ui, text: &str, color: Color32) {
    egui::Frame::new()
        .fill(color.gamma_multiply(0.12))
        .corner_radius(4)
        .inner_margin(egui::Margin::symmetric(4, 1))
        .show(ui, |ui| {
            ui.label(RichText::new(text).size(9.0).color(color).strong());
        });
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

#[derive(Clone, Copy, Debug, PartialEq)]
struct AxisScale {
    factor: f64,
    prefix: &'static str,
}

impl AxisScale {
    fn for_magnitude(magnitude: f64) -> Self {
        if magnitude.is_finite() && magnitude > 0.0 {
            for (factor, prefix) in [
                (1e12, "T"),
                (1e9, "G"),
                (1e6, "M"),
                (1e3, "k"),
                (1.0, ""),
                (1e-3, "m"),
                (1e-6, "µ"),
                (1e-9, "n"),
                (1e-12, "p"),
                (1e-15, "f"),
                (1e-18, "a"),
            ] {
                if magnitude >= factor * (1.0 - 1e-12) {
                    return Self { factor, prefix };
                }
            }
            return Self {
                factor: 1e-18,
                prefix: "a",
            };
        }
        Self {
            factor: 1.0,
            prefix: "",
        }
    }

    fn tick(self, value: f64, spacing: f64) -> String {
        let step = spacing.abs() / self.factor;
        let decimals = if step.is_finite() && step > 0.0 {
            (-step.log10()).ceil().clamp(0.0, 9.0) as usize
        } else {
            2
        };
        let value = value / self.factor;
        let value = if value.abs() < 0.5 * 10_f64.powi(-(decimals as i32)) {
            0.0
        } else {
            value
        };
        format!("{value:.decimals$}")
    }
}

fn extend_plot_extent(extent: &mut [f64; 2], x: f64, y: f64) {
    for (maximum, value) in extent.iter_mut().zip([x, y]) {
        if value.is_finite() {
            *maximum = maximum.max(value.abs());
        }
    }
}

fn display_reading(value: f64, resolution: Option<f64>, unit: &str) -> String {
    let scale = AxisScale::for_magnitude(value.abs().max(resolution.unwrap_or(0.0)));
    display_reading_at_scale(value, resolution, unit, scale)
}

fn display_reading_at_scale(
    value: f64,
    resolution: Option<f64>,
    unit: &str,
    scale: AxisScale,
) -> String {
    match resolution.filter(|r| r.is_finite() && *r > 0.0) {
        Some(resolution) => {
            // Round the display to a decimal place supported by the instrument.
            // The original value is never modified for plotting or persistence.
            let place = (resolution.log10() - 1e-12).ceil();
            let decimals = (scale.factor.log10() - place).round().clamp(0.0, 15.0) as usize;
            let step = 10_f64.powf(place);
            let rounded = (value / step).round() * step;
            let shown = if rounded == 0.0 {
                0.0
            } else {
                rounded / scale.factor
            };
            format!("{shown:.decimals$} {}{unit}", scale.prefix)
        }
        None => format!(
            "{:.6e} {}{unit} (resolution unknown)",
            value / scale.factor,
            scale.prefix
        ),
    }
}

fn plot_series(samples: &[(f64, f64, String, [Option<f64>; 2])]) -> [Vec<PlotPoint>; 3] {
    let mut series = std::array::from_fn(|_| Vec::new());
    let mut previous = None;
    for (x, y, direction, _) in samples {
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
        // Share the measured turning point across the two colored segments.
        // This is a drawing anchor only; it does not create another CSV sample.
        if last < 2
            && current < 2
            && let Some(anchor) = series[last].last().copied()
            && anchor.x.is_finite()
            && anchor.y.is_finite()
        {
            series[current].push(anchor);
        }
        series[last].push(PlotPoint::new(f64::NAN, f64::NAN));
    }
    series[current].push(PlotPoint::new(x, y));
}

#[derive(Clone, Copy)]
struct LiveSample {
    time: f64,
    voltage: f64,
    current: f64,
}

fn append_live_sample(samples: &mut VecDeque<LiveSample>, record: &iv_core::Record) {
    let source = record.source_readback.unwrap_or(record.source_set);
    let (voltage, current) = if record.mode == Mode::Iv {
        (source, record.meter_value)
    } else {
        (record.meter_value, source)
    };
    if ![record.elapsed_s, voltage, current]
        .into_iter()
        .all(f64::is_finite)
    {
        return;
    }
    samples.push_back(LiveSample {
        time: record.elapsed_s,
        voltage,
        current,
    });
    while samples.len() > 1024
        || samples
            .front()
            .is_some_and(|s| record.elapsed_s - s.time > 30.0)
    {
        samples.pop_front();
    }
}

pub struct IvApp {
    client: Result<Client, String>,
    settings: Settings,
    state: Snapshot,
    previous_phase: String,
    previous_log: String,
    state_revision: u64,
    run_id: u64,
    samples: Vec<(f64, f64, String, [Option<f64>; 2])>,
    plot_series: [Vec<PlotPoint>; 3],
    plot_extent: [f64; 2],
    live_samples: VecDeque<LiveSample>,
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
            plot_extent: [0.0; 2],
            live_samples: VecDeque::new(),
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
            config_path: settings_path().display().to_string(),
            notice: String::new(),
            start_error: None,
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
            "Connecting" | "Scanning" | "Switching mode" | "Preparing" | "Running" | "Cleaning up"
        )
    }

    fn active(&self) -> bool {
        matches!(
            self.state.phase.as_str(),
            "Preparing" | "Running" | "Cleaning up"
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
            if let Some(record) = &next.live
                && self.last_live_timestamp != record.timestamp
            {
                let mut sample = record.clone();
                sample.elapsed_s = self.boot.elapsed().as_secs_f64();
                append_live_sample(&mut self.live_samples, &sample);
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
            if next.phase != self.previous_phase {
                self.phase_start = Instant::now();
                self.previous_phase = next.phase.clone();
            }
            if !next.error.is_empty()
                && next.error != self.state.error
                && (self.pending_run || self.active())
            {
                self.start_error = Some(format!(
                    "{}\n\nCheck the log and instrument connections. Verify output is off before trying again.",
                    next.error
                ));
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
        if self.pending_run {
            return;
        }
        if let Some(reason) = start_block_reason(
            &self.settings.config,
            self.state.connected,
            self.state.source_simulation == self.state.meter_simulation,
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
        if !self.smoke {
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
                            self.profile(ui);
                            ui.add_space(4.0);
                            egui::ScrollArea::vertical()
                                .id_salt("settings_scroll")
                                .auto_shrink([false, false])
                                .show(ui, |ui| {
                                    self.instrument(ui, true);
                                    ui.add_space(4.0);
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

    fn live_monitor(&self, ui: &mut egui::Ui, source: bool) {
        let mode = self
            .live_reading
            .as_ref()
            .map_or(self.settings.config.mode, |r| r.mode);
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
                RichText::new(if running && stale {
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
            if source {
                state_badge(
                    ui,
                    &format!("Output {}", self.state.output.to_lowercase()),
                    if self.state.output == "Off" {
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
        let extent = self.live_samples.iter().fold(0_f64, |m, s| {
            m.max(if channel == 0 {
                s.current.abs()
            } else {
                s.voltage.abs()
            })
        });
        let scale = AxisScale::for_magnitude(extent);
        let text = self
            .live_reading
            .as_ref()
            .map(|record| {
                let source = record.source_readback.unwrap_or(record.source_set);
                let source_channel = if record.mode == Mode::Iv { 1 } else { 0 };
                let (value, resolution) = if channel == source_channel {
                    (source, record.source_resolution)
                } else {
                    (record.meter_value, record.meter_resolution)
                };
                display_reading_at_scale(value, resolution, unit, scale)
            })
            .unwrap_or_else(|| format!("— {unit}"));
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
                    if channel == 0 { s.current } else { s.voltage },
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
    }

    fn instrument(&mut self, ui: &mut egui::Ui, source: bool) {
        let title = if source { "Source" } else { "Meter" };
        section(ui, title, |ui| {
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
            egui::CollapsingHeader::new(format!("Device  ·  {summary}"))
                .id_salt(("instrument_device_settings", source))
                .default_open(false)
                .show_background(true)
                .show_unindented(ui, |ui| {
                    self.instrument_connection_controls(ui, source, connected)
                })
                .header_response
                .on_hover_text(hint);
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
                            for candidate in if source { SOURCE_MODELS } else { METER_MODELS } {
                                ui.add_enabled_ui(model_supported(candidate), |ui| {
                                    ui.selectable_value(model, candidate.to_string(), *candidate);
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
                    self.live_samples.clear();
                    self.live_reading = None;
                    if self.state.source_connected {
                        self.send(Command::SetLiveMode {
                            mode: self.settings.config.mode,
                        });
                    }
                }
                form_label(ui, "Sweep", 45.0);
                egui::ComboBox::from_id_salt("pattern")
                    .width(width)
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
                        false,
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
                    "Saved {} points to {}\nThe companion .run.json contains the measurement settings.",
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
                .add_enabled(!self.busy() && !self.pending_run, start_button)
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
            if button(ui, "Save", width, !self.busy() && !self.pending_run)
                .on_hover_text("Save a complete copy of the acquired CSV and run settings.")
                .on_disabled_hover_text(
                    "Stop the measurement and wait for cleanup before saving a copy.",
                )
                .clicked()
            {
                self.save_measurement();
            }
        });
        ui.label(
            RichText::new(format!(
                "{} readings / point · NPLC {}",
                iv_core::readings_per_point(&self.settings.config),
                self.settings.config.nplc
            ))
            .small()
            .color(MUTED),
        );
        if self.preview.is_ok()
            && let Ok(effective) = iv_core::prepare_config(&self.settings.config)
        {
            let unit = if effective.mode == Mode::Vi { "A" } else { "V" };
            ui.label(
                RichText::new(format!(
                    "Auto: step {:e} {unit} · ramp {:e} {unit}",
                    effective.step.abs(),
                    effective.ramp_step
                ))
                .small()
                .color(MUTED),
            );
        }
        if let Err(reason) = &self.preview {
            ui.add_space(4.0);
            ui.label(
                RichText::new(format!("Cannot start: {reason}"))
                    .small()
                    .color(ORANGE),
            );
        }
    }

    fn start_error_dialog(&mut self, ctx: &egui::Context) {
        let Some(reason) = self.start_error.clone() else {
            return;
        };
        let response = egui::Modal::new(egui::Id::new("start_error")).show(ctx, |ui| {
            ui.set_width(380.0);
            ui.heading("Measurement needs attention");
            ui.add_space(8.0);
            ui.label(reason);
            ui.add_space(8.0);
            ui.label(
                RichText::new("Review the settings, then press Start again.")
                    .small()
                    .color(MUTED),
            );
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
        let response = egui::Modal::new(egui::Id::new("timing_compliance"))
            .frame(egui::Frame::popup(&ctx.style()).fill(BG).inner_margin(16))
            .show(ctx, |ui| {
                ui.set_width(360.0);
                ui.strong("Timing / Compliance");
                ui.add_space(8.0);
                egui::ScrollArea::vertical().max_height((ctx.content_rect().height() - 150.0).clamp(200.0, 600.0)).show(ui, |ui| {
                    ui.add_enabled_ui(!self.busy(), |ui| {
                        section(ui, "Timing", |ui| {
                            number(ui, "Wait time (s)", &mut self.settings.config.settle_s, 1.0, "", 0.0..=iv_core::MAX_SECONDS);

                        });
                        ui.add_space(8.0);
                        section(ui, "Compliance", |ui| {
                            let width = ui.available_width();
                            ui.label("Current limit (V source)");
                            quantity(ui, "current_compliance", &mut self.settings.current_compliance,
                                &mut self.settings.compliance_units[0], "A", width, true);
                            ui.add_space(8.0);
                            ui.label("Voltage limit (I source)");
                            quantity(ui, "voltage_compliance", &mut self.settings.voltage_compliance,
                                &mut self.settings.compliance_units[1], "V", width, true);
                            self.settings.config.compliance = if self.settings.config.mode == Mode::Iv {
                                self.settings.current_compliance
                            } else { self.settings.voltage_compliance };
                            ui.label("Stop when |measured value| ≥ limit.\nChecked after every individual meter reading.");
                        });
                        ui.add_space(8.0);
                        egui::CollapsingHeader::new("Advanced").show(ui, |ui| {
                            let unit = if self.settings.config.mode == Mode::Iv { "mV" } else { "mA" };
                            number(ui, "Source limit", &mut self.settings.config.source_limit, 0.001, unit, 0.0..=f64::MAX);
                            number(ui, "First wait (s)", &mut self.settings.config.start_settle_s, 1.0, "", 0.0..=iv_core::MAX_SECONDS);
                            ui.label("Ramp step: automatic (at least the sweep step)");
                            number(ui, "Ramp wait (s)", &mut self.settings.config.ramp_wait_s, 1.0, "", 0.0..=iv_core::MAX_SECONDS);
                            ui.horizontal(|ui| {
                                ui.label("Readings per point (Repeat)");
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
        let (x_name, x_unit, y_name, y_unit) = if self.settings.config.mode == Mode::Iv {
            ("Voltage", "V", "Current", "A")
        } else {
            ("Current", "A", "Voltage", "V")
        };
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
        self.timing_dialog(ctx);
        self.start_error_dialog(ctx);
        self.save_dialog(ctx);
        if self.busy() || self.state.connected {
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
    visuals.window_fill = PANEL;
    visuals.window_corner_radius = egui::CornerRadius::same(12);
    visuals.window_stroke = Stroke::new(1.0_f32, BORDER);
    visuals.extreme_bg_color = INPUT;
    visuals.text_edit_bg_color = Some(INPUT);
    visuals.faint_bg_color = PANEL;
    visuals.override_text_color = Some(TEXT);
    visuals.selection.bg_fill = Color32::from_rgb(48, 91, 158);
    visuals.selection.stroke = Stroke::new(1.0_f32, TEXT);
    for widget in [
        &mut visuals.widgets.inactive,
        &mut visuals.widgets.hovered,
        &mut visuals.widgets.active,
    ] {
        widget.bg_stroke = Stroke::new(1.0_f32, BORDER);
        widget.corner_radius = egui::CornerRadius::same(6);
        widget.fg_stroke = Stroke::new(1.0_f32, TEXT);
    }
    visuals.widgets.inactive.bg_stroke = Stroke::new(0.7_f32, BORDER);
    visuals.widgets.inactive.bg_fill = BUTTON;
    visuals.widgets.inactive.weak_bg_fill = INPUT;
    visuals.widgets.hovered.bg_fill = Color32::from_rgb(62, 64, 74);
    visuals.widgets.active.bg_fill = Color32::from_rgb(48, 91, 158);
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, BORDER);
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

fn number(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut f64,
    scale: f64,
    unit: &str,
    range: std::ops::RangeInclusive<f64>,
) {
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
                        .range((*range.start() / scale)..=(*range.end() / scale))
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
    nonnegative: bool,
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
                [(width - 54.0 - ui.spacing().item_spacing.x).max(65.0), 24.0],
                egui::DragValue::new(&mut display)
                    .range(if nonnegative {
                        0.0..=f64::MAX
                    } else {
                        f64::MIN..=f64::MAX
                    })
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

fn start_block_reason(config: &RunConfig, connected: bool, matching_modes: bool) -> Option<String> {
    if !connected {
        return Some("Connect both Source and Meter before starting. Click Refresh, select the correct resources, then click All Connect.".into());
    }
    if !matching_modes {
        return Some("Source and Meter must both use real instruments, or both use simulation. Check the Device selections and reconnect.".into());
    }
    iv_core::plan(config).err().map(|reason| {
        let help = if reason.contains("exceeds effective limit") {
            "Reduce Start / End, or review Source limit in Timing / Compliance > Advanced. Increase a limit only when it is safe for your setup."
        } else if reason.contains("must be a multiple") {
            "Set Start / End to whole multiples of the source resolution stated in the error. Step and ramp are adjusted automatically."
        } else if reason.contains("points") {
            "Increase Step or reduce the sweep span."
        } else if reason.contains("GS210 range") {
            "Reduce Start / End to values supported by the selected source."
        } else {
            "Check Measurement and Timing / Compliance for an invalid value."
        };
        format!("{reason}\n\n{help}")
    })
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
            | "YOKOGAWA_7651"
            | "AGILENT_34401A"
            | "KEYSIGHT_34465A"
            | "ADCMT_7461A"
            | "AGILENT_34411A"
            | "KEYSIGHT_34411A"
            | "SIMULATED_SOURCE"
            | "SIMULATED_METER"
    )
}

#[cfg(test)]
mod tests {
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
        let values = record.native_csv_values();
        assert_eq!(
            values,
            [
                "2026-10-14T00:00:00Z",
                "5e-1",
                "4",
                "2",
                "3",
                "backward",
                "1e-3",
                "V",
                "1e-3",
                "1e-6",
                "source_readback",
                "meter",
                "false",
                "running"
            ]
            .map(str::to_string)
        );
    }

    #[test]
    fn live_history_is_bounded_and_iv_vi_values_keep_their_channel_units() {
        let mut samples = VecDeque::new();
        let config = iv_core::RunConfig::default();
        let point = iv_core::SweepPoint {
            target: 0.001,
            direction: "forward".into(),
            repeat_index: 1,
            sweep_index: 1,
            point_index: 1,
            total_points: 1,
        };
        for i in 0..2000 {
            append_live_sample(
                &mut samples,
                &iv_core::simulate_record(&config, &point, i as f64 * 0.01),
            );
        }
        assert_eq!(samples.len(), 1024);
        assert_eq!(samples.back().unwrap().voltage, 0.001);
        assert_eq!(samples.back().unwrap().current, 1e-6);
        let mut record = iv_core::simulate_record(
            &iv_core::RunConfig {
                mode: Mode::Vi,
                ..config
            },
            &point,
            50.0,
        );
        append_live_sample(&mut samples, &record);
        assert_eq!(samples.len(), 1);
        assert_eq!(samples[0].current, 0.001);
        assert_eq!(samples[0].voltage, 1.0);
        record.meter_value = f64::NAN;
        append_live_sample(&mut samples, &record);
        assert_eq!(samples.len(), 1);
    }

    use super::*;

    #[test]
    fn timing_inputs_clamp_negative_values_but_accept_zero() {
        let context = egui::Context::default();
        let mut value = -0.25;
        let _ = context.run(Default::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                number(
                    ui,
                    "Wait time",
                    &mut value,
                    1.0,
                    "s",
                    0.0..=iv_core::MAX_SECONDS,
                );
            });
        });
        assert_eq!(value, 0.0);
        value = iv_core::MAX_SECONDS + 1.0;
        let _ = context.run(Default::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                number(
                    ui,
                    "Wait time",
                    &mut value,
                    1.0,
                    "s",
                    0.0..=iv_core::MAX_SECONDS,
                );
            });
        });
        assert_eq!(value, iv_core::MAX_SECONDS);
    }

    #[test]
    fn limits_reject_negative_ui_values_without_clamping_sweep_targets() {
        let context = egui::Context::default();
        let mut target = -0.001;
        let mut compliance = -0.001;
        let mut units = [1, 1];
        let _ = context.run(Default::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                quantity(ui, "target", &mut target, &mut units[0], "V", 200.0, false);
                quantity(
                    ui,
                    "compliance",
                    &mut compliance,
                    &mut units[1],
                    "A",
                    200.0,
                    true,
                );
            });
        });
        assert_eq!(target, -0.001);
        assert_eq!(compliance, 0.0);
    }

    #[test]
    fn engineering_axis_labels_and_ticks_share_the_same_scale() {
        for (value, expected_factor, prefix) in [
            (2.0, 1.0, ""),
            (0.001, 1e-3, "m"),
            (1e-6, 1e-6, "µ"),
            (7e-9, 1e-9, "n"),
            (3e-12, 1e-12, "p"),
            (5e-15, 1e-15, "f"),
        ] {
            let scale = AxisScale::for_magnitude(value);
            assert_eq!(scale.factor, expected_factor);
            assert_eq!(scale.prefix, prefix);
        }
        let millivolts = AxisScale::for_magnitude(0.001);
        assert_eq!(millivolts.tick(-0.001, 0.0005), "-1.0");
        assert_eq!(millivolts.tick(0.0005, 0.0005), "0.5");
        assert_eq!(millivolts.tick(-0.0, 0.0005), "0.0");
        let microamps = AxisScale::for_magnitude(1e-6);
        assert_eq!(microamps.tick(1e-6, 2e-7), "1.0");
        assert_eq!(microamps.tick(2e-7, 2e-7), "0.2");
        assert_eq!(AxisScale::for_magnitude(0.0).prefix, "");
        assert_eq!(AxisScale::for_magnitude(f64::NAN).factor, 1.0);
        assert_eq!(AxisScale::for_magnitude(1e-6 * (1.0 - 1e-15)).prefix, "µ");
    }

    #[test]
    fn axis_extent_ignores_breaks_and_keeps_plot_coordinates_in_si() {
        let samples = vec![
            (-0.001, -1e-6, "forward".into(), [None; 2]),
            (0.001, 1e-6, "backward".into(), [None; 2]),
        ];
        let mut extent = [0.0; 2];
        for (x, y, _, _) in &samples {
            extend_plot_extent(&mut extent, *x, *y);
        }
        extend_plot_extent(&mut extent, f64::NAN, f64::INFINITY);
        assert_eq!(extent, [0.001, 1e-6]);
        let plotted = plot_series(&samples);
        assert_eq!(plotted[0][0], PlotPoint::new(-0.001, -1e-6));
        assert_eq!(plotted[1][0], plotted[0][0]);
        assert_eq!(plotted[1][1], PlotPoint::new(0.001, 1e-6));
        assert_eq!(AxisScale::for_magnitude(extent[0]).prefix, "m");
        assert_eq!(AxisScale::for_magnitude(extent[1]).prefix, "µ");
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
        assert!(model_supported("AGILENT_34401A"));
        assert!(model_supported("KEYSIGHT_34465A"));
        assert!(model_supported("ADCMT_7461A"));
        assert!(!model_supported("YOKOGAWA_GS200"));
        assert!(model_supported("YOKOGAWA_7651"));
    }

    #[test]
    fn cached_plot_series_preserves_direction_breaks() {
        let samples = vec![
            (0.0, 1.0, "forward".into(), [None; 2]),
            (1.0, 2.0, "backward".into(), [None; 2]),
            (2.0, 3.0, "forward".into(), [None; 2]),
            (3.0, 4.0, "custom-a".into(), [None; 2]),
            (4.0, 5.0, "custom-b".into(), [None; 2]),
        ];
        let series = plot_series(&samples);

        assert_eq!(series[0].len(), 5);
        assert_eq!(series[0][0], PlotPoint::new(0.0, 1.0));
        assert!(series[0][1].x.is_nan() && series[0][1].y.is_nan());
        assert_eq!(series[0][2], PlotPoint::new(1.0, 2.0));
        assert_eq!(series[0][3], PlotPoint::new(2.0, 3.0));
        assert!(series[0][4].x.is_nan() && series[0][4].y.is_nan());
        assert_eq!(series[1].len(), 3);
        assert_eq!(series[1][0], PlotPoint::new(0.0, 1.0));
        assert_eq!(series[1][1], PlotPoint::new(1.0, 2.0));
        assert!(series[1][2].x.is_nan() && series[1][2].y.is_nan());
        assert_eq!(
            series[2],
            vec![PlotPoint::new(3.0, 4.0), PlotPoint::new(4.0, 5.0)]
        );
    }
}
#[test]
fn display_digits_follow_resolution_without_modifying_measurement() {
    let value = 2.123456789e-9_f64;
    let bits = value.to_bits();
    assert_eq!(display_reading(value, Some(1e-11), "A"), "2.12 nA");
    assert_eq!(display_reading(value, Some(1e-10), "A"), "2.1 nA");
    assert_eq!(display_reading(value, Some(1e-12), "A"), "2.123 nA");
    assert_eq!(value.to_bits(), bits);
    assert_eq!(display_reading(-0.0, Some(1e-9), "A"), "0 nA");
    assert!(display_reading(value, None, "A").contains("resolution unknown"));
}
