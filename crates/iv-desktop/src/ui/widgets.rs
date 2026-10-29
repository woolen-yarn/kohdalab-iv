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

pub(super) const DC_BUTTON_WIDTH: f32 = 92.0;

pub(super) fn device_width(ui: &egui::Ui) -> f32 {
    (ui.available_width() - DC_BUTTON_WIDTH - ui.spacing().item_spacing.x).max(80.0)
}

pub(super) fn device_disclosure(
    ui: &mut egui::Ui,
    summary: &str,
    width: f32,
    open: &mut bool,
) -> egui::Response {
    let label = format!("   Device · {summary}");
    let response = ui.add_sized(
        [width, 24.0],
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
    response.on_hover_text(summary)
}

pub(super) fn meter_range_help(model: &str, mode: iv_core::Mode, range: Option<f64>) -> String {
    let function = if mode == iv_core::Mode::Iv {
        "current"
    } else {
        "voltage"
    };
    let mut help = format!(
        "{function} measurement range. Fixed holds one range for the whole sweep; Limit is a separate stop threshold."
    );
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

// Keep paths and resource identities readable at rest; preserve full manual editing.
pub(super) fn compact_field(
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

pub(super) fn state_badge(ui: &mut egui::Ui, text: &str, color: Color32) {
    egui::Frame::new()
        .fill(color.gamma_multiply(0.12))
        .corner_radius(4)
        .inner_margin(egui::Margin::symmetric(4, 1))
        .show(ui, |ui| {
            ui.label(RichText::new(text).size(9.0).color(color).strong());
        });
}

pub(super) fn style(ctx: &egui::Context) {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_headers_share_width_and_config_browse_reaches_the_card_edge() {
        let context = egui::Context::default();
        let mut rects = Vec::new();
        let mut ends = None;
        let _ = context.run(Default::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.set_max_width(244.0);
                let width = device_width(ui);
                let mut open = false;
                ui.horizontal(|ui| {
                    rects.push(device_disclosure(ui, "Yokogawa GS210", width, &mut open).rect);
                    button(ui, "DC output", DC_BUTTON_WIDTH, true);
                });
                ui.horizontal(|ui| {
                    rects.push(
                        device_disclosure(ui, "Long meter model identifier", width, &mut open).rect,
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
    fn expanded_dc_editor_reserves_space_for_actions_and_following_meter() {
        let context = egui::Context::default();
        for width in [230.0, 280.0] {
            let mut editor_clip = egui::Rect::NOTHING;
            let mut actions = egui::Rect::NOTHING;
            let mut source_monitor = egui::Rect::NOTHING;
            let mut meter_monitor = egui::Rect::NOTHING;
            let _ = context.run(Default::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
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
}
