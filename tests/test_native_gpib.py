from __future__ import annotations

import subprocess
import sys
import json

import pytest

from kohdalab_iv.interfaces import common, native_gpib as native
from kohdalab_iv.instruments import visa_base


@pytest.fixture
def helper(tmp_path, monkeypatch):
    path = tmp_path / "helper"
    path.write_text(
        f"#!{sys.executable}\n"
        + """
import sys
if len(sys.argv) > 1 and sys.argv[1] == '--available':
    print('1', flush=True)
    sys.exit(0)
print('driver log', flush=True)
print('KIV\\tOK\\t', flush=True)
for line in sys.stdin:
    op, addr, timeout, data = line.split()
    text = bytes.fromhex(data).decode() if data != '-' else ''
    if text == 'FAIL':
        print('KIV\\tERR\\t' + b'communication failed'.hex(), flush=True)
    else:
        result = text if op == 'QUERY' else ('1.0' if op == 'READ' else '')
        if op == 'LIST': result = 'GPIB0::5::INSTR\\nGPIB0::9::INSTR\\nGPIB0::17::INSTR\\n'
        print('KIV\\tOK\\t' + result.encode().hex(), flush=True)
    if op == 'EXIT':
        break
"""
    )
    path.chmod(0o755)
    monkeypatch.setenv("KOHDALAB_IV_GPIB_HELPER", str(path))
    yield path
    for bridge in native._BRIDGES.values():
        bridge.stop()
    native._BRIDGES.clear()


def test_helper_discovery(monkeypatch, tmp_path):
    monkeypatch.delenv("KOHDALAB_IV_GPIB_HELPER", raising=False)
    monkeypatch.setattr(native.sys, "platform", "linux")
    assert native.helper_path() is None
    assert not native.enabled()
    monkeypatch.setattr(native.sys, "platform", "darwin")
    monkeypatch.setattr(native.sys, "frozen", False, raising=False)
    assert native.helper_path() is None
    monkeypatch.setattr(native.sys, "frozen", True)
    monkeypatch.setattr(native.sys, "_MEIPASS", str(tmp_path), raising=False)
    assert native.helper_path() == tmp_path / "kohdalab-gpib-helper"
    assert not native.enabled()
    monkeypatch.setenv("KOHDALAB_IV_GPIB_HELPER", str(tmp_path / "missing"))
    assert not native.enabled()
    with pytest.raises(RuntimeError, match="missing"):
        native.NativeGpibHandle("GPIB0::5::INSTR")
    monkeypatch.delenv("KOHDALAB_IV_GPIB_HELPER")
    monkeypatch.setattr(native.sys, "platform", "linux")
    with pytest.raises(RuntimeError, match="missing"):
        native.NativeGpibHandle("GPIB0::5::INSTR")


@pytest.mark.parametrize(
    "resource",
    ["GPIB1::5::INSTR", "USB0::1::INSTR", "GPIB0::0::INSTR", "GPIB0::31::INSTR"],
)
def test_unsupported_resource(resource):
    with pytest.raises(ValueError, match="GPIB0"):
        native.NativeGpibHandle(resource)


def test_shared_adapter_resources_and_local_cleanup(helper):
    a = common.open_visa("GPIB0::5::INSTR")
    b = common.open_visa("GPIB0::9::INSTR", timeout_ms=10000)
    assert a.bridge is b.bridge
    assert a.session == 5 and b.timeout == 10000
    assert common.list_visa_resources() == (
        "GPIB0::5::INSTR",
        "GPIB0::9::INSTR",
        "GPIB0::17::INSTR",
    )
    a.write("*CLS\n")
    assert a.query("*IDN?\n") == "*IDN?\n"
    assert b.read() == "1.0"
    a.clear()
    a.control_ren(6)
    a.control_ren(0)
    a.control_ren(2)
    with pytest.raises(ValueError, match="REN mode"):
        a.control_ren(99)
    native.release_remote("GPIB1")
    visa_base.release_gpib_remote("GPIB0")
    with pytest.raises(RuntimeError, match="communication failed"):
        a.bridge.request("QUERY", 5, 5000, "FAIL")
    # An instrument timeout/error must leave the shared transport usable for OFF.
    a.write(":OUTP OFF")
    a.close()
    a.close()
    assert a.session is None and b.session == 9
    with pytest.raises(RuntimeError, match="resource is closed"):
        a.read()
    b.close()
    assert native._BRIDGES == {}
    assert b.bridge.process.poll() is not None


def test_eof_and_dead_transport(helper):
    a = native.NativeGpibHandle("GPIB0::5::INSTR")
    a.bridge.process.terminate()
    a.bridge.process.wait()
    assert a.session is None
    with pytest.raises(RuntimeError, match="closed"):
        a.read()
    with pytest.raises(RuntimeError, match="exited unexpectedly"):
        a.bridge._response(1)
    with pytest.raises(RuntimeError, match="closed"):
        a.close()
    assert not native._BRIDGES


