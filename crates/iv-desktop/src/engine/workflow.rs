//! Shared source progression and completion rules for measurement execution.
//! Emergency shutdown stays in the transport: it must not wait behind a ramp.
use crate::state::Phase;

#[derive(Clone, Copy)]
pub(super) enum RampPolicy {
    ApproachTarget,
    ReturnToZero,
}

/// Calls `apply` in the existing order. The sweep's final exact target write is
/// performed by the caller, including when no intermediate ramp step is needed.
pub(super) fn ramp(
    level: &mut f64,
    target: f64,
    step: f64,
    policy: RampPolicy,
    mut stop: impl FnMut() -> bool,
    mut apply: impl FnMut(f64) -> Result<bool, String>,
) -> Result<bool, String> {
    while match policy {
        RampPolicy::ApproachTarget => (*level - target).abs() > step,
        RampPolicy::ReturnToZero => *level != 0.0,
    } {
        if stop() {
            return Ok(false);
        }
        *level = match policy {
            RampPolicy::ApproachTarget => *level + (target - *level).signum() * step,
            RampPolicy::ReturnToZero => {
                if level.abs() <= step {
                    0.0
                } else {
                    *level - level.signum() * step
                }
            }
        };
        if !apply(*level)? {
            return Ok(false);
        }
    }
    Ok(true)
}

pub(super) fn completion_phase(
    failed: bool,
    cleanup_failed: bool,
    compliance: bool,
    stopped: bool,
) -> Phase {
    if failed || cleanup_failed {
        Phase::Failed
    } else if compliance {
        Phase::ComplianceStop
    } else if stopped {
        Phase::Stopped
    } else {
        Phase::Completed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ramp_preserves_intermediate_levels_and_leaves_exact_target_to_caller() {
        for (start, target) in [(0.0, 1.0), (1.0, -1.0), (-1.0, 0.0)] {
            let mut expected = Vec::new();
            let mut original: f64 = start;
            while (original - target).abs() > 0.25 {
                original += (target - original).signum() * 0.25;
                expected.push(original.to_bits());
            }
            let mut actual = Vec::new();
            let mut level = start;
            assert!(
                ramp(
                    &mut level,
                    target,
                    0.25,
                    RampPolicy::ApproachTarget,
                    || false,
                    |v| {
                        actual.push(v.to_bits());
                        Ok(true)
                    }
                )
                .unwrap()
            );
            assert_eq!(actual, expected);
            assert_eq!(level.to_bits(), original.to_bits());
        }
    }
    #[test]
    fn shutdown_ramp_retains_zero_step_and_stops_immediately_on_emergency_or_io_error() {
        let mut level = 0.6;
        let mut trace = Vec::new();
        ramp(
            &mut level,
            0.0,
            0.25,
            RampPolicy::ReturnToZero,
            || false,
            |v| {
                trace.push(v);
                Ok(true)
            },
        )
        .unwrap();
        assert_eq!(trace, vec![0.35, 0.09999999999999998, 0.0]);
        let mut untouched = 1.0;
        assert!(
            !ramp(
                &mut untouched,
                0.0,
                0.25,
                RampPolicy::ReturnToZero,
                || true,
                |_| panic!("must not write")
            )
            .unwrap()
        );
        assert_eq!(untouched, 1.0);
        let mut calls = 0;
        assert!(
            ramp(
                &mut untouched,
                0.0,
                0.25,
                RampPolicy::ReturnToZero,
                || false,
                |_| {
                    calls += 1;
                    Err("disconnected".into())
                }
            )
            .is_err()
        );
        assert_eq!(calls, 1);
    }
    #[test]
    fn cleanup_failure_always_wins_over_stop_or_compliance() {
        for stopped in [false, true] {
            for compliance in [false, true] {
                assert_eq!(
                    completion_phase(false, true, compliance, stopped),
                    Phase::Failed
                );
                assert_eq!(
                    completion_phase(true, false, compliance, stopped),
                    Phase::Failed
                );
            }
        }
        assert_eq!(
            completion_phase(false, false, true, true),
            Phase::ComplianceStop
        );
        assert_eq!(completion_phase(false, false, false, true), Phase::Stopped);
        assert_eq!(
            completion_phase(false, false, false, false),
            Phase::Completed
        );
    }
}
