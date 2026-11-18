//! One device-card renderer for every Source and Meter.
//!
//! Default and added instruments differ only in the data they read: the
//! measurement-role stream (Source 1, Source 2, Meter, second Meter), an
//! independently registered inventory stream, or the fixed-bias stream.  The
//! header, Device row, connection fields, Connect | Disconnect | Settings row,
//! live display, Settings rows and DC output controls are drawn here once.
use super::*;
use widgets::CardDelete;

/// Three 24 px Settings rows, the 4 px gap before each and item spacing.
const SETTINGS_ROWS_HEIGHT: f32 = 3.0 * 24.0 + 3.0 * 4.0 + 2.0 * 4.0;

/// Measurement role currently bound to one saved device.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum DeviceRole {
    Source1,
    Source2,
    Meter,
    ReferenceMeter,
    Unassigned,
}

pub(super) struct AddDevice {
    pub(super) role: InstrumentRole,
    pub(super) label: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct DeleteDevice {
    pub(super) source: bool,
    pub(super) id: u64,
}

/// Display data for the shared live readout.  Every data path fills the
/// same fields, so collapsed cards keep one height whatever they observe.
struct LiveView {
    plot: egui::Id,
    quantity: &'static str,
    unit: &'static str,
    color: Color32,
    tag: &'static str,
    tag_hover: &'static str,
    status: &'static str,
    status_active: bool,
    /// Blinking LIMIT/FAULT slot: label and hover detail.
    alert: Option<(&'static str, String)>,
    badge: Option<(String, Color32)>,
    value: String,
    fraction: f32,
    points: Vec<[f64; 2]>,
    window: f64,
    scale: AxisScale,
    caption: String,
    caption_color: Color32,
}

impl LiveView {
    fn waiting(plot: egui::Id, source: bool, voltage: bool) -> Self {
        let (quantity, unit, color) = if voltage {
            ("V", "V", ORANGE)
        } else {
            ("I", "A", TEAL)
        };
        Self {
            plot,
            quantity,
            unit,
            color,
            tag: if source { "SOURCE LEVEL" } else { "MEASURED" },
            tag_hover: if source {
                "Programmed source level; not an independent output measurement."
            } else {
                "Measured by the meter."
            },
            status: "WAITING",
            status_active: false,
            alert: None,
            badge: None,
            value: format!("— {unit}"),
            fraction: 0.0,
            points: Vec::new(),
            window: 2.0,
            scale: AxisScale::for_magnitude(0.0),
            caption: format!("Time · s ago · {unit}"),
            caption_color: MUTED,
        }
    }
}

fn trend_fraction(points: &[[f64; 2]]) -> f32 {
    let extent = points.iter().fold(0.0_f64, |m, p| m.max(p[1].abs()));
    match points.last() {
        Some(point) if extent > 0.0 => (point[1] / extent).clamp(-1.0, 1.0) as f32,
        _ => 0.0,
    }
}

/// Draw one live readout.  Rows have fixed heights: status, value, trend
/// bar, 55 px plot, caption and one summary line (blank when unused).
fn show_live_view(ui: &mut egui::Ui, view: LiveView, summary: Option<String>) -> egui::Rect {
    ui.add_space(4.0);
    ui.separator();
    ui.horizontal(|ui| {
        ui.set_height(18.0);
        ui.label(RichText::new(view.quantity).color(view.color).strong());
        ui.label(RichText::new(view.tag).size(9.0).color(MUTED))
            .on_hover_text(view.tag_hover);
        ui.label(
            RichText::new(view.status)
                .size(9.0)
                .color(if view.status_active { BLUE } else { MUTED }),
        );
        // Reserve the LIMIT slot in every state so a trip never moves rows.
        let (slot, response) = ui.allocate_exact_size(egui::vec2(34.0, 14.0), egui::Sense::hover());
        if let Some((label, detail)) = &view.alert {
            let bright = (ui.input(|input| input.time) * 2.0) as i64 % 2 == 0;
            ui.ctx().request_repaint_after(Duration::from_millis(100));
            ui.painter().rect_filled(
                slot,
                3.0,
                if bright {
                    Color32::from_rgb(220, 49, 66)
                } else {
                    Color32::from_rgb(142, 39, 48)
                },
            );
            ui.painter().text(
                slot.center(),
                egui::Align2::CENTER_CENTER,
                *label,
                egui::FontId::proportional(9.0),
                Color32::WHITE,
            );
            response.on_hover_text(detail);
        }
        // The output badge takes only the remaining width (right-aligned,
        // clipped) so narrow two-column cards never grow.
        let (rest, _) = ui.allocate_exact_size(
            egui::vec2(ui.available_width().max(0.0), 14.0),
            egui::Sense::hover(),
        );
        if let Some((text, color)) = &view.badge {
            let galley =
                ui.painter()
                    .layout_no_wrap(text.clone(), egui::FontId::proportional(9.0), *color);
            let width = galley.size().x + 8.0;
            let badge = egui::Rect::from_min_max(
                egui::pos2((rest.right() - width).max(rest.left()), rest.top()),
                rest.right_bottom(),
            );
            let painter = ui.painter_at(rest);
            painter.rect_filled(badge, 4.0, color.gamma_multiply(0.12));
            painter.galley(
                egui::pos2(badge.left() + 4.0, badge.center().y - galley.size().y / 2.0),
                galley,
                *color,
            );
        }
    });
    ui.add(
        egui::Label::new(
            RichText::new(&view.value)
                .monospace()
                .size(22.0)
                .color(view.color),
        )
        .truncate(),
    );
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 6.0), egui::Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, 2.0, BORDER);
    let center = rect.center().x;
    let end = center + view.fraction * rect.width() * 0.5;
    painter.rect_filled(
        egui::Rect::from_min_max(
            egui::pos2(center.min(end), rect.top()),
            egui::pos2(center.max(end), rect.bottom()),
        ),
        0.0,
        view.color,
    );
    painter.line_segment(
        [
            egui::pos2(center, rect.top()),
            egui::pos2(center, rect.bottom()),
        ],
        Stroke::new(1.0_f32, Color32::WHITE),
    );
    let scale = view.scale;
    let plot = Plot::new(view.plot)
        .height(55.0)
        .show_background(false)
        .show_axes([true, true])
        .show_grid([true, true])
        .allow_drag(false)
        .allow_zoom(false)
        .allow_scroll(false)
        .include_x(-view.window)
        .include_x(0.0)
        .include_y(0.0)
        .y_axis_formatter(move |mark, _| scale.tick(mark.value, mark.step_size))
        .label_formatter(|_, _| String::new())
        .show(ui, |plot_ui| {
            plot_ui.line(
                Line::new("trend", view.points)
                    .color(view.color)
                    .width(1.5_f32)
                    .fill(0.0_f32)
                    .fill_alpha(0.75_f32),
            );
        })
        .response
        .rect;
    ui.add(
        egui::Label::new(
            RichText::new(view.caption)
                .size(9.0)
                .color(view.caption_color),
        )
        .truncate(),
    );
    let (line, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 14.0), egui::Sense::hover());
    if let Some(summary) = summary {
        ui.scope_builder(
            egui::UiBuilder::new()
                .max_rect(line)
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
            |ui| {
                ui.add(egui::Label::new(RichText::new(summary).small().color(MUTED)).truncate());
            },
        );
    }
    plot
}

