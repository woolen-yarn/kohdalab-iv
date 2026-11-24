//! Sweep settings and source/meter measurement controls.

use super::*;

impl IvApp {
    pub(super) fn sweep(&mut self, ui: &mut egui::Ui) {
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

    pub(super) fn sweep_owns_live_mode(&self) -> bool {
        self.pending_run
            || matches!(
                self.state.phase,
                Phase::Preparing | Phase::Running | Phase::CleaningUp
            ) && self.pending_dc.is_none()
    }

    pub(super) fn verified_dc_mode(&self) -> Option<Mode> {
        self.state
            .source_dc
            .as_ref()
            .filter(|_| self.state.output == OutputState::On)
            .map(|dc| dc.mode)
    }

    pub(super) fn card_control_mode(&self) -> Mode {
        card_mode(
            self.settings.config.mode,
            self.sweep_owns_live_mode(),
            self.verified_dc_mode(),
            self.dc_preview_mode,
        )
    }

    pub(super) fn meter_live_mode(&self) -> Mode {
        card_mode(
            self.settings.config.mode,
            self.sweep_owns_live_mode(),
            self.verified_dc_mode(),
            None,
        )
    }

    /// Show the sweep protection alongside the manual DC compliance.
    pub(super) fn source_compliance_control(&mut self, ui: &mut egui::Ui) {
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
    pub(super) fn meter_measurement_controls(&mut self, ui: &mut egui::Ui) {
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

    pub(super) fn measurement_controls(&mut self, ui: &mut egui::Ui) {
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

    pub(super) fn source_target_limit_control(&mut self, ui: &mut egui::Ui) {
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

    pub(super) fn source_protection_effective_note(&self, ui: &mut egui::Ui, unit: &str) {
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
}
