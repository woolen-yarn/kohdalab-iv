//! Normal sweep cleanup and the transition to verified output OFF.
use super::*;

const ZERO_RETURN_BUDGET: Duration = Duration::from_secs(10);
const ZERO_RETURN_WARNING: &str =
    "Zero return reached the 10-second limit. Output OFF verification follows.";

pub(super) fn clear_timeout_warning(state: &mut Snapshot) {
    state
        .warnings
        .retain(|warning| warning != ZERO_RETURN_WARNING);
}

pub(super) enum ZeroReturn<'a> {
    Single {
        config: &'a RunConfig,
        level: f64,
    },
    Balanced {
        config: &'a RunConfig,
        plan: &'a iv_core::BalancedPlan,
        levels: [f64; 2],
    },
}

#[derive(serde::Serialize)]
pub(super) struct ZeroReturnTimeout {
    last_programmed_levels: ProgrammedLevels,
}

#[derive(serde::Serialize)]
struct ProgrammedLevels {
    source: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    source2: Option<f64>,
}

impl ZeroReturn<'_> {
    fn run(
        self,
        pair: &mut DevicePair,
        control: &Control,
    ) -> Result<Option<ZeroReturnTimeout>, String> {
        let deadline = Instant::now() + ZERO_RETURN_BUDGET;
        let config = match &self {
            Self::Single { config, .. } | Self::Balanced { config, .. } => *config,
        };
        let wait = Duration::from_secs_f64(config.ramp_wait_s.min(0.1));
        let pause =
            || std::thread::sleep(wait.min(deadline.saturating_duration_since(Instant::now())));
        let cancelled = || control.immediate_stop() || Instant::now() >= deadline;
        let timed_out = |source, source2: Option<f64>| {
            (!control.immediate_stop() && Instant::now() >= deadline).then_some(ZeroReturnTimeout {
                last_programmed_levels: ProgrammedLevels {
                    source: commanded_value(source),
                    source2: source2.map(commanded_value),
                },
            })
        };
        match self {
            Self::Single { mut level, .. } => {
                let completed = workflow::ramp(
                    &mut level,
                    0.0,
                    config.ramp_step,
                    RampPolicy::ReturnToZero,
                    cancelled,
                    |next| {
                        set_level(pair, next)?;
                        pause();
                        Ok(true)
                    },
                )?;
                Ok(if completed {
                    None
                } else {
                    timed_out(level, None)
                })
            }
            Self::Balanced {
                plan, mut levels, ..
            } => {
                for next in plan.zero_return_targets(levels, config.ramp_step)? {
                    if cancelled() {
                        return Ok(timed_out(levels[0], Some(levels[1])));
                    }
                    let [first, second] = next?;
                    set_level(pair, first)?;
                    // A cutoff or emergency between writes takes priority over
                    // finishing the pair. Preserve each last programmed level.
                    if cancelled() {
                        return Ok(timed_out(first, Some(levels[1])));
                    }
                    pair.write(Role::Source2, &format!(":SOUR:LEV:FIX {second:.12e}"))?;
                    levels = [first, second];
                    pause();
                }
                Ok(None)
            }
        }
    }
}

// Match the numeric value sent in the source level command.
fn commanded_value(level: f64) -> f64 {
    format!("{level:.12e}")
        .parse()
        .expect("formatted finite level")
}

pub(super) fn log_timeout(
    log: &mut RunLog,
    timeout: Option<&ZeroReturnTimeout>,
) -> Result<(), String> {
    if let Some(timeout) = timeout {
        log.append("WARN", "zero_return_timeout", serde_json::json!(timeout))?;
    }
    Ok(())
}

