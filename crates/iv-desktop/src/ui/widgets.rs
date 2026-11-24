//! Shared visual style and input widgets for the existing UI layout.

use eframe::egui::{self, Color32, RichText, Stroke, Vec2};

pub(super) const BG: Color32 = Color32::from_rgb(18, 19, 23);
pub(super) const PANEL: Color32 = Color32::from_rgb(26, 27, 32);
pub(super) const INPUT: Color32 = Color32::from_rgb(20, 21, 25);
pub(super) const BUTTON: Color32 = Color32::from_rgb(45, 46, 53);
pub(super) const BORDER: Color32 = Color32::from_rgb(56, 58, 67);
pub(super) const TEXT: Color32 = Color32::from_rgb(235, 236, 241);
pub(super) const BLUE: Color32 = Color32::from_rgb(102, 174, 255);
pub(super) const ORANGE: Color32 = Color32::from_rgb(245, 183, 82);
pub(super) const PURPLE: Color32 = Color32::from_rgb(179, 155, 248);
pub(super) const TEAL: Color32 = Color32::from_rgb(63, 215, 194);
pub(super) const MUTED: Color32 = Color32::from_rgb(160, 163, 176);
pub(super) const RED: Color32 = Color32::from_rgb(255, 123, 130);
/// TRKR device-inventory header and status colours.
pub(super) const GRIP: Color32 = Color32::from_rgb(108, 113, 123);
pub(super) const CONNECTED: Color32 = Color32::from_rgb(93, 196, 146);
pub(super) const DROP_LINE: Color32 = Color32::from_rgb(79, 143, 238);
pub(super) const DELETE_WIDTH: f32 = 53.0;

pub(super) fn meter_range_choice(
    ui: &mut egui::Ui,
    id: &str,
    model: &str,
    mode: iv_core::Mode,
    value: &mut Option<f64>,
) -> bool {
    let ranges = iv_core::meter_ranges(model, mode).unwrap_or_default();
    let unit = if mode == iv_core::Mode::Iv { "A" } else { "V" };
    let before = *value;
    let selected = value.map_or_else(
        || "Auto".into(),
        |v| format!("Fixed · {}", super::formatting::fmt_quantity(v, unit)),
    );
    let help = meter_range_help(model, mode, *value);
    egui::ComboBox::from_id_salt(id)
        .width(ui.available_width())
        .truncate()
        .selected_text(selected)
        .show_ui(ui, |ui| {
            ui.selectable_value(value, None, "Auto");
            for range in ranges.iter().copied() {
                let mut label = format!("Fixed · {}", super::formatting::fmt_quantity(range, unit));
                if let Some(r) = iv_core::meter_input_resistance(model, mode, range) {
                    label.push_str(&format!(
                        " · {} {}",
                        super::formatting::fmt_quantity(r.ohms, "ohm"),
                        if r.kind == iv_core::MeterResistanceKind::Shunt {
                            "shunt"
                        } else {
                            "input"
                        }
                    ));
                }
                ui.selectable_value(value, Some(range), label);
            }
        })
        .response
        .on_hover_text(help);
    before != *value
}

pub(super) const DC_BUTTON_WIDTH: f32 = 80.0;
pub(super) const DEVICE_BUTTON_WIDTH: f32 = 66.0;

pub(super) fn device_width(ui: &egui::Ui) -> f32 {
    (ui.available_width() - DC_BUTTON_WIDTH - ui.spacing().item_spacing.x).max(80.0)
}

pub(super) const STATUS_DOT_X: f32 = 4.0;
pub(super) const STATUS_TEXT_X: f32 = 11.0;
pub(super) const STATUS_FONT_SIZE: f32 = 11.0;

pub(super) fn device_status_text(connected: bool) -> &'static str {
    if connected {
        "Connected"
    } else {
        "Disconnected"
    }
}

