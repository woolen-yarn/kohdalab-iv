from __future__ import annotations

import copy
import json

import pytest

from kohdalab_iv.api import config as config_module
from kohdalab_iv.api.config import (
    DEFAULT_CONFIG,
    instrument_config,
    load_config,
    measurement_settings,
    normalize_config,
    output_path,
    read_last_config_path,
    resolve_config_path,
    role_refs,
    validate_config,
    with_csv_suffix,
)
from kohdalab_iv.api.formatting import format_conductance, format_resistance
from kohdalab_iv.api.units import (
    format_si,
    normalize_unit,
    parse_quantity,
    quantity_float,
)
from kohdalab_iv.instruments.meters.adcmt_7461a import ADCMT7461A
from kohdalab_iv.instruments.simulated import (
    SimulatedMeter,
    SimulatedSource,
    reset_simulated_circuits,
)


def test_config_state_paths_and_plain_text_compatibility(monkeypatch, tmp_path) -> None:
    state_dir = tmp_path / "state"
    explicit_state = tmp_path / "explicit.json"
    monkeypatch.setenv(config_module.CONFIG_STATE_DIR_ENV, str(state_dir))
    assert config_module.config_state_dir() == state_dir
    assert config_module.last_config_state_path() == state_dir / "last_config.json"

    monkeypatch.setenv(config_module.LAST_CONFIG_STATE_PATH_ENV, str(explicit_state))
    assert config_module.last_config_state_path() == explicit_state

    raw_state = tmp_path / "raw.txt"
    raw_state.write_text("/tmp/lab.json\n", encoding="utf-8")
    assert read_last_config_path(raw_state).as_posix() == "/tmp/lab.json"
    raw_state.write_text("", encoding="utf-8")
    assert read_last_config_path(raw_state) is None
    assert read_last_config_path(tmp_path / "missing") is None


def test_config_resolution_covers_explicit_env_last_default_and_missing(
    monkeypatch, tmp_path
) -> None:
    explicit = tmp_path / "explicit.json"
    env_path = tmp_path / "env.json"
    last = tmp_path / "last.json"
    default = tmp_path / "default.json"
    last_state = tmp_path / "state.json"
    for path in (explicit, env_path, last, default):
        path.write_text("{}", encoding="utf-8")

    assert resolve_config_path(explicit).source == "explicit"
    monkeypatch.setenv("TEST_CONFIG", str(env_path))
    assert resolve_config_path(env_var="TEST_CONFIG").source == "TEST_CONFIG"
    monkeypatch.delenv("TEST_CONFIG")
    last_state.write_text(json.dumps({"path": str(last)}), encoding="utf-8")
    assert (
        resolve_config_path(last_state_path=last_state, lab_default_path=default).source
        == "last"
    )
    last.unlink()
    assert (
        resolve_config_path(last_state_path=last_state, lab_default_path=default).source
        == "lab_default"
    )
    default.unlink()
    resolution = resolve_config_path(
        last_state_path=last_state, lab_default_path=default
    )
    assert resolution.path is None
    assert resolution.source == "none"


def test_config_normalization_and_lookup_errors(tmp_path) -> None:
    normalized = normalize_config({"profile": {"name": "custom"}})
    assert normalized["profile"]["name"] == "custom"
    assert "iv" in normalized["measurements"]
    assert with_csv_suffix("") == "iv_run.csv"
    assert with_csv_suffix("result.CSV") == "result.CSV"

    config = copy.deepcopy(DEFAULT_CONFIG)
    config["measurements"]["iv"]["output"] = {
        "dir": "out",
        "filename": "fixed",
        "auto_timestamp_suffix": False,
    }
    assert output_path(config).as_posix() == "out/fixed.csv"

    with pytest.raises(ValueError, match="Missing measurements.other"):
        measurement_settings(config, "other")
    with pytest.raises(ValueError, match="Missing instrument ref"):
        instrument_config(config, "meter.missing")
    with pytest.raises(ValueError, match="Missing roles.vi"):
        broken_roles = copy.deepcopy(config)
        broken_roles["measurements"]["iv"]["mode"] = "dc_vi"
        broken_roles["roles"].pop("vi")
        role_refs(broken_roles)

    invalid_json = tmp_path / "list.json"
    invalid_json.write_text("[]", encoding="utf-8")
    with pytest.raises(ValueError, match="JSON object"):
        load_config(invalid_json)


