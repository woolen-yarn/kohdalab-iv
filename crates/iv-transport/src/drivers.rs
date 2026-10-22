//! Private command translation for the supported instrument dialects.
//!
//! DevicePair owns helper lifetimes, leases, recovery, and deadlines. This module only
//! translates the stable internal operations into model protocol exchanges, which keeps
//! command order testable without a helper process.

use crate::{Role, instruments};
use iv_core::InstrumentDialect;
use std::time::{Duration, Instant};

pub(crate) trait InstrumentIo {
    fn request(&mut self, operation: &str, command: &str) -> Result<String, String>;
    /// Issue a request within a caller-owned absolute deadline. Most protocol
    /// operations use `request`; only a multi-line 7651 panel shares this
    /// budget across its initial query and continuation reads.
    fn request_before(
        &mut self,
        operation: &str,
        command: &str,
        _deadline: Instant,
    ) -> Result<String, String> {
        self.request(operation, command)
    }
    fn wait(&mut self, duration: Duration);
}

fn dialect(role: Role, model: &str) -> Result<InstrumentDialect, String> {
    instruments::dialect(role, model)
}

pub(crate) fn is_dialect(role: Role, model: &str, expected: InstrumentDialect) -> bool {
    dialect(role, model).is_ok_and(|actual| actual == expected)
}

pub(crate) fn is_adc_meter(model: &str) -> bool {
    is_dialect(Role::Meter, model, InstrumentDialect::Adc7461)
}

pub(crate) fn is_legacy_source(model: &str) -> bool {
    is_dialect(Role::Source, model, InstrumentDialect::Yokogawa7651)
}

/// Translate an internal write into its exact on-wire commands.
pub(crate) fn write_commands(
    role: Role,
    model: &str,
    usb: bool,
    command: &str,
) -> Result<Vec<String>, String> {
    match dialect(role, model)? {
        InstrumentDialect::Yokogawa7651 => {
            Ok(instruments::source_write(command)?.into_iter().collect())
        }
        InstrumentDialect::Adc7461 => {
            Ok(instruments::adc_configuration(command, usb)?
                .unwrap_or_else(|| vec![command.into()]))
        }
        InstrumentDialect::Agilent34401 if command.to_ascii_uppercase().contains(":ZERO:AUTO ") => {
            // The 34401A does not support this modern auto-zero command.
            Ok(vec![])
        }
        InstrumentDialect::Scpi => Ok(vec![command.into()]),
        InstrumentDialect::Simulation => Err(format!("Unsupported instrument model: {model}")),
        InstrumentDialect::Agilent34401 => Ok(vec![command.into()]),
    }
}

pub(crate) fn source_write_marks_adc_discard(role: Role, meter_model: &str, command: &str) -> bool {
    role == Role::Source
        && is_adc_meter(meter_model)
        && command.to_ascii_uppercase().contains("SOUR:LEV:FIX ")
}

pub(crate) fn identify<I: InstrumentIo>(
    io: &mut I,
    role: Role,
    model: &str,
) -> Result<String, String> {
    if is_legacy_source(model) {
        Ok(legacy_panel(io)?
            .lines()
            .next()
            .unwrap_or_default()
            .trim()
            .into())
    } else {
        io.request("QUERY", instruments::identity_command(role, model)?)
    }
}

pub(crate) fn query<I: InstrumentIo>(
    io: &mut I,
    role: Role,
    model: &str,
    usb: bool,
    meter_discard: &mut bool,
    command: &str,
) -> Result<String, String> {
    let canonical = command.trim().trim_start_matches(':').to_ascii_uppercase();
    match dialect(role, model)? {
        InstrumentDialect::Yokogawa7651 => legacy_query(io, &canonical, command),
        InstrumentDialect::Adc7461 => adc_query(io, usb, meter_discard, &canonical, command),
        InstrumentDialect::Scpi | InstrumentDialect::Agilent34401 => {
            io.request("QUERY", &terminated(command))
        }
        InstrumentDialect::Simulation => Err(format!("Unsupported instrument model: {model}")),
    }
}

pub(crate) fn terminated(command: &str) -> String {
    format!("{}\n", command.trim_end_matches(['\r', '\n']))
}

fn legacy_panel<I: InstrumentIo>(io: &mut I) -> Result<String, String> {
    // This must be one budget rather than one six-second wait per panel line.
    // It mirrors the legacy `legacy_panel_before` behavior used before the
    // driver extraction.
    let deadline = Instant::now() + Duration::from_secs(6);
    let mut panel = io.request_before("QUERY", "OS;E\n", deadline)?;
    for _ in 0..5 {
        if panel.lines().any(|line| line.trim() == "END") {
            return Ok(panel);
        }
        if panel.len() > 4096 {
            return Err("Oversized 7651 panel response".into());
        }
        panel.push('\n');
        panel.push_str(&io.request_before("READ", "", deadline)?);
    }
    Err("Incomplete 7651 panel response: END was not received".into())
}

