import copy
from dataclasses import replace
from decimal import Decimal

import pytest

from kohdalab_iv.api import measurements as measurements_module
from kohdalab_iv.api import scan_plan as scan_plan_module
from kohdalab_iv.api.config import DEFAULT_CONFIG
from kohdalab_iv.api.measurements import run_iv
from kohdalab_iv.api.scan_plan import iv_plan_from_config
from kohdalab_iv.api.session import DeviceSession


class RecordingSource:
    def __init__(self):
        self.levels = []
        self.output_off_calls = 0

    def configure_source(self, **_kwargs):
        pass

    def output_on(self):
        pass

    def output_off(self):
        self.output_off_calls += 1

    def set_level(self, value):
        self.levels.append(float(value))

    def read_level(self):
        return self.levels[-1] if self.levels else 0.0


class RecordingMeter:
    def configure_measurement(self, **_kwargs):
        pass

    def read_average(self, _count):
        return 0.0


class RecordingSession:
    def __init__(self, source=None, meter=None):
        self.source = source or RecordingSource()
        self.meter = meter or RecordingMeter()

    def require(self, ref):
        return self.source if ref.startswith("source.") else self.meter


def _single_point_config():
    config = copy.deepcopy(DEFAULT_CONFIG)
    settings = config["measurements"]["iv"]
    settings["mode"] = "dc_iv"
    settings["scan"] = {
        "pattern": "linear",
        "start": {"value": 10, "unit": "mV"},
        "stop": {"value": 10, "unit": "mV"},
        "step": {"value": 10, "unit": "mV"},
        "repeat": 1,
    }
    settings["safety"]["max_abs_source"] = {"value": 20, "unit": "mV"}
    settings["safety"]["compliance"] = {"value": 1, "unit": "mA"}
    settings["safety"]["ramp_step"] = {"value": 10, "unit": "mV"}
    settings["timing"].update(
        pre_delay_s=0.0,
        start_settle_s=0.0,
        settle_s=0.0,
        ramp_step_wait_s=0.0,
        post_zero_delay_s=0.0,
    )
    return config


def test_scan_helpers_reject_invalid_ranges_and_cover_empty_inputs():
    with pytest.raises(ValueError, match="non-zero"):
        scan_plan_module._decimal_range(Decimal("0"), Decimal("1"), Decimal("0"))
    with pytest.raises(ValueError, match="Positive step"):
        scan_plan_module._decimal_range(Decimal("1"), Decimal("0"), Decimal("1"))
    with pytest.raises(ValueError, match="Negative step"):
        scan_plan_module._decimal_range(Decimal("0"), Decimal("1"), Decimal("-1"))

    assert scan_plan_module._without_duplicate_join([], [Decimal("1")]) == [
        Decimal("1")
    ]
    assert scan_plan_module._directions_from_points([]) == []
    assert scan_plan_module._directions_from_points([Decimal("1")]) == ["forward"]


@pytest.mark.parametrize(
    ("change", "message"),
    [
        (lambda settings: settings.update(mode="invalid"), "mode must"),
        (lambda settings: settings["scan"].update(repeat=0), "repeat must"),
        (
            lambda settings: settings["scan"].update(
                pattern="custom_list", custom_points=[]
            ),
            "requires custom_points",
        ),
        (
            lambda settings: settings["scan"].update(pattern="unknown"),
            "Unsupported sweep pattern",
        ),
        (lambda settings: settings["timing"].update(average_count=0), "average_count"),
        (
            lambda settings: settings["safety"].update(
                ramp_step={"value": 0, "unit": "mV"}
            ),
            "ramp_step must be positive",
        ),
    ],
)
def test_plan_rejects_invalid_operating_settings(change, message):
    config = _single_point_config()
    change(config["measurements"]["iv"])

    with pytest.raises(ValueError, match=message):
        iv_plan_from_config(config)


def test_custom_scan_repeats_points_and_assigns_repeat_indexes():
    config = _single_point_config()
    settings = config["measurements"]["iv"]
    settings["scan"] = {
        "pattern": "custom_list",
        "custom_points": [
            {"value": 0, "unit": "mV"},
            {"value": 10, "unit": "mV"},
        ],
        "repeat": 2,
    }

    plan = iv_plan_from_config(config)

    assert [point.target for point in plan.points] == pytest.approx(
        [0.0, 0.01, 0.0, 0.01]
    )
    assert [point.repeat_index for point in plan.points] == [1, 1, 2, 2]
    assert plan.source_unit == "V"


def test_measurement_helpers_cover_ramp_validation_interruption_and_optional_prepare(
    monkeypatch,
):
    source = RecordingSource()
    with pytest.raises(ValueError, match="positive"):
        measurements_module._ramp_to(
            source, current=0.0, target=1.0, step=0.0, wait_s=0.0, should_continue=None
        )

    assert (
        measurements_module._ramp_to(
            source, current=1.0, target=1.0, step=0.1, wait_s=0.0, should_continue=None
        )
        == 1.0
    )
    assert (
        measurements_module._ramp_to(
            source,
            current=0.0,
            target=1.0,
            step=0.1,
            wait_s=0.0,
            should_continue=lambda: False,
        )
        == 0.0
    )

    times = iter([0.0, 1.0, 2.0])
    monkeypatch.setattr(measurements_module.time, "monotonic", lambda: next(times))
    assert measurements_module._sleep_interruptible(1.0, lambda: False) is False
    measurements_module._prepare_meter_for_reading(object())