def test_validate_config_rejects_malformed_shapes_and_safety() -> None:
    with pytest.raises(ValueError, match="root"):
        validate_config([])  # type: ignore[arg-type]
    with pytest.raises(ValueError, match="config_version"):
        validate_config({})
    with pytest.raises(ValueError, match="at least one measurement"):
        validate_config({"config_version": 1})
    with pytest.raises(ValueError, match="instruments and roles"):
        validate_config(
            {
                "config_version": 1,
                "measurements": {"iv": {}},
                "instruments": [],
            }
        )

    base = copy.deepcopy(DEFAULT_CONFIG)
    cases = []

    value = copy.deepcopy(base)
    value["measurements"]["iv"] = []
    cases.append((value, "must be an object"))

    value = copy.deepcopy(base)
    value["measurements"]["iv"]["mode"] = "ac"
    cases.append((value, "mode must"))

    value = copy.deepcopy(base)
    value["roles"].pop("iv")
    cases.append((value, "Missing roles.iv"))

    value = copy.deepcopy(base)
    value["roles"]["iv"]["source"] = "missing"
    cases.append((value, "instrument ref"))

    value = copy.deepcopy(base)
    value["instruments"]["source"]["gs210"]["resource"] = ""
    cases.append((value, "requires resource"))

    value = copy.deepcopy(base)
    value["measurements"]["iv"]["safety"]["on_finish"] = "ignore"
    cases.append((value, "on_finish must"))

    value = copy.deepcopy(base)
    value["measurements"]["iv"]["safety"]["on_error"] = "ramp_to_zero_then_off"
    cases.append((value, "on_error must"))

    for config, message in cases:
        with pytest.raises(ValueError, match=message):
            validate_config(config)


def test_units_cover_scalars_aliases_and_invalid_values() -> None:
    assert normalize_unit(" µV ") == "uv"
    assert parse_quantity(
        2, dimension="time", default_unit="ms"
    ).si_float == pytest.approx(0.002)
    assert quantity_float({"value": 3, "unit": "A"}, dimension="current") == 3.0
    assert format_si(None, "V") == "-"
    assert format_si(1.23456789, "V") == "1.23457 V"

    for value, message in (
        ({"value": 1}, "must contain"),
        (True, "Expected quantity"),
        ({"value": 1, "unit": "kg"}, "Unsupported unit"),
        ({"value": "bad", "unit": "V"}, "Invalid numeric"),
    ):
        with pytest.raises(ValueError, match=message):
            parse_quantity(value, dimension="voltage")


def test_formatting_handles_text_nonfinite_negative_and_tiny_values() -> None:
    assert format_resistance("unknown") == "unknown"
    assert format_resistance(float("inf")) == "inf"
    assert format_resistance(float("nan")) == "nan"
    assert format_resistance(-1200) == "-1.2 kOhm"
    assert format_resistance(1e-15) == "0.001 pOhm"
    assert format_conductance(-0.0) == "0 S"


def test_simulated_devices_cover_validation_modes_and_disconnect() -> None:
    reset_simulated_circuits()
    source = SimulatedSource("SIM::edge")
    with pytest.raises(ValueError, match="Unsupported simulated source"):
        source.configure_source(
            source_function="bad", source_range=1, hardware_compliance=1
        )
    with pytest.raises(ValueError, match="positive"):
        SimulatedMeter("SIM::bad", resistance_ohm=0)

    meter = SimulatedMeter("SIM::edge", resistance_ohm=2000, offset=0.5)
    source.configure_source(
        source_function="current", source_range=1, hardware_compliance=1
    )
    source.set_level(0.002)
    assert source.read_level() == 0.0
    source.output_on()
    meter.configure_measurement(measure_function="dc_voltage", nplc=1, auto_range=True)
    meter.prepare_for_reading()
    assert meter.read_average(1) == pytest.approx(4.5)
    with pytest.raises(ValueError, match="Unsupported simulated measure"):
        meter.configure_measurement(measure_function="bad", nplc=1, auto_range=True)
    with pytest.raises(ValueError, match="count"):
        meter.read_average(0)
    source.local()
    meter.local()
    source.close()
    meter.close()
    assert not source.is_connected()
    assert not meter.is_connected()


