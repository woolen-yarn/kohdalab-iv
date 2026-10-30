//! Model dialects ported from the Python drivers at b754cfa^.
//! Commands here are internal engine operations, never arbitrary user SCPI rewrites.
use crate::Role;
use iv_core::{InstrumentDialect, InstrumentRole, instrument_descriptor};

fn catalog_role(role: Role) -> InstrumentRole {
    match role {
        Role::Source | Role::Source2 | Role::Gate => InstrumentRole::Source,
        Role::Meter | Role::ReferenceMeter => InstrumentRole::Meter,
    }
}

pub fn default_model(role: Role) -> &'static str {
    match role {
        Role::Source | Role::Source2 | Role::Gate => "YOKOGAWA_GS210",
        Role::Meter | Role::ReferenceMeter => "AGILENT_34411A",
    }
}
pub fn valid_model(role: Role, model: &str) -> bool {
    instrument_descriptor(model)
        .is_some_and(|descriptor| !descriptor.simulation && descriptor.role == catalog_role(role))
}

pub fn dialect(role: Role, model: &str) -> Result<InstrumentDialect, String> {
    let descriptor = instrument_descriptor(model)
        .filter(|descriptor| !descriptor.simulation && descriptor.role == catalog_role(role))
        .ok_or_else(|| format!("Unsupported instrument model: {model}"))?;
    Ok(descriptor.dialect)
}
pub fn verify_model(role: Role, model: &str, identity: &str) -> Result<(), String> {
    let dialect = dialect(role, model)?;
    let first = identity
        .lines()
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_uppercase();
    let tokens: Vec<_> = first
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|s| !s.is_empty())
        .collect();
    let has = |s: &str| tokens.contains(&s);
    let accepted = match dialect {
        InstrumentDialect::Yokogawa7651 => {
            (first.starts_with("7651") || first.starts_with("YOKOGAWA")) && has("7651")
        }
        InstrumentDialect::Scpi if matches!(role, Role::Source | Role::Source2 | Role::Gate) => {
            has("YOKOGAWA") && has("GS210")
        }
        InstrumentDialect::Adc7461 => {
            (has("ADC") || has("ADCMT") || has("ADVANTEST")) && (has("7461A") || has("AD7461A"))
        }
        InstrumentDialect::Agilent34401 | InstrumentDialect::Scpi => {
            (has("AGILENT") || has("KEYSIGHT") || has("HEWLETT") || has("HP"))
                && has(model.rsplit('_').next().unwrap_or_default())
        }
        InstrumentDialect::Simulation => false,
    };
    if accepted {
        Ok(())
    } else {
        Err(format!(
            "Selected {model} returned an unexpected identity: {identity:?}. Check Device and Resource."
        ))
    }
}
pub fn identity_command(role: Role, model: &str) -> Result<&'static str, String> {
    if dialect(role, model)? == InstrumentDialect::Yokogawa7651 {
        Ok("OS;E\n")
    } else {
        Ok("*IDN?\n")
    }
}
pub fn legacy_status(reply: &str) -> Result<u8, String> {
    let text = reply.trim();
    let start = text.len()
        - text
            .bytes()
            .rev()
            .take_while(|c| c.is_ascii_digit())
            .count();
    if start == text.len() {
        return Err("Invalid 7651 status response".into());
    }
    let start = if start > 0 && matches!(text.as_bytes()[start - 1], b'+' | b'-') {
        start - 1
    } else {
        start
    };
    text[start..]
        .parse::<u8>()
        .map_err(|_| "Invalid 7651 status byte".into())
}
pub fn legacy_output(reply: &str) -> Result<bool, String> {
    Ok(legacy_status(reply)? & 16 != 0)
}
pub fn panel_value(panel: &str, key: u8) -> Result<String, String> {
    let bytes = panel.as_bytes();
    for i in 0..bytes.len().saturating_sub(1) {
        if bytes[i] == key && bytes[i + 1].is_ascii_digit() {
            let end = (i + 1..bytes.len())
                .find(|&j| !bytes[j].is_ascii_digit())
                .unwrap_or(bytes.len());
            return Ok(panel[i + 1..end].into());
        }
    }
    Err(format!("7651 panel response has no {} field", key as char))
}
pub fn numeric_reading(reply: &str) -> Result<String, String> {
    let text = reply.trim();
    let upper = text.to_ascii_uppercase();
    if upper.contains("OVER") || upper.starts_with("OV") {
        return Err(format!("Instrument overload: {reply:?}"));
    }
    // A 7651 header can carry function/range digits before S; they are not data.
    let text = if text.starts_with('F') && text.contains('R') {
        text.split_once('S').map_or(text, |(_, data)| data)
    } else {
        text
    };
    let bytes = text.as_bytes();
    let start = (0..bytes.len())
        .find(|&i| bytes[i].is_ascii_digit() || matches!(bytes[i], b'+' | b'-' | b'.'))
        .ok_or_else(|| format!("Invalid instrument reading: {reply:?}"))?;
    let mut end = start;
    if matches!(bytes[end], b'+' | b'-') {
        end += 1
    }
    let integer = end;
    while end < bytes.len() && bytes[end].is_ascii_digit() {
        end += 1
    }
    let mut digits = end - integer;
    if end < bytes.len() && bytes[end] == b'.' {
        end += 1;
        let fraction = end;
        while end < bytes.len() && bytes[end].is_ascii_digit() {
            end += 1
        }
        digits += end - fraction;
    }
    if digits == 0 {
        return Err(format!("Invalid instrument reading: {reply:?}"));
    }
    if end < bytes.len() && matches!(bytes[end], b'e' | b'E') {
        end += 1;
        if end < bytes.len() && matches!(bytes[end], b'+' | b'-') {
            end += 1
        }
        let exponent = end;
        while end < bytes.len() && bytes[end].is_ascii_digit() {
            end += 1
        }
        if exponent == end {
            return Err(format!("Incomplete exponent: {reply:?}"));
        }
    }
    let token = &text[start..end];
    let value = token
        .parse::<f64>()
        .map_err(|_| format!("Invalid instrument reading: {reply:?}"))?;
    if !value.is_finite() || value.abs() >= 1e30 {
        return Err(format!("Invalid or overloaded reading: {reply:?}"));
    }
    Ok(token.into())
}
pub fn source_write(command: &str) -> Result<Option<String>, String> {
    let upper = command.trim().trim_start_matches(':').to_ascii_uppercase();
    let (name, value) = upper.split_once(' ').unwrap_or((&upper, ""));
    Ok(Some(match name {
        "*CLS" => return Ok(None),
        "OUTP" => match value {
            "ON" | "1" => "O1;E".into(),
            "OFF" | "0" => "O0;E".into(),
            _ => return Err("Invalid source output state".into()),
        },
        "SOUR:FUNC" => match value {
            "VOLT" => "H0;F1;E".into(),
            "CURR" => "H0;F5;E".into(),
            _ => return Err("Invalid source function".into()),
        },
        "SOUR:RANG" => {
            let n = value.parse::<f64>().map_err(|_| "Invalid 7651 range")?;
            if n.fract() != 0.0 || !(2.0..=6.0).contains(&n) {
                return Err("Invalid 7651 range code".into());
            }
            format!("R{n:.0};E")
        }
        "SOUR:LEV:FIX" => {
            let n = value.parse::<f64>().map_err(|_| "Invalid level")?;
            if !n.is_finite() {
                return Err("Nonfinite source level".into());
            }
            format!("S{n:.12e};E")
        }
        "SOUR:PROT:CURR" => {
            let n = value.parse::<f64>().map_err(|_| "Invalid current limit")?;
            if !n.is_finite() || !(0.005..=0.12).contains(&n) {
                return Err("7651 current limit outside 5–120 mA".into());
            }
            format!("LA{:.0};E", n * 1e3)
        }
        "SOUR:PROT:VOLT" => {
            let n = value.parse::<f64>().map_err(|_| "Invalid voltage limit")?;
            if !n.is_finite() || !(1.0..=30.0).contains(&n) {
                return Err("7651 voltage limit outside 1–30 V".into());
            }
            format!("LV{n:.0};E")
        }
        _ => return Err(format!("Unsupported internal 7651 command: {command}")),
    }))
}
pub fn adc_configuration(command: &str, usb: bool) -> Result<Option<Vec<String>>, String> {
    let upper = command.trim().trim_start_matches(':').to_ascii_uppercase();
    if upper.starts_with("CONF:") {
        let voltage = upper == "CONF:VOLT:DC";
        if !voltage && upper != "CONF:CURR:DC" {
            return Err("Unsupported ADC measurement function".into());
        }
        return Ok(Some(if usb {
            vec![
                "*RST".into(),
                "H0".into(),
                if voltage { "F1" } else { "F5" }.into(),
                "R0".into(),
            ]
        } else {
            vec![
                "*RST".into(),
                format!(
                    ":SENSE:FUNCTION '{}'",
                    if voltage { "VOLTAGE:DC" } else { "CURRENT:DC" }
                ),
            ]
        }));
    }
    if upper.contains(":NPLC ") {
        let n = upper
            .split_whitespace()
            .last()
            .unwrap_or_default()
            .parse::<f64>()
            .map_err(|_| "Invalid ADC NPLC")?;
        if !n.is_finite() || !(0.02..=100.0).contains(&n) {
            return Err("ADC NPLC outside 0.02–100".into());
        }
        let function = if upper.contains("VOLT") {
            "VOLTAGE:DC"
        } else {
            "CURRENT:DC"
        };
        let rate = if n <= 0.02 {
            "FAST"
        } else if n <= 0.2 {
            "MED"
        } else if n <= 1.0 {
            "SLOW"
        } else {
            "SSLOW"
        };
        return Ok(Some(vec![if usb {
            format!("ITP{n:.12e}")
        } else {
            format!(":SENSE:{function}:SRATE {rate}")
        }]));
    }
    if upper.contains(":RANG:AUTO ") || upper.contains(":ZERO:AUTO ") || upper == "TRIG:SOUR IMM" {
        return Ok(Some(vec![]));
    }
    Ok(None)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn identities_are_exact_models() {
        assert!(verify_model(Role::Meter, "AGILENT_34401A", "HEWLETT-PACKARD,34401A,0,1").is_ok());
        assert!(verify_model(Role::Meter, "AGILENT_34401A", "Agilent,34411A,0,1").is_err());
        assert!(verify_model(Role::Source, "YOKOGAWA_7651", "7651 REV1").is_ok());
        assert!(verify_model(Role::Source, "YOKOGAWA_7651", "76510 REV1").is_err());
    }
    #[test]
    fn every_non_simulated_catalog_model_has_an_identity_contract() {
        for descriptor in iv_core::instrument_catalog(iv_core::InstrumentRole::Source) {
            if descriptor.simulation {
                continue;
            }
            let identity = match descriptor.id {
                "YOKOGAWA_GS210" => "YOKOGAWA,GS210,MOCK,2.0",
                "YOKOGAWA_7651" => "7651 REV1",
                _ => unreachable!("registered source requires identity fixture"),
            };
            assert!(verify_model(Role::Source, descriptor.id, identity).is_ok());
        }
        for descriptor in iv_core::instrument_catalog(iv_core::InstrumentRole::Meter) {
            if descriptor.simulation {
                continue;
            }
            let identity = match descriptor.id {
                "AGILENT_34401A" => "HEWLETT-PACKARD,34401A,0,1",
                "AGILENT_34411A" => "Agilent Technologies,34411A,0,1",
                "KEYSIGHT_34411A" => "Keysight Technologies,34411A,0,1",
                "KEYSIGHT_34465A" => "Keysight Technologies,34465A,0,1",
                "ADCMT_7461A" => "ADCMT,7461A,0,1",
                _ => unreachable!("registered meter requires identity fixture"),
            };
            assert!(verify_model(Role::Meter, descriptor.id, identity).is_ok());
        }
    }
    #[test]
    fn legacy_dialect_preserves_units_and_status() {
        assert_eq!(
            source_write(":SOUR:PROT:CURR 0.005").unwrap().unwrap(),
            "LA5;E"
        );
        assert_eq!(source_write(":SOUR:RANG 4").unwrap().unwrap(), "R4;E");
        assert!(source_write(":SOUR:RANG 0.01").is_err());
        assert!(legacy_output("STS1=16").unwrap());
        assert!(legacy_output("OC+16").unwrap());
        assert!(legacy_output("STATUS=16").unwrap());
        assert!(legacy_output("OC-16").is_err());
        assert!(!legacy_output("STS1=0").unwrap());
        assert!(legacy_output("STS1=garbage").is_err());
        assert!(legacy_output("STS1=256").is_err());
        assert_eq!(panel_value("F5R6S+0E0", b'R').unwrap(), "6");
        assert_eq!(numeric_reading("NDCV+1.23E-3").unwrap(), "+1.23E-3");
        assert!(numeric_reading("OVLD").is_err());
        assert_eq!(numeric_reading("NDCV1.23E-3").unwrap(), "1.23E-3");
        assert_eq!(numeric_reading("OD1.23E-3").unwrap(), "1.23E-3");
        assert_eq!(numeric_reading("NDCV.125").unwrap(), ".125");
        assert!(numeric_reading("NDCV1.23E-").is_err());
        assert_eq!(numeric_reading("F1R2S+1.2E-3").unwrap(), "+1.2E-3");
    }
    #[test]
    fn adc_transport_dialects_do_not_mix() {
        assert_eq!(
            adc_configuration("CONF:VOLT:DC", true).unwrap().unwrap(),
            vec!["*RST", "H0", "F1", "R0"]
        );
        assert_eq!(
            adc_configuration("SENS:CURR:DC:NPLC 0.2", false)
                .unwrap()
                .unwrap(),
            vec![":SENSE:CURRENT:DC:SRATE MED"]
        );
        assert_eq!(
            adc_configuration("SENS:CURR:DC:NPLC 1", true)
                .unwrap()
                .unwrap(),
            vec!["ITP1.000000000000e0"]
        );
    }
}
