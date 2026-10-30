//! Per-card fixed output controls. Saved values never enable an output on load.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CardRole {
    Source,
    Secondary,
    Bias,
}

pub(super) fn settle_job(
    pending: &mut Option<(String, u64)>,
    popup: &mut Option<String>,
    state: &Snapshot,
) {
    let Some((id, safety)) = pending.as_ref() else {
        return;
    };
    if let Some(done) = state
        .gui_jobs
        .iter()
        .find(|job| &job.id == id)
        .or_else(|| state.gui_job.as_ref().filter(|job| &job.id == id))
    {
        if let Some(error) = done.snapshot["error"].as_str().filter(|e| !e.is_empty()) {
            *popup = Some(format!(
                "{error}\n\nCheck the source settings and verify output is off before trying again."
            ));
        }
        *pending = None;
    } else if state.safety_revision > *safety || state.phase == Phase::Failed {
        if !state.error.is_empty() {
            *popup = Some(state.error.to_string());
        }
        *pending = None;
    }
}

fn ramp_step_on_grid(step: f64, resolution: f64) -> f64 {
    if iv_core::nudge_source_value(0.0, step, resolution, step.max(resolution)).is_ok() {
        step
    } else {
        ((step / resolution).ceil().max(1.0)) * resolution
    }
}

fn dc_ranges(model: &str, mode: Mode) -> Vec<iv_core::SourceRangeProfile> {
    let model = if model == "SIMULATED_SOURCE" {
        "YOKOGAWA_GS210"
    } else {
        model
    };
    match iv_core::instrument_profile(model) {
        Ok(Some(iv_core::InstrumentProfile::Source(profile))) => {
            if mode == Mode::Iv {
                profile.voltage_ranges
            } else {
                profile.current_ranges
            }
        }
        _ => Vec::new(),
    }
}

fn select_dc_range(settings: &mut settings::DcSettings, range: &iv_core::SourceRangeProfile) {
    settings.range = Some(range.command_value);
    if iv_core::nudge_source_value(0.0, settings.value, range.resolution, range.maximum).is_err() {
        settings.value = 0.0;
    }
    settings.ramp_step = ramp_step_on_grid(settings.ramp_step, range.resolution).min(range.maximum);
    settings.units[0] = if settings.mode == Mode::Vi || range.name.contains("mV") {
        1
    } else {
        0
    };
}

fn range_selector(
    ui: &mut egui::Ui,
    ranges: &[iv_core::SourceRangeProfile],
    selected: Option<f64>,
    unit: &str,
) -> Option<f64> {
    let mut clicked = None;
    let index = ranges
        .iter()
        .position(|range| Some(range.command_value) == selected);
    let spacing = ui.spacing().item_spacing.x;
    let center_width = (ui.available_width() - 2.0 * 30.0 - 2.0 * spacing).max(1.0);
    ui.horizontal(|ui| {
        if ui
            .add_enabled(
                index.is_some_and(|index| index > 0),
                egui::Button::new("<").min_size(egui::vec2(30.0, 26.0)),
            )
            .on_hover_text("Previous source range")
            .clicked()
        {
            clicked = index
                .and_then(|index| index.checked_sub(1))
                .map(|index| ranges[index].command_value);
        }
        let range = index.and_then(|index| ranges.get(index));
        let response = ui.add_sized(
            [center_width, 26.0],
            egui::Label::new(
                RichText::new(range.map_or("Select range", |range| range.name.as_str())).strong(),
            )
            .halign(egui::Align::Center),
        );
        if let Some(range) = range {
            response.on_hover_text(format!(
                "±{} · increment {}",
                fmt_quantity(range.maximum, unit),
                fmt_quantity(range.resolution, unit)
            ));
        }
        if ui
            .add_enabled(
                index.is_some_and(|index| index + 1 < ranges.len()),
                egui::Button::new(">").min_size(egui::vec2(30.0, 26.0)),
            )
            .on_hover_text("Next source range")
            .clicked()
        {
            clicked = index.map(|index| ranges[index + 1].command_value);
        }
    });
    clicked
}