class AdcHandle:
    def __init__(self, responses=(), reads=()) -> None:
        self.commands = []
        self.responses = list(responses)
        self.reads = list(reads)
        self.session = object()
        self.usb_calls = []

    def write(self, command):
        self.commands.append(command)

    def query(self, command):
        self.commands.append(command)
        response = self.responses.pop(0) if self.responses else "0,No error"
        if isinstance(response, Exception):
            raise response
        return response

    def read(self):
        response = self.reads.pop(0)
        if isinstance(response, Exception):
            raise response
        return response

    def clear(self):
        return None

    def control_out(self, *args):
        self.usb_calls.append(args)

    def get_visa_attribute(self, attribute):
        return 0


def test_adcmt_covers_usb_local_invalid_functions_and_read_edges(monkeypatch) -> None:
    handle = AdcHandle(reads=["bad", "1", "3"])
    device = ADCMT7461A("USB0::1::INSTR", handle=handle, command_language="adc")
    device.READ_DELAY_S = 0
    device.USB_QUERY_DELAY_S = 0

    device.local()
    assert [call[1] for call in handle.usb_calls] == [0xA1, 0xA0]
    device.local_after_close()
    with pytest.raises(ValueError, match="Unsupported DMM"):
        device.configure_measurement(measure_function="bad", nplc=1)
    device.configure_measurement(
        measure_function="dc_voltage", nplc=1, auto_range=False
    )
    with pytest.raises(RuntimeError, match="Unexpected ADCMT"):
        device.read_once()
    device.prepare_for_reading()
    assert device.read_average(1) == 3.0

    broken = ADCMT7461A("TCPIP::host", handle=AdcHandle(), command_language="adc")
    broken.prepare_for_disconnect()
    assert broken.inst.commands == ["H0", "*CLS"]


def test_adcmt_scpi_error_draining_and_best_effort_writes() -> None:
    handle = AdcHandle(
        responses=[
            "-200,Execution error",
            "-113,Undefined header",
            "0,No error",
            "-201,Settings lost",
            "-202,Another error",
        ]
    )
    device = ADCMT7461A("GPIB0::1::INSTR", handle=handle)

    assert device._drain_scpi_errors() == "-200,Execution error"
    assert device._drain_scpi_errors(max_reads=2) == "-201,Settings lost"
    assert device._is_no_error("No Error")
    assert device._is_undefined_header("ERR-113")
    assert device._sampling_rate(0.01) == "FAST"

    class BrokenHandle(AdcHandle):
        def write(self, command):
            raise RuntimeError("write failed")

    broken = ADCMT7461A("GPIB0::1::INSTR", handle=BrokenHandle())
    broken._try_write("OPTIONAL")
    assert broken._try_scpi_setting("OPTIONAL") is None


def test_adcmt_scpi_validation_usb_status_discard_failure_and_slowest_rate() -> None:
    scpi = ADCMT7461A("GPIB0::1::INSTR", handle=AdcHandle())
    with pytest.raises(ValueError, match="Unsupported DMM"):
        scpi.configure_measurement(measure_function="bad", nplc=1)
    assert scpi._sampling_rate(1000) == "SSLOW"

    failing_read = ADCMT7461A(
        "USB0::1::INSTR",
        handle=AdcHandle(reads=[RuntimeError("discard failed")]),
        command_language="adc",
    )
    failing_read.READ_DELAY_S = 0
    failing_read.prepare_for_reading()

    usb = ADCMT7461A(
        "USB0::2::INSTR",
        handle=AdcHandle(reads=["0,No error"]),
        command_language="scpi",
    )
    usb.USB_QUERY_DELAY_S = 0
    assert usb.connect_status() == "0,No error"

    tcpip = ADCMT7461A("TCPIP0::host::INSTR", handle=AdcHandle())
    tcpip.local()
