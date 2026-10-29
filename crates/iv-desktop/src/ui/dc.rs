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
            let width = (ui.available_width() - 66.0).max(100.0);
            let label = format!("   Device · {summary}");
            let disclosure = button(ui, &label, width, true);
            let center = egui::pos2(disclosure.rect.left() + 11.0, disclosure.rect.center().y);
            let offsets = if device_open {
                [(-4.0, -2.0), (4.0, -2.0), (0.0, 3.0)]
            } else {
                [(-2.0, -4.0), (-2.0, 4.0), (3.0, 0.0)]
            };
            ui.painter().add(egui::Shape::convex_polygon(
                offsets
                    .into_iter()
                    .map(|(x, y)| center + egui::vec2(x, y))
                    .collect(),
                MUTED,
                egui::Stroke::NONE,
            ));
            if disclosure.clicked() {
                device_open = !device_open;
            }
            let open = self.dc_open.contains(&id);
            let text = if unknown {
                "DC ?"
            } else if dc_on {
                "DC ON"
            } else {
                "DC"
            };
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
                .add_sized([58.0, 24.0], control)
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
            }
        });
        ui.ctx()
            .data_mut(|data| data.insert_persisted(key, device_open));
        device_open
    }

    fn dc_digit_limits(
        &self,
        id: u64,
        mode: Mode,
        value: f64,
        on: bool,
    ) -> Option<crate::protocol::DcLimits> {
        if on {
            return match self.dc_role(id) {
                CardRole::Source => self.state.source_dc_limits.clone(),
                CardRole::Secondary => self.state.source2_dc_limits.clone(),
                CardRole::Bias => self.state.gate_dc_limits.clone(),
            };
        }
        let entry = self.settings.sources.iter().find(|entry| entry.id == id)?;
        let model = if entry.model == "SIMULATED_SOURCE" {
            "YOKOGAWA_GS210"
        } else {
            &entry.model
        };
        let Some(iv_core::InstrumentProfile::Source(profile)) =
            iv_core::instrument_profile(model).ok()?
        else {
            return None;
        };
        let (ranges, maximum) = if mode == Mode::Iv {
            (&profile.voltage_ranges, profile.maximum_voltage)
        } else {
            (&profile.current_ranges, profile.maximum_current)
        };
        let range = ranges
            .iter()
            .find(|range| value.abs() <= range.maximum)
            .or_else(|| ranges.last())?;
        Some(crate::protocol::DcLimits {
            range: range.command_value,
            maximum,
            resolution: range.resolution,
        })
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
        let mut digit_change = false;
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
                            ui.add_enabled_ui(!on, |ui| {
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
                                    settings.value = 0.0;
                                    settings.compliance = if settings.mode == Mode::Iv {
                                        0.001
                                    } else {
                                        1.0
                                    };
                                    settings.ramp_step = if settings.mode == Mode::Iv {
                                        0.001
                                    } else {
                                        1e-6
                                    };
                                    settings.units = if settings.mode == Mode::Iv {
                                        [1, 1, 1]
                                    } else {
                                        [2, 0, 2]
                                    };
                                }
                            });
                        } else {
                            settings.mode = Mode::Iv;
                        }
                        let (unit, limit_unit) = if settings.mode == Mode::Iv {
                            ("V", "A")
                        } else {
                            ("A", "V")
                        };
                        if let Some(limits) =
                            self.dc_digit_limits(id, settings.mode, settings.value, on)
                        {
                            let ramp_valid = iv_core::nudge_source_value(
                                settings.ramp_step,
                                0.0,
                                limits.resolution,
                                limits.maximum,
                            )
                            .is_ok()
                                && settings.ramp_step > 0.0;
                            let config = super::digits::DigitControl {
                                value: settings.value,
                                unit: &mut settings.units[0],
                                base: unit,
                                limits,
                                enabled: editable && ramp_valid,
                                fixed_range: on,
                            };
                            if let Some(value) = super::digits::output_digits(ui, id, config) {
                                settings.value = value;
                                digit_change = true;
                            }
                        } else {
                            ui.label(
                            RichText::new(
                                "Verified source range unavailable. Turn output OFF and reconnect.",
                            )
                            .small()
                            .color(RED),
                        );
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
                    ui.label(
                        RichText::new("OFF before function or compliance changes.")
                            .small()
                            .color(MUTED),
                    );
                });
                let valid = settings.value.is_finite()
                    && settings.compliance.is_finite()
                    && settings.compliance > 0.0
                    && settings.ramp_step.is_finite()
                    && settings.ramp_step > 0.0;
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    if !connected {
                        let can_connect = editable
                            && model_supported(&entry.model)
                            && (role != CardRole::Bias || !self.state.gate_connected)
                            && (entry.model == "SIMULATED_SOURCE"
                                || !entry.resource.trim().is_empty());
                        if button(ui, "Connect", 82.0, can_connect).clicked() {
                            match role {
                                CardRole::Source => self.connect_instrument(true),
                                CardRole::Secondary => self.connect_secondary(),
                                CardRole::Bias => {
                                    self.settings.dc_gate_id = id;
                                    self.send_dc_job(Command::ConnectGate {
                                        model: entry.model.clone(),
                                        simulation: entry.model == "SIMULATED_SOURCE",
                                        resource: entry.resource.clone(),
                                    });
                                }
                            }
                        }
                    } else if button(
                        ui,
                        if on { "Apply" } else { "Output ON" },
                        88.0,
                        editable && valid,
                    )
                    .clicked()
                    {
                        let command = match role {
                            CardRole::Source | CardRole::Secondary => Command::SetSourceDc {
                                secondary: role == CardRole::Secondary,
                                mode: settings.mode,
                                value: settings.value,
                                compliance: settings.compliance,
                                ramp_step: settings.ramp_step,
                            },
                            CardRole::Bias => Command::SetGateVoltage {
                                voltage: settings.value,
                                compliance: settings.compliance,
                                ramp_step: settings.ramp_step,
                            },
                        };
                        self.send_dc_job(command);
                    }
                    if button(ui, "OFF", 55.0, connected)
                        .on_hover_text(
                            "Immediate safety operation. If a sweep is running it is stopped.",
                        )
                        .clicked()
                    {
                        self.send(match role {
                            CardRole::Source => Command::SourceDcOff { secondary: false },
                            CardRole::Secondary => Command::SourceDcOff { secondary: true },
                            CardRole::Bias => Command::GateOutputOff,
                        });
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
        let request_valid = self
            .dc_digit_limits(id, settings.mode, settings.value, on)
            .is_some_and(|limits| {
                settings.compliance.is_finite()
                    && settings.compliance > 0.0
                    && settings.ramp_step > 0.0
                    && iv_core::nudge_source_value(
                        settings.value,
                        0.0,
                        limits.resolution,
                        limits.maximum,
                    )
                    .is_ok()
                    && iv_core::nudge_source_value(
                        settings.ramp_step,
                        0.0,
                        limits.resolution,
                        limits.maximum,
                    )
                    .is_ok()
            });
        if digit_change && on && editable && !self.busy() && request_valid {
            let command = match role {
                CardRole::Source | CardRole::Secondary => Command::SetSourceDc {
                    secondary: role == CardRole::Secondary,
                    mode: settings.mode,
                    value: settings.value,
                    compliance: settings.compliance,
                    ramp_step: settings.ramp_step,
                },
                CardRole::Bias => Command::SetGateVoltage {
                    voltage: settings.value,
                    compliance: settings.compliance,
                    ramp_step: settings.ramp_step,
                },
            };
            self.send_dc_job(command);
        }
        if editable
            && let Some(entry) = self
                .settings
                .sources
                .iter_mut()
                .find(|entry| entry.id == id)
        {
            entry.dc = settings;
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
                self.dc_controls(ui, id);
                if connected && !self.dc_open.contains(&id) {
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
                });
                self.dc_smoke_stage = 3;
            }
            3 if self.state.gate_voltage == Some(0.001)
                && self.state.gate_output == OutputState::On =>
            {
                assert_eq!(self.state.gate_dc_limits.as_ref().unwrap().range, 0.1);
                self.send_dc_job(Command::SetGateVoltage {
                    voltage: 0.02,
                    compliance: 0.001,
                    ramp_step: 0.001,
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
                });
                self.dc_smoke_stage = 6;
            }
            6 if self
                .state
                .source_dc
                .as_ref()
                .is_some_and(|dc| dc.value == 0.0011) =>
            {
                self.settings.sources[0].dc.units[0] = 0; // Display only, no engine command.
                self.dc_smoke_stage = 7;
            }
            7 => {
                assert_eq!(self.state.source_dc.as_ref().unwrap().value, 0.0011);
                self.start_run();
                self.dc_smoke_stage = 8;
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
