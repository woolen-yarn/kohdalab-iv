//! Verified fixed DC output for the ordinary Source roles.
//!
//! This deliberately follows the gate-bias safety contract, while retaining
//! the selected voltage/current function and its complementary protection unit.

use super::{Control, safety, workflow};
use crate::protocol::{DcOutputState, Snapshot};
use crate::state::OutputState;
use iv_core::{Mode, Pattern, RunConfig, plan, prepare_config, source_protection_profile};
use iv_transport::{DevicePair, Role};

fn role(secondary: bool) -> Role {
    if secondary {
        Role::Source2
    } else {
        Role::Source
    }
}

fn label(secondary: bool) -> &'static str {
    if secondary { "Source 2" } else { "Source" }
}

fn connected(state: &Snapshot, secondary: bool) -> bool {
    if secondary {
        state.source2_connected
    } else {
        state.source_connected
    }
}

fn simulation(state: &Snapshot, secondary: bool) -> bool {
    if secondary {
        state.source2_simulation
    } else {
        state.source_simulation
    }
}

fn output(state: &Snapshot, secondary: bool) -> OutputState {
    if secondary {
        state.source2_output
    } else {
        state.output
    }
}

fn set_output(state: &mut Snapshot, secondary: bool, value: OutputState) {
    if secondary {
        state.source2_output = value;
    } else {
        state.output = value;
    }
}

fn set_dc(state: &mut Snapshot, secondary: bool, value: Option<DcOutputState>) {
    if secondary {
        state.source2_dc = value;
    } else {
        state.source_dc = value;
    }
}

fn model(pair: &DevicePair, secondary: bool) -> &str {
    if secondary {
        &pair.source2_model
    } else {
        &pair.source_model
    }
}

fn identity(pair: &DevicePair, secondary: bool) -> &str {
    if secondary {
        &pair.source2_identity
    } else {
        &pair.source_identity
    }
}

fn finite(value: f64, name: &str) -> Result<(), String> {
    if value.is_finite() {
        Ok(())
    } else {
        Err(format!("{name} must be finite."))
    }
}

fn positive(value: f64, name: &str) -> Result<(), String> {
    finite(value, name)?;
    if value > 0.0 {
        Ok(())
    } else {
        Err(format!("{name} must be positive."))
    }
}

fn same(left: f64, right: f64) -> bool {
    (left - right).abs() <= left.abs().max(right.abs()).max(1.0) * 1e-9
}

fn ensure_not_cancelled(control: &Control) -> Result<(), String> {
    if control.cancelled() {
        Err("DC output operation cancelled before changing output.".into())
    } else {
        Ok(())
    }
}

fn dc_plan(
    pair: &DevicePair,
    secondary: bool,
    mode: Mode,
    value: f64,
    compliance: f64,
    ramp_step: f64,
) -> Result<(RunConfig, iv_core::Plan), String> {
    finite(value, &format!("{} DC value", label(secondary)))?;
    positive(compliance, "Source protection limit")?;
    positive(ramp_step, "DC ramp step")?;
    if secondary && mode != Mode::Iv {
        return Err("Source 2 manual DC supports voltage source mode only.".into());
    }
    let config = RunConfig {
        source_model: model(pair, secondary).into(),
        mode,
        pattern: Pattern::Linear,
        start: 0.0,
        stop: value,
        step: ramp_step,
        source_limit: value.abs().max(f64::MIN_POSITIVE),
        // There is no meter acquisition for DC mode. This satisfies shared
        // planner validation without changing the source protection request.
        compliance,
        source_compliance: Some(compliance),
        ramp_step,
        ..RunConfig::default()
    };
    let prepared = prepare_config(&config)?;
    if !same(prepared.step.abs(), ramp_step) {
        return Err(format!(
            "DC ramp step {ramp_step:e} is not on the selected {} range grid. Choose {:.12e} or another exact grid increment.",
            model(pair, secondary),
            prepared.step.abs()
        ));
    }
    let planned = plan(&config)?;
    if planned.hardware_compliance > compliance + f64::EPSILON {
        return Err(format!(
            "Source protection limit {compliance:e} cannot be represented without increasing protection on {}.",
            model(pair, secondary)
        ));
    }
    Ok((prepared, planned))
}

fn force_off(
    pair: Option<&mut DevicePair>,
    state: &mut Snapshot,
    secondary: bool,
) -> Result<(), String> {
    if simulation(state, secondary) {
        set_output(state, secondary, OutputState::Off);
        set_dc(state, secondary, None);
        return Ok(());
    }
    let pair = pair.ok_or_else(|| format!("{} transport is unavailable.", label(secondary)))?;
    let report = pair.force_output_off_role(role(secondary))?;
    if !report.output_off_verified {
        set_output(state, secondary, OutputState::Unknown);
        return Err(format!(
            "{} output OFF could not be verified.",
            label(secondary)
        ));
    }
    set_output(state, secondary, OutputState::Off);
    set_dc(state, secondary, None);
    if let Some(warning) = report.zeroing_warning {
        state
            .warnings
            .push(format!("{} zeroing warning: {warning}", label(secondary)));
    }
    Ok(())
}

