//! Instrument inventory, device cards, bindings and connection requests.

use super::*;

impl IvApp {
    pub(super) fn sync_inventory(&mut self, next: &Snapshot) {
        if let Some((job, _)) = &self.pending_bindings
            && let Some(completion) = next
                .gui_jobs
                .iter()
                .find(|completion| completion.id == *job)
        {
            let (_, settings) = self.pending_bindings.take().expect("binding job pending");
            if completion.snapshot["inventory_error"]
                .as_str()
                .unwrap_or_default()
                .is_empty()
            {
                let previous = std::mem::replace(&mut self.settings, settings);
                let mode_changed = previous.config.mode != self.settings.config.mode;
                self.report_device_bindings(&previous, "Applied");
                let primary_changed = mode_changed
                    || previous.source1_id != self.settings.source1_id
                    || previous.meter_id != self.settings.meter_id;
                if primary_changed {
                    self.live_reading = None;
                    self.live_samples.clear();
                    self.idle_samples.clear();
                    self.last_live_timestamp.clear();
                }
                if primary_changed
                    || previous.reference_meter_id != self.settings.reference_meter_id
                {
                    self.reference_live_samples.clear();
                }
                if mode_changed
                    || previous.source2_id != self.settings.source2_id
                    || previous.config.balanced_source.is_some()
                        != self.settings.config.balanced_source.is_some()
                {
                    self.source2_samples.clear();
                }
                if mode_changed
                    || previous.config.balanced_source.is_some()
                        != self.settings.config.balanced_source.is_some()
                {
                    self.samples.clear();
                    self.reference_samples.clear();
                    self.separate = Default::default();
                    self.gate_metric = reference_view::GateMetric::Sweep;
                    self.rebuild_plot_series();
                    self.rebuild_reference_plot_series();
                    self.custom_text.clear();
                }
                if mode_changed {
                    self.send(Command::SetLiveMode {
                        mode: self.settings.config.mode,
                    });
                }
                self.send_meter_ranges();
            } else if let Some(error) = completion.snapshot["inventory_error"].as_str() {
                self.device_change_notice = format!("Device selection rejected. {error}");
                self.report(self.device_change_notice.clone());
            }
        }
        if let Some((job, source, id)) = &self.pending_inventory_selection
            && next.gui_jobs.iter().any(|completion| completion.id == *job)
        {
            if next
                .inventory_devices
                .iter()
                .any(|entry| entry.id == *id && entry.assigned)
            {
                let previous = self.settings.clone();
                self.select_inventory_entry(*source, *id);
                self.report_device_bindings(&previous, "Applied");
                self.live_reading = None;
                self.live_samples.clear();
                self.idle_samples.clear();
                self.last_live_timestamp.clear();
            }
            self.pending_inventory_selection = None;
        }
        if !next.inventory_error.is_empty() && next.inventory_error != self.state.inventory_error {
            self.report(next.inventory_error.clone());
        }
        self.inventory_samples.retain(|id, _| {
            next.inventory_devices
                .iter()
                .any(|entry| entry.id == *id && entry.reading.is_some())
        });
        let time = self.boot.elapsed().as_secs_f64();
        for entry in &next.inventory_devices {
            let Some(reading) = &entry.reading else {
                continue;
            };
            let previous = self
                .state
                .inventory_devices
                .iter()
                .find(|old| old.id == entry.id)
                .and_then(|old| old.reading.as_ref());
            if previous.is_some_and(|old| old.timestamp == reading.timestamp) {
                continue;
            }
            let samples = self.inventory_samples.entry(entry.id).or_default();
            if previous.is_some_and(|old| old.mode != reading.mode) {
                samples.clear();
            }
            samples.push_back([time, reading.value]);
            while samples.len() > 1024
                || samples.front().is_some_and(|point| time - point[0] > 30.0)
            {
                samples.pop_front();
            }
        }
    }

    pub(super) fn inventory_connected(&self, id: u64) -> bool {
        self.state
            .inventory_devices
            .iter()
            .any(|entry| entry.id == id)
    }