/// Width the compact connection state needs on the Device row.
#[cfg(test)]
pub(super) fn device_status_width(ctx: &egui::Context, connected: bool) -> f32 {
    let galley = ctx.fonts_mut(|fonts| {
        fonts.layout_no_wrap(
            device_status_text(connected).to_owned(),
            egui::FontId::proportional(STATUS_FONT_SIZE),
            TEXT,
        )
    });
    STATUS_TEXT_X + galley.size().x
}

/// Device row of every card: disclosure and compact connection state; the
/// model is shown on hover and in the expanded Model field.
pub(super) fn device_disclosure(
    ui: &mut egui::Ui,
    summary: &str,
    connected: bool,
    width: f32,
    open: &mut bool,
) -> egui::Response {
    let label = "   Device";
    let response = ui.add_sized(
        [DEVICE_BUTTON_WIDTH, 24.0],
        egui::Button::new(label).truncate().fill(BUTTON),
    );
    let center = egui::pos2(response.rect.left() + 11., response.rect.center().y);
    let offsets = if *open {
        [(-4., -2.), (4., -2.), (0., 3.)]
    } else {
        [(-2., -4.), (-2., 4.), (3., 0.)]
    };
    ui.painter().add(egui::Shape::convex_polygon(
        offsets
            .into_iter()
            .map(|(x, y)| center + egui::vec2(x, y))
            .collect(),
        MUTED,
        egui::Stroke::NONE,
    ));
    if response.clicked() {
        *open = !*open;
    }
    let status = device_status_text(connected);
    // Compact state: dot and word only.  The model stays in the hover text
    // and in the expanded Model field, so narrow cards never hide the state.
    let (rect, label) = ui.allocate_exact_size(
        Vec2::new(
            (width - DEVICE_BUTTON_WIDTH - ui.spacing().item_spacing.x).max(0.0),
            24.0,
        ),
        egui::Sense::hover(),
    );
    let color = if connected { CONNECTED } else { MUTED };
    let painter = ui.painter_at(rect);
    painter.circle_filled(
        egui::pos2(rect.left() + STATUS_DOT_X, rect.center().y),
        3.5,
        color,
    );
    painter.text(
        egui::pos2(rect.left() + STATUS_TEXT_X, rect.center().y),
        egui::Align2::LEFT_CENTER,
        status,
        egui::FontId::proportional(STATUS_FONT_SIZE),
        color,
    );
    label.on_hover_text(format!("{summary}\n{status}"));
    response.on_hover_text(summary)
}

pub(super) fn meter_range_help(model: &str, mode: iv_core::Mode, range: Option<f64>) -> String {
    let function = if mode == iv_core::Mode::Iv {
        "current"
    } else {
        "voltage"
    };
    let mut help = format!(
        "{function} measurement range. Fixed holds one range for the whole sweep; Limit is a separate stop threshold. Range is full-scale, not the smallest measurable signal. Readout units follow the current value; resolution and accuracy are separate from range."
    );
    if model == "ADCMT_7461A" && mode == iv_core::Mode::Vi {
        help.push_str(
            "\nGPIB only: Fixed 0.1 V or 1 V is verified by Auto OFF and range readback before output ON. \
             The active range is read back after every conversion. Auto stops if input loading changes \
             or cannot be confirmed unchanged. USB supports Auto only, without range monitoring.",
        );
        return help;
    }
    if let Some(range) = range {
        if let Some(r) = iv_core::meter_input_resistance(model, mode, range) {
            help.push_str(&format!(
                "\n{}: {}.",
                if r.kind == iv_core::MeterResistanceKind::Shunt {
                    "Shunt resistance"
                } else {
                    "Input resistance"
                },
                super::formatting::fmt_quantity(r.ohms, "ohm")
            ));
        } else if mode == iv_core::Mode::Vi {
            help.push_str("\nInput impedance depends on the instrument setting.");
        } else {
            help.push_str("\nInternal resistance has not been verified for this model.");
        }
    } else if model == "ADCMT_7461A" {
        help.push_str(
            "\nRange monitoring and fixed-range commands are not verified for this model.",
        );
    } else {
        help.push_str("\nAuto stops on a change of documented input resistance, or an unverified range change. A physical switch can occur before detection.");
    }
    help
}