fn dc_request_valid(model: &str, settings: &settings::DcSettings) -> Result<(), String> {
    let range = settings.range.ok_or("Select a source range.")?;
    let model = if model == "SIMULATED_SOURCE" {
        "YOKOGAWA_GS210"
    } else {
        model
    };
    let selected = iv_core::fixed_manual_source_range(
        model,
        settings.mode,
        settings.value,
        settings.compliance,
        range,
    )?;
    if !settings.ramp_step.is_finite() || settings.ramp_step <= 0.0 {
        return Err("Ramp step must be positive.".into());
    }
    iv_core::nudge_source_value(
        0.0,
        settings.ramp_step,
        selected.resolution,
        selected.maximum,
    )?;
    Ok(())
}

impl IvApp {
    fn dc_role(&self, id: u64) -> CardRole {
        if id == self.settings.source1_id {
            CardRole::Source
        } else if self.settings.config.balanced_source.is_some() && id == self.settings.source2_id {
            CardRole::Secondary
        } else {
            CardRole::Bias
        }
    }

    pub(super) fn dc_gate_matches(&self, id: u64) -> bool {
        let Some(entry) = self.settings.sources.iter().find(|entry| entry.id == id) else {
            return false;
        };
        self.state.gate_connected
            && self.settings.dc_gate_id == id
            && (if self.state.gate_simulation {
                entry.model == "SIMULATED_SOURCE"
            } else {
                entry.resource == self.state.gate_resource
            })
    }

    fn dc_connected(&self, id: u64) -> bool {
        match self.dc_role(id) {
            CardRole::Source => self.state.source_connected,
            CardRole::Secondary => self.state.source2_connected,
            CardRole::Bias => self.dc_gate_matches(id),
        }
    }

    fn dc_output(&self, id: u64) -> OutputState {
        match self.dc_role(id) {
            CardRole::Source => self.state.output,
            CardRole::Secondary => self.state.source2_output,
            CardRole::Bias => self.state.gate_output,
        }
    }
    fn dc_on(&self, id: u64) -> bool {
        match self.dc_role(id) {
            CardRole::Source => {
                self.state.source_dc.is_some() && self.state.output == OutputState::On
            }
            CardRole::Secondary => {
                self.state.source2_dc.is_some() && self.state.source2_output == OutputState::On
            }
            CardRole::Bias => self.dc_gate_matches(id) && self.state.gate_output == OutputState::On,
        }
    }

    /// Device and DC disclosures share one row; closed DC controls add no height.
    pub(super) fn source_card_header(&mut self, ui: &mut egui::Ui, id: u64, summary: &str) -> bool {
        let key = ui.make_persistent_id(("source_device_disclosure", id));
        let mut device_open = ui
            .ctx()
            .data_mut(|data| data.get_persisted::<bool>(key))
            .unwrap_or(false);
        let dc_on = self.dc_on(id);
        let unknown = self.dc_connected(id) && self.dc_output(id) == OutputState::Unknown;
        ui.horizontal(|ui| {
            let width = super::widgets::device_width(ui);
            super::widgets::device_disclosure(ui, summary, width, &mut device_open);
            let open = self.dc_open.contains(&id);
            let text = "DC output";
            let mut control = egui::Button::new(RichText::new(text).color(if dc_on {
                Color32::WHITE
            } else {
                Color32::from_rgb(214, 217, 224)
            }));
            if unknown {
                control = control.fill(RED);
            } else if dc_on {
                control = control.fill(ORANGE.gamma_multiply(0.7));
            } else if open {
                control = control.fill(BUTTON);
            }
            if ui
                .add_sized([super::widgets::DC_BUTTON_WIDTH, 24.0], control)
                .on_hover_text(
                    "Fixed output settings. Sweep locks changes; Output OFF stays available.",
                )
                .clicked()
            {
                if open {
                    self.dc_open.remove(&id);
                } else {
                    self.dc_open.insert(id);
                }
                let mode = if open {
                    self.settings.config.mode
                } else {
                    self.settings
                        .sources
                        .iter()
                        .find(|entry| entry.id == id)
                        .map_or(self.settings.config.mode, |entry| entry.dc.mode)
                };
                self.request_dc_live_mode(id, mode);
            }
        });
        ui.ctx()
            .data_mut(|data| data.insert_persisted(key, device_open));
        device_open
    }

    fn dc_current(&self, id: u64) -> Option<(Mode, f64, f64)> {
        match self.dc_role(id) {
            CardRole::Source => self
                .state
                .source_dc
                .as_ref()
                .map(|dc| (dc.mode, dc.value, dc.compliance)),
            CardRole::Secondary => self
                .state
                .source2_dc
                .as_ref()
                .map(|dc| (dc.mode, dc.value, dc.compliance)),
            CardRole::Bias => self
                .state
                .gate_voltage
                .zip(self.state.gate_compliance)
                .map(|(value, compliance)| (Mode::Iv, value, compliance)),
        }
    }