    pub(super) fn device_cards(&mut self, ui: &mut egui::Ui, two_columns: bool) {
        let cards = self.settings.ordered_cards();
        self.card_geometry.clear();
        let mut drop = None;
        let mut render_card = |ui: &mut egui::Ui, (source, id): (bool, u64)| {
            let card = ui
                .push_id(("device_card", id), |ui| {
                    self.unified_device_card(ui, source, id)
                })
                .response;
            self.card_geometry.insert(format!("{id}.card"), card.rect);
            if self.scroll_to_card == Some(id) {
                card.scroll_to_me(Some(egui::Align::Max));
                self.scroll_to_card = None;
            }
            if self.inventory_editable()
                && let Some(dragged) = card.dnd_hover_payload::<u64>()
                && *dragged != id
                && (!two_columns || cards.contains(&(source, *dragged)))
                && let Some(pointer) = ui.ctx().pointer_interact_pos()
            {
                let after = pointer.y >= card.rect.center().y;
                let y = if after {
                    card.rect.bottom() + 4.0
                } else {
                    card.rect.top() - 4.0
                };
                ui.painter().hline(
                    card.rect.x_range(),
                    y,
                    egui::Stroke::new(2.0_f32, widgets::DROP_LINE),
                );
                if card.dnd_release_payload::<u64>().is_some() {
                    drop = Some((*dragged, id, after));
                }
            }
            ui.add_space(8.0);
        };
        if two_columns {
            let column_area_width = ui.available_width();
            let mut sources = cards.iter().copied().filter(|card| card.0);
            let mut meters = cards.iter().copied().filter(|card| !card.0);
            loop {
                let row = [sources.next(), meters.next()];
                if row == [None, None] {
                    break;
                }
                let mut row_rect = ui.available_rect_before_wrap();
                row_rect.max.x = row_rect.min.x + column_area_width;
                ui.scope_builder(egui::UiBuilder::new().max_rect(row_rect), |ui| {
                    ui.columns_const(|columns: &mut [egui::Ui; 2]| {
                        for (column, card) in columns.iter_mut().zip(row) {
                            if let Some(card) = card {
                                render_card(column, card);
                            }
                        }
                    });
                });
            }
        } else {
            for &card in &cards {
                render_card(ui, card);
            }
        }
        if let Some(&dragged) = egui::DragAndDrop::payload::<u64>(ui.ctx()).as_deref()
            && let Some(pointer) = ui.ctx().pointer_interact_pos()
        {
            if self.inventory_editable() {
                let clip = ui.clip_rect();
                if clip.contains(pointer) {
                    let delta = if pointer.y < clip.top() + 28.0 {
                        10.0
                    } else if pointer.y > clip.bottom() - 28.0 {
                        -10.0
                    } else {
                        0.0
                    };
                    if delta != 0.0 {
                        ui.scroll_with_delta(egui::vec2(0.0, delta));
                        ui.ctx().request_repaint();
                    }
                }
            }
            // TRKR drag preview: "Kind: name" beside the pointer.
            if let Some(&(source, _)) = cards.iter().find(|&&(_, id)| id == dragged) {
                let title = format!(
                    "{}: {}",
                    self.device_card_kind(source, dragged),
                    self.card_display_name(source, dragged)
                );
                let painter = ui.ctx().layer_painter(egui::LayerId::new(
                    egui::Order::Tooltip,
                    egui::Id::new("device-drag-preview"),
                ));
                let galley =
                    painter.layout_no_wrap(title, egui::FontId::proportional(12.0), widgets::TEXT);
                let preview = egui::Rect::from_min_size(
                    pointer + Vec2::new(14.0, 14.0),
                    galley.size() + Vec2::new(16.0, 10.0),
                );
                painter.rect(
                    preview,
                    6.0,
                    widgets::PANEL,
                    Stroke::new(1.0_f32, BORDER),
                    egui::StrokeKind::Inside,
                );
                painter.galley(preview.min + Vec2::new(8.0, 5.0), galley, widgets::TEXT);
            }
        }
        if let Some((dragged, target, after)) = drop
            && self.settings.move_card(dragged, target, after)
            && let Some(&(source, _)) = cards.iter().find(|&&(_, id)| id == dragged)
        {
            self.device_change_notice = format!("Moved {}.", self.inventory_name(source, dragged));
            self.report(self.device_change_notice.clone());
        }
    }