fn fail_closed(
    pair: Option<&mut DevicePair>,
    state: &mut Snapshot,
    secondary: bool,
    error: String,
) -> Result<(), String> {
    match force_off(pair, state, secondary) {
        Ok(()) => Err(error),
        Err(off_error) => {
            set_output(state, secondary, OutputState::Unknown);
            set_dc(state, secondary, None);
            Err(format!(
                "{error} {} output OFF also failed: {off_error}",
                label(secondary)
            ))
        }
    }
}

fn verify_identity(pair: &mut DevicePair, secondary: bool) -> Result<(), String> {
    let expected = identity(pair, secondary).trim().to_owned();
    let actual = pair.query(role(secondary), "*IDN?")?;
    if actual.trim() == expected {
        Ok(())
    } else {
        Err(format!(
            "{} instrument identity changed. Disconnect and select the correct resource.",
            label(secondary)
        ))
    }
}

fn check_limiter(pair: &mut DevicePair, secondary: bool) -> Result<(), String> {
    let Some(profile) = source_protection_profile(model(pair, secondary))? else {
        return Ok(());
    };
    let condition =
        safety::source_condition(&pair.query(role(secondary), profile.query)?, profile)?;
    if condition.stopped() {
        Err(format!(
            "{} protection reached ({}) at condition register {}.",
            label(secondary),
            condition.indicator(),
            condition.register
        ))
    } else {
        Ok(())
    }
}

fn verify_error_queue_clear(pair: &mut DevicePair, secondary: bool) -> Result<(), String> {
    let error = pair.query(role(secondary), "SYST:ERR?")?;
    let code = error.trim().split(',').next().unwrap_or_default().trim();
    if matches!(code, "0" | "+0" | "-0") {
        Ok(())
    } else {
        Err(format!(
            "{} reported an instrument error: {error}",
            label(secondary)
        ))
    }
}

fn verify_settings(
    pair: &mut DevicePair,
    secondary: bool,
    mode: Mode,
    planned: &iv_core::Plan,
) -> Result<f64, String> {
    let role = role(secondary);
    safety::verify_function(&pair.query(role, ":SOUR:FUNC?")?, mode)?;
    let source_model = model(pair, secondary).to_owned();
    let range_query = iv_core::source_range_query(&source_model)?.unwrap_or(":SOUR:RANG?");
    let resolution = safety::verify_range(
        &source_model,
        mode,
        planned.source_range,
        &pair.query(role, range_query)?,
    )?;
    if source_protection_profile(&source_model)?.is_some() {
        let protection = if mode == Mode::Iv {
            ":SOUR:PROT:CURR?"
        } else {
            ":SOUR:PROT:VOLT?"
        };
        safety::verify_hardware_limit(planned.hardware_compliance, &pair.query(role, protection)?)?;
    }
    Ok(resolution)
}

fn set_level(pair: &mut DevicePair, secondary: bool, level: f64) -> Result<(), String> {
    pair.write(role(secondary), &format!(":SOUR:LEV:FIX {level:.12e}"))?;
    check_limiter(pair, secondary)
}

