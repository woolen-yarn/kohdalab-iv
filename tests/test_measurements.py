import copy

from kohdalab_iv.api.config import DEFAULT_CONFIG
from kohdalab_iv.api.measurements import run_iv
from kohdalab_iv.api.scan_plan import iv_plan_from_config
from kohdalab_iv.api.session import DeviceSession


class FakeSource:
    def __init__(self):
        self.level = 0.0
        self.output_off_called = False
        self.commands = []

    def configure_source(self, **kwargs):
        self.commands.append(("configure", kwargs))

    def output_on(self):
        self.commands.append(("on", None))

    def output_off(self):
        self.output_off_called = True
        self.commands.append(("off", None))

    def set_level(self, value):
        self.level = float(value)
        self.commands.append(("level", self.level))

    def read_level(self):
        return self.level


class FakeMeter:
    def __init__(self, values):
        self.values = list(values)
        self.configured = False

    def configure_measurement(self, **kwargs):
        self.configured = True

    def read_average(self, count):
        return self.values.pop(0)


class FakeSession:
    def __init__(self, source, meter):
        self.source = source
        self.meter = meter
        self.disconnected = False

    def require(self, ref):
        return self.source if ref.startswith("source.") else self.meter

    def disconnect_all(self):
        self.disconnected = True


class FakeDeviceForSession:
    def __init__(self):
        self.local_called = False
        self.closed = False

    def local(self):
        self.local_called = True

    def close(self):
        self.closed = True


def _small_config():
    config = copy.deepcopy(DEFAULT_CONFIG)
    settings = config["measurements"]["iv"]
    settings["mode"] = "dc_iv"
    scan = settings["scan"]
    scan["pattern"] = "linear"
    scan["start"] = {"value": 0, "unit": "mV"}
    scan["stop"] = {"value": 20, "unit": "mV"}
    scan["step"] = {"value": 10, "unit": "mV"}
    safety = settings["safety"]
    safety["max_abs_source"] = {"value": 100, "unit": "mV"}
    safety["compliance"] = {"value": 10, "unit": "uA"}
    safety["ramp_step"] = {"value": 10, "unit": "mV"}
    timing = settings["timing"]
    timing["settle_s"] = 0.0
    timing["ramp_step_wait_s"] = 0.0
    return config


def test_run_iv_writes_rows_and_turns_output_off(tmp_path):
    config = _small_config()
    plan = iv_plan_from_config(config)
    source = FakeSource()
    meter = FakeMeter([0.0, 1e-6, 2e-6])
    session = FakeSession(source, meter)

    rows = run_iv(config, plan=plan, output=tmp_path / "run.csv", session=session)

    assert len(rows) == 3
    assert source.output_off_called is True
    assert source.level == 0.0
    assert (tmp_path / "run.csv").read_text(encoding="utf-8").startswith(
        "timestamp,elapsed_s,point_index,direction,target_value,target_unit,voltage_V,current_A,resistance_Ohm,conductance_S"
    )


def test_run_iv_stops_after_compliance(tmp_path):
    config = _small_config()
    config["measurements"]["iv"]["safety"]["compliance"] = {"value": 1.0, "unit": "uA"}
    plan = iv_plan_from_config(config)
    source = FakeSource()
    meter = FakeMeter([0.0, 2e-6, 3e-6])
    session = FakeSession(source, meter)

    rows = run_iv(config, plan=plan, output=tmp_path / "run.csv", session=session)

    assert len(rows) == 2
    assert rows[-1]["compliance"] is True
    assert source.output_off_called is True


def test_run_iv_uses_stop_cleanup_for_user_stop(tmp_path):
    config = _small_config()
    scan = config["measurements"]["iv"]["scan"]
    scan["start"] = {"value": 10, "unit": "mV"}
    scan["stop"] = {"value": 20, "unit": "mV"}
    config["measurements"]["iv"]["safety"]["on_stop"] = "output_off"
    plan = iv_plan_from_config(config)
    source = FakeSource()
    meter = FakeMeter([1e-6, 2e-6])
    session = FakeSession(source, meter)
    running = {"value": True}

    def on_point(_point):
        running["value"] = False

    rows = run_iv(
        config,
        plan=plan,
        output=tmp_path / "run.csv",
        session=session,
        on_point=on_point,
        should_continue=lambda: running["value"],
    )

    assert len(rows) == 1
    assert source.output_off_called is True
    assert source.level == 0.01


def test_device_session_returns_device_to_local_on_disconnect():
    session = DeviceSession(copy.deepcopy(DEFAULT_CONFIG), auto_connect=False)
    fake = FakeDeviceForSession()
    session.sources["gs210"] = fake

    session.disconnect_device("source.gs210")

    assert fake.local_called is True
    assert fake.closed is True