    pub(super) fn connect_all(&mut self) {
        self.settings.store_active_entries();
        let settings = self.settings.clone();
        let mut connections = Vec::new();
        for (source, entries) in [(true, &settings.sources), (false, &settings.meters)] {
            for entry in entries {
                if !self.inventory_connected(entry.id)
                    && !(source && self.dc_gate_matches(entry.id))
                    && let Some(connection) =
                        inventory_connection(source, entry, settings.config.mode)
                {
                    connections.push(connection);
                }
            }
        }
        self.request_bindings(settings, connections);
    }

    pub(super) fn request_bindings(
        &mut self,
        settings: Settings,
        connections: Vec<crate::protocol::InventoryConnection>,
    ) {
        if self.pending_bindings.is_some() {
            return;
        }
        let available = |id| {
            self.inventory_connected(id) || connections.iter().any(|connection| connection.id == id)
        };
        let command = Command::BindInventory {
            source: available(settings.source1_id).then_some(settings.source1_id),
            secondary: (settings.config.balanced_source.is_some()
                && available(settings.source2_id))
            .then_some(settings.source2_id),
            meter: available(settings.meter_id).then_some(settings.meter_id),
            reference: settings.reference_meter_id.filter(|&id| available(id)),
            mode: settings.config.mode,
            connections,
        };
        let id = format!("bindings-{}", uuid::Uuid::new_v4());
        if self.send(crate::protocol::gui_job(id.clone(), command).expect("valid binding job")) {
            self.pending_bindings = Some((id, settings));
        }
    }

    pub(super) fn connect_instrument(&mut self, source: bool) {
        let id = if source {
            self.settings.source1_id
        } else {
            self.settings.meter_id
        };
        if self.inventory_connected(id) {
            self.request_inventory_selection(source, id);
            return;
        }
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
        self.send(Command::ConnectInventory {
            id,
            source,
            assign: true,
            simulation,
            model: model.clone(),
            resource,
            mode: self.settings.config.mode,
        });
        self.send_meter_ranges();
    }

    pub(super) fn connect_reference_meter(&mut self) {
        let Some(id) = self.settings.reference_meter_id else {
            return;
        };
        let settings = self.settings.clone();
        let mut connections = Vec::new();
        if !self.inventory_connected(id) {
            let Some(connection) = settings
                .meters
                .iter()
                .find(|entry| entry.id == id)
                .and_then(|entry| inventory_connection(false, entry, settings.config.mode))
            else {
                return;
            };
            connections.push(connection);
        }
        self.request_bindings(settings, connections);
    }

    pub(super) fn send_meter_ranges(&mut self) {
        self.send(Command::SetMeterRanges {
            meter_voltage_range: self.settings.config.meter_voltage_range,
            meter_current_range: self.settings.config.meter_current_range,
        });
    }

    pub(super) fn connect_secondary(&mut self) {
        self.settings.store_active_entries();
        let id = self.settings.source2_id;
        let settings = self.settings.clone();
        let mut connections = Vec::new();
        if !self.inventory_connected(id) {
            let Some(connection) = settings
                .sources
                .iter()
                .find(|entry| entry.id == id)
                .and_then(|entry| inventory_connection(true, entry, settings.config.mode))
            else {
                return;
            };
            connections.push(connection);
        }
        self.request_bindings(settings, connections);
    }

