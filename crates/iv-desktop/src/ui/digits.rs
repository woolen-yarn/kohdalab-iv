//! A display-unit-only selector and per-decimal-place fixed-output controls.
use super::*;
use crate::protocol::DcLimits;

const SCALES: [f64; 5] = [1.0, 1e-3, 1e-6, 1e-9, 1e-12];

pub(super) struct DigitControl<'a> {
    pub value: f64,
    pub unit: &'a mut usize,
    pub base: &'a str,
    pub limits: DcLimits,
    pub enabled: bool,
    pub fixed_range: bool,
}

#[derive(Debug)]
struct Digits {
    text: String,
    scale: f64,
    exponent: i32,
    integer: usize,
}
impl Digits {
    fn new(value: f64, unit: usize, resolution: f64) -> Self {
        let scale = SCALES[unit.min(4)];
        let shown = (value / scale).abs();
        let integer = if shown >= 1.0 {
            shown.log10().floor() as usize + 1
        } else {
            1
        };
        let decimals = (-(resolution / scale).log10() - 1e-9).ceil().max(0.0) as usize;
        let exponent = if integer + decimals > 8 || (shown == 0.0 && resolution / scale > 1.0) {
            if shown > 0.0 {
                (shown.log10() / 3.0).floor() as i32 * 3
            } else {
                ((resolution / scale).log10() / 3.0).ceil() as i32 * 3
            }
        } else {
            0
        };
        let scale = scale * 10_f64.powi(exponent);
        let shown = (value / scale).abs();
        let integer = if shown >= 1.0 {
            shown.log10().floor() as usize + 1
        } else {
            1
        };
        let decimals = (-(resolution / scale).log10() - 1e-9).ceil().max(0.0) as usize;
        let decimals = decimals.min(8_usize.saturating_sub(integer));
        let text = format!("{shown:.decimals$}");
        let integer = text.find('.').unwrap_or(text.len());
        Self {
            text,
            scale,
            exponent,
            integer,
        }
    }
    fn step(&self, digit: usize) -> f64 {
        self.scale * 10_f64.powi(self.integer as i32 - digit as i32 - 1)
    }
}

#[derive(Clone)]
struct TypedDraft {
    value: f64,
    unit: usize,
    base: String,
    text: String,
}
impl TypedDraft {
    fn new(value: f64, unit: usize, base: &str) -> Self {
        Self {
            value,
            unit,
            base: base.to_owned(),
            text: format!("{}", value / SCALES[unit]),
        }
    }
    fn synchronize(&mut self, value: f64, unit: usize, base: &str) {
        if self.value != value || self.unit != unit || self.base != base {
            *self = Self::new(value, unit, base);
        }
    }
}

fn arrow(ui: &mut egui::Ui, up: bool, enabled: bool, width: f32) -> egui::Response {
    let enabled = enabled && ui.is_enabled();
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(width, 16.0),
        if enabled {
            egui::Sense::click()
        } else {
            egui::Sense::hover()
        },
    );
    let fill = if enabled && response.is_pointer_button_down_on() {
        BORDER
    } else {
        BUTTON
    };
    ui.painter().rect_filled(rect, 4.0, fill);
    response.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::WidgetType::Button,
            enabled,
            if up {
                "Increase digit"
            } else {
                "Decrease digit"
            },
        )
    });
    let center = response.rect.center();
    let points = if up {
        [(-3.5, 2.0), (3.5, 2.0), (0.0, -2.5)]
    } else {
        [(-3.5, -2.0), (3.5, -2.0), (0.0, 2.5)]
    };
    ui.painter().add(egui::Shape::convex_polygon(
        points
            .into_iter()
            .map(|(x, y)| center + egui::vec2(x, y))
            .collect(),
        if enabled {
            MUTED
        } else {
            MUTED.gamma_multiply(0.35)
        },
        egui::Stroke::NONE,
    ));
    response
}

