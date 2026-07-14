import copy
from decimal import Decimal
from types import SimpleNamespace

import pytest

from kohdalab_iv.api import cli
from kohdalab_iv.api.config import ConfigPathResolution
from kohdalab_iv.api.experiment import Experiment
from kohdalab_iv.api.measurement_rows import _safe_div
from kohdalab_iv.api.scan_plan import (
    _nplc_supported,
    _select_source_range,
    _target_decimals,
)
from kohdalab_iv.api.session import DeviceSession
from kohdalab_iv.api.specs import spec_for


def test_cli_measure_runs_experiment_and_formats_callbacks(
    monkeypatch, tmp_path, capsys
):
    output = tmp_path / "measurement.csv"
    plan = SimpleNamespace(
        summary="one-point plan",
        measurement_name="iv",
        source_ref="source.simulated",
        source_model="SIMULATED_SOURCE",
        measure_ref="meter.simulated",
        measure_model="SIMULATED_METER",
    )
    calls = []

    class FakeExperiment:
        def __init__(self, config, *, auto_connect):
            calls.append((config, auto_connect))

        def run_iv(self, **kwargs):
            kwargs["on_status"]("running")
            kwargs["on_point"](
                SimpleNamespace(
                    index=1,
                    total_points=1,
                    row={
                        "target_value": 1.0,
                        "target_unit": "V",
                        "voltage_V": 1.0,
                        "current_A": 0.001,
                        "resistance_Ohm": 1000.0,
                        "status": "running",
                    },
                )
            )
            return [{}]

    monkeypatch.setattr(
        cli,
        "resolve_config_path",
        lambda _path: ConfigPathResolution(
            path=tmp_path / "config.json", source="test", candidates=[]
        ),
    )
    monkeypatch.setattr(cli, "load_config", lambda _path: {"config": True})
    monkeypatch.setattr(cli, "iv_plan_from_config", lambda config, name="iv": plan)
    monkeypatch.setattr(cli, "output_path", lambda config, name: output)
    monkeypatch.setattr(cli, "Experiment", FakeExperiment)

    assert cli.main(["measure"]) == 0
    captured = capsys.readouterr()
    assert "Starting one-point plan" in captured.out
    assert "status: running" in captured.out
    assert "[1/1]" in captured.out
    assert "Saved 1 rows" in captured.out
    assert calls == [({"config": True}, True)]


def test_cli_handles_unresolved_config_interrupt_and_unsupported_command(
    monkeypatch, capsys
):
    monkeypatch.setattr(
        cli,
        "resolve_config_path",
        lambda _path: ConfigPathResolution(path=None, source=None, candidates=[]),
    )
    assert cli.main(["check-config"]) == 1
    assert "No configuration file" in capsys.readouterr().err

    monkeypatch.setattr(
        cli, "list_visa_resources", lambda: (_ for _ in ()).throw(KeyboardInterrupt)
    )
    assert cli.main(["list-resources"]) == 130
    assert "Interrupted." in capsys.readouterr().err

    parser = SimpleNamespace(
        parse_args=lambda _argv: SimpleNamespace(command="unsupported", config=None)
    )
    monkeypatch.setattr(cli, "build_parser", lambda: parser)
    monkeypatch.setattr(
        cli,
        "resolve_config_path",
        lambda _path: ConfigPathResolution(
            path="config.json", source="test", candidates=[]
        ),
    )
    monkeypatch.setattr(cli, "load_config", lambda _path: {})
    with pytest.raises(ValueError, match="Unsupported command"):
        cli.main([])


def test_experiment_constructors_and_session_delegates(monkeypatch, tmp_path):
    path = tmp_path / "config.json"
    path.write_text("{}", encoding="utf-8")
    experiment = Experiment.from_config(path, auto_connect=False)
    calls = []

    class Session:
        def set_config(self, config):
            calls.append(("set_config", config))

        def connect_all(self):
            calls.append(("connect_all",))

        def connect_device(self, ref):
            calls.append(("connect_device", ref))
            return "device"

        def disconnect_all(self):
            calls.append(("disconnect_all",))

        def disconnect_device(self, ref):
            calls.append(("disconnect_device", ref))
            return [ref]

        def connected_devices(self):
            return {"source.a": True}

    experiment.session = Session()
    experiment.config = {}
    experiment.connect_all()
    assert experiment.connect_device("source.a") == "device"
    experiment.disconnect_all()
    assert experiment.disconnect_device("source.a") == ["source.a"]
    assert experiment.connected_devices() == {"source.a": True}
    assert [call[0] for call in calls] == [
        "set_config",
        "connect_all",
        "connect_device",
        "disconnect_all",
        "disconnect_device",
    ]