    /// Change only the idle live function; this never enables an output.
    fn request_dc_live_mode(&mut self, id: u64, mode: Mode) {
        if id == self.settings.source1_id
            && self.state.source_connected
            && self.state.output == OutputState::Off
            && !self.busy()
            && !self.python_locked()
        {
            self.live_samples.clear();
            self.live_reading = None;
            self.send(Command::SetLiveMode { mode });
        }
    }

    pub(super) fn dc_controls(&mut self, ui: &mut egui::Ui, id: u64) {
        if !self.dc_open.contains(&id) {
            return;
        }
        let Some(entry) = self
            .settings
            .sources
            .iter()
            .find(|entry| entry.id == id)
            .cloned()
        else {
            return;
        };
        let mut settings = entry.dc.clone();
        let role = self.dc_role(id);
        let connected = self.dc_connected(id);
        let on = self.dc_on(id);
        let editable = !self.busy() && !self.python_locked();
        if on && let Some((mode, value, compliance)) = self.dc_current(id) {
            settings.mode = mode;
            settings.value = value;
            settings.compliance = compliance;
        }
        let ranges = dc_ranges(&entry.model, settings.mode);
        if on {
            let held = match role {
                CardRole::Source => &self.state.source_dc_limits,
                CardRole::Secondary => &self.state.source2_dc_limits,
                CardRole::Bias => &self.state.gate_dc_limits,
            };
            if let Some(held) = held {
                if settings.range != Some(held.range) {
                    super::digits::reset_editor(ui.ctx(), id);
                }
                settings.range = Some(held.range);
            }
        } else if !ranges
            .iter()
            .any(|range| Some(range.command_value) == settings.range)
        {
            let initial = iv_core::manual_source_range(
                if entry.model == "SIMULATED_SOURCE" {
                    "YOKOGAWA_GS210"
                } else {
                    &entry.model
                },
                settings.mode,
                settings.value,
                settings.compliance,
            )
            .ok();
            if let Some(range) = ranges
                .iter()
                .find(|range| {
                    initial
                        .as_ref()
                        .is_some_and(|initial| initial.command_value == range.command_value)
                })
                .or_else(|| ranges.first())
            {
                select_dc_range(&mut settings, range);
            }
        }
        let mut digit_change = false;
        let mut off_requested = false;
        ui.add_space(5.0);
        egui::Frame::new()
            .fill(INPUT)
            .corner_radius(7)
            .inner_margin(8)
            .show(ui, |ui| {
                ui.label(
                    RichText::new(if self.busy() {
                        "Locked · source operation or sweep"
                    } else if role == CardRole::Bias {
                        "Fixed bias · held during IV"
                    } else {
                        "Fixed output · idle only"
                    })
                    .small()
                    .color(if self.busy() { ORANGE } else { MUTED }),
                );
                // Only the editor scrolls. The output actions below it and
                // the live monitor after this frame keep their own layout
                // space, even when typing or device settings add rows.
                super::widgets::dc_editor(ui, id, |ui| {
                    ui.add_enabled_ui(editable, |ui| {
                        if role == CardRole::Source {
                            if !on && self.state.reference_meter_connected && settings.mode != self.settings.config.mode {
                                settings.reset_function(self.settings.config.mode);
                            }
                            ui.add_enabled_ui(!on && !self.state.reference_meter_connected, |ui| {
                                let before = settings.mode;
                                egui::ComboBox::from_id_salt(("dc_mode", id))
                                    .width(ui.available_width())
                                    .selected_text(if settings.mode == Mode::Iv {
                                        "Voltage"
                                    } else {
                                        "Current"
                                    })
                                    .show_ui(ui, |ui| {
                                        ui.selectable_value(
                                            &mut settings.mode,
                                            Mode::Iv,
                                            "Voltage",
                                        );
                                        ui.selectable_value(
                                            &mut settings.mode,
                                            Mode::Vi,
                                            "Current",
                                        );
                                    });
                                if settings.mode != before {
                                    settings.reset_function(settings.mode);
                                }
                            }).response.on_hover_text("With both Meters connected, keep their voltage/current assignments fixed. Disconnect them before changing the source function.");
                        } else {
                            settings.mode = Mode::Iv;
                        }
                        let (unit, limit_unit) = if settings.mode == Mode::Iv {
                            ("V", "A")
                        } else {
                            ("A", "V")
                        };
                        let ranges = dc_ranges(&entry.model, settings.mode);
                        ui.add_enabled_ui(!on, |ui| {
                            if let Some(command) = range_selector(ui, &ranges, settings.range, unit)
                                && let Some(range) = ranges.iter().find(|range| range.command_value == command) {
                                select_dc_range(&mut settings, range);
                                super::digits::reset_editor(ui.ctx(), id);
                            }
                        }).response.on_disabled_hover_text("Turn output OFF before changing range.");
                        if let Some(range) = ranges.iter().find(|range| Some(range.command_value) == settings.range) {
                            let limits = crate::protocol::DcLimits { range: range.command_value, maximum: range.maximum, resolution: range.resolution };
                            settings.units[0] = if settings.mode == Mode::Vi || range.name.contains("mV") { 1 } else { 0 };
                            let config = super::digits::DigitControl {
                                value: settings.value,
                                unit: &mut settings.units[0],
                                base: unit,
                                limits,
                                display_maximum: range.maximum,
                                display_resolution: range.resolution,
                                enabled: editable,
                            };
                            if let Some(value) = super::digits::output_digits(ui, id, config) {
                                settings.value = value;
                                digit_change = true;
                            }
                        } else {
                            ui.label(RichText::new("No source ranges available.").small().color(RED));
                        }
                        ui.add_space(3.0);
                        let field_width = (ui.available_width() - 74.0).max(125.0);
                        egui::Grid::new(("manual_dc_fields", id))
                            .num_columns(2)
                            .spacing([5.0, 5.0])
                            .show(ui, |ui| {
                                for (label, index, value, unit, nonnegative) in [
                                    ("Compliance", 1, &mut settings.compliance, limit_unit, true),
                                    ("Ramp step", 2, &mut settings.ramp_step, unit, true),
                                ] {
                                    // Primary compliance is the single compact row on its card.
                                    if role == CardRole::Source && index == 1 {
                                        continue;
                                    }
                                    form_label(ui, label, 62.0);
                                    ui.add_enabled_ui(!on || index != 1, |ui| {
                                        quantity(
                                            ui,
                                            ("dc_value", id, index),
                                            value,
                                            &mut settings.units[index],
                                            unit,
                                            field_width,
                                            nonnegative,
                                        );
                                    });
                                    ui.end_row();
                                }
                            });
                    });
                });
                let validation = dc_request_valid(&entry.model, &settings);
                if let Err(error) = &validation {
                    ui.label(RichText::new(error).small().color(ORANGE));
                }
                ui.add_space(4.0);
                if !connected {
                    let can_connect = editable && model_supported(&entry.model)
                        && (role != CardRole::Bias || !self.state.gate_connected)
                        && (entry.model == "SIMULATED_SOURCE" || !entry.resource.trim().is_empty());
                    if button(ui, "Connect", 82.0, can_connect).clicked() {
                        match role {
                            CardRole::Source => self.connect_instrument(true),
                            CardRole::Secondary => self.connect_secondary(),
                            CardRole::Bias => {
                                self.settings.dc_gate_id = id;
                                self.send_dc_job(Command::ConnectGate { model: entry.model.clone(), simulation: entry.model == "SIMULATED_SOURCE", resource: entry.resource.clone() });
                            }
                        }
                    }
                }
                let action_width = (ui.available_width() - ui.spacing().item_spacing.x) / 2.0;
                ui.horizontal(|ui| {
                    let state = self.dc_output(id);
                    let can_turn_off = connected && state != OutputState::Off;
                    let can_turn_on = connected && state == OutputState::Off && editable && validation.is_ok();
                    let (text, fill, color) = match state {
                        OutputState::On => ("Output ON", ORANGE, Color32::BLACK),
                        OutputState::Unknown => ("Output ?", RED, Color32::WHITE),
                        OutputState::Off => ("Output OFF", INPUT, MUTED),
                    };
                    if ui.add_enabled(can_turn_off || can_turn_on, egui::Button::new(RichText::new(text).color(color)).fill(fill).min_size(egui::vec2(action_width, 26.0)))
                        .on_hover_text(if can_turn_off { "Turn output OFF immediately. Stops an active sweep." } else { "Enable the selected DC level. Range stays fixed while ON." }).clicked() {
                        if can_turn_off {
                            off_requested = true;
                            self.send(match role {
                                CardRole::Source => Command::SourceDcOff { secondary:false },
                                CardRole::Secondary => Command::SourceDcOff { secondary:true },
                                CardRole::Bias => Command::GateOutputOff,
                            });
                        } else {
                            let command = match role {
                                CardRole::Source | CardRole::Secondary => Command::SetSourceDc { secondary:role == CardRole::Secondary, mode:settings.mode, value:settings.value, compliance:settings.compliance, ramp_step:settings.ramp_step, range:settings.range },
                                CardRole::Bias => Command::SetGateVoltage { voltage:settings.value, compliance:settings.compliance, ramp_step:settings.ramp_step, range:settings.range },
                            };
                            self.send_dc_job(command);
                        }
                    }
                    if ui.add_enabled(editable && settings.range.is_some() && (!on || validation.is_ok()) && (!connected || state != OutputState::Unknown), egui::Button::new("Zero").fill(BUTTON).min_size(egui::vec2(action_width, 26.0)))
                        .on_hover_text("Ramp to zero within the selected range. Output stays ON if already ON; while OFF this only edits the level.").clicked() {
                        settings.value = 0.0;
                        super::digits::reset_editor(ui.ctx(), id);
                        digit_change = on;
                    }
                });
                if connected
                    && role == CardRole::Bias
                    && button(ui, "Disconnect", 88.0, editable).clicked()
                {
                    self.send_dc_job(Command::DisconnectGate);
                }
                if role == CardRole::Bias && self.state.gate_connected && !connected {
                    ui.label(
                        RichText::new(
                            "Another fixed-bias source is connected. Disconnect it first.",
                        )
                        .small()
                        .color(ORANGE),
                    );
                }
                if connected {
                    let actual = match role {
                        CardRole::Source => {
                            self.state.source_dc.as_ref().map(|s| (s.value, s.mode))
                        }
                        CardRole::Secondary => {
                            self.state.source2_dc.as_ref().map(|s| (s.value, s.mode))
                        }
                        CardRole::Bias => self.state.gate_voltage.map(|v| (v, Mode::Iv)),
                    };
                    if let Some((value, mode)) = actual {
                        ui.label(
                            RichText::new(format!(
                                "{} · {}",
                                match self.dc_output(id) {
                                    OutputState::On => "DC ON",
                                    OutputState::Off => "Output off",
                                    OutputState::Unknown => "Output unknown",
                                },
                                fmt_quantity(value, if mode == Mode::Iv { "V" } else { "A" })
                            ))
                            .color(if on { ORANGE } else { MUTED }),
                        );
                    }
                }
            });
        let request_valid = dc_request_valid(&entry.model, &settings).is_ok();
        if digit_change && on && editable && !off_requested && !self.busy() && request_valid {
            let command = match role {
                CardRole::Source | CardRole::Secondary => Command::SetSourceDc {
                    secondary: role == CardRole::Secondary,
                    mode: settings.mode,
                    value: settings.value,
                    compliance: settings.compliance,
                    ramp_step: settings.ramp_step,
                    range: settings.range,
                },
                CardRole::Bias => Command::SetGateVoltage {
                    voltage: settings.value,
                    compliance: settings.compliance,
                    ramp_step: settings.ramp_step,
                    range: settings.range,
                },
            };
            self.send_dc_job(command);
        }
        let next_mode = settings.mode;
        let mode_changed = next_mode != entry.dc.mode;
        if editable
            && let Some(entry) = self
                .settings
                .sources
                .iter_mut()
                .find(|entry| entry.id == id)
        {
            entry.dc = settings;
        }
        if editable && mode_changed {
            self.request_dc_live_mode(id, next_mode);
        }
    }