def test_reconnect_replaces_dead_shared_bridge_without_losing_new_handle(helper):
    a = native.NativeGpibHandle("GPIB0::5::INSTR")
    a.bridge.stop()
    b = native.NativeGpibHandle("GPIB0::9::INSTR")
    assert b.bridge is not a.bridge
    with pytest.raises(RuntimeError, match="closed"):
        a.close()
    assert native._BRIDGES[b.key] is b.bridge
    assert b.read() == "1.0"
    b.close()


def test_response_timeout_closes_transport(helper, monkeypatch):
    a = native.NativeGpibHandle("GPIB0::5::INSTR")
    monkeypatch.setattr(native.select, "select", lambda *args: ([], [], []))
    with pytest.raises(TimeoutError, match="stopped responding"):
        a.bridge._response(0.01 if a.bridge.chunks is not None else 1)
    assert a.session is None
    with pytest.raises(TimeoutError):
        a.bridge._response(0)
    with pytest.raises(RuntimeError):
        a.close()


def test_startup_error_stops_helper(tmp_path):
    path = tmp_path / "bad-helper"
    path.write_text(
        f"#!{sys.executable}\nprint('KIV\\tERR\\t' + b'no adapter'.hex(), flush=True)\n"
    )
    path.chmod(0o755)
    with pytest.raises(RuntimeError, match="no adapter"):
        native._Bridge(path)


def test_non_gpib_keeps_visa_backend(helper, monkeypatch):
    import pyvisa

    class Manager:
        def open_resource(self, resource):
            assert resource == "USB0::1::INSTR"
            return type("Handle", (), {})()

    monkeypatch.setattr(pyvisa, "ResourceManager", Manager)
    assert common.open_visa("USB0::1::INSTR").timeout == 5000


def test_stop_handles_missing_streams():
    bridge = object.__new__(native._Bridge)
    bridge.label = "test"
    bridge.process = subprocess.Popen(
        [sys.executable, "-c", "pass"], stdout=None, stdin=None
    )
    bridge.process.wait()
    bridge.stop()


def test_windows_pipe_reader_handles_frames_errors_eof_and_timeout(helper, monkeypatch):
    # Execute the Windows pipe-reading path with a real subprocess on this host.
    with monkeypatch.context() as patch:
        patch.setattr(native.sys, "platform", "win32")
        patch.setattr(native.select, "select", lambda *args: pytest.fail("pipe select"))
        bridge = native._Bridge(helper)
    assert bridge.chunks is not None
    try:
        assert (
            bridge.request("QUERY", 5, 1000, "multi-byte 日本語\n")
            == "multi-byte 日本語\n"
        )
        with pytest.raises(RuntimeError, match="communication failed"):
            bridge.request("QUERY", 5, 1000, "FAIL")
        assert bridge.request("READ", 5, 1000) == "1.0"
        with pytest.raises(TimeoutError, match="stopped responding"):
            bridge._response(0.01)
        with pytest.raises(RuntimeError, match="exited unexpectedly"):
            bridge._response(1)
    finally:
        bridge.stop()


def test_windows_frozen_helper_names(monkeypatch, tmp_path):
    from kohdalab_iv.interfaces import native_usb

    monkeypatch.delenv("KOHDALAB_IV_GPIB_HELPER", raising=False)
    monkeypatch.delenv("KOHDALAB_IV_USB_HELPER", raising=False)
    monkeypatch.setattr(native.sys, "platform", "win32")
    monkeypatch.setattr(native.sys, "frozen", True, raising=False)
    monkeypatch.setattr(native.sys, "_MEIPASS", str(tmp_path), raising=False)
    assert native.helper_path() == tmp_path / "kohdalab-gpib-helper.exe"
    assert native_usb.helper_path() == tmp_path / "kohdalab-usbtmc-helper.exe"


def test_gpib_discovery_finds_unregistered_addresses_and_closes_temporary_helper(
    helper,
):
    assert native.list_resources() == (
        "GPIB0::5::INSTR",
        "GPIB0::9::INSTR",
        "GPIB0::17::INSTR",
    )
    assert not native._BRIDGES


def test_gpib_discovery_reuses_open_adapter_without_reset(helper, monkeypatch):
    handle = native.NativeGpibHandle("GPIB0::5::INSTR")
    try:
        monkeypatch.setattr(
            native.subprocess,
            "run",
            lambda *args, **kwargs: (_ for _ in ()).throw(
                AssertionError("Adapter reopened")
            ),
        )
        assert "GPIB0::17::INSTR" in native.list_resources()
        assert handle.session == 5
        assert handle.bridge.references == 1
        assert handle.read() == "1.0"
    finally:
        handle.close()


def test_gpib_discovery_does_not_use_dead_cached_bridge(helper):
    handle = native.NativeGpibHandle("GPIB0::5::INSTR")
    handle.bridge.stop()
    assert "GPIB0::17::INSTR" in native.list_resources()
    with pytest.raises(RuntimeError, match="closed"):
        handle.close()
    assert not native._BRIDGES


