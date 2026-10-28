//! Fixed gate-bias control. The gate is intentionally independent from the
//! IV source roles: it can remain enabled while an IV sweep runs, but every
//! gate fault is handled locally and fails closed.

use super::{Control, safety, workflow};
use crate::protocol::Snapshot;
use crate::state::OutputState;
use iv_core::{Mode, Pattern, RunConfig, plan, prepare_config, source_protection_profile};
use iv_transport::{DevicePair, Role};

fn finite(value: f64, name: &str) -> Result<(), String> {
    if value.is_finite() {
        Ok(())
    } else {
        Err(format!("Gate {name} must be finite."))
    }
}

fn finite_positive(value: f64, name: &str) -> Result<(), String> {
    finite(value, name)?;
    if value > 0.0 {
        Ok(())
    } else {
        Err(format!("Gate {name} must be positive."))
    }
}

fn same_setting(left: f64, right: f64) -> bool {
    (left - right).abs() <= left.abs().max(right.abs()).max(1.0) * 1e-9
}

fn ensure_not_cancelled(control: &Control) -> Result<(), String> {
    if control.cancelled() {
        Err("Gate operation cancelled before changing output.".into())
    } else {
        Ok(())
    }
}

/// Validate a gate setting through the ordinary single-source planner.  The
/// meter values are placeholders only; no meter is contacted by this module.
fn gate_plan(
    pair: &DevicePair,
    voltage: f64,
    compliance: f64,
    ramp_step: f64,
) -> Result<(RunConfig, iv_core::Plan), String> {
    finite(voltage, "voltage")?;
    finite_positive(compliance, "compliance")?;
    finite_positive(ramp_step, "ramp step")?;
    let config = RunConfig {
        source_model: pair.gate_model.clone(),
        mode: Mode::Iv,
        pattern: Pattern::Linear,
        start: 0.0,
        stop: voltage,
        step: ramp_step,
        // `source_limit` is differential only for balanced IV. Here it is
        // the requested gate voltage bound and exists solely for core safety
        // validation; the target itself is never rounded.
        source_limit: voltage.abs().max(f64::MIN_POSITIVE),
        compliance: 1e-3,
        source_compliance: Some(compliance),
        ramp_step,
        ..RunConfig::default()
    };
    let prepared = prepare_config(&config)?;
    if !same_setting(prepared.step.abs(), ramp_step) {
        return Err(format!(
            "Gate ramp step {ramp_step:e} V is not on the selected {} range grid. Choose {:.12e} V or another exact grid increment.",
            pair.gate_model,
            prepared.step.abs()
        ));
    }
    let planned = plan(&config)?;
    if planned.hardware_compliance > compliance + f64::EPSILON {
        return Err(format!(
            "Gate compliance {compliance:e} A cannot be represented without increasing protection on {}. Choose the reported hardware increment before enabling output.",
            pair.gate_model
        ));
    }
    Ok((prepared, planned))
}

fn force_off(pair: Option<&mut DevicePair>, state: &mut Snapshot) -> Result<(), String> {
    if state.gate_simulation {
        state.gate_output = OutputState::Off;
        state.gate_voltage = Some(0.0);
        state.gate_revision = state.gate_revision.wrapping_add(1);
        return Ok(());
    }
    let pair = pair.ok_or("Gate transport is unavailable.")?;
    let report = pair.force_output_off_role(Role::Gate)?;
    if !report.output_off_verified {
        state.gate_output = OutputState::Unknown;
        return Err("Gate output OFF could not be verified.".into());
    }
    state.gate_output = OutputState::Off;
    state.gate_voltage = Some(0.0);
    state.gate_revision = state.gate_revision.wrapping_add(1);
    if let Some(warning) = report.zeroing_warning {
        state
            .warnings
            .push(format!("Gate zeroing warning: {warning}"));
    }
    Ok(())
}

fn fail_closed(
    pair: Option<&mut DevicePair>,
    state: &mut Snapshot,
    error: String,
) -> Result<(), String> {
    match force_off(pair, state) {
        Ok(()) => Err(error),
        Err(off_error) => {
            state.gate_output = OutputState::Unknown;
            Err(format!("{error} Gate output OFF also failed: {off_error}"))
        }
    }
}

fn verify_identity(pair: &mut DevicePair) -> Result<(), String> {
    let expected = pair.gate_identity.trim().to_owned();
    let actual = pair.query(Role::Gate, "*IDN?")?;
    if actual.trim() == expected {
        Ok(())
    } else {
        Err("Gate instrument identity changed. Disconnect and select the correct resource.".into())
    }
}