    pub(super) fn device_card_kind(&self, source: bool, id: u64) -> &'static str {
        let mode = self.settings.config.mode;
        let unassigned_mode = self
            .state
            .inventory_devices
            .iter()
            .find(|entry| entry.id == id && !entry.assigned)
            .map(|entry| entry.mode);
        if source {
            let source_mode = if let Some(mode) = unassigned_mode {
                mode
            } else if id == self.settings.source1_id {
                self.card_control_mode()
            } else if self.settings.config.balanced_source.is_some()
                && id == self.settings.source2_id
            {
                Mode::Iv
            } else {
                self.settings
                    .sources
                    .iter()
                    .find(|entry| entry.id == id)
                    .map_or(mode, |entry| entry.dc.mode)
            };
            match source_mode {
                Mode::Iv => "Source(V)",
                Mode::Vi => "Source(I)",
            }
        } else {
            let observes_source = self.settings.reference_meter_id == Some(id);
            let mode = if id == self.settings.meter_id {
                self.meter_live_mode()
            } else {
                mode
            };
            if unassigned_mode.map_or((mode == Mode::Iv) == observes_source, |mode| {
                mode == Mode::Vi
            }) {
                "Meter(V)"
            } else {
                "Meter(I)"
            }
        }
    }

    /// Inventory management stays beside the device cards; Settings only
    /// assigns their measurement roles and the dual-source ratio.
    pub(super) fn device_add_footer(&mut self, ui: &mut egui::Ui) -> egui::Rect {
        let response = ui.add_enabled(
            self.inventory_editable(),
            egui::Button::new("+ Add device")
                .fill(BUTTON)
                .corner_radius(6)
                .min_size(Vec2::new(ui.available_width(), 24.0)),
        );
        if response.clicked() {
            self.add_device = Some(AddDevice {
                role: InstrumentRole::Source,
                label: String::new(),
            });
        }
        response.rect
    }

    pub(super) fn inventory_editable(&self) -> bool {
        !self.busy() && !self.pending_run && !self.python_locked()
    }

    pub(super) fn request_inventory_selection(&mut self, source: bool, id: u64) {
        if self.pending_inventory_selection.is_some() {
            return;
        }
        let job = format!("inventory-{}", uuid::Uuid::new_v4());
        let command =
            crate::protocol::gui_job(job.clone(), Command::SelectInventory { id, source })
                .expect("selection is a finite inventory command");
        if self.send(command) {
            self.pending_inventory_selection = Some((job, source, id));
        }
    }

    pub(super) fn select_inventory_entry(&mut self, source: bool, id: u64) {
        self.settings.store_active_entries();
        if source {
            self.settings.source1_id = id;
        } else {
            self.settings.meter_id = id;
        }
        self.settings.bind_active_entries();
    }

    pub(super) fn report_device_bindings(&mut self, previous: &Settings, action: &str) {
        let settings = &self.settings;
        let voltage = settings.config.mode == Mode::Iv;
        let mut changes = Vec::new();
        let was_dual = previous.config.balanced_source.is_some();
        let dual = settings.config.balanced_source.is_some();
        if settings.config.mode != previous.config.mode || was_dual != dual {
            changes.push(format!(
                "Mode: {}",
                if dual {
                    "Dual V-I"
                } else {
                    mode_name(settings.config.mode)
                }
            ));
        }
        for (label, old, new, source) in [
            (
                "Source 1",
                Some(previous.source1_id),
                Some(settings.source1_id),
                true,
            ),
            (
                "Source 2",
                (previous.source2_id != settings::NO_DEVICE).then_some(previous.source2_id),
                (settings.source2_id != settings::NO_DEVICE).then_some(settings.source2_id),
                true,
            ),
            (
                if voltage { "Meter (I)" } else { "Meter (V)" },
                Some(previous.meter_id),
                Some(settings.meter_id),
                false,
            ),
            (
                if voltage { "Meter (V)" } else { "Meter (I)" },
                previous.reference_meter_id,
                settings.reference_meter_id,
                false,
            ),
        ] {
            if old != new {
                let device =
                    new.map_or_else(|| "Unassigned".into(), |id| self.inventory_name(source, id));
                changes.push(format!("{label}: {device}"));
            }
        }
        self.device_change_notice = if changes.is_empty() {
            "Device assignments confirmed.".into()
        } else {
            format!("{action} · {}", changes.join(" · "))
        };
        self.report(self.device_change_notice.clone());
    }

    pub(super) fn device_bindings(&mut self, ui: &mut egui::Ui) {
        self.settings.store_active_entries();
        section(ui, "Devices", |ui| {
            ui.label(
                RichText::new(
                    "Choose the saved instruments used by this IV run. Other cards remain stored choices.",
                )
                .small()
                .color(MUTED),
            );
            if self.pending_bindings.is_some() || self.pending_inventory_selection.is_some() {
                ui.label(
                    RichText::new("Applying device selection…")
                        .small()
                        .color(MUTED),
                );
            } else if !self.device_change_notice.is_empty() {
                ui.label(RichText::new(&self.device_change_notice).small());
            }
            ui.add_space(6.0);
            let locked = self.busy()
                || self.state.source_connected
                || self.state.source2_connected
                || self.state.meter_connected
                || self.state.reference_meter_connected
                || self.state.gate_connected;
            let source_choices = self
                .settings
                .sources
                .iter()
                .enumerate()
                .map(|(index, entry)| (entry.id, inventory_entry_name("Source", index, entry)))
                .collect::<Vec<_>>();
            let meter_choices = self
                .settings
                .meters
                .iter()
                .enumerate()
                .map(|(index, entry)| (entry.id, inventory_entry_name("Meter", index, entry)))
                .collect::<Vec<_>>();
            if !self.state.inventory_devices.is_empty() {
                let mut candidate = self.settings.clone();
                let current = [
                    Some(candidate.source1_id),
                    (candidate.source2_id != settings::NO_DEVICE).then_some(candidate.source2_id),
                    Some(candidate.meter_id),
                    candidate.reference_meter_id,
                ];
                let mut selected = current;
                let voltage_source = candidate.config.mode == Mode::Iv;
                ui.add_enabled_ui(self.inventory_editable(), |ui| {
                    egui::Grid::new("connected_device_bindings")
                        .num_columns(2)
                        .show(ui, |ui| {
                            for (index, label) in [
                                "Source 1",
                                "Source 2",
                                if voltage_source {
                                    "Meter (I)"
                                } else {
                                    "Meter (V)"
                                },
                                if voltage_source {
                                    "Meter (V)"
                                } else {
                                    "Meter (I)"
                                },
                            ]
                            .into_iter()
                            .enumerate()
                            {
                                if index == 1 && candidate.config.balanced_source.is_none() {
                                    continue;
                                }
                                let source = index < 2;
                                form_label(ui, label, 65.0);
                                egui::ComboBox::from_id_salt(("connected_binding", index))
                                    .truncate()
                                    .width(ui.available_width())
                                    .selected_text(
                                        current[index]
                                            .map(|id| self.inventory_name(source, id))
                                            .unwrap_or_else(|| {
                                                if index == 1 {
                                                    "Unassigned"
                                                } else {
                                                    "Source value only"
                                                }
                                                .into()
                                            }),
                                    )
                                    .show_ui(ui, |ui| {
                                        if index == 3 {
                                            ui.selectable_value(
                                                &mut selected[index],
                                                None,
                                                "Source value only",
                                            );
                                        }
                                        for (id, name) in if source {
                                            &source_choices
                                        } else {
                                            &meter_choices
                                        } {
                                            ui.add_enabled_ui(
                                                self.inventory_connected(*id)
                                                    && !self.dc_gate_matches(*id),
                                                |ui| {
                                                    ui.selectable_value(
                                                        &mut selected[index],
                                                        Some(*id),
                                                        name,
                                                    );
                                                },
                                            );
                                        }
                                    });
                                ui.end_row();
                            }
                        });
                });
                if let Some(index) =
                    (0..selected.len()).find(|&index| selected[index] != current[index])
                {
                    if let Some(id) = selected[index]
                        && let Some(other) = (0..current.len())
                            .find(|&other| other != index && current[other] == Some(id))
                    {
                        selected[other] = current[index];
                    }
                    candidate.source1_id = selected[0].unwrap_or(candidate.source1_id);
                    candidate.source2_id = selected[1].unwrap_or(candidate.source2_id);
                    candidate.meter_id = selected[2].unwrap_or(candidate.meter_id);
                    candidate.reference_meter_id = selected[3];
                    if selected[0].is_some()
                        && selected[2].is_some()
                        && (candidate.config.balanced_source.is_none() || selected[1].is_some())
                    {
                        candidate.bind_active_entries();
                        self.request_bindings(candidate, Vec::new());
                    }
                }
                return;
            }
            let previous = self.settings.clone();
            let mut changed = false;
            let response = ui.add_enabled_ui(!locked, |ui| {
                egui::Grid::new("device_role_bindings")
                    .num_columns(2)
                    .spacing([6.0, 6.0])
                    .show(ui, |ui| {
                        form_label(ui, "Source 1", 65.0);
                        let before = self.settings.source1_id;
                        egui::ComboBox::from_id_salt("source1_binding")
                            .truncate()
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
                                .truncate()
                                .width(ui.available_width())
                                .selected_text(if before == settings::NO_DEVICE {
                                    "Unassigned · add a second Source".to_owned()
                                } else {
                                    self.inventory_name(true, before)
                                })
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

                        let source_quantity = if self.settings.config.mode == Mode::Iv {
                            "Meter (V)"
                        } else {
                            "Meter (I)"
                        };
                        let measured_quantity = if self.settings.config.mode == Mode::Iv {
                            "Meter (I)"
                        } else {
                            "Meter (V)"
                        };
                        form_label(ui, measured_quantity, 65.0);
                        let before = self.settings.meter_id;
                        egui::ComboBox::from_id_salt("meter_binding")
                            .truncate()
                            .width(ui.available_width())
                            .selected_text(self.inventory_name(false, before))
                            .show_ui(ui, |ui| {
                                for (id, label) in &meter_choices {
                                    ui.add_enabled_ui(
                                        self.settings.reference_meter_id != Some(*id),
                                        |ui| {
                                            ui.selectable_value(
                                                &mut self.settings.meter_id,
                                                *id,
                                                label,
                                            );
                                        },
                                    );
                                }
                            });
                        changed |= before != self.settings.meter_id;
                        ui.end_row();

                        form_label(ui, source_quantity, 65.0);
                        let before = self.settings.reference_meter_id;
                        egui::ComboBox::from_id_salt("reference_meter_binding")
                            .truncate()
                            .width(ui.available_width())
                            .selected_text(
                                before
                                    .map(|id| self.inventory_name(false, id))
                                    .unwrap_or_else(|| "Source value only".into()),
                            )
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut self.settings.reference_meter_id,
                                    None,
                                    "Source value only",
                                );
                                for (id, label) in &meter_choices {
                                    ui.add_enabled_ui(*id != self.settings.meter_id, |ui| {
                                        ui.selectable_value(
                                            &mut self.settings.reference_meter_id,
                                            Some(*id),
                                            label,
                                        );
                                    });
                                }
                            });
                        changed |= before != self.settings.reference_meter_id;
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
                        .map_or(settings::NO_DEVICE, |entry| entry.id);
                }
                if self.settings.reference_meter_id == Some(self.settings.meter_id) {
                    self.settings.reference_meter_id = None;
                }
                if let Some(reference_id) = self.settings.reference_meter_id {
                    let primary_resource = self
                        .settings
                        .meters
                        .iter()
                        .find(|entry| entry.id == self.settings.meter_id)
                        .map(|entry| entry.resource.as_str());
                    let reference_resource = self
                        .settings
                        .meters
                        .iter()
                        .find(|entry| entry.id == reference_id)
                        .map(|entry| entry.resource.as_str());
                    if primary_resource.zip(reference_resource).is_some_and(
                        |(primary, reference)| !primary.is_empty() && primary == reference,
                    ) {
                        self.settings.reference_meter_id = None;
                        self.report("The same physical resource cannot be assigned to both Meter (V) and Meter (I).".into());
                    }
                }
                self.settings.bind_active_entries();
                self.report_device_bindings(&previous, "Selected");
            }
        });
    }

    pub(super) fn inventory_name(&self, source: bool, id: u64) -> String {
        let entries = if source {
            &self.settings.sources
        } else {
            &self.settings.meters
        };
        let role = if source { "Source" } else { "Meter" };
        entries
            .iter()
            .enumerate()
            .find(|(_, entry)| entry.id == id)
            .map(|(index, entry)| inventory_entry_name(role, index, entry))
            .unwrap_or_else(|| format!("{role} unavailable"))
    }
}