fn legacy_query<I: InstrumentIo>(
    io: &mut I,
    canonical: &str,
    original: &str,
) -> Result<String, String> {
    match canonical {
        "*IDN?" => Ok(legacy_panel(io)?
            .lines()
            .next()
            .unwrap_or_default()
            .trim()
            .into()),
        "SOUR:FUNC?" => match instruments::panel_value(&legacy_panel(io)?, b'F')?.as_str() {
            "1" => Ok("VOLT".into()),
            "5" => Ok("CURR".into()),
            _ => Err("Unsupported 7651 source function".into()),
        },
        "SOUR:RANG?" => instruments::panel_value(&legacy_panel(io)?, b'R'),
        "SOUR:LEV?" | "SOUR:LEV:FIX?" | "SOUR:LEV:AUTO?" => {
            instruments::numeric_reading(&io.request("QUERY", "OD;E\n")?)
        }
        "OUTP?" => Ok(
            if instruments::legacy_output(&io.request("QUERY", "OC;E\n")?)? {
                "1"
            } else {
                "0"
            }
            .into(),
        ),
        "SYST:ERR?" => {
            let reply = io.request("QUERY", "OC;E\n")?;
            let status = instruments::legacy_status(&reply)?;
            Ok(if status & 4 != 0 {
                "1,7651 command error"
            } else {
                "0,No error"
            }
            .into())
        }
        _ => Err(format!("Unsupported internal 7651 query: {original}")),
    }
}