/// Bound expanding DC controls without borrowing the live monitor's space.
/// Output ON/OFF actions deliberately stay outside this clipped viewport.
pub(super) fn dc_editor<R>(ui: &mut egui::Ui, id: u64, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    egui::ScrollArea::vertical()
        .id_salt(("dc_editor", id))
        .max_height(190.0)
        .auto_shrink([false, true])
        .show(ui, add)
        .inner
}

// Preserve the original density while placing headings inside quiet cards.
pub(super) fn section(ui: &mut egui::Ui, title: &str, add: impl FnOnce(&mut egui::Ui)) {
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

/// Delete availability for one card header.  A disabled Delete always keeps
/// its explanation as hover text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CardDelete<'a> {
    Allowed,
    Blocked(&'a str),
}

/// Every Source and Meter card frame: TRKR drag grip, `Kind:` inline label
/// and Delete in the header, then the shared body.  Returns whether Delete
/// was clicked.
pub(super) fn device_card(
    ui: &mut egui::Ui,
    id: u64,
    reorder: bool,
    delete: CardDelete<'_>,
    header: impl FnOnce(&mut egui::Ui),
    body: impl FnOnce(&mut egui::Ui),
) -> bool {
    let mut delete_clicked = false;
    egui::Frame::new()
        .fill(PANEL)
        .corner_radius(10)
        .stroke(Stroke::new(1.0_f32, BORDER))
        .inner_margin(egui::Margin::same(10))
        .show(ui, |ui| {
            ui.set_min_size(Vec2::new(ui.available_width(), 198.0));
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                let (grip_rect, grip) = ui.allocate_exact_size(
                    Vec2::new(17.0, 22.0),
                    if reorder {
                        egui::Sense::drag()
                    } else {
                        egui::Sense::hover()
                    },
                );
                let dot = if grip.hovered() { TEXT } else { GRIP };
                for dx in [5.0, 10.0] {
                    for dy in [6.0, 11.0, 16.0] {
                        ui.painter()
                            .circle_filled(grip_rect.min + Vec2::new(dx, dy), 1.0, dot);
                    }
                }
                if reorder {
                    grip.dnd_set_drag_payload(id);
                    grip.on_hover_cursor(egui::CursorIcon::Grab)
                        .on_hover_text("Drag to rearrange · order is saved with Config");
                }
                let title_width =
                    (ui.available_width() - DELETE_WIDTH - ui.spacing().item_spacing.x).max(10.0);
                ui.allocate_ui_with_layout(
                    Vec2::new(title_width, 23.0),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| {
                        ui.set_width(title_width);
                        header(ui);
                    },
                );
                let allowed = delete == CardDelete::Allowed;
                let response = ui.add_enabled(
                    allowed,
                    egui::Button::new("Delete")
                        .small()
                        .min_size(Vec2::new(DELETE_WIDTH, 23.0)),
                );
                delete_clicked = match delete {
                    CardDelete::Allowed => response
                        .on_hover_text("Delete this device from the configuration")
                        .clicked(),
                    CardDelete::Blocked(reason) => {
                        response.on_disabled_hover_text(reason);
                        false
                    }
                };
            });
            body(ui);
        });
    delete_clicked
}

