import pytest

from kohdalab_iv.apps.gui_state import MeasurementPhase, MeasurementRunState


def test_measurement_state_runs_and_finishes() -> None:
    state = MeasurementRunState()

    assert state.phase is MeasurementPhase.IDLE
    assert state.controls_enabled
    assert not state.active
    assert not state.can_stop

    state.begin()

    assert state.phase is MeasurementPhase.RUNNING
    assert state.active
    assert not state.controls_enabled
    assert state.can_stop

    state.finish(12)

    assert state.phase is MeasurementPhase.IDLE
    assert state.points_collected == 12
    assert state.controls_enabled


def test_measurement_state_stop_is_idempotent() -> None:
    state = MeasurementRunState()

    assert not state.request_stop()
    state.begin()
    assert state.request_stop()
    assert state.phase is MeasurementPhase.STOPPING
    assert state.active
    assert not state.can_stop
    assert not state.request_stop()

    state.finish(3)
    assert state.points_collected == 3


def test_measurement_state_retains_error_until_next_run() -> None:
    state = MeasurementRunState()
    state.begin()
    state.record_error("meter timeout")
    state.finish(2)

    assert state.last_error == "meter timeout"
    assert state.points_collected == 2

    state.begin()
    assert state.last_error is None
    assert state.points_collected == 0


def test_measurement_state_rejects_invalid_transitions() -> None:
    state = MeasurementRunState()

    with pytest.raises(RuntimeError, match="while idle"):
        state.record_error("unexpected")

    state.begin()
    with pytest.raises(RuntimeError, match="already running"):
        state.begin()
    with pytest.raises(ValueError, match="non-negative"):
        state.finish(-1)


def test_measurement_state_reset_clears_partial_run() -> None:
    state = MeasurementRunState()
    state.begin()
    state.record_error("start failed")

    state.reset()

    assert state == MeasurementRunState()