def test_measurement_sleep_and_ramp_stop_during_active_wait(monkeypatch):
    times = iter([0.0, 0.1])
    monkeypatch.setattr(measurements_module.time, "monotonic", lambda: next(times))
    assert measurements_module._sleep_interruptible(1.0, lambda: False) is False

    source = RecordingSource()
    monkeypatch.setattr(
        measurements_module, "_sleep_interruptible", lambda *_args: False
    )
    assert (
        measurements_module._ramp_to(
            source,
            current=0.0,
            target=1.0,
            step=0.25,
            wait_s=1.0,
            should_continue=lambda: True,
        )
        == 0.25
    )


@pytest.mark.parametrize("stop_at", ["pre_delay", "start_settle", "point_settle"])
def test_run_iv_honors_stop_requests_at_each_wait_boundary(
    tmp_path, monkeypatch, stop_at
):
    config = _single_point_config()
    plan = iv_plan_from_config(config)
    session = RecordingSession()
    calls = 0

    def fake_sleep(_duration, _should_continue):
        nonlocal calls
        calls += 1
        boundary = {"pre_delay": 1, "start_settle": 2, "point_settle": 3}[stop_at]
        return calls != boundary

    monkeypatch.setattr(measurements_module, "_sleep_interruptible", fake_sleep)

    rows = run_iv(
        config, plan=plan, output=tmp_path / f"{stop_at}.csv", session=session
    )

    assert rows == []
    assert session.source.output_off_calls == 1


def test_run_iv_ignores_output_off_failure_while_preserving_measurement_error(tmp_path):
    config = _single_point_config()
    plan = iv_plan_from_config(config)

    class BrokenSource(RecordingSource):
        def output_off(self):
            raise RuntimeError("output failure")

    class BrokenMeter(RecordingMeter):
        def read_average(self, _count):
            raise ValueError("read failure")

    with pytest.raises(ValueError, match="read failure"):
        run_iv(
            config,
            plan=plan,
            output=tmp_path / "failure.csv",
            session=RecordingSession(BrokenSource(), BrokenMeter()),
        )


def test_run_iv_disconnects_internally_owned_session(tmp_path, monkeypatch):
    config = _single_point_config()
    plan = iv_plan_from_config(config)
    session = RecordingSession()
    session.disconnected = False

    def disconnect_all():
        session.disconnected = True

    session.disconnect_all = disconnect_all
    monkeypatch.setattr(measurements_module, "DeviceSession", lambda _config: session)

    rows = run_iv(config, plan=plan, output=tmp_path / "owned.csv")

    assert len(rows) == 1
    assert session.disconnected


def test_run_iv_handles_empty_plan_alternate_cleanup_and_error_policy(tmp_path):
    config = _single_point_config()
    base_plan = iv_plan_from_config(config)

    empty_source = RecordingSource()
    rows = run_iv(
        config,
        plan=replace(base_plan, points=[]),
        output=tmp_path / "empty.csv",
        session=RecordingSession(empty_source, RecordingMeter()),
    )
    assert rows == []

    leave_on_source = RecordingSource()
    rows = run_iv(
        config,
        plan=replace(base_plan, on_finish="leave_output_on"),
        output=tmp_path / "leave-on.csv",
        session=RecordingSession(leave_on_source, RecordingMeter()),
    )
    assert len(rows) == 1
    assert leave_on_source.output_off_calls == 0

    class BrokenMeter(RecordingMeter):
        def read_average(self, _count):
            raise RuntimeError("read failed")

    with pytest.raises(RuntimeError, match="read failed"):
        run_iv(
            config,
            plan=replace(base_plan, on_error="leave_output_on"),
            output=tmp_path / "error-policy.csv",
            session=RecordingSession(RecordingSource(), BrokenMeter()),
        )

    class MissingSourceSession(RecordingSession):
        def require(self, _ref):
            raise RuntimeError("source missing")

    with pytest.raises(RuntimeError, match="source missing"):
        run_iv(
            config,
            plan=base_plan,
            output=tmp_path / "source-missing.csv",
            session=MissingSourceSession(),
        )


def test_device_session_validates_refs_models_and_absent_devices():
    session = DeviceSession(copy.deepcopy(DEFAULT_CONFIG), auto_connect=False)

    assert session.disconnect_device("source.gs210") == []
    with pytest.raises(ValueError, match="Invalid device ref"):
        session.disconnect_device("invalid")
    with pytest.raises(ValueError, match="Invalid device ref"):
        session.disconnect_device("source.")
    with pytest.raises(ValueError, match="Unsupported device kind"):
        session._controller_for("other", "MODEL")
    with pytest.raises(ValueError, match="Unsupported source model"):
        session._controller_for("source", "MODEL")
    with pytest.raises(ValueError, match="Unsupported device kind"):
        session._map("other")


def test_device_session_handles_devices_with_missing_or_broken_optional_methods():
    session = DeviceSession(copy.deepcopy(DEFAULT_CONFIG), auto_connect=False)
    plain = object()
    assert session._device_is_connected(plain) is True
    session._return_and_close_device(plain)

    class BrokenDevice:
        def is_connected(self):
            raise RuntimeError

        def output_off(self):
            raise RuntimeError

        def set_level(self, _value):
            raise RuntimeError

        def close(self):
            raise RuntimeError

    broken = BrokenDevice()
    assert session._device_is_connected(broken) is False
    session._return_and_close_device(broken)