/// A Settings row that belongs to a measurement role this device does not
/// hold.  It stays visible, disabled, and explains where it applies.
fn role_setting_row(ui: &mut egui::Ui, label: &str, text: &str, hint: &str) {
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        form_label(ui, label, 85.0);
        let width = ui.available_width();
        ui.add_enabled(
            false,
            egui::Button::new(RichText::new(text).color(MUTED))
                .fill(INPUT)
                .truncate()
                .min_size(egui::vec2(width, 24.0)),
        )
        .on_disabled_hover_text(hint);
    });
}

impl IvApp {
    pub(super) fn device_role(&self, source: bool, id: u64) -> DeviceRole {
        let settings = &self.settings;
        if source {
            if id == settings.source1_id {
                DeviceRole::Source1
            } else if settings.config.balanced_source.is_some() && id == settings.source2_id {
                DeviceRole::Source2
            } else {
                DeviceRole::Unassigned
            }
        } else if id == settings.meter_id {
            DeviceRole::Meter
        } else if settings.reference_meter_id == Some(id) {
            DeviceRole::ReferenceMeter
        } else {
            DeviceRole::Unassigned
        }
    }

    /// Connected through the engine's measurement role, not the inventory.
    fn role_assigned(&self, role: DeviceRole) -> bool {
        match role {
            DeviceRole::Source1 => self.state.source_connected,
            DeviceRole::Source2 => self.state.source2_connected,
            DeviceRole::Meter => self.state.meter_connected,
            DeviceRole::ReferenceMeter => self.state.reference_meter_connected,
            DeviceRole::Unassigned => false,
        }
    }

    pub(super) fn card_connected(&self, source: bool, id: u64) -> bool {
        self.role_assigned(self.device_role(source, id))
            || self.inventory_connected(id)
            || (source && self.dc_gate_matches(id))
    }

    fn entries(&self, source: bool) -> &[settings::InstrumentEntry] {
        if source {
            &self.settings.sources
        } else {
            &self.settings.meters
        }
    }

    /// Automatic name: kind and position in the saved inventory.
    pub(super) fn card_fallback_name(&self, source: bool, id: u64) -> String {
        let index = self
            .entries(source)
            .iter()
            .position(|entry| entry.id == id)
            .unwrap_or(0);
        format!("{} {}", if source { "Source" } else { "Meter" }, index + 1)
    }

    /// Saved label, or the automatic name when the label is blank.
    pub(super) fn card_display_name(&self, source: bool, id: u64) -> String {
        self.entries(source)
            .iter()
            .find(|entry| entry.id == id)
            .map(|entry| entry.label.trim())
            .filter(|label| !label.is_empty())
            .map_or_else(|| self.card_fallback_name(source, id), str::to_owned)
    }

    /// Active roles keep editing the legacy scalar fields that the engine
    /// commands use; all other cards edit their saved inventory entry.
    fn card_device(&self, source: bool, id: u64, role: DeviceRole) -> (String, String) {
        let settings = &self.settings;
        match role {
            DeviceRole::Source1 => (settings.source_model.clone(), settings.source.clone()),
            DeviceRole::Source2 => (settings.source2_model.clone(), settings.source2.clone()),
            DeviceRole::Meter => (settings.meter_model.clone(), settings.meter.clone()),
            DeviceRole::ReferenceMeter | DeviceRole::Unassigned => self
                .entries(source)
                .iter()
                .find(|entry| entry.id == id)
                .map(|entry| (entry.model.clone(), entry.resource.clone()))
                .unwrap_or_default(),
        }
    }

    fn set_card_device(
        &mut self,
        source: bool,
        id: u64,
        role: DeviceRole,
        model: String,
        resource: String,
    ) {
        let settings = &mut self.settings;
        match role {
            DeviceRole::Source1 => {
                settings.source_model.clone_from(&model);
                settings.source.clone_from(&resource);
            }
            DeviceRole::Source2 => {
                settings.source2_model.clone_from(&model);
                settings.source2.clone_from(&resource);
            }
            DeviceRole::Meter => {
                settings.meter_model.clone_from(&model);
                settings.meter.clone_from(&resource);
            }
            DeviceRole::ReferenceMeter | DeviceRole::Unassigned => {}
        }
        let entries = if source {
            &mut settings.sources
        } else {
            &mut settings.meters
        };
        if let Some(entry) = entries.iter_mut().find(|entry| entry.id == id) {
            entry.model = model;
            entry.resource = resource;
        }
        if role == DeviceRole::ReferenceMeter {
            settings.bind_active_entries();
        }
    }