fn verify_existing_settings(pair: &mut DevicePair, planned: &iv_core::Plan) -> Result<f64, String> {
    safety::verify_function(&pair.query(Role::Gate, ":SOUR:FUNC?")?, Mode::Iv)?;
    let model = pair.gate_model.clone();
    let range_query = iv_core::source_range_query(&model)?.unwrap_or(":SOUR:RANG?");
    let range_reply = pair.query(Role::Gate, range_query)?;
    let resolution = safety::verify_range(&model, Mode::Iv, planned.source_range, &range_reply)?;
    if source_protection_profile(&model)?.is_some() {
        safety::verify_hardware_limit(
            planned.hardware_compliance,
            &pair.query(Role::Gate, ":SOUR:PROT:CURR?")?,
        )?;
    }
    Ok(resolution)
}

fn check_limiter(pair: &mut DevicePair) -> Result<(), String> {
    let Some(profile) = source_protection_profile(&pair.gate_model)? else {
        return Ok(());
    };
    let condition = safety::source_condition(&pair.query(Role::Gate, profile.query)?, profile)?;
    if condition.stopped() {
        Err(format!(
            "Gate source compliance reached ({}) at condition register {}.",
            condition.indicator(),
            condition.register
        ))
    } else {
        Ok(())
    }
}

fn set_level(pair: &mut DevicePair, level: f64) -> Result<(), String> {
    pair.write(Role::Gate, &format!(":SOUR:LEV:FIX {level:.12e}"))?;
    check_limiter(pair)
}

fn verify_error_queue_clear(pair: &mut DevicePair) -> Result<(), String> {
    let error = pair.query(Role::Gate, "SYST:ERR?")?;
    let code = error.trim().split(',').next().unwrap_or_default().trim();
    if code == "0" || code == "+0" || code == "-0" {
        Ok(())
    } else {
        Err(format!("Gate reported an instrument error: {error}"))
    }
}

/// Confirm the final physical state before publishing a gate voltage to
/// callers.  A successful write only acknowledges transport; it is not proof
/// of the applied level, enabled output, or an empty instrument error queue.
fn verify_final_level(pair: &mut DevicePair, expected: f64, resolution: f64) -> Result<(), String> {
    let actual = pair
        .query(Role::Gate, ":SOUR:LEV?")?
        .trim()
        .parse::<f64>()
        .map_err(|_| "Gate final level could not be read back as a number.")?;
    if !actual.is_finite() {
        return Err("Gate final level readback is not finite.".into());
    }
    // The planner has already established the command grid.  Allow at most
    // half a hardware step plus a tiny floating-point margin in readback.
    if (actual - expected).abs() > resolution.abs() * 0.5 + 1e-12 {
        return Err(format!(
            "Gate final level did not match the requested value (requested {expected:.12e} V, read {actual:.12e} V)."
        ));
    }
    if safety::output_state(&pair.query(Role::Gate, ":OUTP?")?)? != OutputState::On {
        return Err("Gate output was not ON after setting the final level.".into());
    }
    verify_error_queue_clear(pair)?;
    check_limiter(pair)
}

fn ensure_gate(pair: &mut DevicePair, state: &Snapshot) -> Result<(), String> {
    if !state.gate_connected || !pair.is_connected(Role::Gate) {
        return Err("Connect the gate source before setting gate voltage.".into());
    }
    if pair.gate_identity.trim().is_empty() {
        return Err("Gate identity is unavailable. Disconnect and reconnect the gate.".into());
    }
    Ok(())
}