pub(super) fn sweep(
    mut pair: Option<&mut DevicePair>,
    state: &mut Snapshot,
    ramp: Option<ZeroReturn<'_>>,
    control: &Control,
) -> (Option<ZeroReturnTimeout>, Result<(), String>) {
    state.phase = Phase::CleaningUp;
    state.output = OutputState::Unknown;
    let mut timeout = None;
    let mut errors = Vec::new();
    if let (Some(ramp), Some(pair)) = (ramp, pair.as_deref_mut())
        && !control.immediate_stop()
    {
        state.activity = Activity::ReturningToZero;
        match ramp.run(pair, control) {
            Ok(Some(event)) => {
                state.warnings.push(ZERO_RETURN_WARNING.into());
                timeout = Some(event);
            }
            Ok(None) => {}
            Err(error) => errors.push(error),
        }
    }

    // Always attempt verified OFF before persisting any ramp evidence.
    state.activity = Activity::VerifyingOutputOff;
    if let Err(error) = verify_sweep_off(pair, state) {
        errors.push(error);
    }
    let result = if errors.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "{} {}",
            errors.join(" · "),
            if state.output == OutputState::Off {
                "Output off was verified."
            } else {
                "Check the instrument output manually."
            }
        ))
    };
    (timeout, result)
}

fn verify_sweep_off(pair: Option<&mut DevicePair>, state: &mut Snapshot) -> Result<(), String> {
    if state.source2_connected {
        return shutdown_balanced(pair, state);
    }
    if !state.source_connected {
        return Err("Connect a source before using Output Off.".into());
    }
    if !state.source_simulation {
        let pair = pair.ok_or("Source transport is unavailable.")?;
        let report = pair.force_output_off()?;
        if !report.output_off_verified {
            return Err("Source output off was not verified.".into());
        }
        if let Some(warning) = report.zeroing_warning {
            state.warnings.push(format!(
                "Output off was verified; resetting the source level to zero failed: {warning}"
            ));
        }
    }
    state.output = OutputState::Off;
    state.source_dc = None;
    state.source_dc_limits = None;
    Ok(())
}

pub(super) fn shutdown_balanced(
    pair: Option<&mut DevicePair>,
    state: &mut Snapshot,
) -> Result<(), String> {
    state.output = OutputState::Unknown;
    if state.source2_connected {
        state.source2_output = OutputState::Unknown;
    }
    let mut verified = [!state.source_connected, !state.source2_connected];
    if state.source_simulation && state.source_connected {
        verified[0] = true;
    }
    if state.source2_simulation && state.source2_connected {
        verified[1] = true;
        state.source2_output = OutputState::Off;
    }
    let mut errors = Vec::new();
    if let Some(pair) = pair {
        for (role, result) in pair.force_sources_output_off() {
            let index = if role == Role::Source { 0 } else { 1 };
            match result {
                Ok(report) if report.output_off_verified => {
                    verified[index] = true;
                    if index == 1 {
                        state.source2_output = OutputState::Off;
                    }
                    if let Some(warning) = report.zeroing_warning {
                        state.warnings.push(format!(
                            "{} output OFF verified; {warning}",
                            role_label(role)
                        ));
                    }
                }
                Ok(_) => errors.push(format!("{} output OFF not verified", role_label(role))),
                Err(error) => errors.push(format!("{}: {error}", role_label(role))),
            }
        }
    }
    if verified.into_iter().all(|v| v) {
        state.output = OutputState::Off;
        state.source2_level = Some(0.0);
        state.source_dc = None;
        state.source2_dc = None;
        state.source_dc_limits = None;
        state.source2_dc_limits = None;
    } else if errors.is_empty() {
        errors.push("Not all sweep sources could be verified OFF. Check both instruments.".into());
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join(" · "))
    }
}

// Protection is deliberately separate from normal Stop: no ramp or unrelated
// instrument queries may precede the source OFF request after a limit trip.
pub(super) fn protection_off(
    pair: Option<&mut DevicePair>,
    state: &mut Snapshot,
) -> Result<(), String> {
    state.activity = Activity::VerifyingOutputOff;
    state.output = OutputState::Unknown;
    if state.source2_connected {
        return shutdown_balanced(pair, state);
    }
    if state.source_simulation {
        state.output = OutputState::Off;
        return Ok(());
    }
    let report = pair
        .ok_or("Source transport unavailable during protection stop")?
        .force_output_off()?;
    if !report.output_off_verified {
        return Err("Protection stop could not verify source output OFF".into());
    }
    state.output = OutputState::Off;
    if let Some(warning) = report.zeroing_warning {
        state.warnings.push(format!(
            "Protection stop: output OFF verified; zeroing warning: {warning}"
        ));
    }
    Ok(())
}
