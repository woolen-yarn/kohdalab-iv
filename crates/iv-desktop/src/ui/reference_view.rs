//! Separate-circuit presentation; never changes the acquisition or CSV contract.
use super::*;
use reference_analysis::{ChannelFit, Sample};
use settings::{ReferenceDisplay, ReferencePurpose};

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(super) enum GateMetric {
    Sweep,
    Resistance,
    Conductance,
}

/// Online centered moments use every acquired point, independently of drawing
/// decimation. A missing observer reading cannot shift its pairing with x.
#[derive(Default)]
pub(super) struct SeparateHistory {
    pub samples: Vec<Sample>,
    pub series: [Vec<PlotPoint>; 3],
    pub extent: [f64; 2],
    n: usize,
    mean_x: f64,
    mean_y: f64,
    xx: f64,
    xy: f64,
}
impl SeparateHistory {
    pub fn push(&mut self, record: &iv_core::Record) {
        let x = record.source_readback.unwrap_or(record.source_set);
        let y = record.meter_value;
        if x.is_finite() && y.is_finite() {
            self.n += 1;
            let dx = x - self.mean_x;
            self.mean_x += dx / self.n as f64;
            self.mean_y += (y - self.mean_y) / self.n as f64;
            self.xx += dx * (x - self.mean_x);
            self.xy += dx * (y - self.mean_y);
        }
        if let Some(y) = record.reference_meter_value.filter(|y| y.is_finite())
            && x.is_finite()
        {
            append_plot_point(
                &mut self.series,
                self.samples.last().map(|p| p.2.as_str()),
                x,
                y,
                &record.point.direction,
            );
            self.samples.push((
                x,
                y,
                record.point.direction.clone(),
                [record.source_resolution, record.reference_meter_resolution],
            ));
            extend_plot_extent(&mut self.extent, x, y);
        } else if self.samples.last().is_some_and(|p| p.1.is_finite()) {
            self.samples
                .push((x, f64::NAN, record.point.direction.clone(), [None; 2]));
            for series in &mut self.series {
                series.push(PlotPoint::new(f64::NAN, f64::NAN));
            }
        }
        if self.samples.len() > 50000 {
            self.samples = self
                .samples
                .chunks(2)
                .map(|chunk| {
                    chunk
                        .iter()
                        .find(|p| !p.1.is_finite())
                        .unwrap_or(&chunk[0])
                        .clone()
                })
                .collect();
            self.series = plot_series(&self.samples);
        }
    }

    pub fn fit(&self, mode: Mode) -> Option<ChannelFit> {
        if self.n < 2 || self.xx <= 0.0 {
            return None;
        }
        let slope = self.xy / self.xx;
        let offset = self.mean_y - slope * self.mean_x;
        if !slope.is_finite() || !offset.is_finite() {
            return None;
        }
        let reciprocal = (slope != 0.0)
            .then(|| 1.0 / slope)
            .filter(|v| v.is_finite());
        let (resistance, conductance) = if mode == Mode::Vi {
            (Some(slope), reciprocal)
        } else {
            (reciprocal, Some(slope))
        };
        Some(ChannelFit {
            resistance,
            conductance,
            offset,
            points: self.n,
        })
    }
}