def test_measurement_division_fallback_supports_non_decimal_numeric_types():
    class Numeric:
        def __str__(self):
            return "not-a-decimal"

        def __truediv__(self, other):
            assert other == 2
            return 12.5

    assert _safe_div(Numeric(), 2) == 12.5


def test_scan_helpers_cover_round_trip_range_and_nplc_fallbacks():
    points, directions, pattern = _target_decimals(
        {
            "pattern": "round_trip",
            "start": {"value": 0, "unit": "V"},
            "stop": {"value": 2, "unit": "V"},
            "step": {"value": 1, "unit": "V"},
        },
        "voltage",
    )
    assert points == [
        Decimal("0"),
        Decimal("1"),
        Decimal("2"),
        Decimal("1"),
        Decimal("0"),
    ]
    assert directions == ["forward", "forward", "forward", "backward", "backward"]
    assert pattern == "round_trip"

    with pytest.raises(ValueError, match="Source targets exceed"):
        _select_source_range(
            {
                "display_name": "tiny source",
                "voltage_ranges": [
                    {
                        "name": "1V",
                        "command_value": 1,
                        "max_abs_V": 1,
                        "resolution_V": 0.1,
                    }
                ],
            },
            "voltage",
            2.0,
        )
    assert not _nplc_supported({}, 1.0)

    with pytest.raises(ValueError, match="step must be non-zero"):
        _target_decimals(
            {
                "pattern": "linear",
                "start": {"value": 0, "unit": "V"},
                "stop": {"value": 1, "unit": "V"},
                "step": {"value": 0, "unit": "V"},
            },
            "voltage",
        )

    repeated, repeated_directions, _ = _target_decimals(
        {
            "pattern": "custom_list",
            "custom_points": [
                {"value": 1, "unit": "V"},
                {"value": 1, "unit": "V"},
            ],
        },
        "voltage",
    )
    assert repeated == [Decimal("1"), Decimal("1")]
    assert repeated_directions == ["forward", "forward"]


def test_specs_report_unknown_models_and_accept_aliases(monkeypatch):
    with pytest.raises(ValueError, match="Unsupported source model"):
        spec_for("source", "unknown")

    monkeypatch.setitem(
        __import__("kohdalab_iv.api.specs", fromlist=["MODEL_ALIASES"]).MODEL_ALIASES[
            "source"
        ],
        "ALIAS",
        "YOKOGAWA_GS210",
    )
    assert spec_for("source", " alias ")["display_name"]


def test_device_session_connect_all_and_internal_release_paths(monkeypatch):
    config = {
        "instruments": {
            "source": {"one": {"model": "SIMULATED_SOURCE", "resource": "SIM::SOURCE"}},
            "meter": {"one": {"model": "SIMULATED_METER", "resource": "SIM::METER"}},
        }
    }
    session = DeviceSession(copy.deepcopy(config), auto_connect=False)
    connected = []
    monkeypatch.setattr(session, "connect_device", lambda ref: connected.append(ref))
    session.connect_all()
    assert connected == ["source.one", "meter.one"]

    released = []
    monkeypatch.setattr("kohdalab_iv.api.session.release_gpib_remote", released.append)

    class Device:
        resource = "GPIB2::1::INSTR"

    monkeypatch.setattr(session, "_return_and_close_device", lambda device: None)
    session.sources["one"] = Device()
    session._disconnect_device("source", "one", release_board=True)
    assert released == ["GPIB2"]

    session.config = {"instruments": {"source": [], "meter": {}}}
    assert session._configured_keys("source") == []


def test_device_session_returns_connected_devices_and_handles_identity_races():
    session = DeviceSession({"instruments": {}}, auto_connect=False)

    class Connected:
        def is_connected(self):
            return True

    connected = Connected()
    session.sources["one"] = connected
    assert session.require("source.one") is connected
    assert session._connection_state("source", "one") is True

    replacement = object()
    session.sources["one"] = replacement
    session._discard_device("source", "one", connected, close=False)
    assert session.sources["one"] is replacement

    class SetOnly:
        values = []

        def set_level(self, _value):
            self.values.append(_value)

    set_only = SetOnly()
    session._safe_output_off(set_only)
    assert set_only.values == [0.0]

    class OutputOnly:
        calls = 0

        def output_off(self):
            self.calls += 1

    output_only = OutputOnly()
    session._safe_output_off(output_only)
    assert output_only.calls == 2
    session._disconnect_device("source", "missing", release_board=True)
