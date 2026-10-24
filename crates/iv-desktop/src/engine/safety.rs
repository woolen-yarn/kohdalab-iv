//! Checks for the source's programmed settings, not external output accuracy.
use crate::state::OutputState;
use iv_core::Mode;

pub(super) fn verify_function(reply: &str, mode: Mode) -> Result<(), String> {
    let actual = reply.trim().trim_matches('"').to_ascii_uppercase();
    let valid = match mode {
        Mode::Iv => matches!(actual.as_str(), "VOLT" | "VOLTAGE"),
        Mode::Vi => matches!(actual.as_str(), "CURR" | "CURRENT"),
    };
    if valid {
        Ok(())
    } else {
        Err(format!(
            "Source function was not confirmed for {} (returned {reply:?}). Output will remain off.",
            mode.as_str()
        ))
    }
}

pub(super) fn verify_range(
    model: &str,
    mode: Mode,
    expected: f64,
    reply: &str,
) -> Result<f64, String> {
    let actual = reply
        .trim()
        .parse::<f64>()
        .ok()
        .filter(|v| v.is_finite() && *v > 0.0);
    let Some(actual) = actual else {
        return Err(format!(
            "Invalid source range readback from {model}: {reply:?}. Output will remain off."
        ));
    };
    // Both values use the profile's command units: SI ranges for GS210 and
    // integer range codes for 7651. Never compare a 7651 code to an SI range.
    if (actual / expected - 1.0).abs() > 1e-9 {
        return Err(format!(
            "Source range mismatch for {model}: requested {expected:e}, returned {actual:e}. Output will remain off."
        ));
    }
    iv_core::source_display_resolution(model, mode, actual)?.ok_or_else(|| {
        format!("Unrecognized source range for {model}: {actual:e}. Output will remain off.")
    })
}

pub(super) fn verify_zero(reply: f64, resolution: f64) -> Result<(), String> {
    if reply.is_finite() && reply.abs() <= resolution * 0.5 {
        Ok(())
    } else {
        Err(format!(
            "Source zero level was not confirmed (returned {reply:e}). Output will remain off."
        ))
    }
}

pub(super) fn output_state(reply: &str) -> Result<OutputState, String> {
    match reply.trim().trim_matches('"').to_ascii_uppercase().as_str() {
        "0" | "+0" | "OFF" => Ok(OutputState::Off),
        "1" | "+1" | "ON" => Ok(OutputState::On),
        _ => Err(format!("Unknown source output state: {reply:?}")),
    }
}

/// Decode only documented condition bits. Bit12 (TRP) is not a condition
/// bit and must not be mistaken for the event-register trip flag.
#[derive(Clone, Debug, serde::Serialize)]
pub(super) struct SourceCondition {
    pub register: u16,
    pub low_limiter: bool,
    pub high_limiter: bool,
    pub emergency: bool,
}
impl SourceCondition {
    pub fn stopped(&self) -> bool {
        self.low_limiter || self.high_limiter || self.emergency
    }
    pub fn indicator(&self) -> &'static str {
        if self.emergency {
            "EMR"
        } else if self.low_limiter && self.high_limiter {
            "H+L"
        } else if self.high_limiter {
            "H"
        } else if self.low_limiter {
            "L"
        } else {
            "none"
        }
    }
}
pub(super) fn source_condition(
    reply: &str,
    profile: iv_core::SourceProtectionProfile,
) -> Result<SourceCondition, String> {
    let register = reply.trim().parse::<u16>()
        .map_err(|_| format!("Cannot read source hardware limiter status: invalid condition register {reply:?}. Output OFF will be verified."))?;
    Ok(SourceCondition {
        register,
        low_limiter: register & profile.low_limiter_mask != 0,
        high_limiter: register & profile.high_limiter_mask != 0,
        emergency: register & profile.emergency_mask != 0,
    })
}
pub(super) fn verify_hardware_limit(expected: f64, reply: &str) -> Result<(), String> {
    let actual = reply
        .trim()
        .parse::<f64>()
        .ok()
        .filter(|v| v.is_finite() && *v > 0.0);
    if actual.is_some_and(|actual| (actual / expected - 1.0).abs() < 1e-6) {
        Ok(())
    } else {
        Err(format!(
            "Source hardware protection limit mismatch: requested {expected:e}, returned {reply:?}. Output will remain off."
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn gs210_condition_bits_are_strict_and_non_monitor_capability_is_explicit() {
        let profile = iv_core::source_protection_profile("YOKOGAWA_GS210")
            .unwrap()
            .unwrap();
        assert_eq!(profile.query, ":STAT:COND?");
        for (reply, label) in [
            ("+2304\n", "H"),
            ("1280", "L"),
            ("3328", "H+L"),
            ("8448", "EMR"),
        ] {
            let status = source_condition(reply, profile).unwrap();
            assert!(status.stopped());
            assert_eq!(status.indicator(), label);
        }
        for value in ["0", "256", "4096"] {
            assert!(!source_condition(value, profile).unwrap().stopped());
        }
        for value in ["", "1.0", "NaN", "-1", "65536", "H"] {
            assert!(source_condition(value, profile).is_err());
        }
        assert!(
            iv_core::source_protection_profile("YOKOGAWA_7651")
                .unwrap()
                .is_none()
        );
        assert!(verify_hardware_limit(1.0, "1e0").is_ok());
        assert!(verify_hardware_limit(1.0, "2e0").is_err());
    }

    #[test]
    fn all_source_profile_ranges_use_their_own_command_units() {
        for model in ["YOKOGAWA_GS210", "YOKOGAWA_7651"] {
            for mode in [Mode::Iv, Mode::Vi] {
                let Some(iv_core::InstrumentProfile::Source(profile)) =
                    iv_core::instrument_profile(model).unwrap()
                else {
                    panic!("missing source profile")
                };
                let ranges = if mode == Mode::Iv {
                    profile.voltage_ranges
                } else {
                    profile.current_ranges
                };
                for range in ranges {
                    let command = range.command_value;
                    assert!(verify_range(model, mode, command, &format!("{command:e}")).is_ok());
                    assert!(
                        verify_range(model, mode, command, &format!("{:e}", command * 1.1))
                            .is_err()
                    );
                    for bad in ["NaN", "inf", "0", "-1", "malformed"] {
                        assert!(verify_range(model, mode, command, bad).is_err());
                    }
                }
            }
        }
    }
    #[test]
    fn function_zero_and_output_confirmations_reject_ambiguous_states() {
        assert!(verify_function("\"VOLT\"", Mode::Iv).is_ok());
        assert!(verify_function("CURRENT", Mode::Vi).is_ok());
        assert!(verify_function("CURR", Mode::Iv).is_err());
        assert!(verify_function("VOLTAGE_EXTRA", Mode::Iv).is_err());
        assert!(verify_zero(0.0, 1e-6).is_ok());
        assert!(verify_zero(1e-3, 1e-6).is_err());
        assert!(verify_zero(f64::NAN, 1e-6).is_err());
        assert_eq!(output_state("+1").unwrap(), OutputState::On);
        assert_eq!(output_state("OFF").unwrap(), OutputState::Off);
        assert!(output_state("1,unknown").is_err());
    }
}