impl IvApp {
    pub(super) fn separate_enabled(&self) -> bool {
        self.settings.reference_display == ReferenceDisplay::Separate
            && self.settings.reference_meter_id.is_some()
            && self.settings.config.balanced_source.is_none()
    }
    pub(super) fn meter_axis_name(&self, reference: bool, quantity: &str) -> String {
        let id = if reference {
            self.settings.reference_meter_id
        } else {
            Some(self.settings.meter_id)
        };
        id.map_or_else(
            || quantity.to_owned(),
            |id| format!("{quantity} {}", self.card_display_name(false, id)),
        )
    }
    pub(super) fn reference_quantity(&self) -> String {
        self.meter_axis_name(
            true,
            if self.settings.config.mode == Mode::Iv {
                "Voltage"
            } else {
                "Current"
            },
        )
    }
    pub(super) fn reference_header(&mut self, ui: &mut egui::Ui) {
        let assigned = self.settings.reference_meter_id.is_some();
        let width = (ui.clip_rect().right() - ui.cursor().left()).max(100.0);
        ui.allocate_ui_with_layout(
            egui::vec2(width, 24.0),
            egui::Layout::left_to_right(egui::Align::Center).with_main_wrap(true),
            |ui| {
                ui.label("Reference display:");
                ui.add_enabled_ui(assigned, |ui| {
                    ui.selectable_value(
                        &mut self.settings.reference_display,
                        ReferenceDisplay::Overlay,
                        "Overlay",
                    );
                    ui.selectable_value(
                        &mut self.settings.reference_display,
                        ReferenceDisplay::Separate,
                        "Separate",
                    );
                });
                if assigned && self.settings.reference_display == ReferenceDisplay::Separate {
                    egui::ComboBox::from_id_salt("reference_purpose")
                        .selected_text(
                            if self.settings.reference_purpose == ReferencePurpose::Gate {
                                "Gate circuit"
                            } else {
                                "Reference circuit"
                            },
                        )
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut self.settings.reference_purpose,
                                ReferencePurpose::Reference,
                                "Reference circuit",
                            );
                            if self.settings.config.mode == Mode::Vi {
                                ui.selectable_value(
                                    &mut self.settings.reference_purpose,
                                    ReferencePurpose::Gate,
                                    "Gate circuit",
                                );
                            }
                        });
                    ui.selectable_value(&mut self.gate_metric, GateMetric::Sweep, "Sweep");
                    if self.settings.reference_purpose != ReferencePurpose::Gate {
                        self.gate_metric = GateMetric::Sweep;
                    }
                    if self.settings.reference_purpose == ReferencePurpose::Gate
                        && !self.gate_summaries.is_empty()
                    {
                        ui.selectable_value(&mut self.gate_metric, GateMetric::Resistance, "R₂T");
                        ui.selectable_value(&mut self.gate_metric, GateMetric::Conductance, "G₂T");
                    }
                    if self.settings.config.mode == Mode::Vi
                        && ui.small_button("Load gate summary…").clicked()
                        && let Some(path) = rfd::FileDialog::new()
                            .add_filter("Gate summary", &["csv"])
                            .pick_file()
                    {
                        match reference_analysis::read_gate_summary(&path) {
                            Ok(rows) if !rows.is_empty() => {
                                self.gate_summary_revision += 1;
                                self.gate_summaries = rows;
                                self.settings.reference_purpose = ReferencePurpose::Gate;
                                self.gate_metric = GateMetric::Resistance;
                                self.notice.clear();
                            }
                            Ok(_) => self.report(
                                "No completed gate fits with explicit gate voltage were found."
                                    .into(),
                            ),
                            Err(error) => self.report(error),
                        }
                    }
                }
            },
        );
    }
    pub(super) fn separate_view(&self, ui: &mut egui::Ui) {
        let height = ((ui.clip_rect().bottom() - ui.cursor().top() - 8.0) / 2.0).max(80.0);
        let empty = Default::default();
        if self.gate_metric != GateMetric::Sweep
            && self.settings.reference_purpose == ReferencePurpose::Gate
            && self.settings.config.mode == Mode::Vi
        {
            let resistance = self.gate_metric == GateMetric::Resistance;
            let primary: Vec<Sample> = self
                .gate_summaries
                .iter()
                .map(|s| {
                    let value = if resistance {
                        s.fit.resistance
                    } else {
                        s.fit.conductance
                    }
                    .unwrap_or(f64::NAN);
                    (s.gate_voltage, value, s.origin.clone(), [None; 2])
                })
                .collect();
            let reference: Vec<Sample> = self
                .gate_summaries
                .iter()
                .map(|s| {
                    (
                        s.gate_voltage,
                        s.leakage.unwrap_or(f64::NAN),
                        format!("{} · zero-current readings: {}", s.origin, s.zero_count),
                        [None; 2],
                    )
                })
                .collect();
            let make = |samples: &[Sample]| {
                let mut series: [Vec<PlotPoint>; 3] = Default::default();
                let mut extent = [0.0; 2];
                for (x, y, _, _) in samples {
                    series[0].push(PlotPoint::new(*x, *y));
                    extend_plot_extent(&mut extent, *x, *y);
                }
                (series, extent)
            };
            let (a, mut ae) = make(&primary);
            let (b, mut be) = make(&reference);
            let gate_extent = self
                .gate_summaries
                .iter()
                .map(|s| s.gate_voltage.abs())
                .fold(0.0, f64::max);
            ae[0] = gate_extent;
            be[0] = gate_extent;
            ui.small("Completed runs · leakage: mean of valid readings at source 0 A");
            let height = (height - 9.0).max(80.0);
            self.render_sweep_plot(
                ui,
                "separate_gate_primary",
                "V_G",
                "",
                "V",
                &self.meter_axis_name(
                    false,
                    if resistance {
                        "Resistance"
                    } else {
                        "Conductance"
                    },
                ),
                if resistance { "Ω" } else { "S" },
                &primary,
                &a,
                ae,
                &[],
                &empty,
                [0.0; 2],
                height,
                false,
            );
            self.render_sweep_plot(
                ui,
                "separate_gate_reference",
                "V_G",
                "",
                "V",
                &self.reference_quantity(),
                "A",
                &reference,
                &b,
                be,
                &[],
                &empty,
                [0.0; 2],
                height,
                false,
            );
            return;
        }
        let current_source = self.settings.config.mode == Mode::Vi;
        let (x, xu, y, yu, ru) = if current_source {
            (
                "I_source",
                "A",
                self.meter_axis_name(false, "Voltage"),
                "V",
                "A",
            )
        } else {
            (
                "V_source",
                "V",
                self.meter_axis_name(false, "Current"),
                "A",
                "V",
            )
        };
        self.render_sweep_plot(
            ui,
            "separate_primary",
            x,
            "",
            xu,
            &y,
            yu,
            &self.samples,
            &self.plot_series,
            self.plot_extent,
            &[],
            &empty,
            [0.0; 2],
            height,
            true,
        );
        if self.separate.samples.is_empty() {
            ui.small(format!(
                "{} · waiting for readings",
                self.reference_quantity()
            ));
        }
        self.render_sweep_plot(
            ui,
            "separate_reference",
            x,
            "",
            xu,
            &self.reference_quantity(),
            ru,
            &self.separate.samples,
            &self.separate.series,
            [self.plot_extent[0], self.separate.extent[1]],
            &[],
            &empty,
            [0.0; 2],
            if self.separate.samples.is_empty() {
                height - 18.0
            } else {
                height
            },
            true,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn legacy_settings_default_to_overlay() {
        let settings: Settings = serde_json::from_str("{}").unwrap();
        assert!(settings.reference_display == ReferenceDisplay::Overlay);
        assert!(settings.reference_purpose == ReferencePurpose::Reference);
    }
}

impl IvApp {
    /// Explicit, isolated GUI acceptance hook: canonical columns only, no device
    /// connection, acquisition, output, file rewrite or inferred run completion.
    pub(super) fn load_reference_replay(&mut self, path: &std::path::Path) -> Result<(), String> {
        let args: Vec<_> = std::env::args().collect();
        let status = crate::argument(&args, "--replay-status")
            .ok_or("Replay requires explicit --replay-status completed|stopped")?;
        self.state.phase = match status.as_str() {
            "completed" => Phase::Completed,
            "stopped" => Phase::Stopped,
            _ => return Err("Replay status must be completed or stopped".into()),
        };
        let records = read_replay(path)?;
        self.reference_replay = true;
        self.run_id = 1;
        self.state.run_id = 1;
        self.state.output = OutputState::Off;
        self.state.points = records.len();
        self.state.total = records.len();
        self.state.path = path.display().to_string();
        self.settings.config.mode = Mode::Vi;
        self.settings.config.balanced_source = None;
        self.settings.config.start = records
            .iter()
            .map(|r| r.source_set)
            .fold(f64::INFINITY, f64::min);
        self.settings.config.stop = records
            .iter()
            .map(|r| r.source_set)
            .fold(f64::NEG_INFINITY, f64::max);
        for record in records {
            let x = record.source_readback.unwrap_or(record.source_set);
            self.samples.push((
                x,
                record.meter_value,
                record.point.direction.clone(),
                [None; 2],
            ));
            self.separate.push(&record);
        }
        self.rebuild_plot_series();
        self.report("Read-only replay · no instruments connected".into());
        let gate_voltage = crate::argument(&args, "--replay-gate-voltage")
            .map(|v| {
                v.parse::<f64>()
                    .map_err(|_| "Invalid explicit gate voltage")
            })
            .transpose()?;
        self.gate_summaries = reference_analysis::summarize_runs(&[reference_analysis::GateRun {
            gate_voltage,
            complete: status == "completed",
            mode: Mode::Vi,
            samples: self.samples.clone(),
            leakage: self.separate.samples.clone(),
            origin: path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
        }]);
        if let Some(summary) = crate::argument(&args, "--replay-summary") {
            self.gate_summaries =
                reference_analysis::read_gate_summary(std::path::Path::new(&summary))?;
            self.gate_metric = if std::env::args().any(|arg| arg == "--replay-conductance") {
                GateMetric::Conductance
            } else {
                GateMetric::Resistance
            };
        }
        Ok(())
    }
    pub(super) fn reference_replay_capture(&mut self, ctx: &egui::Context) {
        let Some(path) = &self.screenshot else {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        };
        if self.started.elapsed() > Duration::from_secs(1) && !self.screenshot_requested {
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
            let bytes = image.pixels.iter().flat_map(|p| p.to_array()).collect();
            self.screenshot_saved =
                image::RgbaImage::from_raw(image.width() as u32, image.height() as u32, bytes)
                    .is_some_and(|image| image.save(path).is_ok());
            if self.screenshot_saved && !self.success.swap(true, Ordering::AcqRel) {
                println!(
                    "{}",
                    serde_json::json!({
                        "reference_replay":"passed", "points":self.samples.len(), "phase":self.state.phase,
                        "output":self.state.output,"channel_fit":self.separate.fit(Mode::Vi).map(|f|f.resistance),
                        "completed_gate_fits":self.gate_summaries.len(),
                        "primary_bounds":ctx.data(|data|data.get_temp::<[f64;4]>(egui::Id::new(("plot_bounds","separate_primary")))),
                        "reference_bounds":ctx.data(|data|data.get_temp::<[f64;4]>(egui::Id::new(("plot_bounds","separate_reference"))))
                    })
                );
            }
        }
        if self.screenshot_saved || self.started.elapsed() > Duration::from_secs(12) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
}

fn read_replay(path: &std::path::Path) -> Result<Vec<iv_core::Record>, String> {
    let mut reader = csv::Reader::from_path(path).map_err(|e| e.to_string())?;
    let headers = reader.headers().map_err(|e| e.to_string())?.clone();
    let index = |name: &str| {
        headers
            .iter()
            .position(|h| h == name)
            .ok_or_else(|| format!("Missing replay column: {name}"))
    };
    let source = index("I_source_A")?;
    let primary = index("V_meter_V")?;
    let reference = index("I_meter_A")?;
    let run = index("run_id")?;
    let direction = index("direction")?;
    let mut run_id = None;
    let config = RunConfig {
        mode: Mode::Vi,
        ..RunConfig::default()
    };
    let mut records = Vec::new();
    for (i, row) in reader.records().enumerate() {
        let row = row.map_err(|e| e.to_string())?;
        let id = row.get(run).ok_or("Missing run_id")?;
        if run_id.as_deref().is_some_and(|previous| previous != id) {
            return Err("Replay requires one run_id per CSV".into());
        }
        run_id = Some(id.to_owned());
        let number = |column: usize| {
            row.get(column)
                .and_then(|s| s.parse::<f64>().ok())
                .filter(|v| v.is_finite())
        };
        let x = number(source).ok_or_else(|| format!("Missing source at row {}", i + 2))?;
        let y =
            number(primary).ok_or_else(|| format!("Missing primary reading at row {}", i + 2))?;
        let point = iv_core::SweepPoint {
            target: x,
            direction: row.get(direction).unwrap_or("forward").to_owned(),
            repeat_index: 1,
            sweep_index: 1,
            point_index: i + 1,
            total_points: 0,
        };
        let mut record = iv_core::simulate_record(&config, &point, i as f64);
        record.source_set = x;
        record.source_readback = Some(x);
        record.meter_value = y;
        record.reference_meter_value = number(reference);
        records.push(record);
    }
    if records.is_empty() {
        return Err("Replay has no points".into());
    }
    Ok(records)
}

impl IvApp {
    pub(super) fn plot_history_report(&self) -> serde_json::Value {
        serde_json::json!({
            "run_id":self.run_id,"primary_points":self.samples.len(),
            "reference_points":self.separate.samples.iter().filter(|p|p.1.is_finite()).count(),
            "display":self.settings.reference_display,
            "primary_bounds":self.plot_context.data(|d|d.get_temp::<[f64;4]>(egui::Id::new(("plot_bounds","separate_primary")))),
            "reference_bounds":self.plot_context.data(|d|d.get_temp::<[f64;4]>(egui::Id::new(("plot_bounds","separate_reference")))),
        })
    }
}

#[cfg(test)]
mod history_tests {
    use super::*;
    fn record(x: f64, reference: Option<f64>) -> iv_core::Record {
        let config = RunConfig {
            mode: Mode::Vi,
            ..Default::default()
        };
        let point = iv_core::SweepPoint {
            target: x,
            direction: "forward".into(),
            repeat_index: 1,
            sweep_index: 1,
            point_index: 1,
            total_points: 1,
        };
        let mut record = iv_core::simulate_record(&config, &point, 0.0);
        record.source_readback = Some(x);
        record.meter_value = -0.033 + 1904.37 * x;
        record.reference_meter_value = reference;
        record
    }
    #[test]
    fn same_record_pairing_missing_reference_break_and_fit_independence() {
        let mut history = SeparateHistory::default();
        history.push(&record(-1e-5, Some(-35e-6)));
        history.push(&record(0.0, None));
        history.push(&record(1e-5, Some(-36e-6)));
        assert_eq!(history.samples[0].0, -1e-5);
        assert_eq!(history.samples[0].1, -35e-6);
        assert!(history.samples[1].1.is_nan());
        assert_eq!(history.samples[2].0, 1e-5);
        assert_eq!(history.samples[2].1, -36e-6);
        assert!(history.series[0][1].y.is_nan());
        assert_eq!(history.fit(Mode::Vi).unwrap().points, 3);
        assert!((history.fit(Mode::Vi).unwrap().resistance.unwrap() - 1904.37).abs() < 1e-9);
    }
    #[test]
    fn fit_uses_all_points_and_drawing_history_is_bounded() {
        let mut history = SeparateHistory::default();
        for i in 0..60000 {
            history.push(&record(
                (i as f64 - 30000.0) * 1e-9,
                if i % 11 == 0 { None } else { Some(-35e-6) },
            ));
        }
        assert!(history.samples.len() <= 50000);
        assert!(history.series.iter().all(|s| s.len() <= 50000));
        assert_eq!(history.fit(Mode::Vi).unwrap().points, 60000);
        assert!((history.fit(Mode::Vi).unwrap().resistance.unwrap() - 1904.37).abs() < 1e-6);
        for segment in history.series[0].split(|p| p.y.is_nan()) {
            assert!(
                segment.len() <= 10,
                "Missing reference values must continue to break lines after reduction"
            );
        }
    }
    #[test]
    fn never_observed_reference_remains_empty_without_fabricated_zero() {
        let mut history = SeparateHistory::default();
        for _ in 0..60000 {
            history.push(&record(0.0, None));
        }
        assert!(history.samples.is_empty());
        assert!(history.series.iter().all(Vec::is_empty));
        assert!(history.fit(Mode::Vi).is_none());
    }
}

pub(super) fn channel_fit_text(fit: Option<&ChannelFit>) -> String {
    let resistance = fit
        .and_then(|f| f.resistance)
        .map(|v| compact_fit_quantity(v, "Ω"))
        .unwrap_or_else(|| "—".into());
    let conductance = fit
        .and_then(|f| f.conductance)
        .map(|v| compact_fit_quantity(v, "S"))
        .unwrap_or_else(|| "—".into());
    format!("{resistance} / {conductance}")
}

#[cfg(test)]
mod presentation_tests {
    use super::*;
    #[test]
    fn valid_fit_component_survives_unavailable_reciprocal() {
        let zero_resistance = ChannelFit {
            resistance: Some(0.0),
            conductance: None,
            offset: -0.033,
            points: 5,
        };
        let text = channel_fit_text(Some(&zero_resistance));
        assert!(text.contains('0') && text.contains('Ω') && text.ends_with('—'));
        let zero_conductance = ChannelFit {
            resistance: None,
            conductance: Some(0.0),
            offset: 0.0,
            points: 5,
        };
        let text = channel_fit_text(Some(&zero_conductance));
        assert!(text.starts_with('—') && text.contains('0') && text.contains('S'));
    }
    #[test]
    fn missing_conductance_does_not_clip_lower_only_gate_point() {
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
        app.settings.config.mode = Mode::Vi;
        app.settings.reference_meter_id = Some(3);
        app.settings.reference_display = ReferenceDisplay::Separate;
        app.settings.reference_purpose = ReferencePurpose::Gate;
        app.gate_metric = GateMetric::Conductance;
        app.gate_summaries = vec![
            reference_analysis::GateSummary {
                gate_voltage: -0.5,
                fit: ChannelFit {
                    resistance: Some(1904.37),
                    conductance: Some(1.0 / 1904.37),
                    offset: 0.0,
                    points: 5,
                },
                leakage: None,
                zero_count: 0,
                origin: "a".into(),
            },
            reference_analysis::GateSummary {
                gate_voltage: 10.0,
                fit: ChannelFit {
                    resistance: Some(0.0),
                    conductance: None,
                    offset: 0.0,
                    points: 5,
                },
                leakage: Some(-35e-6),
                zero_count: 2,
                origin: "b".into(),
            },
        ];
        let _ = context.run_ui(egui::RawInput::default(), |ui| app.separate_view(ui));
        let bounds = |id| {
            context.data(|data| {
                data.get_temp::<[f64; 4]>(egui::Id::new(("plot_bounds", id)))
                    .unwrap()
            })
        };
        let upper = bounds("separate_gate_primary");
        let lower = bounds("separate_gate_reference");
        assert!(upper[0] <= -0.5 && upper[2] >= 10.0);
        assert_eq!(upper[0], lower[0]);
        assert_eq!(upper[2], lower[2]);
    }
}

#[cfg(test)]
mod device_label_tests {
    use super::*;
    #[test]
    fn separate_axes_follow_card_labels_and_quantity_not_inferred_purpose() {
        let context = egui::Context::default();
        let creation = eframe::CreationContext::_new_kittest(context);
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
        app.settings.config.mode = Mode::Vi;
        app.settings.meters[0].label = " Channel ".into();
        let reference = app.settings.add_meter("Gate");
        app.settings.reference_meter_id = Some(reference);
        assert_eq!(app.meter_axis_name(false, "Voltage"), "Voltage Channel");
        assert_eq!(app.reference_quantity(), "Current Gate");
        assert!(app.settings.reference_purpose == ReferencePurpose::Reference);
        app.settings.reference_purpose = ReferencePurpose::Gate;
        app.settings.meters.last_mut().unwrap().label = "Leak monitor".into();
        assert_eq!(app.reference_quantity(), "Current Leak monitor");
        app.settings.config.mode = Mode::Iv;
        assert_eq!(app.meter_axis_name(false, "Current"), "Current Channel");
        assert_eq!(app.reference_quantity(), "Voltage Leak monitor");
        app.settings.meters.last_mut().unwrap().label.clear();
        assert_eq!(app.reference_quantity(), "Voltage Meter 2");
    }
}