    fn card_delete(&self, source: bool, id: u64) -> CardDelete<'static> {
        if !self.inventory_editable() {
            CardDelete::Blocked("Wait for the current operation to finish.")
        } else if self.card_connected(source, id) {
            CardDelete::Blocked("Disconnect this device before deleting it.")
        } else if let Some(reason) = self.settings.deletion_blocker(source, id) {
            CardDelete::Blocked(reason)
        } else {
            CardDelete::Allowed
        }
    }

    fn record_card_rect(&mut self, key: String, rect: egui::Rect) {
        self.card_geometry.insert(key, rect);
    }

    /// The single Source/Meter card used for default and added devices.
    pub(super) fn unified_device_card(&mut self, ui: &mut egui::Ui, source: bool, id: u64) {
        if !self.entries(source).iter().any(|entry| entry.id == id) {
            return;
        }
        let role = self.device_role(source, id);
        let connected = self.card_connected(source, id);
        let editable = self.inventory_editable();
        let kind = self.device_card_kind(source, id);
        let fallback = self.card_fallback_name(source, id);
        let delete = self.card_delete(source, id);
        let mut label = self
            .entries(source)
            .iter()
            .find(|entry| entry.id == id)
            .map_or_else(String::new, |entry| entry.label.clone());
        let mut label_changed = false;
        let delete_clicked = widgets::device_card(
            ui,
            id,
            editable,
            delete,
            |ui| {
                let response = widgets::device_name(ui, kind, id, &fallback, &mut label, editable);
                label_changed = response.changed() || response.lost_focus();
                if response.lost_focus() {
                    label = label.trim().to_owned();
                }
            },
            |ui| self.device_card_body(ui, source, id, role, connected),
        );
        if label_changed {
            let entries = if source {
                &mut self.settings.sources
            } else {
                &mut self.settings.meters
            };
            if let Some(entry) = entries.iter_mut().find(|entry| entry.id == id) {
                entry.label = label;
            }
        }
        if delete_clicked {
            self.delete_device = Some(DeleteDevice { source, id });
        }
    }

    fn device_card_body(
        &mut self,
        ui: &mut egui::Ui,
        source: bool,
        id: u64,
        role: DeviceRole,
        connected: bool,
    ) {
        let (model, resource) = self.card_device(source, id, role);
        let summary = device_summary(&model).to_owned();
        let settings_key = ui.make_persistent_id(("instrument_settings", id));
        if self.open_all_cards {
            ui.ctx()
                .data_mut(|data| data.insert_temp(settings_key, true));
        }
        let device_open = if source {
            if self.open_all_cards {
                let key = ui.make_persistent_id(("source_device_disclosure", id));
                ui.ctx().data_mut(|data| data.insert_persisted(key, true));
            }
            self.source_card_header(ui, id, &summary, connected)
        } else {
            let key = ui.make_persistent_id(("meter_device_disclosure", id));
            if self.open_all_cards {
                ui.ctx().data_mut(|data| data.insert_persisted(key, true));
            }
            let mut open = ui
                .ctx()
                .data_mut(|data| data.get_persisted::<bool>(key))
                .unwrap_or(false);
            let width = widgets::device_width(ui);
            ui.horizontal(|ui| {
                widgets::device_disclosure(ui, &summary, connected, width, &mut open)
                    .on_hover_text(format!("{model}\n{}", resource_summary_or_none(&resource)));
            });
            ui.ctx().data_mut(|data| data.insert_persisted(key, open));
            open
        };
        let settings_open = device_open
            && self.card_connection_controls(ui, source, id, role, connected, settings_key);
        let view = self.card_live_view(ui, source, id, role, connected);
        let summary = self.card_summary(role);
        let plot = show_live_view(ui, view, summary);
        self.record_card_rect(format!("{id}.plot"), plot);
        if settings_open {
            // Every Source and Meter has three Settings rows; reserve one
            // height so expanded cards of both kinds stay the same size.
            let rows = ui
                .scope(|ui| {
                    ui.set_min_height(SETTINGS_ROWS_HEIGHT);
                    self.card_settings(ui, source, id, role);
                })
                .response
                .rect;
            self.record_card_rect(format!("{id}.settings_rows"), rows);
        }
        if source {
            self.dc_controls(ui, id);
        }
    }

    /// Model/Resource fields, then the equal Connect | Disconnect | Settings
    /// row.  Returns whether instrument Settings are open.
    fn card_connection_controls(
        &mut self,
        ui: &mut egui::Ui,
        source: bool,
        id: u64,
        role: DeviceRole,
        connected: bool,
        settings_key: egui::Id,
    ) -> bool {
        let mut settings_open = ui
            .ctx()
            .data(|data| data.get_temp::<bool>(settings_key))
            .unwrap_or(false);
        let idle = if role == DeviceRole::Unassigned {
            self.inventory_editable()
        } else {
            !self.busy()
        };
        let (before_model, before_resource) = self.card_device(source, id, role);
        let (mut model, mut resource) = (before_model.clone(), before_resource.clone());
        let width = (ui.available_width() - 91.0).max(0.0);
        let mut refresh = false;
        egui::Grid::new(("card_device_fields", id))
            .num_columns(2)
            .spacing([6.0, 6.0])
            .show(ui, |ui| {
                widgets::device_field_label(ui, "Model");
                ui.add_enabled_ui(idle && !connected, |ui| {
                    egui::ComboBox::from_id_salt(("card_model", id))
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
                                    ui.selectable_value(
                                        &mut model,
                                        candidate.to_string(),
                                        candidate,
                                    );
                                })
                                .response
                                .on_disabled_hover_text("Not yet supported in the Rust preview");
                            }
                        });
                });
                ui.end_row();
                widgets::device_field_label(ui, "Resource");
                ui.horizontal(|ui| {
                    let resource_width = (width - 76.0).max(65.0);
                    self.card_resource_field(
                        ui,
                        id,
                        &mut resource,
                        resource_width,
                        idle && !connected,
                    );
                    if button(ui, "Refresh", 68.0, idle).clicked() {
                        refresh = true;
                    }
                });
                ui.end_row();
            });
        if model != before_model || resource != before_resource {
            self.set_card_device(source, id, role, model.clone(), resource.clone());
        }
        let mode = self.settings.config.mode;
        let valid = match role {
            DeviceRole::Unassigned => {
                let mut entry = self
                    .entries(source)
                    .iter()
                    .find(|entry| entry.id == id)
                    .cloned()
                    .unwrap_or_default();
                entry.model.clone_from(&model);
                entry.resource.clone_from(&resource);
                inventory_connection(source, &entry, mode).is_some()
            }
            _ => {
                instrument_descriptor(&model).is_some_and(|device| device.supports_mode(mode))
                    && (model.starts_with("SIMULATED_") || !resource.is_empty())
            }
        };
        let mut connect = false;
        let mut disconnect = false;
        let mut rects = [egui::Rect::NOTHING; 3];
        ui.horizontal(|ui| {
            let button_width = (ui.available_width() - 2.0 * ui.spacing().item_spacing.x) / 3.0;
            let response = button(ui, "Connect", button_width, idle && !connected && valid);
            connect = response.clicked();
            rects[0] = response.rect;
            let response = button(ui, "Disconnect", button_width, idle && connected);
            disconnect = response.clicked();
            rects[1] = response.rect;
            let response = button(ui, "Settings", button_width, true);
            if response.clicked() {
                settings_open = !settings_open;
            }
            rects[2] = response.rect;
        });
        for (name, rect) in ["connect", "disconnect", "settings"].into_iter().zip(rects) {
            self.record_card_rect(format!("{id}.{name}"), rect);
        }
        if refresh {
            self.send(Command::Discover);
        }
        if connect {
            self.card_connect(source, id, role);
        }
        if disconnect {
            self.card_disconnect(source, id, role);
        }
        ui.ctx()
            .data_mut(|data| data.insert_temp(settings_key, settings_open));
        settings_open
    }

    fn card_resource_field(
        &self,
        ui: &mut egui::Ui,
        id: u64,
        selection: &mut String,
        width: f32,
        enabled: bool,
    ) {
        ui.add_enabled_ui(enabled, |ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            ui.spacing_mut().button_padding.x = 3.0;
            ui.spacing_mut().icon_width = 14.0;
            ui.spacing_mut().icon_spacing = 0.0;
            let summary = resource_summary(selection);
            compact_field(
                ui,
                ("card_resource_edit", id),
                selection,
                (width - 28.0).max(30.0),
                summary,
            );
            egui::ComboBox::from_id_salt(("card_resources", id))
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

    fn card_connect(&mut self, source: bool, id: u64, role: DeviceRole) {
        match role {
            DeviceRole::Source1 => self.connect_instrument(true),
            DeviceRole::Source2 => self.connect_secondary(),
            DeviceRole::Meter => self.connect_instrument(false),
            DeviceRole::ReferenceMeter => self.connect_reference_meter(),
            DeviceRole::Unassigned => {
                let command = self
                    .entries(source)
                    .iter()
                    .find(|entry| entry.id == id)
                    .and_then(|entry| {
                        inventory_connect_command(source, entry, self.settings.config.mode)
                    });
                if let Some(command) = command {
                    self.send(command);
                }
            }
        }
    }

    fn card_disconnect(&mut self, source: bool, id: u64, role: DeviceRole) {
        if self.inventory_connected(id) {
            self.send(Command::DisconnectInventory { id });
            return;
        }
        match role {
            DeviceRole::Source1 => {
                self.send(Command::DisconnectSource);
            }
            DeviceRole::Source2 => {
                self.send(Command::DisconnectSource2);
            }
            DeviceRole::Meter => {
                self.send(Command::DisconnectMeter);
            }
            DeviceRole::ReferenceMeter => {
                self.send(Command::DisconnectReferenceMeter);
            }
            DeviceRole::Unassigned if source && self.dc_gate_matches(id) => {
                self.send_dc_job(Command::DisconnectGate);
            }
            DeviceRole::Unassigned => {}
        }
    }

    /// One summary line under the live display; blank for roles without one.
    fn card_summary(&self, role: DeviceRole) -> Option<String> {
        let config = &self.settings.config;
        match role {
            DeviceRole::Source1 => {
                let (target_unit, limit_unit) = if config.mode == Mode::Iv {
                    ("V", "A")
                } else {
                    ("A", "V")
                };
                let limit = self.preview.as_ref().ok().map_or_else(
                    || "—".into(),
                    |preview| fmt_quantity(preview.hardware_compliance, limit_unit),
                );
                Some(format!(
                    "Sweep compliance {limit}  ·  Max {}",
                    fmt_quantity(config.source_limit, target_unit)
                ))
            }
            DeviceRole::Meter => {
                let (limit, unit) = if config.mode == Mode::Iv {
                    (self.settings.current_compliance, "A")
                } else {
                    (self.settings.voltage_compliance, "V")
                };
                Some(format!("Sweep limit {}", fmt_quantity(limit, unit)))
            }
            DeviceRole::Source2 | DeviceRole::ReferenceMeter | DeviceRole::Unassigned => None,
        }
    }

    fn card_live_view(
        &self,
        ui: &egui::Ui,
        source: bool,
        id: u64,
        role: DeviceRole,
        connected: bool,
    ) -> LiveView {
        let assigned = self.role_assigned(role);
        match role {
            DeviceRole::Source1 | DeviceRole::Meter if assigned || !connected => {
                self.assigned_live_view(ui, source)
            }
            DeviceRole::Source2 if assigned || !connected => self.source2_live_view(ui, assigned),
            DeviceRole::ReferenceMeter if assigned || !connected => {
                self.reference_live_view(ui, assigned)
            }
            DeviceRole::Unassigned if source && self.dc_gate_matches(id) => {
                self.gate_live_view(ui, id)
            }
            _ => self.inventory_live_view(ui, source, id),
        }
    }

    /// Source 1 / measurement Meter stream (idle readings or paired live).
    fn assigned_live_view(&self, ui: &egui::Ui, source: bool) -> LiveView {
        let idle_reading = self
            .state
            .idle_reading
            .as_ref()
            .filter(|reading| reading.source == source);
        let mode = if source {
            self.card_control_mode()
        } else {
            self.meter_live_mode()
        };
        let idle_reading = idle_reading.filter(|reading| reading.mode == mode);
        let paired_reading = self
            .live_reading
            .as_ref()
            .filter(|reading| reading.mode == mode);
        let source_channel = if mode == Mode::Iv { 1 } else { 0 };
        let channel = if source {
            source_channel
        } else {
            1 - source_channel
        };
        let mut view = LiveView::waiting(
            ui.make_persistent_id(("live_channel", source, channel)),
            source,
            channel == 1,
        );
        let unit = view.unit;
        let (connected, simulation) = if source {
            (self.state.source_connected, self.state.source_simulation)
        } else {
            (self.state.meter_connected, self.state.meter_simulation)
        };
        let running = connected && self.state.error.is_empty();
        let last_sample = if idle_reading.is_some() {
            self.idle_samples.back().map(|sample| sample[0])
        } else {
            paired_reading.and_then(|_| self.live_samples.back().map(|sample| sample.time))
        };
        let stale = last_sample.is_some_and(|time| self.boot.elapsed().as_secs_f64() - time > 3.0);
        if simulation {
            view.tag = "SIM";
        }
        view.status = if source && self.state.source_dc.is_some() {
            "DC SET"
        } else if running && stale {
            "STALE"
        } else if last_sample.is_none() {
            "WAITING"
        } else if running {
            "LIVE"
        } else {
            "HELD"
        };
        view.status_active = running;
        let alert = self.state.protection_alert.as_ref().filter(|alert| {
            protection_role(alert) == source && (!source || alert.source_index != Some(2))
        });
        if let Some(alert) = alert {
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
            let label = if alert.indicator.contains("EMR") {
                "FAULT"
            } else {
                "LIMIT"
            };
            view.alert = Some((label, detail));
        }
        if source {
            view.badge = Some((
                format!("Output {}", self.state.output.to_lowercase()),
                if self.state.output == OutputState::Off {
                    MUTED
                } else {
                    ORANGE
                },
            ));
        }
        let newest = if connected {
            self.boot.elapsed().as_secs_f64()
        } else {
            last_sample.unwrap_or(0.0)
        };
        view.window = newest.clamp(2.0, 30.0);
        let source1_factor = if self.state.balanced_active {
            self.settings
                .config
                .balanced_source
                .as_ref()
                .map_or(0.5, |dual| dual.source1_fraction())
        } else {
            1.0
        };
        let paired_value = |sample: &LiveSample| {
            if channel == 0 {
                sample.current
            } else {
                sample.voltage * if source { source1_factor } else { 1.0 }
            }
        };
        let extent = if idle_reading.is_some() {
            self.idle_samples
                .iter()
                .fold(0.0_f64, |max, sample| max.max(sample[1].abs()))
        } else if paired_reading.is_some() {
            self.live_samples
                .iter()
                .fold(0.0_f64, |max, sample| max.max(paired_value(sample).abs()))
        } else {
            0.0
        };
        let scale = AxisScale::for_magnitude(extent);
        view.scale = scale;
        let dc_text = if source {
            self.state.source_dc.as_ref().map(|dc| {
                display_reading_at_scale(
                    dc.value,
                    self.state
                        .source_dc_limits
                        .as_ref()
                        .map(|limits| limits.resolution),
                    unit,
                    AxisScale::for_magnitude(dc.value.abs()),
                )
            })
        } else {
            None
        };
        let text = dc_text.unwrap_or_else(|| {
            if let Some(reading) = idle_reading {
                return display_reading_at_scale(reading.value, reading.resolution, unit, scale);
            }
            paired_reading
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
        view.value = text.trim_end_matches(" (resolution unknown)").to_owned();
        view.points = if idle_reading.is_some() {
            self.idle_samples
                .iter()
                .filter(|sample| newest - sample[0] <= view.window)
                .map(|sample| [sample[0] - newest, sample[1]])
                .collect()
        } else if paired_reading.is_some() {
            self.live_samples
                .iter()
                .filter(|sample| newest - sample.time <= view.window)
                .map(|sample| [sample.time - newest, paired_value(sample)])
                .collect()
        } else {
            Vec::new()
        };
        let current = view.points.last().map_or(0.0, |p| p[1]);
        view.fraction = if extent > 0.0 {
            (current / extent).clamp(-1.0, 1.0) as f32
        } else {
            0.0
        };
        view.caption = format!(
            "Time · s ago · {}{} · updated {:.1} s ago{}",
            scale.prefix,
            unit,
            last_sample.map_or(0.0, |time| (self.boot.elapsed().as_secs_f64() - time)
                .max(0.0)),
            if unknown {
                " · resolution unknown"
            } else {
                ""
            }
        );
        view
    }

    /// Source 2 programmed level while it is bound to Dual V-I.
    fn source2_live_view(&self, ui: &egui::Ui, connected: bool) -> LiveView {
        let mut view = LiveView::waiting(ui.make_persistent_id("source2_live"), true, true);
        if self.state.source2_simulation {
            view.tag = "SIM";
        }
        view.status = if connected { "LIVE" } else { "WAITING" };
        view.status_active = connected;
        if let Some(alert) = self.state.protection_alert.as_ref().filter(|alert| {
            alert.kind == ProtectionKind::SourceCompliance && alert.source_index == Some(2)
        }) {
            view.alert = Some((
                "LIMIT",
                format!(
                    "Source 2 compliance: {}",
                    fmt_quantity(alert.limit, &alert.unit)
                ),
            ));
        }
        view.badge = Some((
            format!("Output {}", self.state.source2_output.to_lowercase()),
            MUTED,
        ));
        let resolution = self
            .state
            .source2_resolution
            .or(self.state.source2_simulation.then_some(1e-9));
        let value = self.state.source2_level;
        let text = value
            .map(|v| display_reading(v, resolution, "V"))
            .unwrap_or_else(|| "— V".into());
        let unknown = value.is_some() && resolution.is_none();
        view.value = text.trim_end_matches(" (resolution unknown)").to_owned();
        let now = self.boot.elapsed().as_secs_f64();
        view.window = now.clamp(2.0, 30.0);
        view.scale = AxisScale::for_magnitude(
            self.source2_samples
                .iter()
                .fold(0.0_f64, |m, s| m.max(s.voltage.abs())),
        );
        view.points = self
            .source2_samples
            .iter()
            .filter(|s| now - s.time <= 30.0)
            .map(|s| [s.time - now, s.voltage])
            .collect();
        view.fraction = trend_fraction(&view.points);
        view.caption = format!(
            "Time · s ago · programmed source level{}",
            if unknown {
                " · resolution unknown"
            } else {
                ""
            }
        );
        view
    }

    /// Optional second Meter measuring the source quantity.
    fn reference_live_view(&self, ui: &egui::Ui, connected: bool) -> LiveView {
        let voltage = self.settings.config.mode == Mode::Iv;
        let mut view = LiveView::waiting(
            ui.make_persistent_id("reference_live_trend"),
            false,
            voltage,
        );
        let value = connected
            .then_some(self.live_reading.as_ref())
            .flatten()
            .and_then(|record| record.reference_meter_value);
        view.status = if value.is_some() { "LIVE" } else { "WAITING" };
        view.status_active = value.is_some();
        if let Some(value) = value {
            view.value = fmt_quantity(value, view.unit);
        }
        let now = self.boot.elapsed().as_secs_f64();
        view.window = now.clamp(2.0, 30.0);
        view.points = self
            .reference_live_samples
            .iter()
            .filter(|point| connected && now - point[0] <= 30.0)
            .map(|point| [point[0] - now, point[1]])
            .collect();
        let extent = view.points.iter().fold(0.0_f64, |m, p| m.max(p[1].abs()));
        view.scale = AxisScale::for_magnitude(extent);
        view.fraction = trend_fraction(&view.points);
        view.caption = format!("Time · s ago · {}{}", view.scale.prefix, view.unit);
        view
    }

    /// Fixed-bias source held independently of the IV sweep.
    fn gate_live_view(&self, ui: &egui::Ui, id: u64) -> LiveView {
        let mut view = LiveView::waiting(ui.make_persistent_id(("gate_live", id)), true, true);
        if self.state.gate_simulation {
            view.tag = "SIM";
        }
        if let Some(value) = self.state.gate_voltage {
            view.value = fmt_quantity(value, "V");
            view.status = "DC SET";
            view.status_active = true;
        }
        view.badge = Some((
            format!("Output {}", self.state.gate_output.to_lowercase()),
            match self.state.gate_output {
                OutputState::Off => MUTED,
                OutputState::On => ORANGE,
                OutputState::Unknown => RED,
            },
        ));
        view.caption = "Time · s ago · fixed bias · held during IV".into();
        view
    }

    /// Independently registered inventory device.
    fn inventory_live_view(&self, ui: &egui::Ui, source: bool, id: u64) -> LiveView {
        let device = self
            .state
            .inventory_devices
            .iter()
            .find(|entry| entry.id == id);
        let kind_voltage = self.device_card_kind(source, id).ends_with("(V)");
        let reading = device.and_then(|device| device.reading.as_ref());
        let voltage = reading.map_or(kind_voltage, |reading| {
            reading.source == (reading.mode == Mode::Iv)
        });
        let mut view = LiveView::waiting(
            ui.make_persistent_id(("inventory_live", id)),
            source,
            voltage,
        );
        let Some(device) = device else {
            return view;
        };
        if device.simulation {
            view.tag = "SIM";
        }
        if device.source {
            view.badge = Some((
                format!("Output {}", device.output.to_lowercase()),
                if device.output == OutputState::Off {
                    MUTED
                } else {
                    ORANGE
                },
            ));
        }
        if !device.error.is_empty() {
            view.caption = device.error.clone();
            view.caption_color = ORANGE;
        }
        let Some(reading) = reading else {
            return view;
        };
        let now = self.boot.elapsed().as_secs_f64();
        let samples = self.inventory_samples.get(&id);
        let stale = samples
            .and_then(|points| points.back())
            .is_some_and(|point| now - point[0] > 3.0);
        let scale = AxisScale::for_magnitude(
            samples
                .into_iter()
                .flatten()
                .fold(reading.value.abs(), |extent, point| {
                    extent.max(point[1].abs())
                }),
        );
        view.scale = scale;
        view.status = if stale { "STALE" } else { "LIVE" };
        view.status_active = !stale;
        view.value = if reading.resolution.is_some() {
            display_reading_at_scale(reading.value, reading.resolution, view.unit, scale)
        } else {
            fmt_quantity(reading.value, view.unit)
        };
        view.window = now.clamp(2.0, 30.0);
        view.points = samples
            .into_iter()
            .flatten()
            .filter(|point| now - point[0] <= 30.0)
            .map(|point| [point[0] - now, point[1]])
            .collect();
        view.fraction = trend_fraction(&view.points);
        if device.error.is_empty() {
            view.caption = format!(
                "Time · s ago · {}{}{}",
                scale.prefix,
                view.unit,
                if reading.resolution.is_none() {
                    " · resolution unknown"
                } else {
                    ""
                }
            );
        }
        view
    }

    /// Instrument Settings, drawn below the live display when toggled.
    fn card_settings(&mut self, ui: &mut egui::Ui, source: bool, id: u64, role: DeviceRole) {
        if source {
            match role {
                DeviceRole::Source1 => self.source_compliance_control(ui),
                DeviceRole::Source2 => self.source2_compliance_control(ui),
                _ => role_setting_row(
                    ui,
                    "Compliance",
                    "Source 1 sweep only",
                    "Sweep source protection applies to the device assigned as Source 1 in Settings → Devices.",
                ),
            }
            self.dc_compliance_control(ui, id);
            match role {
                DeviceRole::Source1 => self.source_target_limit_control(ui),
                _ => role_setting_row(
                    ui,
                    "Max. target",
                    "Source 1 sweep only",
                    "The maximum sweep target is set on Source 1; Source 2 follows the Dual V-I split.",
                ),
            }
            return;
        }
        match role {
            DeviceRole::Meter => self.meter_measurement_controls(ui),
            DeviceRole::ReferenceMeter => {
                role_setting_row(
                    ui,
                    "Limit",
                    "Measurement Meter only",
                    "The sweep stop threshold applies to the Meter measuring the opposite quantity.",
                );
                self.reference_meter_range_control(ui, id);
                self.shared_nplc_control(ui);
            }
            _ => {
                for label in ["Limit", "Range", "NPLC"] {
                    role_setting_row(
                        ui,
                        label,
                        "Assigned Meter only",
                        "Applies when this device is assigned as a Meter in Settings → Devices.",
                    );
                }
            }
        }
    }

    /// Dual V-I Source 2 hardware compliance.
    fn source2_compliance_control(&mut self, ui: &mut egui::Ui) {
        ui.add_space(4.0);
        ui.add_enabled_ui(self.inventory_editable(), |ui| {
            if let Some(dual) = &mut self.settings.config.balanced_source {
                ui.horizontal(|ui| {
                    form_label(ui, "Compliance", 85.0);
                    let width = ui.available_width();
                    quantity(
                        ui,
                        "source2_compliance",
                        &mut dual.source_compliance,
                        &mut self.settings.source2_compliance_unit,
                        "A",
                        width,
                        true,
                    );
                });
            }
        });
    }

    /// The second Meter observes the source quantity, i.e. voltage in V-I
    /// and current in I-V.  Reuse the normal meter range widget with the
    /// flipped measurement function so range and input-resistance wording
    /// remain device-specific.
    fn reference_meter_range_control(&mut self, ui: &mut egui::Ui, id: u64) {
        let Some(model) = self
            .settings
            .meters
            .iter()
            .find(|entry| entry.id == id)
            .map(|entry| entry.model.clone())
        else {
            return;
        };
        let reference_mode = if self.settings.config.mode == Mode::Iv {
            Mode::Vi
        } else {
            Mode::Iv
        };
        let range_locked_by_operation = self.busy() || self.python_locked();
        ui.add_space(4.0);
        let range_edit = ui.add_enabled_ui(
            !range_locked_by_operation && crate::gui_control::outputs_off(&self.state),
            |ui| {
                ui.horizontal(|ui| {
                    form_label(ui, "Range", 85.0);
                    match reference_mode {
                        Mode::Iv => widgets::meter_range_choice(
                            ui,
                            "reference_meter_current_range",
                            &model,
                            reference_mode,
                            &mut self.settings.config.meter_current_range,
                        ),
                        Mode::Vi => widgets::meter_range_choice(
                            ui,
                            "reference_meter_voltage_range",
                            &model,
                            reference_mode,
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
        if range_edit.inner {
            self.send_meter_ranges();
        }
    }

    /// NPLC is one run setting applied to both Meters.
    fn shared_nplc_control(&mut self, ui: &mut egui::Ui) {
        ui.add_space(4.0);
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
        })
        .response
        .on_hover_text("Integration time shared by both Meters.");
    }

    /// TRKR Add Device modal.  Adding never changes measurement roles.
    pub(super) fn add_device_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut device) = self.add_device.take() else {
            return;
        };
        let editable = self.inventory_editable();
        let mut add = false;
        let mut cancel = false;
        let response = egui::Modal::new(egui::Id::new("add_device"))
            .frame(
                egui::Frame::popup(&ctx.global_style())
                    .fill(widgets::PANEL)
                    .inner_margin(16),
            )
            .show(ctx, |ui| {
                ui.set_width(360.0);
                ui.strong("Add Device");
                ui.add_space(12.0);
                ui.add_enabled_ui(editable, |ui| {
                    ui.horizontal(|ui| {
                        ui.selectable_value(&mut device.role, InstrumentRole::Source, "Source");
                        ui.selectable_value(&mut device.role, InstrumentRole::Meter, "Meter");
                    });
                    ui.add_space(12.0);
                    ui.label("Label");
                    ui.add(
                        egui::TextEdit::singleline(&mut device.label)
                            .hint_text("Optional name, e.g. Gate")
                            .char_limit(64)
                            .desired_width(f32::INFINITY),
                    );
                    ui.add_space(8.0);
                    ui.label(
                        RichText::new(
                            "Numbered automatically. Configure the connection in its device card.",
                        )
                        .small()
                        .color(Color32::GRAY),
                    );
                });
                ui.add_space(16.0);
                ui.horizontal(|ui| {
                    cancel = ui
                        .add(egui::Button::new("Cancel").min_size(Vec2::new(80.0, 26.0)))
                        .clicked();
                    add = ui
                        .add_enabled(
                            editable,
                            egui::Button::new("Add").min_size(Vec2::new(80.0, 26.0)),
                        )
                        .clicked();
                });
            });
        if add {
            self.confirm_add_device(&device);
        } else if !cancel && !response.should_close() {
            self.add_device = Some(device);
        }
    }

    /// Add button of the dialog: append the device and bring its card into
    /// view.  Measurement roles stay as they are.
    fn confirm_add_device(&mut self, device: &AddDevice) -> u64 {
        let source = device.role == InstrumentRole::Source;
        let id = if source {
            self.settings.add_source(&device.label)
        } else {
            self.settings.add_meter(&device.label)
        };
        self.scroll_to_card = Some(id);
        let name = self.inventory_name(source, id);
        self.report(format!("Added {name}."));
        id
    }

    /// TRKR Delete Device confirmation.  Connected devices and devices used
    /// by a measurement role cannot be deleted; the reason stays visible.
    pub(super) fn delete_device_dialog(&mut self, ctx: &egui::Context) {
        let Some(target) = self.delete_device.take() else {
            return;
        };
        let DeleteDevice { source, id } = target;
        if !self.entries(source).iter().any(|entry| entry.id == id) {
            return;
        }
        let name = self.inventory_name(source, id);
        let blocked = match self.card_delete(source, id) {
            CardDelete::Allowed => None,
            CardDelete::Blocked(reason) => Some(reason),
        };
        let mut accepted = false;
        let mut cancelled = false;
        let response = egui::Modal::new(egui::Id::new("delete_device"))
            .frame(
                egui::Frame::popup(&ctx.global_style())
                    .fill(widgets::PANEL)
                    .inner_margin(16),
            )
            .show(ctx, |ui| {
                ui.set_width(360.0);
                ui.strong("Delete Device?");
                ui.add_space(12.0);
                ui.label(format!("Remove {name} from this configuration?"));
                ui.label("Its saved connection, label and DC settings are removed. Recorded measurements are not changed.");
                ui.add_space(8.0);
                if let Some(reason) = blocked {
                    ui.colored_label(Color32::LIGHT_RED, reason);
                }
                ui.add_space(16.0);
                ui.horizontal(|ui| {
                    cancelled = ui
                        .add(egui::Button::new("Cancel").min_size(Vec2::new(80.0, 26.0)))
                        .clicked();
                    accepted = ui
                        .add_enabled(
                            blocked.is_none(),
                            egui::Button::new("Delete").min_size(Vec2::new(80.0, 26.0)),
                        )
                        .clicked();
                });
            });
        if accepted {
            let removed = if source {
                self.settings.remove_source(id)
            } else {
                self.settings.remove_meter(id)
            };
            if removed {
                self.dc_open.remove(&id);
                self.inventory_samples.remove(&id);
                self.report(format!("Deleted {name}."));
            }
        } else if !cancelled && !response.should_close() {
            self.delete_device = Some(target);
        }
    }

    /// JSON geometry for smoke validation: each card and its controls.
    pub(super) fn card_geometry_report(&self) -> serde_json::Value {
        let rect = |key: String| {
            self.card_geometry
                .get(&key)
                .map(|r| serde_json::json!([r.min.x, r.min.y, r.max.x, r.max.y]))
        };
        self.settings
            .ordered_cards()
            .into_iter()
            .map(|(source, id)| {
                serde_json::json!({
                    "id": id,
                    "source": source,
                    "role": format!("{:?}", self.device_role(source, id)),
                    "card": rect(format!("{id}.card")),
                    "plot": rect(format!("{id}.plot")),
                    "connect": rect(format!("{id}.connect")),
                    "disconnect": rect(format!("{id}.disconnect")),
                    "settings": rect(format!("{id}.settings")),
                    "settings_rows": rect(format!("{id}.settings_rows")),
                })
            })
            .collect()
    }
}

fn resource_summary_or_none(resource: &str) -> String {
    if resource.is_empty() {
        "No resource selected".into()
    } else {
        resource_summary(resource)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> (egui::Context, IvApp) {
        let context = egui::Context::default();
        widgets::style(&context);
        let creation_context = eframe::CreationContext::_new_kittest(context.clone());
        let app = IvApp::new(
            &creation_context,
            true,
            None,
            Arc::new(AtomicBool::new(false)),
            Instant::now(),
            None,
            None,
        )
        .unwrap();
        (context, app)
    }

    /// Lay out the production card list in a sidebar-sized column.
    fn render(context: &egui::Context, app: &mut IvApp, width: f32, two_columns: bool) {
        for _ in 0..3 {
            let _ = context.run_ui(egui::RawInput::default(), |ui| {
                egui::CentralPanel::default().show_inside(ui, |ui| {
                    let column =
                        egui::Rect::from_min_size(ui.max_rect().min, egui::vec2(width, 4000.0));
                    ui.scope_builder(
                        egui::UiBuilder::new()
                            .max_rect(column)
                            .layout(egui::Layout::top_down(egui::Align::Min)),
                        |ui| app.device_cards(ui, two_columns),
                    );
                });
            });
        }
    }

    fn rect(app: &IvApp, key: String) -> egui::Rect {
        *app.card_geometry
            .get(&key)
            .unwrap_or_else(|| panic!("missing {key}"))
    }

    #[test]
    fn default_and_added_cards_share_expanded_controls_in_one_and_two_columns() {
        for (width, two_columns) in [(330.0, false), (534.0, true)] {
            let (context, mut app) = app();
            let added_source = app.settings.add_source("");
            let added_meter = app.settings.add_meter("Hall");
            app.open_all_cards = true;
            render(&context, &mut app, width, two_columns);
            let cards = app.settings.ordered_cards();
            assert_eq!(cards.len(), 4);
            assert_eq!(app.device_role(true, added_source), DeviceRole::Unassigned);
            assert_eq!(app.device_role(false, added_meter), DeviceRole::Unassigned);
            for (source, id) in cards {
                let card = rect(&app, format!("{id}.card"));
                let buttons = ["connect", "disconnect", "settings"]
                    .map(|name| rect(&app, format!("{id}.{name}")));
                for button in buttons {
                    assert!(
                        (button.width() - buttons[0].width()).abs() < 0.5,
                        "{id}: {buttons:?}"
                    );
                    assert!(button.left() >= card.left() && button.right() <= card.right() + 0.5);
                    assert!((button.top() - buttons[0].top()).abs() < 0.5, "one row");
                }
                let rows = rect(&app, format!("{id}.settings_rows"));
                let plot = rect(&app, format!("{id}.plot"));
                assert!(
                    plot.top() > buttons[0].bottom(),
                    "live display after Device row"
                );
                assert!(
                    rows.top() >= plot.bottom(),
                    "Settings rows below the live plot"
                );
                assert!(rows.right() <= card.right() + 0.5 && rows.bottom() <= card.bottom());
                let column = if two_columns {
                    (width - 8.0) / 2.0
                } else {
                    width
                };
                assert!(
                    card.width() <= column + 0.5,
                    "{id} width {} > {column}",
                    card.width()
                );
                if two_columns {
                    let left = card.left() < width / 2.0;
                    assert_eq!(left, source, "Sources left, Meters right");
                }
            }
            // Device and Settings open: Sources and Meters, original and
            // added, all keep one height.
            let heights: Vec<_> = app
                .settings
                .ordered_cards()
                .into_iter()
                .map(|(_, id)| rect(&app, format!("{id}.card")).height())
                .collect();
            assert!(
                heights.iter().all(|h| (h - heights[0]).abs() < 0.5),
                "{width}: {heights:?}"
            );
            // Original and added cards of one kind have identical widths.
            for source in [true, false] {
                let widths: Vec<_> = app
                    .settings
                    .ordered_cards()
                    .into_iter()
                    .filter(|card| card.0 == source)
                    .map(|(_, id)| rect(&app, format!("{id}.card")).width())
                    .collect();
                assert!(
                    widths.windows(2).all(|w| (w[0] - w[1]).abs() < 0.5),
                    "{widths:?}"
                );
            }
        }
    }

    #[test]
    fn collapsed_cards_have_one_size_whatever_their_role_or_origin() {
        for (width, two_columns) in [(330.0, false), (534.0, true)] {
            let (context, mut app) = app();
            app.settings.add_source("Gate");
            let reference = app.settings.add_meter("");
            app.settings.reference_meter_id = Some(reference);
            app.settings.add_meter("Spare");
            render(&context, &mut app, width, two_columns);
            let cards: Vec<_> = app
                .settings
                .ordered_cards()
                .into_iter()
                .map(|(_, id)| rect(&app, format!("{id}.card")))
                .collect();
            assert_eq!(cards.len(), 5);
            assert!(
                cards
                    .iter()
                    .all(|card| (card.height() - cards[0].height()).abs() < 0.5),
                "{width}: {:?}",
                cards.iter().map(|card| card.height()).collect::<Vec<_>>()
            );
            assert!(
                app.card_geometry
                    .keys()
                    .all(|key| !key.ends_with(".connect")),
                "collapsed cards show no connection row"
            );
        }
    }

    #[test]
    fn delete_is_offered_for_added_devices_and_explained_for_roles() {
        let (_context, mut app) = app();
        let (source1, meter) = (app.settings.source1_id, app.settings.meter_id);
        assert!(matches!(
            app.card_delete(true, source1),
            CardDelete::Blocked(_)
        ));
        assert!(matches!(
            app.card_delete(false, meter),
            CardDelete::Blocked(_)
        ));
        let added = app.settings.add_source("");
        assert_eq!(app.card_delete(true, added), CardDelete::Allowed);
        // An original device is deletable once it no longer holds a role.
        app.settings.source1_id = added;
        app.settings.bind_active_entries();
        assert_eq!(app.card_delete(true, source1), CardDelete::Allowed);
        assert!(matches!(
            app.card_delete(true, added),
            CardDelete::Blocked(_)
        ));
        // Connected inventory devices are never deleted.
        app.state
            .inventory_devices
            .push(crate::protocol::InventoryDevice {
                id: source1,
                source: true,
                simulation: true,
                model: "SIMULATED_SOURCE".into(),
                resource: String::new(),
                identity: String::new(),
                mode: Mode::Iv,
                assigned: false,
                auxiliary: false,
                output: OutputState::Off,
                dc: None,
                dc_limits: None,
                dc_job: None,
                reading: None,
                error: String::new(),
            });
        assert_eq!(
            app.card_delete(true, source1),
            CardDelete::Blocked("Disconnect this device before deleting it.")
        );
    }

    #[test]
    fn adding_a_device_keeps_measurement_roles_and_appends_its_card() {
        let (context, mut app) = app();
        let roles = (
            app.settings.source1_id,
            app.settings.meter_id,
            app.settings.source2_id,
            app.settings.reference_meter_id,
        );
        for role in [InstrumentRole::Source, InstrumentRole::Meter] {
            app.add_device = Some(AddDevice {
                role,
                label: " New ".into(),
            });
            // Without input the modal stays open and changes nothing.
            let _ = context.run_ui(egui::RawInput::default(), |ui| {
                app.add_device_dialog(ui.ctx());
            });
            let device = app.add_device.take().expect("dialog stays open");
            let id = app.confirm_add_device(&device);
            let source = role == InstrumentRole::Source;
            assert_eq!(app.card_display_name(source, id), "New");
            assert_eq!(app.settings.ordered_cards().last(), Some(&(source, id)));
            render(&context, &mut app, 330.0, false);
            assert_eq!(app.scroll_to_card, None, "new card was scrolled into view");
            assert!(app.card_geometry.contains_key(&format!("{id}.card")));
        }
        assert_eq!(
            roles,
            (
                app.settings.source1_id,
                app.settings.meter_id,
                app.settings.source2_id,
                app.settings.reference_meter_id,
            )
        );
    }
}
