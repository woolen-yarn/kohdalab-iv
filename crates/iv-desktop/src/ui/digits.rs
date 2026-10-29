//! A display-unit-only selector and per-decimal-place fixed-output controls.
use super::*;
use crate::protocol::DcLimits;

const SCALES: [f64; 5] = [1.0, 1e-3, 1e-6, 1e-9, 1e-12];

pub(super) struct DigitControl<'a> {
    pub value: f64,
    pub unit: &'a mut usize,
    pub base: &'a str,
    pub limits: DcLimits,
    /// Stable display geometry derived from the full documented source profile.
    /// These are deliberately independent of the active hardware range.
    pub display_maximum: f64,
    pub display_resolution: f64,
    pub enabled: bool,
    pub fixed_range: bool,
    pub source: Option<(&'a str, Mode, f64)>,
}

#[derive(Debug)]
struct Digits {
    text: String,
    scale: f64,
    integer: usize,
}
impl Digits {
    fn new(value: f64, unit: usize, resolution: f64, maximum: f64) -> Self {
        let scale = SCALES[unit.min(4)];
        let shown = (value / scale).abs();
        let resolution = (resolution / scale).abs();
        // Decimal places follow the documented source grid in the selected
        // unit. No padded sub-resolution places or exponent switching.
        let decimals = if resolution.is_finite() && resolution > 0.0 {
            (-(resolution.log10()) - 1e-9).ceil().max(0.0) as usize
        } else {
            0
        };
        let max_shown = (maximum / scale).abs();
        let reserved_integer = if max_shown >= 1.0 {
            max_shown.log10().floor() as usize + 1
        } else {
            1
        };
        let value_integer = if shown >= 1.0 {
            shown.log10().floor() as usize + 1
        } else {
            1
        };
        let integer = reserved_integer.max(value_integer);
        let unpadded = format!("{shown:.decimals$}");
        let text = if let Some((whole, fraction)) = unpadded.split_once('.') {
            format!("{whole:0>integer$}.{fraction}")
        } else {
            format!("{unpadded:0>integer$}")
        };
        Self {
            text,
            scale,
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
    fn new(value: f64, unit: usize, base: &str, text: &str) -> Self {
        Self {
            value,
            unit,
            base: base.to_owned(),
            // Digit buttons display magnitude; the editable value must keep
            // polarity when Type is opened and confirmed without changes.
            text: if value.is_sign_negative() {
                format!("-{text}")
            } else {
                text.to_owned()
            },
        }
    }
    fn synchronize(&mut self, value: f64, unit: usize, base: &str, text: &str) {
        if self.value != value || self.unit != unit || self.base != base {
            *self = Self::new(value, unit, base, text);
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
        display_maximum,
        display_resolution,
        enabled,
        fixed_range,
        source,
    } = control;
    let validate = |next: f64| {
        if let Some((model, mode, compliance)) = source {
            iv_core::manual_source_range(model, mode, next, compliance)
                .ok()
                .map(|_| next)
        } else {
            iv_core::nudge_source_value(0.0, next, limits.resolution, limits.maximum).ok()
        }
    };
    *unit = (*unit).min(4);
    let layout = Digits::new(value, *unit, display_resolution, display_maximum);
    let tone = if base == "A" { TEAL } else { ORANGE };
    let edit_id = ui.make_persistent_id(("dc_typed_value", id));
    let mut edit = ui
        .ctx()
        .data_mut(|data| data.get_temp::<bool>(edit_id))
        .unwrap_or(false);
    let mut candidate = None;
    // Keep the unit chooser pinned on the right.  Very small display units can
    // require many fixed leading digits, so their digit strip scrolls instead
    // of shrinking controls or changing the selected unit.
    let columns = layout.text.chars().filter(char::is_ascii_digit).count();
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
                    candidate = validate(-value);
                }
            },
        );
        let digits_width = (ui.available_width() - 51.0 - 4.0).max(48.0);
        // Normal V/mV displays occupy the card without scrolling.  The
        // minimum remains comfortably clickable; only unusually long p-unit
        // representations need the horizontal strip.
        let spacing = 2.0 * (layout.text.chars().count().saturating_sub(1)) as f32;
        let column_width =
            ((digits_width - 4.0 - spacing) / columns.max(1) as f32).clamp(11.0, 16.0);
        let font_size = (column_width * 1.45).clamp(14.0, 20.0);
        ui.allocate_ui_with_layout(
            egui::vec2(digits_width, 64.0),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                egui::ScrollArea::horizontal()
                    .id_salt(("dc_digit_strip", id))
                    .max_height(64.0)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        ui.horizontal_top(|ui| {
                            let mut digit = 0;
                            for (column, character) in layout.text.chars().enumerate() {
                                if character == '.' {
                                    let (rect, _) = ui.allocate_exact_size(
                                        egui::vec2(4.0, 64.0),
                                        egui::Sense::hover(),
                                    );
                                    ui.painter().text(
                                        egui::pos2(rect.center().x, rect.top() + 30.0),
                                        egui::Align2::CENTER_CENTER,
                                        ".",
                                        egui::FontId::monospace(font_size),
                                        tone,
                                    );
                                    continue;
                                }
                                let step = layout.step(digit);
                                let up = validate(value + step);
                                let down = validate(value - step);
                                ui.push_id(("dc_digit", id, column), |ui| {
                                    ui.allocate_ui_with_layout(
                                        egui::vec2(column_width, 64.0),
                                        egui::Layout::top_down(egui::Align::Center),
                                        |ui| {
                                            ui.spacing_mut().item_spacing.y = 2.0;
                                            if arrow(
                                                ui,
                                                true,
                                                enabled && !edit && up.is_some(),
                                                column_width,
                                            )
                                            .on_hover_text(format!(
                                                "Increase by {}",
                                                fmt_quantity(step, base)
                                            ))
                                            .clicked()
                                            {
                                                candidate = up;
                                            }
                                            let number = ui.add(
                                                egui::Label::new(
                                                    RichText::new(character.to_string())
                                                        .monospace()
                                                        .size(font_size)
                                                        .color(tone),
                                                )
                                                .sense(egui::Sense::click()),
                                            );
                                            if enabled && number.clicked() {
                                                edit = true;
                                            }
                                            if arrow(
                                                ui,
                                                false,
                                                enabled && !edit && down.is_some(),
                                                column_width,
                                            )
                                            .on_hover_text(format!(
                                                "Decrease by {}",
                                                fmt_quantity(step, base)
                                            ))
                                            .clicked()
                                            {
                                                candidate = down;
                                            }
                                        },
                                    );
                                });
                                digit += 1;
                            }
                        });
                    })
            },
        );
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
        let text = format!(
            "{} ±{}",
            if fixed_range { "Range" } else { "Max" },
            fmt_quantity(limits.maximum, base)
        );
        ui.label(RichText::new(text).size(9.0).color(MUTED)).on_hover_text("Range is selected automatically. A range change briefly switches output OFF, verifies the new settings, then ramps to the requested value. Grey arrows are outside the source limits or below its resolution.");
        if ui.add_enabled(enabled, egui::Button::new("Type…").small()).clicked() { edit = !edit; }
    });
    let buffer_id = edit_id.with("buffer");
    if edit {
        let mut draft = ui
            .ctx()
            .data_mut(|data| data.get_temp::<TypedDraft>(buffer_id))
            .unwrap_or_else(|| TypedDraft::new(value, *unit, base, &layout.text));
        draft.synchronize(value, *unit, base, &layout.text);
        let buffer = &mut draft.text;
        let mut valid = None;
        ui.add_enabled_ui(enabled, |ui| {
            ui.horizontal(|ui| {
                let width = (ui.available_width() - 45.0).max(65.0);
                let response = ui.add_sized([width, 24.0], egui::TextEdit::singleline(buffer));
                valid = buffer
                    .trim()
                    .parse::<f64>()
                    .ok()
                    .and_then(|display| validate(display * SCALES[*unit]));
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
    fn fixed_typed_draft_preserves_negative_polarity_in_every_unit() {
        for (unit, scale) in SCALES.iter().enumerate() {
            let value = -0.0011;
            let layout = Digits::new(value, unit, 1e-7, 32.0);
            let mut draft = TypedDraft::new(value, unit, "V", &layout.text);
            let commit = draft.text.parse::<f64>().unwrap() * scale;
            assert!(commit < 0.0, "{}", draft.text);
            let validated = iv_core::nudge_source_value(0., commit, 1e-7, 32.).unwrap();
            assert!((validated - value).abs() < 1e-15);
            draft.synchronize(value, unit, "V", &layout.text);
            assert!(draft.text.starts_with('-'));
            draft.synchronize(-value, unit, "V", &layout.text);
            assert!(!draft.text.starts_with('-'));
        }
    }

    #[test]
    fn gs200_unit_digits_match_documented_minimum_increments() {
        assert_eq!(Digits::new(0.0, 0, 100e-9, 32.0).text, "00.0000000");
        assert_eq!(Digits::new(0.0, 1, 100e-9, 32.0).text, "00000.0000");
        assert_eq!(Digits::new(0.0, 1, 10e-9, 0.2).text, "000.00000");
        // With a <200mA protection request, divider ranges are ineligible.
        assert_eq!(Digits::new(0.0, 0, 10e-6, 32.0).text, "00.00000");
    }

    #[test]
    fn decimal_places_follow_the_display_unit_without_changing_si_value() {
        let value = 0.012345;
        for unit in 0..5 {
            let layout = Digits::new(value, unit, 1e-6, 30.0);
            let restored = layout.text.parse::<f64>().unwrap() * layout.scale;
            assert!((restored - value).abs() < 1e-12, "{layout:?}");
            assert!(!layout.text.contains('e'), "{layout:?}");
        }
        assert_eq!(Digits::new(0.001, 1, 1e-7, 30.0).step(0), 10.0);
        assert_eq!(Digits::new(0.001, 0, 1e-7, 30.0).step(0), 10.0);
    }
    #[test]
    fn fixed_geometry_is_stable_across_sign_zero_and_magnitude() {
        for (unit, resolution, maximum) in [(0, 1e-7, 30.0), (1, 1e-7, 30.0), (4, 1e-12, 0.2)] {
            let zero = Digits::new(0.0, unit, resolution, maximum);
            let positive = Digits::new(resolution * 2.0, unit, resolution, maximum);
            let negative = Digits::new(-maximum, unit, resolution, maximum);
            assert_eq!(zero.text.len(), positive.text.len());
            assert_eq!(zero.text.len(), negative.text.len());
            assert_eq!(zero.integer, positive.integer);
            assert_eq!(zero.integer, negative.integer);
            if unit != 4 {
                assert!(zero.text.contains('.'));
            }
            assert!(!zero.text.contains('e'));
        }
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
                                display_maximum: 0.01,
                                display_resolution: 1e-7,
                                enabled: true,
                                fixed_range: true,
                                source: None,
                            },
                        );
                    });
                },
            );
            (result, origin)
        };
        let (_, origin) = frame(vec![], false);
        // The first reserved position is 10 mV and is disabled by the
        // 10 mV limit; click the stable 1 mV place instead.
        let position = origin + egui::vec2(20.0 + 2.0 + 16.0 + 2.0 + 8.0, 8.0);
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
        let mut draft = TypedDraft::new(0.001, 1, "V", "01.000000");
        draft.text = "999".into();
        draft.synchronize(0.001, 1, "V", "01.000000");
        assert_eq!(draft.text, "999");
        draft.synchronize(0.0011, 1, "V", "01.100000");
        assert_ne!(draft.text, "999");
        assert_eq!(draft.text, "01.100000");
        draft.text = "888".into();
        draft.synchronize(0.0011, 0, "V", "00.001100");
        assert_eq!(draft.text, "00.001100");
    }
    #[test]
    fn zero_has_at_least_one_usable_digit_even_in_extreme_display_units() {
        for unit in 0..5 {
            for resolution in [1e-9, 1e-8, 1e-7, 1e-5] {
                let layout = Digits::new(0.0, unit, resolution, 30.0);
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
    fn extreme_display_units_remain_plain_decimal_and_exact() {
        let layout = Digits::new(0.2, 4, 1e-7, 30.0);
        assert!(!layout.text.contains('e'));
        assert!((layout.text.parse::<f64>().unwrap() * layout.scale - 0.2).abs() < 1e-15);
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
                            display_maximum: 0.01,
                            display_resolution: 1e-7,
                            enabled: true,
                            fixed_range: true,
                            source: None,
                        },
                    );
                });
            });
            assert!(changed.is_none());
        }
    }
}