fn adc_query<I: InstrumentIo>(
    io: &mut I,
    usb: bool,
    meter_discard: &mut bool,
    canonical: &str,
    original: &str,
) -> Result<String, String> {
    if canonical == "READ?" {
        let mut read = || -> Result<String, String> {
            if usb {
                io.wait(Duration::from_millis(20));
                instruments::numeric_reading(&io.request("READ", "")?)
            } else {
                instruments::numeric_reading(&io.request("QUERY", "READ?\n")?)
            }
        };
        if *meter_discard {
            read()?;
            *meter_discard = false;
        }
        return read();
    }
    if canonical == "SYST:ERR?" {
        if usb {
            io.request("WRITE", "ERR?\n")?;
            io.wait(Duration::from_millis(20));
            return io.request("READ", "");
        }
        return io.request("QUERY", ":SYSTem:ERRor?\n");
    }
    io.request("QUERY", &terminated(original))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct FakeIo {
        calls: Vec<(String, String)>,
        exchanges: Vec<(String, String, Result<String, String>)>,
        replies: Vec<Result<String, String>>,
        waits: Vec<Duration>,
        panel_deadlines: Vec<Instant>,
    }

    impl InstrumentIo for FakeIo {
        fn request(&mut self, operation: &str, command: &str) -> Result<String, String> {
            self.calls.push((operation.into(), command.into()));
            let result = self.replies.remove(0);
            self.exchanges
                .push((operation.into(), command.into(), result.clone()));
            result
        }
        fn request_before(
            &mut self,
            operation: &str,
            command: &str,
            deadline: Instant,
        ) -> Result<String, String> {
            self.panel_deadlines.push(deadline);
            self.request(operation, command)
        }
        fn wait(&mut self, duration: Duration) {
            self.waits.push(duration);
        }
    }

    #[test]
    fn every_catalog_model_has_a_protocol_translation_contract() {
        for descriptor in iv_core::instrument_catalog(iv_core::InstrumentRole::Source) {
            if !descriptor.simulation {
                assert!(write_commands(Role::Source, descriptor.id, false, "*CLS").is_ok());
            }
        }
        for descriptor in iv_core::instrument_catalog(iv_core::InstrumentRole::Meter) {
            if !descriptor.simulation {
                assert!(write_commands(Role::Meter, descriptor.id, false, "TRIG:SOUR IMM").is_ok());
            }
        }
    }

    #[test]
    fn legacy_panel_and_adc_usb_preserve_protocol_order() {
        let mut legacy = FakeIo {
            replies: vec![Ok("7651 REV1\n".into()), Ok("END\n".into())],
            ..FakeIo::default()
        };
        assert_eq!(
            identify(&mut legacy, Role::Source, "YOKOGAWA_7651").unwrap(),
            "7651 REV1"
        );
        assert_eq!(
            legacy.calls,
            vec![
                ("QUERY".into(), "OS;E\n".into()),
                ("READ".into(), "".into()),
            ]
        );
        assert_eq!(legacy.panel_deadlines.len(), 2);
        assert_eq!(legacy.panel_deadlines[0], legacy.panel_deadlines[1]);

        let mut adc = FakeIo {
            replies: vec![Ok("+1.0E-3".into()), Ok("+2.0E-3".into())],
            ..FakeIo::default()
        };
        let mut discard = true;
        assert_eq!(
            query(
                &mut adc,
                Role::Meter,
                "ADCMT_7461A",
                true,
                &mut discard,
                "READ?"
            )
            .unwrap(),
            "+2.0E-3"
        );
        assert!(!discard);
        assert_eq!(
            adc.calls,
            vec![("READ".into(), "".into()), ("READ".into(), "".into())]
        );
        assert_eq!(
            adc.waits,
            vec![Duration::from_millis(20), Duration::from_millis(20)]
        );
    }

    #[test]
    fn normalized_results_do_not_replace_raw_legacy_or_adc_exchanges() {
        let mut legacy = FakeIo {
            replies: vec![Ok("OD+1.23000E-3\r\n".into())],
            ..FakeIo::default()
        };
        let mut discard = false;
        assert_eq!(
            query(
                &mut legacy,
                Role::Source,
                "YOKOGAWA_7651",
                false,
                &mut discard,
                ":SOUR:LEV?"
            )
            .unwrap(),
            "+1.23000E-3"
        );
        assert_eq!(
            legacy.exchanges,
            vec![(
                "QUERY".into(),
                "OD;E\n".into(),
                Ok("OD+1.23000E-3\r\n".into())
            )]
        );

        let mut adc = FakeIo {
            replies: vec![Ok("NDCV+9.000E-6\n".into()), Ok("NDCV+1.23000E-6\n".into())],
            ..FakeIo::default()
        };
        let mut discard = true;
        assert_eq!(
            query(
                &mut adc,
                Role::Meter,
                "ADCMT_7461A",
                false,
                &mut discard,
                "READ?"
            )
            .unwrap(),
            "+1.23000E-6"
        );
        assert_eq!(
            adc.exchanges.len(),
            2,
            "discard and accepted ADC exchanges are retained"
        );
        assert_eq!(adc.exchanges[0].2, Ok("NDCV+9.000E-6\n".into()));
        assert_eq!(adc.exchanges[1].2, Ok("NDCV+1.23000E-6\n".into()));

        let mut failed = FakeIo {
            replies: vec![Err("mock transport failure".into())],
            ..FakeIo::default()
        };
        let mut discard = false;
        assert!(
            query(
                &mut failed,
                Role::Meter,
                "ADCMT_7461A",
                false,
                &mut discard,
                "READ?"
            )
            .is_err()
        );
        assert_eq!(failed.exchanges[0].2, Err("mock transport failure".into()));
    }

    #[test]
    fn injected_timeout_disconnect_and_malformed_reading_are_not_hidden() {
        let mut io = FakeIo {
            replies: vec![Err("Instrument helper timed out. Disconnect and reconnect the selected instrument before retrying.".into())],
            ..FakeIo::default()
        };
        let mut discard = false;
        assert!(
            query(
                &mut io,
                Role::Meter,
                "ADCMT_7461A",
                false,
                &mut discard,
                "READ?"
            )
            .unwrap_err()
            .contains("timed out")
        );

        let mut io = FakeIo {
            replies: vec![Err("cable disconnected".into())],
            ..FakeIo::default()
        };
        assert!(
            query(
                &mut io,
                Role::Meter,
                "ADCMT_7461A",
                false,
                &mut discard,
                "READ?"
            )
            .unwrap_err()
            .contains("cable disconnected")
        );

        let mut io = FakeIo {
            replies: vec![Ok("OVLD".into())],
            ..FakeIo::default()
        };
        assert!(
            query(
                &mut io,
                Role::Meter,
                "ADCMT_7461A",
                false,
                &mut discard,
                "READ?"
            )
            .unwrap_err()
            .contains("overload")
        );
    }

    #[test]
    fn legacy_panel_uses_one_deadline_and_propagates_continuation_timeout() {
        let mut io = FakeIo {
            replies: vec![
                Ok("7651 REV1\n".into()),
                Err("Instrument helper timed out. Disconnect and reconnect the selected instrument before retrying.".into()),
            ],
            ..FakeIo::default()
        };
        assert!(
            identify(&mut io, Role::Source, "YOKOGAWA_7651")
                .unwrap_err()
                .contains("timed out")
        );
        assert_eq!(io.panel_deadlines.len(), 2);
        assert_eq!(io.panel_deadlines[0], io.panel_deadlines[1]);
    }
}
