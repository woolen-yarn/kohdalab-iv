from __future__ import annotations

import json
import subprocess
import sys
from types import SimpleNamespace

import pytest

from kohdalab_iv.interfaces import common, native_usb as native

RESOURCE = "USB0::0x0957::0x0A07::MY53000981::INSTR"


@pytest.fixture
def usb_helper(tmp_path, monkeypatch):
    path = tmp_path / "usb-helper"
    path.write_text(
        f"#!{sys.executable}\n"
        + """
import sys
if sys.argv[1] == '--list':
    print('KIV\\tOK\\t' + b'USB0::0x0957::0x0A07::MY53000981::INSTR\\n'.hex(), flush=True)
    sys.exit(0)
assert (sys.argv[1], *sys.argv[2:]) in [('MY53000981', '0x957', '0xa07'), ('91N928756', '0xb21', '0x39')]
print('KIV\\tOK\\t', flush=True)
for line in sys.stdin:
    op, addr, timeout, data = line.split()
    assert addr == '0'
    text = bytes.fromhex(data).decode() if data != '-' else ''
    if text.strip() == 'FAIL':
        print('KIV\\tERR\\t' + b'USB transfer failed'.hex(), flush=True)
    else:
        result = text if op == 'QUERY' else ('1.25e-6' if op == 'READ' else '')
        if op == 'QUERY' and text.strip() == 'READ?': result = '1.25e-6'
        print('KIV\\tOK\\t' + result.encode().hex(), flush=True)
    if op == 'EXIT':
        break
"""
    )
    path.chmod(0o755)
    monkeypatch.setenv("KOHDALAB_IV_USB_HELPER", str(path))
    return path


def test_usb_helper_discovery(monkeypatch, tmp_path):
    monkeypatch.delenv("KOHDALAB_IV_USB_HELPER", raising=False)
    monkeypatch.setattr(native.sys, "platform", "linux")
    assert native.helper_path() is None and not native.enabled()
    with pytest.raises(RuntimeError, match="missing"):
        native.NativeUsbHandle(RESOURCE)
    monkeypatch.setattr(native.sys, "platform", "darwin")
    monkeypatch.setattr(native.sys, "frozen", False, raising=False)
    assert native.helper_path() is None
    monkeypatch.setattr(native.sys, "frozen", True)
    monkeypatch.setattr(native.sys, "_MEIPASS", str(tmp_path), raising=False)
    assert native.helper_path() == tmp_path / "kohdalab-usbtmc-helper"
    monkeypatch.setenv("KOHDALAB_IV_USB_HELPER", str(tmp_path / "missing"))
    assert not native.enabled()
    with pytest.raises(RuntimeError, match="missing"):
        native.NativeUsbHandle(RESOURCE)


@pytest.mark.parametrize(
    "resource",
    [
        "USB0::1::INSTR",
        "USB1::2391::2567::MY53000981::INSTR",
        "USB0::0x0957::0x0001::MY53000981::INSTR",
        "USB0::2391::1::S::INSTR",
        "USB0::1::2567::S::INSTR",
    ],
)
def test_usb_rejects_unsupported_device(resource):
    with pytest.raises(ValueError, match="34411A"):
        native.NativeUsbHandle(resource)


@pytest.mark.parametrize("resource", [RESOURCE, "USB0::2391::2567::MY53000981::INSTR"])
def test_usb_transport_lifecycle_and_error(usb_helper, resource):
    handle = common.open_visa(resource, 10000)
    try:
        assert handle.session == 0 and handle.timeout == 10000
        assert common.list_visa_resources() == (RESOURCE,)
        assert handle.query("*IDN?\n") == "*IDN?\n"
        handle.write("CONF:CURR:DC\n")
        assert handle.read() == "1.25e-6"
        with pytest.raises(NotImplementedError, match="clear"):
            handle.clear()
        for mode in (6, 0, 2):
            handle.control_ren(mode)
        with pytest.raises(ValueError, match="REN mode"):
            handle.control_ren(99)
        with pytest.raises(RuntimeError, match="USB transfer failed"):
            handle.query("FAIL")
    finally:
        handle.close()
    handle.close()
    assert handle.session is None and handle.bridge.process.poll() is not None
    with pytest.raises(RuntimeError, match="resource is closed"):
        handle.read()