pub(super) fn set(
    pair: Option<&mut DevicePair>,
    state: &mut Snapshot,
    secondary: bool,
    request: DcOutputState,
    control: &Control,
) -> Result<(), String> {
    let DcOutputState {
        mode,
        value,
        compliance,
        ramp_step,
    } = request;
    if !connected(state, secondary) {
        return Err(format!(
            "Connect {} before enabling DC output.",
            label(secondary)
        ));
    }
    if !state.error.is_empty() {
        return Err(
            "Resolve the previous error and reconnect before enabling DC output again.".into(),
        );
    }
    if simulation(state, secondary) {
        finite(value, "DC value")?;
        positive(compliance, "Source protection limit")?;
        positive(ramp_step, "DC ramp step")?;
        if secondary && mode != Mode::Iv {
            return Err("Source 2 manual DC supports voltage source mode only.".into());
        }
        ensure_not_cancelled(control)?;
        set_output(state, secondary, OutputState::On);
        set_dc(
            state,
            secondary,
            Some(DcOutputState {
                mode,
                value,
                compliance,
                ramp_step,
            }),
        );
        return Ok(());
    }
    let pair = pair.ok_or_else(|| format!("{} transport is unavailable.", label(secondary)))?;
    if !pair.is_connected(role(secondary)) || identity(pair, secondary).trim().is_empty() {
        return Err(format!(
            "{} identity is unavailable. Disconnect and reconnect it.",
            label(secondary)
        ));
    }
    let (_prepared, planned) = match dc_plan(pair, secondary, mode, value, compliance, ramp_step) {
        Ok(value) => value,
        Err(error) => return fail_closed(Some(pair), state, secondary, error),
    };
    let was_on = output(state, secondary) == OutputState::On;
    let operation = (|| -> Result<(), String> {
        ensure_not_cancelled(control)?;
        verify_identity(pair, secondary)?;
        let resolution = if was_on {
            let previous = if secondary {
                state.source2_dc.as_ref()
            } else {
                state.source_dc.as_ref()
            };
            if previous.is_some_and(|previous| {
                previous.mode != mode || !same(previous.compliance, planned.hardware_compliance)
            }) {
                return Err(format!(
                    "Changing {} DC function or protection while output is on is unsafe. Turn DC output off first.",
                    label(secondary)
                ));
            }
            verify_settings(pair, secondary, mode, &planned)?
        } else {
            force_off(Some(pair), state, secondary)?;
            let definition = iv_core::measurement_definition(mode);
            ensure_not_cancelled(control)?;
            pair.write(role(secondary), "*CLS")?;
            ensure_not_cancelled(control)?;
            pair.write(
                role(secondary),
                &format!(":SOUR:FUNC {}", definition.source_function),
            )?;
            ensure_not_cancelled(control)?;
            pair.write(
                role(secondary),
                &format!(":SOUR:RANG {:.12e}", planned.source_range),
            )?;
            ensure_not_cancelled(control)?;
            pair.write(
                role(secondary),
                &format!(
                    ":SOUR:PROT:{} {:.12e}",
                    definition.protection_function, planned.hardware_compliance
                ),
            )?;
            ensure_not_cancelled(control)?;
            pair.write(role(secondary), ":SOUR:LEV:FIX 0")?;
            let resolution = verify_settings(pair, secondary, mode, &planned)?;
            let zero = pair
                .query(role(secondary), ":SOUR:LEV?")?
                .trim()
                .parse::<f64>()
                .map_err(|_| format!("{} zero level could not be read back.", label(secondary)))?;
            safety::verify_zero(zero, resolution)?;
            check_limiter(pair, secondary)?;
            ensure_not_cancelled(control)?;
            pair.write(role(secondary), ":OUTP ON")?;
            if safety::output_state(&pair.query(role(secondary), ":OUTP?")?)? != OutputState::On {
                return Err(format!("{} did not confirm output ON.", label(secondary)));
            }
            resolution
        };
        let mut current = if was_on {
            pair.query(role(secondary), ":SOUR:LEV?")?
                .trim()
                .parse::<f64>()
                .map_err(|_| format!("{} level could not be read back.", label(secondary)))?
        } else {
            0.0
        };
        if !current.is_finite() {
            return Err(format!(
                "{} level readback is not finite.",
                label(secondary)
            ));
        }
        if !workflow::ramp(
            &mut current,
            value,
            ramp_step,
            workflow::RampPolicy::ApproachTarget,
            || control.cancelled(),
            |level| {
                ensure_not_cancelled(control)?;
                set_level(pair, secondary, level)?;
                Ok(true)
            },
        )? {
            return Err("DC output operation cancelled during ramp.".into());
        }
        ensure_not_cancelled(control)?;
        set_level(pair, secondary, value)?;
        let actual = pair
            .query(role(secondary), ":SOUR:LEV?")?
            .trim()
            .parse::<f64>()
            .map_err(|_| format!("{} final level could not be read back.", label(secondary)))?;
        if !actual.is_finite() || (actual - value).abs() > resolution.abs() * 0.5 + 1e-12 {
            return Err(format!(
                "{} final level did not match the requested value.",
                label(secondary)
            ));
        }
        if safety::output_state(&pair.query(role(secondary), ":OUTP?")?)? != OutputState::On {
            return Err(format!(
                "{} output was not ON after setting DC level.",
                label(secondary)
            ));
        }
        verify_error_queue_clear(pair, secondary)?;
        check_limiter(pair, secondary)
    })();
    match operation {
        Ok(()) => {
            set_output(state, secondary, OutputState::On);
            set_dc(
                state,
                secondary,
                Some(DcOutputState {
                    mode,
                    value,
                    compliance: planned.hardware_compliance,
                    ramp_step,
                }),
            );
            Ok(())
        }
        Err(error) => fail_closed(Some(pair), state, secondary, error),
    }
}

pub(super) fn output_off(
    pair: Option<&mut DevicePair>,
    state: &mut Snapshot,
    secondary: bool,
) -> Result<(), String> {
    if !connected(state, secondary) {
        return Err(format!("{} is not connected.", label(secondary)));
    }
    force_off(pair, state, secondary)
}

/// Idle DC protection polling. Any query failure or latched limiter fails
/// closed for the affected source before the engine reports the error.
pub(super) fn check(
    pair: Option<&mut DevicePair>,
    state: &mut Snapshot,
    secondary: bool,
) -> Result<(), String> {
    if !connected(state, secondary)
        || simulation(state, secondary)
        || output(state, secondary) != OutputState::On
    {
        return Ok(());
    }
    let pair = pair.ok_or_else(|| format!("{} transport is unavailable.", label(secondary)))?;
    if safety::output_state(&pair.query(role(secondary), ":OUTP?")?)? != OutputState::On {
        return fail_closed(
            Some(pair),
            state,
            secondary,
            format!("{} output unexpectedly turned off.", label(secondary)),
        );
    }
    if let Err(error) =
        verify_error_queue_clear(pair, secondary).and_then(|()| check_limiter(pair, secondary))
    {
        return fail_closed(Some(pair), state, secondary, error);
    }
    Ok(())
}