@pytest.mark.parametrize("available", ["0", "bad"])
def test_gpib_discovery_absent_and_invalid_adapter(helper, monkeypatch, available):
    from types import SimpleNamespace

    monkeypatch.setattr(
        native.subprocess,
        "run",
        lambda *args, **kwargs: SimpleNamespace(stdout=available),
    )
    if available == "0":
        assert native.list_resources() == ()
    else:
        with pytest.raises(RuntimeError, match="Invalid"):
            native.list_resources()


def test_gpib_discovery_missing_helper(monkeypatch, tmp_path):
    monkeypatch.setattr(native.sys, "platform", "linux")
    monkeypatch.delenv("KOHDALAB_IV_GPIB_HELPER", raising=False)
    with pytest.raises(RuntimeError, match="missing"):
        native.list_resources()
    monkeypatch.setenv("KOHDALAB_IV_GPIB_HELPER", str(tmp_path / "missing"))
    with pytest.raises(RuntimeError, match="missing"):
        native.list_resources()


def test_gpib_discovery_scan_error_still_releases_helper(helper, monkeypatch):
    operations = []
    original = native._Bridge.request

    def request(self, operation, *args, **kwargs):
        operations.append(operation)
        if operation == "LIST":
            raise RuntimeError("Bus disconnected")
        return original(self, operation, *args, **kwargs)

    monkeypatch.setattr(native._Bridge, "request", request)
    with pytest.raises(RuntimeError, match="Bus disconnected"):
        native.list_resources()
    assert operations == ["LIST", "EXIT"]
    assert not native._BRIDGES


def test_measurement_uses_shared_native_transport_and_turns_output_off(
    helper, tmp_path
):
    from kohdalab_iv.api.config import DEFAULT_CONFIG_PATH
    from kohdalab_iv.api.experiment import Experiment

    helper.write_text(
        f"#!{sys.executable}\n"
        + """
import sys
level = 0.0
output = False
print('KIV\\tOK\\t', flush=True)
for line in sys.stdin:
    op, addr, timeout, data = line.split()
    text = bytes.fromhex(data).decode().strip() if data != '-' else ''
    result = ''
    if op == 'WRITE':
        if text.startswith(':SOUR:LEV:FIX '):
            level = float(text.split()[-1])
        elif text == ':OUTP ON':
            output = True
        elif text == ':OUTP OFF':
            output = False
    elif op == 'QUERY':
        if text == '*IDN?':
            result = 'YOKOGAWA,GS210,TEST,1.0' if addr == '5' else 'Agilent Technologies,34411A,TEST,1.0'
        elif text == ':OUTP?':
            result = str(int(output))
        elif text.startswith(':SOUR:LEV'):
            result = str(level)
        elif text == 'READ?':
            result = str(level / 1000 if output else 0)
        else:
            result = '0,No error'
    print('KIV\\tOK\\t' + result.encode().hex(), flush=True)
    if op == 'EXIT':
        break
"""
    )
    config = json.loads(DEFAULT_CONFIG_PATH.read_text())
    config["instruments"]["source"] = {
        "gs210": dict(
            config["instruments"]["source"]["gs210"], resource="GPIB0::5::INSTR"
        )
    }
    config["instruments"]["meter"] = {
        "dmm_34411a": dict(
            config["instruments"]["meter"]["dmm_34411a"], resource="GPIB0::9::INSTR"
        )
    }
    config["roles"] = {
        name: {"source": "source.gs210", "measure": "meter.dmm_34411a"}
        for name in ("iv", "vi")
    }
    for key in (
        "pre_delay_s",
        "start_settle_s",
        "settle_s",
        "post_zero_delay_s",
        "ramp_step_wait_s",
    ):
        config["measurements"]["iv"]["timing"][key] = 0
    experiment = Experiment(config, auto_connect=True)
    try:
        rows = experiment.run_iv(output=tmp_path / "native.csv")
        source = experiment.session.require("source.gs210")
        meter = experiment.session.require("meter.dmm_34411a")
        assert source.inst.bridge is meter.inst.bridge
        assert source.output_state() is False
        assert source.read_level() == 0
        assert rows and (tmp_path / "native.csv").is_file()
        for row in rows:
            assert row["current_A"] == pytest.approx(row["voltage_V"] / 1000)
    finally:
        experiment.disconnect_all()
    assert not native._BRIDGES


def test_pipe_reader_surfaces_os_error_and_posix_poll_timeout(monkeypatch):
    import queue
    from types import SimpleNamespace

    bridge = object.__new__(native._Bridge)
    bridge.label = "test"
    bridge.process = SimpleNamespace(stdout=SimpleNamespace(fileno=lambda: 1))
    bridge.chunks = queue.Queue()
    with monkeypatch.context() as patch:
        patch.setattr(
            native.os,
            "read",
            lambda *args: (_ for _ in ()).throw(OSError("broken pipe")),
        )
        bridge._read_pipe()
    with pytest.raises(RuntimeError, match="pipe reader failed"):
        bridge._chunk(1)
    bridge.chunks = None
    monkeypatch.setattr(native.select, "select", lambda *args: ([], [], []))
    with pytest.raises(TimeoutError):
        bridge._chunk(1)