    fn send_dc_job(&mut self, command: Command) {
        if self.busy() || self.python_locked() {
            return;
        }
        let id = uuid::Uuid::new_v4().to_string();
        let result = self
            .client
            .as_ref()
            .map_err(Clone::clone)
            .and_then(|client| client.send_job(id.clone(), command));
        match result {
            Ok(()) => {
                self.pending_dc = Some((id, self.state.safety_revision));
            }
            Err(error) => {
                self.start_error = Some(error.clone());
                self.report(error);
            }
        }
    }
}

impl IvApp {
    pub(super) fn dc_stored_source_card(&mut self, ui: &mut egui::Ui, id: u64) {
        let Some(index) = self
            .settings
            .sources
            .iter()
            .position(|entry| entry.id == id)
        else {
            return;
        };
        let entry = self.settings.sources[index].clone();
        let connected = self.dc_gate_matches(id);
        let on = self.dc_on(id);
        let editable = !self.busy() && !self.python_locked() && !connected;
        egui::Frame::new()
            .fill(if connected {
                BG
            } else {
                BG.gamma_multiply(0.9)
            })
            .corner_radius(10)
            .stroke(Stroke::new(
                1.0_f32,
                if connected {
                    BORDER
                } else {
                    BORDER.gamma_multiply(0.62)
                },
            ))
            .inner_margin(egui::Margin::symmetric(10, 8))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(format!("Source {}", index + 1)).color(if connected {
                            Color32::from_rgb(214, 217, 224)
                        } else {
                            MUTED
                        }),
                    );
                    ui.label(
                        RichText::new(if on {
                            "DC ON"
                        } else if connected && self.state.gate_output == OutputState::Unknown {
                            "UNKNOWN"
                        } else if connected {
                            "DC READY"
                        } else {
                            "INACTIVE"
                        })
                        .size(9.0)
                        .color(if on { ORANGE } else { MUTED }),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if button(
                            ui,
                            "Remove",
                            62.0,
                            editable && self.settings.sources.len() > 2,
                        )
                        .clicked()
                        {
                            self.settings.remove_source(id);
                            self.dc_open.remove(&id);
                        }
                    });
                });
                if self.source_card_header(ui, id, device_summary(&entry.model)) {
                    ui.label(
                        RichText::new("Fixed bias source · separate from the sweep")
                            .small()
                            .color(MUTED),
                    );
                    let mut model = entry.model.clone();
                    let mut resource = entry.resource.clone();
                    ui.add_enabled_ui(editable, |ui| {
                        egui::ComboBox::from_id_salt(("bias_model", id))
                            .width(ui.available_width())
                            .selected_text(&model)
                            .show_ui(ui, |ui| {
                                for descriptor in instrument_catalog(InstrumentRole::Source) {
                                    ui.add_enabled_ui(model_supported(descriptor.id), |ui| {
                                        ui.selectable_value(
                                            &mut model,
                                            descriptor.id.to_owned(),
                                            descriptor.id,
                                        );
                                    });
                                }
                            });
                        ui.horizontal(|ui| {
                            let width = (ui.available_width() - 28.0).max(60.0);
                            let summary = resource_summary(&resource);
                            compact_field(ui, ("bias_resource", id), &mut resource, width, summary);
                            egui::ComboBox::from_id_salt(("bias_resources", id))
                                .width(20.0)
                                .selected_text("")
                                .show_ui(ui, |ui| {
                                    ui.set_min_width(470.0);
                                    for item in &self.state.resources {
                                        ui.selectable_value(
                                            &mut resource,
                                            item.resource.clone(),
                                            &item.label,
                                        );
                                    }
                                });
                        });
                        if button(ui, "Refresh", 70.0, editable).clicked() {
                            self.send(Command::Discover);
                        }
                    });
                    if let Some(entry) = self
                        .settings
                        .sources
                        .iter_mut()
                        .find(|entry| entry.id == id)
                    {
                        entry.model = model;
                        entry.resource = resource;
                    }
                }

                if connected {
                    ui.add_space(5.0);
                    ui.label(
                        RichText::new(
                            self.state
                                .gate_voltage
                                .map(|v| fmt_quantity(v, "V"))
                                .unwrap_or_else(|| "— V".into()),
                        )
                        .monospace()
                        .size(22.0)
                        .color(ORANGE),
                    );
                    ui.label(
                        RichText::new(format!(
                            "Source level · output {}",
                            self.state.gate_output.to_lowercase()
                        ))
                        .small()
                        .color(
                            if self.state.gate_output == OutputState::Unknown {
                                RED
                            } else {
                                MUTED
                            },
                        ),
                    );
                }
                self.dc_controls(ui, id);
            });
    }
}

