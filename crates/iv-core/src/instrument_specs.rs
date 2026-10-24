//! Native display policy from reviewed instrument TOML resources.
use crate::Mode;
use std::sync::OnceLock;

fn sources() -> Result<&'static toml::Value, String> {
    static SPECS: OnceLock<Result<toml::Value, String>> = OnceLock::new();
    SPECS
        .get_or_init(|| {
            toml::from_str(include_str!("../../../specs/sources.toml"))
                .map_err(|e| format!("Source display specification: {e}"))
        })
        .as_ref()
        .map_err(Clone::clone)
}
fn meters() -> Result<&'static toml::Value, String> {
    static SPECS: OnceLock<Result<toml::Value, String>> = OnceLock::new();
    SPECS
        .get_or_init(|| {
            toml::from_str(include_str!("../../../specs/meters.toml"))
                .map_err(|e| format!("Meter display specification: {e}"))
        })
        .as_ref()
        .map_err(Clone::clone)
}

/// Resolve a source's programmed range (including legacy range codes).
/// This is setting resolution, not measurement accuracy or /MON resolution.
pub fn source_display_resolution(
    model: &str,
    mode: Mode,
    range: f64,
) -> Result<Option<f64>, String> {
    if !range.is_finite() || range <= 0.0 {
        return Ok(None);
    }
    let Some(spec) = sources()?.get(model) else {
        return Ok(None);
    };
    if spec
        .get("display_precision")
        .and_then(|p| p.get("method"))
        .and_then(toml::Value::as_str)
        != Some("source_range_table")
    {
        return Ok(None);
    }
    let (ranges, resolution) = if mode == Mode::Iv {
        ("voltage_ranges", "resolution_V")
    } else {
        ("current_ranges", "resolution_A")
    };
    Ok(spec
        .get(ranges)
        .and_then(toml::Value::as_array)
        .and_then(|rows| {
            rows.iter().find(|row| {
                row.get("command_value")
                    .and_then(number)
                    .is_some_and(|r| (range / r - 1.0).abs() < 1e-6)
            })
        })
        .and_then(|row| row.get(resolution))
        .and_then(number)
        .filter(|r| r.is_finite() && *r > 0.0))
}
fn number(v: &toml::Value) -> Option<f64> {
    v.as_float().or_else(|| v.as_integer().map(|i| i as f64))
}

pub fn source_range_query(model: &str) -> Result<Option<&'static str>, String> {
    Ok(sources()?
        .get(model)
        .and_then(|s| s.get("display_precision"))
        .and_then(|p| p.get("range_query"))
        .and_then(toml::Value::as_str))
}

/// Reviewed non-clearing hardware status capability. Unsupported profiles
/// return None; a malformed declared profile is an error, never guessed.
#[derive(Clone, Copy, Debug)]
pub struct SourceProtectionProfile {
    pub query: &'static str,
    pub low_limiter_mask: u16,
    pub high_limiter_mask: u16,
    pub emergency_mask: u16,
}
pub fn source_protection_profile(model: &str) -> Result<Option<SourceProtectionProfile>, String> {
    let Some(profile) = sources()?
        .get(model)
        .and_then(|s| s.get("protection_status"))
    else {
        return Ok(None);
    };
    let mask = |key| {
        profile
            .get(key)
            .and_then(toml::Value::as_integer)
            .and_then(|v| u16::try_from(v).ok())
            .filter(|v| *v != 0)
            .ok_or_else(|| format!("Invalid source protection profile {model}: {key}"))
    };
    Ok(Some(SourceProtectionProfile {
        query: profile
            .get("query")
            .and_then(toml::Value::as_str)
            .ok_or_else(|| format!("Missing protection query for {model}"))?,
        low_limiter_mask: mask("low_limiter_mask")?,
        high_limiter_mask: mask("high_limiter_mask")?,
        emergency_mask: mask("emergency_mask")?,
    }))
}

pub fn simulation_display_precision(mode: Mode) -> Result<(Option<f64>, Option<f64>), String> {
    let (ranges, source_key, meter_key) = if mode == Mode::Iv {
        ("voltage_ranges", "resolution_V", "current_resolution_A")
    } else {
        ("current_ranges", "resolution_A", "voltage_resolution_V")
    };
    let source = sources()?
        .get("SIMULATED_SOURCE")
        .and_then(|s| s.get(ranges))
        .and_then(toml::Value::as_array)
        .and_then(|r| r.first())
        .and_then(|r| r.get(source_key))
        .and_then(number);
    let meter = meters()?
        .get("SIMULATED_METER")
        .and_then(|s| s.get("display_precision"))
        .and_then(|p| p.get(meter_key))
        .and_then(number);
    Ok((source, meter))
}

/// Reviewed read-only SCPI queries; unknown/unfinished profiles return None.
/// Having a profile does not enable that model in the native transport.
pub fn meter_display_queries(
    model: &str,
    mode: Mode,
) -> Result<Option<(&'static str, &'static str)>, String> {
    let Some(p) = meters()?
        .get(model)
        .and_then(|s| s.get("display_precision"))
    else {
        return Ok(None);
    };
    if p.get("method").and_then(toml::Value::as_str) != Some("scpi_range_resolution_query") {
        return Ok(None);
    }
    let (range, resolution) = if mode == Mode::Iv {
        ("current_range_query", "current_resolution_query")
    } else {
        ("voltage_range_query", "voltage_resolution_query")
    };
    Ok(p.get(range)
        .and_then(toml::Value::as_str)
        .zip(p.get(resolution).and_then(toml::Value::as_str)))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reviewed_tables_cover_all_source_ranges_and_keep_legacy_codes_separate() {
        for (model, spec) in sources().unwrap().as_table().unwrap() {
            for (mode, key, res) in [
                (Mode::Iv, "voltage_ranges", "resolution_V"),
                (Mode::Vi, "current_ranges", "resolution_A"),
            ] {
                for row in spec[key].as_array().unwrap() {
                    let range = number(&row["command_value"]).unwrap();
                    assert_eq!(
                        source_display_resolution(model, mode, range).unwrap(),
                        number(&row[res])
                    );
                }
            }
            assert!(
                spec["display_precision"]["review_status"]
                    .as_str()
                    .is_some()
            );
        }
        assert_eq!(
            source_display_resolution("YOKOGAWA_7651", Mode::Iv, 2.0).unwrap(),
            Some(1e-7)
        );
        assert_eq!(
            source_display_resolution("YOKOGAWA_GS210", Mode::Iv, 2.0).unwrap(),
            None
        );
        assert_eq!(
            source_display_resolution("unknown", Mode::Iv, 0.01).unwrap(),
            None
        );
    }
    #[test]
    fn reviewed_meter_policies_separate_brands_modes_and_pending_protocols() {
        assert_eq!(
            meter_display_queries("AGILENT_34411A", Mode::Iv).unwrap(),
            Some(("SENS:CURR:DC:RANG?", "SENS:CURR:DC:RES?"))
        );
        assert_eq!(
            meter_display_queries("KEYSIGHT_34411A", Mode::Vi).unwrap(),
            meter_display_queries("AGILENT_34411A", Mode::Vi).unwrap()
        );
        assert_eq!(
            meter_display_queries("ADCMT_7461A", Mode::Iv).unwrap(),
            None
        );
        assert_eq!(
            meter_display_queries("SIMULATED_METER", Mode::Iv).unwrap(),
            None
        );
        for (_, spec) in meters().unwrap().as_table().unwrap() {
            assert!(
                spec["display_precision"]["review_status"]
                    .as_str()
                    .is_some()
            );
        }
    }
}