/// Set a fixed external gate voltage. It never reads a leakage current: a
/// `/MON` terminal is not required for the documented condition-status check.
pub(super) fn set_voltage(
    pair: Option<&mut DevicePair>,
    state: &mut Snapshot,
    voltage: f64,
    compliance: f64,
    ramp_step: f64,
    control: &Control,
) -> Result<(), String> {
    // Simulation is a transport implementation, not a virtual connection:
    // after DisconnectGate it must not be possible to recreate an ON state.
    if !state.gate_connected {
        return Err("Connect the gate source before setting gate voltage.".into());
    }
    if !state.error.is_empty() {
        return Err(
            "Resolve the previous error and reconnect the gate before enabling it again.".into(),
        );
    }
    if state.gate_simulation {
        finite(voltage, "voltage")?;
        finite_positive(compliance, "compliance")?;
        finite_positive(ramp_step, "ramp step")?;
        ensure_not_cancelled(control)?;
        state.gate_output = OutputState::On;
        state.gate_voltage = Some(voltage);
        state.gate_compliance = Some(compliance);
        state.gate_revision = state.gate_revision.wrapping_add(1);
        return Ok(());
    }

    let pair = pair.ok_or("Gate transport is unavailable.")?;
    ensure_gate(pair, state)?;
    let (_prepared, planned) = match gate_plan(pair, voltage, compliance, ramp_step) {
        Ok(value) => value,
        Err(error) => return fail_closed(Some(pair), state, error),
    };
    let was_on = state.gate_output == OutputState::On;
    let operation = (|| -> Result<(), String> {
        ensure_not_cancelled(control)?;
        verify_identity(pair)?;
        let resolution = if was_on {
            if state
                .gate_compliance
                .is_some_and(|previous| !same_setting(previous, planned.hardware_compliance))
            {
                return Err("Changing gate compliance while output is on is unsafe. Turn Gate Output Off before changing compliance or range.".into());
            }
            verify_existing_settings(pair, &planned)?
        } else {
            // This verifies OFF + zero before configuration and output ON.
            force_off(Some(pair), state)?;
            ensure_not_cancelled(control)?;
            pair.write(Role::Gate, "*CLS")?;
            ensure_not_cancelled(control)?;
            pair.write(Role::Gate, ":SOUR:FUNC VOLT")?;
            ensure_not_cancelled(control)?;
            pair.write(
                Role::Gate,
                &format!(":SOUR:RANG {:.12e}", planned.source_range),
            )?;
            ensure_not_cancelled(control)?;
            pair.write(
                Role::Gate,
                &format!(":SOUR:PROT:CURR {:.12e}", planned.hardware_compliance),
            )?;
            ensure_not_cancelled(control)?;
            pair.write(Role::Gate, ":SOUR:LEV:FIX 0")?;
            ensure_not_cancelled(control)?;
            let resolution = verify_existing_settings(pair, &planned)?;
            let zero = pair.query(Role::Gate, ":SOUR:LEV?")?;
            let zero = zero
                .trim()
                .parse::<f64>()
                .map_err(|_| "Gate zero level could not be read back as a number.")?;
            safety::verify_zero(zero, resolution)?;
            check_limiter(pair)?;
            ensure_not_cancelled(control)?;
            pair.write(Role::Gate, ":OUTP ON")?;
            ensure_not_cancelled(control)?;
            if safety::output_state(&pair.query(Role::Gate, ":OUTP?")?)? != OutputState::On {
                return Err("Gate did not confirm output ON.".into());
            }
            resolution
        };

        let mut current = if was_on {
            let readback = pair
                .query(Role::Gate, ":SOUR:LEV?")?
                .trim()
                .parse::<f64>()
                .map_err(|_| "Gate level could not be read back as a number.")?;
            if !readback.is_finite() {
                return Err("Gate level readback is not finite.".into());
            }
            readback
        } else {
            0.0
        };
        if !workflow::ramp(
            &mut current,
            voltage,
            ramp_step,
            workflow::RampPolicy::ApproachTarget,
            || control.cancelled(),
            |level| {
                ensure_not_cancelled(control)?;
                set_level(pair, level)?;
                Ok(true)
            },
        )? {
            return Err("Gate operation cancelled during ramp.".into());
        }
        ensure_not_cancelled(control)?;
        set_level(pair, voltage)?;
        verify_final_level(pair, voltage, resolution)?;
        Ok(())
    })();
    match operation {
        Ok(()) => {
            state.gate_output = OutputState::On;
            state.gate_voltage = Some(voltage);
            state.gate_compliance = Some(planned.hardware_compliance);
            state.gate_revision = state.gate_revision.wrapping_add(1);
            Ok(())
        }
        Err(error) => fail_closed(Some(pair), state, error),
    }
}

/// Explicitly turn the gate off and verify it. This does not affect either IV
/// source role.
pub(super) fn output_off(
    pair: Option<&mut DevicePair>,
    state: &mut Snapshot,
) -> Result<(), String> {
    if !state.gate_connected {
        return Err("Gate source is not connected.".into());
    }
    force_off(pair, state)
}

/// Poll the reviewed, non-clearing source condition register. On a limiter,
/// emergency, or query failure the gate is shut down before reporting failure.
pub(super) fn check(pair: Option<&mut DevicePair>, state: &mut Snapshot) -> Result<(), String> {
    if !state.gate_connected || state.gate_simulation || state.gate_output != OutputState::On {
        return Ok(());
    }
    let pair = pair.ok_or("Gate transport is unavailable.")?;
    if safety::output_state(&pair.query(Role::Gate, ":OUTP?")?)? != OutputState::On {
        return fail_closed(
            Some(pair),
            state,
            "Gate output unexpectedly turned off during the IV run.".into(),
        );
    }
    if let Err(error) = verify_error_queue_clear(pair) {
        return fail_closed(Some(pair), state, error);
    }
    if let Err(error) = check_limiter(pair) {
        return fail_closed(Some(pair), state, error);
    }
    Ok(())
}