impl IvApp {
    pub(super) fn dc_smoke_step(&mut self) {
        if self.busy() || !self.state.connected {
            return;
        }
        match self.dc_smoke_stage {
            0 => {
                self.send_dc_job(Command::ConnectGate {
                    model: "SIMULATED_SOURCE".into(),
                    simulation: true,
                    resource: "SIM_GATE".into(),
                });
                self.dc_smoke_stage = 1;
            }
            1 if self.state.gate_connected => {
                self.send_dc_job(Command::SetGateVoltage {
                    voltage: 0.02,
                    compliance: 0.001,
                    ramp_step: 0.001,
                    range: Some(1.0),
                });
                self.dc_smoke_stage = 2;
            }
            2 if self.state.gate_voltage == Some(0.02)
                && self.state.gate_output == OutputState::On =>
            {
                let limits = self
                    .state
                    .gate_dc_limits
                    .as_ref()
                    .expect("simulated gate range");
                let voltage =
                    iv_core::nudge_source_value(0.02, -0.019, limits.resolution, limits.maximum)
                        .expect("valid gate decrement");
                self.send_dc_job(Command::SetGateVoltage {
                    voltage,
                    compliance: 0.001,
                    ramp_step: 0.001,
                    range: Some(1.0),
                });
                self.dc_smoke_stage = 3;
            }
            3 if self.state.gate_voltage == Some(0.001)
                && self.state.gate_output == OutputState::On =>
            {
                assert_eq!(self.state.gate_dc_limits.as_ref().unwrap().range, 1.0);
                self.send_dc_job(Command::SetGateVoltage {
                    voltage: 0.02,
                    compliance: 0.001,
                    ramp_step: 0.001,
                    range: Some(1.0),
                });
                self.dc_smoke_stage = 4;
            }
            4 if self.state.gate_voltage == Some(0.02) => {
                self.send_dc_job(Command::SetSourceDc {
                    secondary: false,
                    mode: Mode::Iv,
                    value: 0.001,
                    compliance: 0.001,
                    ramp_step: 0.001,
                    range: Some(1.0),
                });
                self.dc_smoke_stage = 5;
            }
            5 if self.state.source_dc.is_some() && self.state.output == OutputState::On => {
                let limits = self.state.source_dc_limits.as_ref().unwrap();
                let value =
                    iv_core::nudge_source_value(0.001, 0.0001, limits.resolution, limits.maximum)
                        .unwrap();
                self.send_dc_job(Command::SetSourceDc {
                    secondary: false,
                    mode: Mode::Iv,
                    value,
                    compliance: 0.001,
                    ramp_step: 0.001,
                    range: Some(1.0),
                });
                self.dc_smoke_stage = 6;
            }
            6 if self
                .state
                .source_dc
                .as_ref()
                .is_some_and(|dc| dc.value == 0.0011) =>
            {
                self.settings.sources[0].dc.units[0] = 0;
                if std::env::args().any(|arg| arg == "--screenshot-dc-on")
                    && self.screenshot.is_some()
                    && !self.screenshot_saved
                {
                    return;
                }
                self.send_dc_job(Command::SetSourceDc {
                    secondary: false,
                    mode: Mode::Iv,
                    value: 0.0,
                    compliance: 0.001,
                    ramp_step: 0.001,
                    range: Some(1.0),
                });
                self.dc_smoke_stage = 7;
            }
            7 if self
                .state
                .source_dc
                .as_ref()
                .is_some_and(|dc| dc.value == 0.0) =>
            {
                assert_eq!(self.state.output, OutputState::On);
                assert_eq!(self.state.source_dc_limits.as_ref().unwrap().range, 1.0);
                self.send_dc_job(Command::SetSourceDc {
                    secondary: false,
                    mode: Mode::Iv,
                    value: 0.0011,
                    compliance: 0.001,
                    ramp_step: 0.001,
                    range: Some(1.0),
                });
                self.dc_smoke_stage = 8;
            }
            8 if self
                .state
                .source_dc
                .as_ref()
                .is_some_and(|dc| dc.value == 0.0011) =>
            {
                self.dc_smoke_stage = 9;
            }
            9 => {
                self.start_run();
                self.dc_smoke_stage = 10;
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::GuiJobCompletion;
    #[test]
    fn manual_range_selector_fits_one_row_in_a_compact_card() {
        let context = egui::Context::default();
        let ranges = dc_ranges("YOKOGAWA_GS210", Mode::Iv);
        let _ = context.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(240.0, 300.0),
                )),
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.set_width(224.0);
                    let before = ui.cursor().min.y;
                    assert_eq!(range_selector(ui, &ranges, Some(1.0), "V"), None);
                    assert!(ui.cursor().min.y - before < 35.0);
                    assert!(
                        ui.min_rect().width() <= 224.1,
                        "range selector width {}",
                        ui.min_rect().width()
                    );
                });
            },
        );
    }

    #[test]
    fn selecting_a_manual_range_sets_units_grid_and_resets_only_invalid_off_levels() {
        let ranges = dc_ranges("YOKOGAWA_GS210", Mode::Iv);
        let mut settings = settings::DcSettings {
            value: 0.002,
            ..Default::default()
        };
        select_dc_range(&mut settings, &ranges[0]);
        assert_eq!(settings.range, Some(0.01));
        assert_eq!(settings.units[0], 1);
        assert_eq!(settings.value, 0.002);
        settings.value = 0.0020001;
        select_dc_range(&mut settings, &ranges[2]);
        assert_eq!(
            settings.value, 0.0,
            "never round an off-grid target into another output"
        );
        assert_eq!(settings.units[0], 0);
        assert_eq!(settings.range, Some(1.0));
        settings.value = 2.0;
        select_dc_range(&mut settings, &ranges[0]);
        assert_eq!(settings.value, 0.0);
        let ranges = dc_ranges("YOKOGAWA_GS210", Mode::Vi);
        settings.reset_function(Mode::Vi);
        select_dc_range(&mut settings, &ranges[0]);
        assert_eq!(settings.units[0], 1);
        assert_eq!(settings.range, Some(0.001));
    }

    #[test]
    fn manual_range_validation_does_not_auto_select_or_weaken_compliance() {
        let mut settings = settings::DcSettings::default();
        assert!(dc_request_valid("YOKOGAWA_GS210", &settings).is_ok());
        settings.value = 1e-7;
        assert!(
            dc_request_valid("YOKOGAWA_GS210", &settings).is_err(),
            "1V range cannot emit 100nV"
        );
        settings.range = Some(0.01);
        assert!(
            dc_request_valid("YOKOGAWA_GS210", &settings).is_err(),
            "divider ranges require 200mA"
        );
        settings.compliance = 0.2;
        assert!(dc_request_valid("YOKOGAWA_GS210", &settings).is_ok());
        settings.value = 0.02;
        assert!(
            dc_request_valid("YOKOGAWA_GS210", &settings).is_err(),
            "10mV range must remain fixed"
        );
    }

    #[test]
    fn range_change_adapts_only_the_ramp_increment_to_the_hardware_grid() {
        assert_eq!(ramp_step_on_grid(1e-7, 1e-5), 1e-5);
        assert_eq!(ramp_step_on_grid(2e-5, 1e-5), 2e-5);
        assert_eq!(ramp_step_on_grid(1.5e-5, 1e-5), 2e-5);
    }

    #[test]
    fn manual_dc_lock_waits_for_matching_completion_and_survives_idle_updates() {
        let mut pending = Some(("dc-operation".into(), 0));
        let mut popup = None;
        let mut state = Snapshot {
            gui_job: Some(GuiJobCompletion {
                id: "other".into(),
                snapshot: serde_json::json!({"error":""}),
            }),
            ..Default::default()
        };
        settle_job(&mut pending, &mut popup, &state);
        assert!(pending.is_some());
        state.gui_job = Some(GuiJobCompletion {
            id: "dc-operation".into(),
            snapshot: serde_json::json!({"phase":"Stopped","error":""}),
        });
        settle_job(&mut pending, &mut popup, &state);
        assert!(pending.is_none());
        assert!(popup.is_none());
    }
    #[test]
    fn safety_ack_releases_manual_dc_lock_and_job_errors_explain_failure() {
        let mut pending = Some(("dc-operation".into(), 2));
        let mut popup = None;
        let mut state = Snapshot {
            safety_revision: 3,
            ..Default::default()
        };
        settle_job(&mut pending, &mut popup, &state);
        assert!(pending.is_none());
        pending = Some(("failed-dc".into(), 3));
        state.gui_jobs.push(GuiJobCompletion {
            id: "failed-dc".into(),
            snapshot: serde_json::json!({"error":"Source output OFF could not be verified."}),
        });
        settle_job(&mut pending, &mut popup, &state);
        assert!(pending.is_none());
        assert!(popup.unwrap().contains("could not be verified"));
    }
}