/// Returns an SI value only for an explicit digit or typed-value commit.
/// Unit selection cannot produce a value-change request.
pub(super) fn output_digits(ui: &mut egui::Ui, id: u64, control: DigitControl<'_>) -> Option<f64> {
    let DigitControl {
        value,
        unit,
        base,
        limits,
        enabled,
        fixed_range,
    } = control;
    *unit = (*unit).min(4);
    let layout = Digits::new(value, *unit, limits.resolution);
    let tone = if base == "A" { TEAL } else { ORANGE };
    let edit_id = ui.make_persistent_id(("dc_typed_value", id));
    let mut edit = ui
        .ctx()
        .data_mut(|data| data.get_temp::<bool>(edit_id))
        .unwrap_or(false);
    let mut candidate = None;
    let columns = layout.text.chars().filter(char::is_ascii_digit).count();
    let column_width = ((ui.available_width() - 20.0 - 5.0 - 51.0 - 2.0 * (columns + 3) as f32)
        / columns.max(1) as f32)
        .clamp(14.0, 17.0);
    let mut unit_changed = false;
    let previous_spacing = ui.spacing().item_spacing.x;
    ui.spacing_mut().item_spacing.x = 2.0;
    ui.horizontal_top(|ui| {
        ui.allocate_ui_with_layout(
            egui::vec2(20.0, 64.0),
            egui::Layout::top_down(egui::Align::Center),
            |ui| {
                ui.add_space(18.0);
                let sign = ui.add_enabled(
                    enabled && !edit && value != 0.0,
                    egui::Button::new(if value < 0.0 { "-" } else { "+" })
                        .min_size(egui::vec2(19.0, 22.0)),
                );
                if sign.on_hover_text("Reverse polarity").clicked() {
                    candidate =
                        iv_core::nudge_source_value(0.0, -value, limits.resolution, limits.maximum)
                            .ok();
                }
            },
        );
        let mut digit = 0;
        for (column, character) in layout.text.chars().enumerate() {
            if character == '.' {
                let (rect, _) = ui.allocate_exact_size(egui::vec2(4.0, 64.0), egui::Sense::hover());
                ui.painter().text(
                    egui::pos2(rect.center().x, rect.top() + 30.0),
                    egui::Align2::CENTER_CENTER,
                    ".",
                    egui::FontId::monospace(20.0),
                    tone,
                );
                continue;
            }
            let step = layout.step(digit);
            let up =
                iv_core::nudge_source_value(value, step, limits.resolution, limits.maximum).ok();
            let down =
                iv_core::nudge_source_value(value, -step, limits.resolution, limits.maximum).ok();
            ui.push_id(("dc_digit", id, column), |ui| {
                ui.allocate_ui_with_layout(
                    egui::vec2(column_width, 64.0),
                    egui::Layout::top_down(egui::Align::Center),
                    |ui| {
                        ui.spacing_mut().item_spacing.y = 2.0;
                        if arrow(ui, true, enabled && !edit && up.is_some(), column_width)
                            .on_hover_text(format!("Increase by {}", fmt_quantity(step, base)))
                            .clicked()
                        {
                            candidate = up;
                        }
                        let number = ui.add(
                            egui::Label::new(
                                RichText::new(character.to_string())
                                    .monospace()
                                    .size(20.0)
                                    .color(tone),
                            )
                            .sense(egui::Sense::click()),
                        );
                        if enabled && number.clicked() {
                            edit = true;
                        }
                        if arrow(ui, false, enabled && !edit && down.is_some(), column_width)
                            .on_hover_text(format!("Decrease by {}", fmt_quantity(step, base)))
                            .clicked()
                        {
                            candidate = down;
                        }
                    },
                );
            });
            digit += 1;
        }
        ui.allocate_ui_with_layout(
            egui::vec2(51.0, 64.0),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                ui.add_space(18.0);
                let names = [
                    base.to_owned(),
                    format!("m{base}"),
                    format!("u{base}"),
                    format!("n{base}"),
                    format!("p{base}"),
                ];
                egui::ComboBox::from_id_salt(("dc_digit_unit", id))
                    .width(51.0)
                    .selected_text(&names[*unit])
                    .show_ui(ui, |ui| {
                        for (index, name) in names.iter().enumerate() {
                            unit_changed |= ui.selectable_value(unit, index, name).changed();
                        }
                    })
                    .response
                    .on_hover_text("Display unit only; changing it does not change output.");
            },
        );
    });
    ui.horizontal(|ui| {
        let text = if layout.exponent == 0 { format!("{} ±{}", if fixed_range { "Range" } else { "Limit" }, fmt_quantity(limits.maximum, base)) } else { format!("×10^{} · ±{}", layout.exponent, fmt_quantity(limits.maximum, base)) };
        ui.label(RichText::new(text).size(9.0).color(MUTED)).on_hover_text("Output range is fixed while ON. Grey arrows are outside the range or below its resolution.");
        if ui.add_enabled(enabled, egui::Button::new("Type…").small()).clicked() { edit = !edit; }
    });
    let buffer_id = edit_id.with("buffer");
    if edit {
        let mut draft = ui
            .ctx()
            .data_mut(|data| data.get_temp::<TypedDraft>(buffer_id))
            .unwrap_or_else(|| TypedDraft::new(value, *unit, base));
        draft.synchronize(value, *unit, base);
        let buffer = &mut draft.text;
        let mut valid = None;
        ui.add_enabled_ui(enabled, |ui| {
            ui.horizontal(|ui| {
                let width = (ui.available_width() - 45.0).max(65.0);
                let response = ui.add_sized([width, 24.0], egui::TextEdit::singleline(buffer));
                valid = buffer.trim().parse::<f64>().ok().and_then(|display| {
                    iv_core::nudge_source_value(
                        0.0,
                        display * SCALES[*unit],
                        limits.resolution,
                        limits.maximum,
                    )
                    .ok()
                });
                let enter =
                    response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
                if (ui
                    .add_enabled(valid.is_some(), egui::Button::new("Set"))
                    .clicked()
                    || enter)
                    && valid.is_some()
                {
                    candidate = valid;
                    edit = false;
                }
            });
            if valid.is_none() {
                ui.label(
                    RichText::new("Use a value within range and on the source grid.")
                        .size(9.0)
                        .color(RED),
                );
            }
        });
        ui.ctx().data_mut(|data| data.insert_temp(buffer_id, draft));
    } else {
        ui.ctx()
            .data_mut(|data| data.remove::<TypedDraft>(buffer_id));
    }
    ui.ctx().data_mut(|data| data.insert_temp(edit_id, edit));
    ui.spacing_mut().item_spacing.x = previous_spacing;
    if enabled && !unit_changed && !edit {
        candidate.filter(|next| *next != value)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn decimal_places_follow_the_display_unit_without_changing_si_value() {
        let value = 0.012345;
        for unit in 0..5 {
            let layout = Digits::new(value, unit, 1e-6);
            let restored = layout.text.parse::<f64>().unwrap() * layout.scale;
            assert!((restored - value).abs() < 1e-12, "{layout:?}");
            assert!(layout.text.chars().filter(char::is_ascii_digit).count() <= 8);
        }
        assert_eq!(Digits::new(0.001, 1, 1e-7).step(0), 0.001);
        assert_eq!(Digits::new(0.001, 0, 1e-7).step(0), 1.0);
    }
    #[test]
    fn real_pointer_digit_click_produces_one_exact_increment_and_is_locked_while_typing() {
        let context = egui::Context::default();
        let mut unit = 1;
        let mut origin = egui::Pos2::ZERO;
        let mut frame = |events, typing| {
            let mut result = None;
            let _ = context.run(
                egui::RawInput {
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        ui.set_max_width(224.0);
                        origin = ui.cursor().min;
                        if typing {
                            let id = ui.make_persistent_id(("dc_typed_value", 1_u64));
                            ui.ctx().data_mut(|data| data.insert_temp(id, true));
                        }
                        result = output_digits(
                            ui,
                            1,
                            DigitControl {
                                value: 0.001,
                                unit: &mut unit,
                                base: "V",
                                limits: DcLimits {
                                    range: 0.01,
                                    maximum: 0.01,
                                    resolution: 1e-7,
                                },
                                enabled: true,
                                fixed_range: true,
                            },
                        );
                    });
                },
            );
            (result, origin)
        };
        let (_, origin) = frame(vec![], false);
        let position = origin + egui::vec2(20.0 + 2.0 + 8.5, 8.0);
        let mouse = |pressed| egui::Event::PointerButton {
            pos: position,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        assert!(
            frame(
                vec![egui::Event::PointerMoved(position), mouse(true)],
                false
            )
            .0
            .is_none()
        );
        assert_eq!(frame(vec![mouse(false)], false).0, Some(0.002));
        assert!(frame(vec![mouse(true)], true).0.is_none());
        assert!(frame(vec![mouse(false)], true).0.is_none());
    }

    #[test]
    fn typed_draft_is_invalidated_by_verified_level_or_display_unit_changes() {
        let mut draft = TypedDraft::new(0.001, 1, "V");
        draft.text = "999".into();
        draft.synchronize(0.001, 1, "V");
        assert_eq!(draft.text, "999");
        draft.synchronize(0.0011, 1, "V");
        assert_ne!(draft.text, "999");
        assert_eq!(draft.text.parse::<f64>().unwrap() * 1e-3, 0.0011);
        draft.text = "888".into();
        draft.synchronize(0.0011, 0, "V");
        assert_eq!(draft.text, "0.0011");
    }
    #[test]
    fn zero_has_at_least_one_usable_digit_even_in_extreme_display_units() {
        for unit in 0..5 {
            for resolution in [1e-9, 1e-8, 1e-7, 1e-5] {
                let layout = Digits::new(0.0, unit, resolution);
                let count = layout.text.chars().filter(char::is_ascii_digit).count();
                assert!(
                    (0..count).any(|digit| iv_core::nudge_source_value(
                        0.0,
                        layout.step(digit),
                        resolution,
                        30.0
                    )
                    .is_ok()),
                    "{layout:?}"
                );
            }
        }
    }
    #[test]
    fn engineering_display_keeps_large_unit_values_compact_and_exact() {
        let layout = Digits::new(0.2, 4, 1e-7);
        assert!(layout.exponent > 0);
        assert_eq!(layout.text.parse::<f64>().unwrap() * layout.scale, 0.2);
    }
    #[test]
    fn drawing_and_unit_selection_do_not_request_any_output_change() {
        let context = egui::Context::default();
        for initial_unit in 0..5 {
            let mut unit = initial_unit;
            let mut changed = None;
            let _ = context.run(Default::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    changed = output_digits(
                        ui,
                        1,
                        DigitControl {
                            value: 0.001,
                            unit: &mut unit,
                            base: "V",
                            limits: DcLimits {
                                range: 0.01,
                                maximum: 0.01,
                                resolution: 1e-7,
                            },
                            enabled: true,
                            fixed_range: true,
                        },
                    );
                });
            });
            assert!(changed.is_none());
        }
    }
}
