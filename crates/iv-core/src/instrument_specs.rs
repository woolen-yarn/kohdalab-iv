//! Native display policy from reviewed instrument TOML resources.
use crate::Mode;
use crate::model_spec::{
    SCPI_RANGE_RESOLUTION_QUERY, SOURCE_RANGE_TABLE, meter_model, source_model,
};

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
    let Some(spec) = source_model(model)? else {
        return Ok(None);
    };
    if spec
        .display_precision
        .as_ref()
        .map(|precision| precision.method.as_str())
        != Some(SOURCE_RANGE_TABLE)
    {
        return Ok(None);
    }
    Ok(spec
        .ranges(mode)
        .iter()
        .find(|row| (range / row.command.value() - 1.0).abs() < 1e-6)
        .map(|row| row.resolution)
        .filter(|r| r.is_finite() && *r > 0.0))
}

pub fn source_range_query(model: &str) -> Result<Option<&'static str>, String> {
    Ok(source_model(model)?
        .and_then(|s| s.display_precision.as_ref())
        .and_then(|p| p.range_query.as_deref()))
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
    let Some(profile) = source_model(model)?.and_then(|s| s.protection_status.as_ref()) else {
        return Ok(None);
    };
    let mask = |key: &str, value: Option<i64>| {
        value
            .and_then(|v| u16::try_from(v).ok())
            .filter(|v| *v != 0)
            .ok_or_else(|| format!("Invalid source protection profile {model}: {key}"))
    };
    Ok(Some(SourceProtectionProfile {
        query: profile
            .query
            .as_deref()
            .ok_or_else(|| format!("Missing protection query for {model}"))?,
        low_limiter_mask: mask("low_limiter_mask", profile.low_limiter_mask)?,
        high_limiter_mask: mask("high_limiter_mask", profile.high_limiter_mask)?,
        emergency_mask: mask("emergency_mask", profile.emergency_mask)?,
    }))
}

pub fn simulation_display_precision(mode: Mode) -> Result<(Option<f64>, Option<f64>), String> {
    let source = source_model("SIMULATED_SOURCE")?
        .and_then(|s| s.ranges(mode).first())
        .map(|r| r.resolution);
    let meter = meter_model("SIMULATED_METER")?
        .and_then(|s| s.display_precision.as_ref())
        .and_then(|p| match mode {
            Mode::Iv => p.current_resolution_a,
            Mode::Vi => p.voltage_resolution_v,
        });
    Ok((source, meter))
}

/// Reviewed read-only SCPI queries; unknown/unfinished profiles return None.
/// Having a profile does not enable that model in the native transport.
pub fn meter_display_queries(
    model: &str,
    mode: Mode,
) -> Result<Option<(&'static str, &'static str)>, String> {
    let Some(p) = meter_model(model)?.and_then(|s| s.display_precision.as_ref()) else {
        return Ok(None);
    };
    if p.method != SCPI_RANGE_RESOLUTION_QUERY {
        return Ok(None);
    }
    let (range, resolution) = if mode == Mode::Iv {
        (&p.current_range_query, &p.current_resolution_query)
    } else {
        (&p.voltage_range_query, &p.voltage_resolution_query)
    };
    Ok(range.as_deref().zip(resolution.as_deref()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model_spec::{meter_models, source_models};
    #[test]
    fn reviewed_tables_cover_all_source_ranges_and_keep_legacy_codes_separate() {
        for (model, spec) in source_models().unwrap() {
            for mode in [Mode::Iv, Mode::Vi] {
                for row in spec.ranges(mode) {
                    assert_eq!(
                        source_display_resolution(model, mode, row.command.value()).unwrap(),
                        Some(row.resolution)
                    );
                }
            }
            assert!(
                spec.display_precision
                    .as_ref()
                    .and_then(|p| p.review_status.as_ref())
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
        for spec in meter_models().unwrap().values() {
            assert!(
                spec.display_precision
                    .as_ref()
                    .and_then(|p| p.review_status.as_ref())
                    .is_some()
            );
        }
    }
    #[test]
    fn protection_and_simulation_policies_are_preserved() {
        let protection = source_protection_profile("YOKOGAWA_GS210")
            .unwrap()
            .unwrap();
        assert_eq!(protection.query, ":STAT:COND?");
        assert_eq!(
            (
                protection.low_limiter_mask,
                protection.high_limiter_mask,
                protection.emergency_mask
            ),
            (1024, 2048, 8192)
        );
        assert!(
            source_protection_profile("YOKOGAWA_7651")
                .unwrap()
                .is_none()
        );
        assert_eq!(
            source_range_query("YOKOGAWA_GS210").unwrap(),
            Some(":SOUR:RANG?")
        );
        assert_eq!(source_range_query("YOKOGAWA_7651").unwrap(), None);
        assert_eq!(
            simulation_display_precision(Mode::Iv).unwrap(),
            (Some(1e-9), Some(1e-12))
        );
        assert_eq!(
            simulation_display_precision(Mode::Vi).unwrap(),
            (Some(1e-12), Some(1e-9))
        );
    }
}