pub(super) fn device_name(
    ui: &mut egui::Ui,
    kind: &str,
    id: u64,
    fallback: &str,
    label: &mut String,
    editable: bool,
) -> egui::Response {
    ui.label(RichText::new(format!("{kind}:")).color(Color32::GRAY))
        .on_hover_text(fallback);
    let hint = fallback.rsplit_once('·').map_or(fallback, |(_, hint)| hint);
    let hint = hint
        .strip_prefix("Source ")
        .or_else(|| hint.strip_prefix("Meter "))
        .unwrap_or(hint)
        .trim();
    let hint = match hint {
        "(I)" | "(V)" => "Label",
        _ => hint,
    };
    let field_id = egui::Id::new(("device_card_label", id));
    if !editable {
        ui.memory_mut(|memory| memory.surrender_focus(field_id));
    }
    ui.add_enabled(
        editable,
        egui::TextEdit::singleline(label)
            .id(field_id)
            .interactive(editable)
            .hint_text(hint)
            .char_limit(64)
            .frame(egui::Frame::NONE)
            .margin(Vec2::new(3.0, 3.0))
            .desired_width(ui.available_width()),
    )
    .on_hover_text(format!("Rename {kind}"))
    .on_disabled_hover_text("Rename when measurement and external control are idle.")
}

pub(super) fn button(ui: &mut egui::Ui, text: &str, width: f32, enabled: bool) -> egui::Response {
    ui.add_enabled(
        enabled,
        egui::Button::new(text)
            .fill(BUTTON)
            .min_size(Vec2::new(width, 24.0)),
    )
}

pub(super) fn form_label(ui: &mut egui::Ui, text: &str, width: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 24.0), egui::Sense::hover());
    ui.painter().text(
        rect.right_center(),
        egui::Align2::RIGHT_CENTER,
        text,
        egui::FontId::proportional(12.0),
        MUTED,
    );
}

pub(super) fn device_field_label(ui: &mut egui::Ui, text: &str) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(85.0, 24.0), egui::Sense::hover());
    ui.painter().text(
        rect.left_center() + Vec2::new(9.0, 0.0),
        egui::Align2::LEFT_CENTER,
        text,
        egui::FontId::proportional(12.0),
        TEXT,
    );
}

