//! Display-only engineering units and instrument-resolution formatting.
//! Values passed to plotting and persistence remain in SI units.

pub(super) fn fmt_quantity(value: f64, unit: &str) -> String {
    if value == 0.0 {
        return format!("0 {unit}");
    }
    let mag = value.abs();
    let (scale, prefix) = if mag >= 1e6 {
        (1e6, "M")
    } else if mag >= 1e3 {
        (1e3, "k")
    } else if mag >= 1.0 {
        (1.0, "")
    } else if mag >= 1e-3 {
        (1e-3, "m")
    } else if mag >= 1e-6 {
        (1e-6, "µ")
    } else if mag >= 1e-9 {
        (1e-9, "n")
    } else {
        (1e-12, "p")
    };
    format!("{:.4} {prefix}{unit}", value / scale)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct AxisScale {
    pub(super) factor: f64,
    pub(super) prefix: &'static str,
}

impl AxisScale {
    pub(super) fn for_magnitude(magnitude: f64) -> Self {
        if magnitude.is_finite() && magnitude > 0.0 {
            for (factor, prefix) in [
                (1e12, "T"),
                (1e9, "G"),
                (1e6, "M"),
                (1e3, "k"),
                (1.0, ""),
                (1e-3, "m"),
                (1e-6, "µ"),
                (1e-9, "n"),
                (1e-12, "p"),
                (1e-15, "f"),
                (1e-18, "a"),
            ] {
                if magnitude >= factor * (1.0 - 1e-12) {
                    return Self { factor, prefix };
                }
            }
            return Self {
                factor: 1e-18,
                prefix: "a",
            };
        }
        Self {
            factor: 1.0,
            prefix: "",
        }
    }

    pub(super) fn tick(self, value: f64, spacing: f64) -> String {
        let step = spacing.abs() / self.factor;
        let decimals = if step.is_finite() && step > 0.0 {
            (-step.log10()).ceil().clamp(0.0, 9.0) as usize
        } else {
            2
        };
        let value = value / self.factor;
        let value = if value.abs() < 0.5 * 10_f64.powi(-(decimals as i32)) {
            0.0
        } else {
            value
        };
        format!("{value:.decimals$}")
    }
}

pub(super) fn display_reading(value: f64, resolution: Option<f64>, unit: &str) -> String {
    let scale = AxisScale::for_magnitude(value.abs().max(resolution.unwrap_or(0.0)));
    display_reading_at_scale(value, resolution, unit, scale)
}

pub(super) fn display_reading_at_scale(
    value: f64,
    resolution: Option<f64>,
    unit: &str,
    scale: AxisScale,
) -> String {
    match resolution.filter(|r| r.is_finite() && *r > 0.0) {
        Some(resolution) => {
            // Round the display to a decimal place supported by the instrument.
            // The original value is never modified for plotting or persistence.
            let place = (resolution.log10() - 1e-12).ceil();
            let decimals = (scale.factor.log10() - place).round().clamp(0.0, 15.0) as usize;
            let step = 10_f64.powf(place);
            let rounded = (value / step).round() * step;
            let shown = if rounded == 0.0 {
                0.0
            } else {
                rounded / scale.factor
            };
            format!("{shown:.decimals$} {}{unit}", scale.prefix)
        }
        None => format!(
            "{:.6e} {}{unit} (resolution unknown)",
            value / scale.factor,
            scale.prefix
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engineering_axis_labels_and_ticks_share_the_same_scale() {
        for (value, expected_factor, prefix) in [
            (2.0, 1.0, ""),
            (0.001, 1e-3, "m"),
            (1e-6, 1e-6, "µ"),
            (7e-9, 1e-9, "n"),
            (3e-12, 1e-12, "p"),
            (5e-15, 1e-15, "f"),
        ] {
            let scale = AxisScale::for_magnitude(value);
            assert_eq!(scale.factor, expected_factor);
            assert_eq!(scale.prefix, prefix);
        }
        let millivolts = AxisScale::for_magnitude(0.001);
        assert_eq!(millivolts.tick(-0.001, 0.0005), "-1.0");
        assert_eq!(millivolts.tick(0.0005, 0.0005), "0.5");
        assert_eq!(millivolts.tick(-0.0, 0.0005), "0.0");
        let microamps = AxisScale::for_magnitude(1e-6);
        assert_eq!(microamps.tick(1e-6, 2e-7), "1.0");
        assert_eq!(microamps.tick(2e-7, 2e-7), "0.2");
        assert_eq!(AxisScale::for_magnitude(0.0).prefix, "");
        assert_eq!(AxisScale::for_magnitude(f64::NAN).factor, 1.0);
        assert_eq!(AxisScale::for_magnitude(1e-6 * (1.0 - 1e-15)).prefix, "µ");
    }

    #[test]
    fn display_digits_follow_resolution_without_modifying_measurement() {
        let value = 2.123456789e-9_f64;
        let bits = value.to_bits();
        assert_eq!(display_reading(value, Some(1e-11), "A"), "2.12 nA");
        assert_eq!(display_reading(value, Some(1e-10), "A"), "2.1 nA");
        assert_eq!(display_reading(value, Some(1e-12), "A"), "2.123 nA");
        assert_eq!(value.to_bits(), bits);
        assert_eq!(display_reading(-0.0, Some(1e-9), "A"), "0 nA");
        assert!(display_reading(value, None, "A").contains("resolution unknown"));
    }
}
