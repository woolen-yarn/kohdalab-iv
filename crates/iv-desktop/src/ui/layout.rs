//! Top-level panels: sidebar, profile, run controls, detail and main views.

use super::*;

impl IvApp {
    pub(super) fn sidebar(&mut self, root: &mut egui::Ui) {
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
            .show(root, |ui| {
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

    pub(super) fn profile(&mut self, ui: &mut egui::Ui) {
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

    pub(super) fn run_controls(&mut self, ui: &mut egui::Ui) {
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

    pub(super) fn detail_panel(&mut self, root: &mut egui::Ui) {
        let width = if self.details { 288.0 } else { 40.0 };
        egui::Panel::right("details")
            .exact_size(width)
            .resizable(false)
            .frame(egui::Frame::new().fill(BG).inner_margin(8))
            .show(root, |ui| {
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

    pub(super) fn main_view(&mut self, ui: &mut egui::Ui) {
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
    pub(super) fn render_sweep_plot(
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
            .label_formatter(move |hover| {
                let (name, point) = match hover {
                    HoverPosition::NearDataPoint {
                        plot_name,
                        position,
                        ..
                    } => (*plot_name, position),
                    HoverPosition::Elsewhere { position } => ("", position),
                };
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
                Some(text)
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