def test_usb_dead_transport_cleanup(usb_helper):
    handle = native.NativeUsbHandle(RESOURCE)
    handle.bridge.stop()
    assert handle.session is None
    with pytest.raises(RuntimeError, match="USBTMC connection is closed"):
        handle.close()
    handle.close()


def test_usb_list_requires_helper(monkeypatch, tmp_path):
    monkeypatch.delenv("KOHDALAB_IV_USB_HELPER", raising=False)
    monkeypatch.setattr(native.sys, "platform", "linux")
    with pytest.raises(RuntimeError, match="missing"):
        native.list_resources()
    monkeypatch.setenv("KOHDALAB_IV_USB_HELPER", str(tmp_path / "missing"))
    with pytest.raises(RuntimeError, match="missing"):
        native.list_resources()


@pytest.mark.parametrize(
    "output, code, error",
    [
        ("garbage\n", 0, "invalid response"),
        ("KIV\tERR\t" + b"Cannot open USB".hex() + "\n", 0, "Cannot open USB"),
        ("KIV\tOK\t\n", 1, "USB discovery failed"),
    ],
)
def test_usb_list_errors_are_visible(usb_helper, monkeypatch, output, code, error):
    monkeypatch.setattr(
        native.subprocess,
        "run",
        lambda *args, **kwargs: SimpleNamespace(stdout=output, returncode=code),
    )
    with pytest.raises(RuntimeError, match=error):
        native.list_resources()


def test_usb_list_empty_and_timeout(usb_helper, monkeypatch):
    monkeypatch.setattr(
        native.subprocess,
        "run",
        lambda *args, **kwargs: SimpleNamespace(stdout="KIV\tOK\t\n", returncode=0),
    )
    assert native.list_resources() == ()

    def timeout(*args, **kwargs):
        raise subprocess.TimeoutExpired(args[0], kwargs["timeout"])

    monkeypatch.setattr(native.subprocess, "run", timeout)
    with pytest.raises(subprocess.TimeoutExpired):
        native.list_resources()


def test_usb_discovery_merges_visa_without_duplicate_resources(usb_helper, monkeypatch):
    import pyvisa

    closed = []

    class Manager:
        def list_resources(self):
            return (RESOURCE, "GPIB0::5::INSTR")

        def close(self):
            closed.append(True)

    monkeypatch.setattr(pyvisa, "ResourceManager", Manager)
    assert common.list_visa_resources() == (RESOURCE, "GPIB0::5::INSTR")
    assert closed == [True]


@pytest.mark.parametrize("native_enabled", [False, True])
def test_missing_visa_runtime_fallback(monkeypatch, native_enabled):
    import pyvisa
    from kohdalab_iv.interfaces import native_gpib

    def missing():
        raise ValueError("VISA missing")

    monkeypatch.setattr(native, "enabled", lambda: False)
    monkeypatch.setattr(native_gpib, "enabled", lambda: native_enabled)
    monkeypatch.setattr(native_gpib, "list_resources", lambda: ())
    monkeypatch.setattr(pyvisa, "ResourceManager", missing)
    if native_enabled:
        assert common.list_visa_resources() == ()
    else:
        with pytest.raises(ValueError, match="VISA missing"):
            common.list_visa_resources()