// Keep paths and resource identities readable at rest; preserve full manual editing.
pub(super) fn compact_field(
    ui: &mut egui::Ui,
    salt: impl egui::AsIdSalt,
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

pub(super) fn path_summary(value: &str) -> String {
    let trimmed = value.trim_end_matches(['/', '\\']);
    match trimmed.rsplit_once(['/', '\\']) {
        Some((_, tail)) if !tail.is_empty() => format!("…/{tail}"),
        _ => value.to_owned(),
    }
}

pub(super) fn resource_summary(value: &str) -> String {
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

pub(super) fn style(ctx: &egui::Context) {
    ctx.set_theme(egui::ThemePreference::Dark);
    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = BG;
    visuals.window_fill = PANEL;
    visuals.window_corner_radius = egui::CornerRadius::same(12);
    visuals.window_stroke = Stroke::new(1.0_f32, BORDER);
    visuals.extreme_bg_color = BG;
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
    ctx.global_style_mut(|style| {
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

pub(super) fn number(
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

pub(super) fn quantity(
    ui: &mut egui::Ui,
    id: impl egui::AsIdSalt,
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
                [(width - 54.0 - ui.spacing().item_spacing.x).max(56.0), 24.0],
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connected_card_names_are_editable_inline_without_renaming_the_other_card() {
        let context = egui::Context::default();
        let mut source_label = String::new();
        let mut meter_label = String::from("Voltage");
        let mut frame = |events, editable| {
            let mut source_name = egui::Rect::NOTHING;
            let _ = context.run_ui(
                egui::RawInput {
                    events,
                    ..Default::default()
                },
                |ui| {
                    egui::CentralPanel::default().show(ui, |ui| {
                        ui.set_width(230.0);
                        device_card(
                            ui,
                            1,
                            editable,
                            CardDelete::Allowed,
                            |ui| {
                                source_name = device_name(
                                    ui,
                                    "Source(V)",
                                    1,
                                    "Source 1",
                                    &mut source_label,
                                    editable,
                                )
                                .rect;
                            },
                            |_| {},
                        );
                        device_card(
                            ui,
                            2,
                            editable,
                            CardDelete::Allowed,
                            |ui| {
                                device_name(
                                    ui,
                                    "Meter(V)",
                                    2,
                                    "Meter 1",
                                    &mut meter_label,
                                    editable,
                                );
                            },
                            |_| {},
                        );
                    });
                },
            );
            (source_name, source_label.clone(), meter_label.clone())
        };
        let (name, _, _) = frame(vec![], true);
        let pos = name.center();
        let click = |pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        frame(vec![egui::Event::PointerMoved(pos), click(true)], true);
        frame(vec![click(false)], true);
        let (_, source, meter) = frame(vec![egui::Event::Text("Bias 1".into())], true);
        assert_eq!(source, "Bias 1");
        assert_eq!(meter, "Voltage");
        let (_, source, _) = frame(vec![egui::Event::Text(" ignored".into())], false);
        assert_eq!(source, "Bias 1");
    }

    #[test]
    fn device_headers_share_width_and_config_browse_reaches_the_card_edge() {
        let context = egui::Context::default();
        let mut rects = Vec::new();
        let mut ends = None;
        let _ = context.run_ui(Default::default(), |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                ui.set_max_width(244.0);
                let width = device_width(ui);
                let mut open = false;
                ui.horizontal(|ui| {
                    rects.push(
                        device_disclosure(ui, "Yokogawa GS210", false, width, &mut open).rect,
                    );
                    button(ui, "DC output", DC_BUTTON_WIDTH, true);
                });
                ui.horizontal(|ui| {
                    rects.push(
                        device_disclosure(
                            ui,
                            "Long meter model identifier",
                            true,
                            width,
                            &mut open,
                        )
                        .rect,
                    );
                });
                ui.horizontal(|ui| {
                    let right = ui.max_rect().right();
                    let w = ui.available_width() - 70.0 - ui.spacing().item_spacing.x;
                    let mut value = "path".to_owned();
                    compact_field(ui, "test_config_path", &mut value, w, "path".into());
                    let browse = button(ui, "Browse", 70.0, true);
                    ends = Some((right, browse.rect.right()));
                });
            });
        });
        assert!((rects[0].width() - rects[1].width()).abs() < 0.1);
        let (edge, browse) = ends.unwrap();
        assert!((edge - browse).abs() < 0.1, "edge={edge} browse={browse}");
    }

    #[test]
    fn narrow_source_device_rows_keep_the_connection_state_visible_beside_dc_output() {
        let context = egui::Context::default();
        style(&context);
        // Inner widths of the fixed two-column and one-column sidebars.
        for width in [243.0_f32, 310.0] {
            let mut checks = Vec::new();
            let _ = context.run_ui(Default::default(), |ui| {
                egui::CentralPanel::default().show(ui, |ui| {
                    let column =
                        egui::Rect::from_min_size(ui.max_rect().min, Vec2::new(width, 100.0));
                    ui.scope_builder(egui::UiBuilder::new().max_rect(column), |ui| {
                        for connected in [false, true] {
                            ui.horizontal(|ui| {
                                let row = device_width(ui);
                                let mut open = false;
                                let device = device_disclosure(
                                    ui,
                                    "Long simulated source model",
                                    connected,
                                    row,
                                    &mut open,
                                );
                                let dc = button(ui, "DC output", DC_BUTTON_WIDTH, true);
                                let status = dc.rect.left()
                                    - ui.spacing().item_spacing.x
                                    - device.rect.right()
                                    - ui.spacing().item_spacing.x;
                                checks.push((
                                    connected,
                                    status,
                                    device_status_width(ui.ctx(), connected),
                                    dc.rect.right(),
                                    column.right(),
                                ));
                            });
                        }
                    });
                });
            });
            for (connected, available, needed, dc_right, edge) in checks {
                assert!(
                    available + 0.5 >= needed,
                    "{width}: {} needs {needed}, has {available}",
                    device_status_text(connected)
                );
                assert!(dc_right <= edge + 0.5, "{width}: DC output overflows");
            }
        }
    }

    #[test]
    fn card_header_delete_is_clickable_only_when_allowed_and_stays_inside_the_card() {
        let context = egui::Context::default();
        for width in [230.0_f32, 330.0] {
            for delete in [
                CardDelete::Allowed,
                CardDelete::Blocked("Disconnect first."),
            ] {
                let frame = |events: Vec<egui::Event>| {
                    let mut clicked = false;
                    let mut card = egui::Rect::NOTHING;
                    let _ = context.run_ui(
                        egui::RawInput {
                            events,
                            ..Default::default()
                        },
                        |ui| {
                            egui::CentralPanel::default().show(ui, |ui| {
                                let column = egui::Rect::from_min_size(
                                    ui.max_rect().min,
                                    Vec2::new(width, 400.0),
                                );
                                card = ui
                                    .scope_builder(egui::UiBuilder::new().max_rect(column), |ui| {
                                        clicked = device_card(
                                            ui,
                                            7,
                                            true,
                                            delete,
                                            |ui| {
                                                ui.label("Source(V):");
                                            },
                                            |_| {},
                                        );
                                    })
                                    .response
                                    .rect;
                            });
                        },
                    );
                    (clicked, card)
                };
                let (_, card) = frame(vec![]);
                // TRKR places Delete at the right edge inside the 10 px margin.
                let pos = egui::pos2(
                    card.right() - 10.0 - DELETE_WIDTH / 2.0,
                    card.top() + 10.0 + 11.0,
                );
                let press = |pressed| egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                };
                frame(vec![egui::Event::PointerMoved(pos), press(true)]);
                let (clicked, card) = frame(vec![press(false)]);
                assert_eq!(clicked, delete == CardDelete::Allowed, "{width} {delete:?}");
                assert!(card.width() <= width + 0.5, "card grew to {}", card.width());
            }
        }
    }

    #[test]
    fn expanded_dc_editor_reserves_space_for_actions_and_following_meter() {
        let context = egui::Context::default();
        for width in [230.0, 280.0] {
            let mut editor_clip = egui::Rect::NOTHING;
            let mut actions = egui::Rect::NOTHING;
            let mut source_monitor = egui::Rect::NOTHING;
            let mut meter_monitor = egui::Rect::NOTHING;
            let _ = context.run_ui(Default::default(), |ui| {
                egui::CentralPanel::default().show(ui, |ui| {
                    ui.set_width(width);
                    section(ui, "Source 1", |ui| {
                        dc_editor(ui, 1, |ui| {
                            editor_clip = ui.clip_rect();
                            for _ in 0..40 {
                                ui.label("Expanded device and typed DC settings");
                            }
                        });
                        actions = button(ui, "OFF", 80.0, true).rect;
                        source_monitor = ui
                            .allocate_exact_size(egui::vec2(width, 110.0), egui::Sense::hover())
                            .0;
                    });
                    section(ui, "Meter 1", |ui| {
                        meter_monitor = ui
                            .allocate_exact_size(egui::vec2(width, 110.0), egui::Sense::hover())
                            .0;
                    });
                });
            });
            // Egui includes a small clip margin for strokes, not extra content.
            assert!(editor_clip.height() <= 200.0, "{editor_clip:?}");
            assert!(actions.top() >= editor_clip.bottom() - 4.0);
            assert!(source_monitor.top() >= actions.bottom());
            assert!(meter_monitor.top() > source_monitor.bottom());
        }
    }

    #[test]
    fn timing_inputs_clamp_negative_values_but_accept_zero() {
        let context = egui::Context::default();
        let mut value = -0.25;
        let _ = context.run_ui(Default::default(), |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
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
        let _ = context.run_ui(Default::default(), |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
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
        let _ = context.run_ui(Default::default(), |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
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
}
