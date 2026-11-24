//! Protection, start-error and timing dialogs.

use super::*;

impl IvApp {
    pub(super) fn protection_dialog(&mut self, ctx: &egui::Context) {
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

    pub(super) fn start_error_dialog(&mut self, ctx: &egui::Context) {
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

    pub(super) fn timing_dialog(&mut self, ctx: &egui::Context) {
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
}