@pytest.mark.parametrize("fail_meter", [False, True])
def test_mixed_gpib_usb_measurement_and_output_off(
    usb_helper, tmp_path, monkeypatch, fail_meter
):
    from kohdalab_iv.api.config import DEFAULT_CONFIG_PATH
    from kohdalab_iv.api.experiment import Experiment
    from kohdalab_iv.interfaces import native_gpib

    # This integration simulator must identify the modeled physical meter.
    usb_helper.write_text(
        usb_helper.read_text().replace(
            "        if op == 'QUERY' and text.strip() == 'READ?':",
            "        if op == 'QUERY' and text.strip() == '*IDN?': result = 'Agilent Technologies,34411A,TEST,1.0'\n"
            "        if op == 'QUERY' and text.strip() == 'READ?':",
        )
    )
    gpib = tmp_path / "gpib-helper"
    gpib.write_text(
        f"#!{sys.executable}\n"
        + """
import sys
level = 0.0
output = False
print('KIV\\tOK\\t', flush=True)
for line in sys.stdin:
    op, addr, timeout, data = line.split()
    assert addr == '5' or op == 'REN'
    text = bytes.fromhex(data).decode().strip() if data != '-' else ''
    result = ''
    if op == 'WRITE':
        if text.startswith(':SOUR:LEV:FIX '): level = float(text.split()[-1])
        if text == ':OUTP ON': output = True
        if text == ':OUTP OFF': output = False
    if op == 'QUERY':
        result = 'YOKOGAWA,GS210,TEST,1.0' if text == '*IDN?' else (str(int(output)) if text == ':OUTP?' else str(level))
    print('KIV\\tOK\\t' + result.encode().hex(), flush=True)
    if op == 'EXIT': break
"""
    )
    gpib.chmod(0o755)
    monkeypatch.setenv("KOHDALAB_IV_GPIB_HELPER", str(gpib))
    config = json.loads(DEFAULT_CONFIG_PATH.read_text())
    config["instruments"]["source"] = {
        "gs210": {
            **config["instruments"]["source"]["gs210"],
            "resource": "GPIB0::5::INSTR",
        }
    }
    config["instruments"]["meter"] = {
        "dmm_34411a": {
            **config["instruments"]["meter"]["dmm_34411a"],
            "resource": RESOURCE,
        }
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
        source = experiment.session.require("source.gs210")
        meter = experiment.session.require("meter.dmm_34411a")
        assert source.inst.bridge is not meter.inst.bridge
        if fail_meter:
            monkeypatch.setattr(meter, "read_once", lambda: meter.inst.query("FAIL"))
            with pytest.raises(RuntimeError, match="USB transfer failed"):
                experiment.run_iv(output=tmp_path / "mixed.csv")
        else:
            rows = experiment.run_iv(output=tmp_path / "mixed.csv")
            assert rows and all(
                row["current_A"] == pytest.approx(1.25e-6) for row in rows
            )
        assert source.output_state() is False
        if not fail_meter:
            assert source.read_level() == 0
    finally:
        experiment.disconnect_all()
    assert not native_gpib._BRIDGES


def test_gs210_usb_identity_and_session(usb_helper):
    handle = common.open_visa("USB0::0x0B21::0x0039::91N928756::INSTR")
    try:
        assert handle.session == 0
        assert handle.query("*IDN?") == "*IDN?\n"
    finally:
        handle.close()


def test_native_portable_does_not_enter_vendor_visa_discovery(monkeypatch):
    import pyvisa
    from kohdalab_iv.interfaces import native_gpib

    monkeypatch.setattr(sys, "frozen", True, raising=False)
    monkeypatch.setattr(native, "enabled", lambda: True)
    monkeypatch.setattr(native_gpib, "enabled", lambda: True)
    monkeypatch.setattr(native, "list_resources", lambda: (RESOURCE,))
    monkeypatch.setattr(native_gpib, "list_resources", lambda: ("GPIB0::3::INSTR",))

    def forbidden():
        raise AssertionError("Portable discovery must not load external VISA")

    monkeypatch.setattr(pyvisa, "ResourceManager", forbidden)
    stages = []
    token = common.DISCOVERY_PROGRESS.set(stages.append)
    try:
        for _ in range(3):
            assert common.list_visa_resources() == (RESOURCE, "GPIB0::3::INSTR")
    finally:
        common.DISCOVERY_PROGRESS.reset(token)
    assert len(stages) == 6
    assert all("external VISA" not in stage for stage in stages)
