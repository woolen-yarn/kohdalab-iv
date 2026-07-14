from __future__ import annotations

import sys
import importlib
from types import SimpleNamespace

import pytest

from kohdalab_iv.instruments.meters.scpi_dmm import ScpiDMM
from kohdalab_iv.instruments.meters.keysight_34411a import Keysight34411A
from kohdalab_iv.instruments.sources.gs210 import YokogawaGS210
from kohdalab_iv.instruments.sources.yokogawa_7651 import Yokogawa7651
from kohdalab_iv.instruments.visa_base import (
    VisaDevice,
    _ni4882_set_ren,
    _release_gpib_remote_pyvisa,
    gpib_board_from_resource,
    release_gpib_remote,
)
from kohdalab_iv.interfaces import common


class Handle:
    def __init__(self, responses=()) -> None:
        self.commands = []
        self.responses = list(responses)
        self.reads = []
        self.session = object()
        self.closed = False

    def write(self, command):
        self.commands.append(command)

    def query(self, command):
        self.commands.append(command)
        if self.responses:
            response = self.responses.pop(0)
            if isinstance(response, Exception):
                raise response
            return response
        return "0"

    def read(self):
        return self.reads.pop(0)

    def close(self):
        self.closed = True
        self.session = None


def test_legacy_meter_modules_export_current_controller_classes() -> None:
    adcmt_module = importlib.import_module("kohdalab_iv.instruments.meters.adcmt_dmm")
    agilent_module = importlib.import_module(
        "kohdalab_iv.instruments.meters.agilent_dmm"
    )

    assert adcmt_module.__all__ == ["ADCMT7461A"]
    assert agilent_module.AgilentDMM is ScpiDMM
    assert set(agilent_module.__all__) == {
        "Agilent34401A",
        "Agilent34411A",
        "AgilentDMM",
        "Keysight34411A",
        "Keysight34465A",
        "ScpiDMM",
    }


@pytest.mark.parametrize(
    ("measure_function", "zero_command"),
    [
        ("dc_voltage", "SENS:VOLT:DC:ZERO:AUTO ON"),
        ("dc_current", "SENS:CURR:DC:ZERO:AUTO ON"),
    ],
)
def test_keysight_34411a_configures_zero_and_immediate_trigger(
    measure_function, zero_command
) -> None:
    handle = Handle()
    device = Keysight34411A("USB0::1::INSTR", handle=handle)

    device.configure_measurement(
        measure_function=measure_function, nplc=1.0, auto_range=False
    )

    assert zero_command in handle.commands
    assert handle.commands[-1] == "TRIG:SOUR IMM"


def test_keysight_34411a_local_control_uses_release_paths(monkeypatch) -> None:
    device = Keysight34411A("GPIB0::1::INSTR", handle=Handle())
    calls = []
    monkeypatch.setattr(
        device, "release_remote_control", lambda: calls.append("release")
    )
    monkeypatch.setattr(
        device,
        "gpib_interface_go_to_local",
        lambda **kwargs: calls.append(("local", kwargs)),
    )

    device.local()
    device.local_after_close()

    assert calls == ["release", ("local", {"release_ren": True})]


def test_gpib_board_parser_accepts_case_and_rejects_non_gpib() -> None:
    assert gpib_board_from_resource("gpib2::7::INSTR") == "GPIB2"
    assert gpib_board_from_resource("USB0::1::INSTR") is None


def test_visa_query_delay_and_numeric_validation(monkeypatch) -> None:
    handle = Handle()
    handle.reads = ["  +1.25E-3 V  "]
    device = VisaDevice("USB0::1::INSTR", handle=handle)
    sleeps = []
    monkeypatch.setattr("kohdalab_iv.instruments.visa_base.time.sleep", sleeps.append)

    assert device.query_float("MEAS?", delay_s=0.25) == pytest.approx(0.00125)
    assert handle.commands == ["MEAS?"]
    assert sleeps == [0.25]

    handle.responses = ["not numeric"]
    with pytest.raises(RuntimeError, match="Unexpected numeric response"):
        device.query_float("BAD?")


def test_visa_clear_local_and_connection_tolerate_minimal_handles() -> None:
    class MinimalHandle:
        session = object()

        def write(self, command):
            raise RuntimeError("unsupported")

        def clear(self):
            raise RuntimeError("unsupported")

        def close(self):
            self.session = None

    handle = MinimalHandle()
    device = VisaDevice("TCPIP0::host::INSTR", handle=handle)

    device.clear()
    device.local()
    assert device.is_connected()
    device.close()
    assert not device.is_connected()


def test_visa_gpib_address_falls_back_to_resource_and_validates_range() -> None:
    class AttributeFailureHandle(Handle):
        def get_visa_attribute(self, attribute):
            raise RuntimeError("attribute unavailable")

    assert VisaDevice(
        "GPIB3::12::INSTR", handle=AttributeFailureHandle()
    )._gpib_location() == ("GPIB3", 12)
    assert (
        VisaDevice(
            "GPIB0::31::INSTR", handle=AttributeFailureHandle()
        )._gpib_primary_address()
        is None
    )
    assert (
        VisaDevice("USB0::1::INSTR", handle=AttributeFailureHandle())._gpib_location()
        is None
    )


def test_visa_usb_control_and_resource_manager_edges() -> None:
    class UsbHandle(Handle):
        def __init__(self):
            super().__init__()
            self.calls = []
            self._resource_manager = SimpleNamespace(
                close=lambda: (_ for _ in ()).throw(RuntimeError("close"))
            )

        def get_visa_attribute(self, attribute):
            raise RuntimeError("no interface attribute")

        def control_out(self, *args):
            self.calls.append(args)
            raise RuntimeError("USB488 unavailable")

    handle = UsbHandle()
    device = VisaDevice("USB0::1::INSTR", handle=handle)
    device.usb_go_to_local()
    device.usb_deassert_ren()
    device.close_resource_manager()

    assert [call[1] for call in handle.calls] == [0xA1, 0xA0]

    VisaDevice("USB0::2::INSTR", handle=Handle()).usb_go_to_local()
    VisaDevice("USB0::2::INSTR", handle=Handle()).close_resource_manager()


def test_release_remote_control_calls_all_available_paths(monkeypatch) -> None:
    device = VisaDevice("GPIB0::1::INSTR", handle=Handle())
    calls = []
    monkeypatch.setattr(
        device,
        "gpib_interface_go_to_local",
        lambda **kwargs: calls.append(("interface", kwargs)),
    )
    monkeypatch.setattr(
        device, "gpib_send_go_to_local", lambda: calls.append(("send", {}))
    )
    monkeypatch.setattr(
        device, "gpib_go_to_local", lambda **kwargs: calls.append(("local", kwargs))
    )
    monkeypatch.setattr(
        device, "usb_go_to_local", lambda: calls.append(("usb-local", {}))
    )
    monkeypatch.setattr(
        device, "usb_deassert_ren", lambda: calls.append(("usb-ren", {}))
    )
    monkeypatch.setattr(
        device, "gpib_deassert_ren", lambda: calls.append(("gpib-ren", {}))
    )

    device.release_remote_control()

    assert calls == [
        ("interface", {"release_ren": True}),
        ("send", {}),
        ("local", {}),
        ("usb-local", {}),
        ("local", {"release_ren": True}),
        ("usb-ren", {}),
        ("gpib-ren", {}),
    ]


def test_ni4882_returns_false_when_libraries_or_board_are_unavailable() -> None:
    assert not _ni4882_set_ren("GPIB0", False)
    assert not _ni4882_set_ren(
        "GPIB0", False, loader=lambda name: (_ for _ in ()).throw(OSError(name))
    )

    library = SimpleNamespace(
        ibfindA=SimpleCallable(lambda name: -1),
        ibonl=SimpleCallable(lambda ud, online: 0),
    )
    assert not _ni4882_set_ren("GPIB0", False, loader=lambda name: library)


class SimpleCallable:
    def __init__(self, callback):
        self.callback = callback
        self.argtypes = None
        self.restype = None

    def __call__(self, *args):
        return self.callback(*args)


def test_ni4882_prefers_ibsre_when_available() -> None:
    calls = []
    library = SimpleNamespace(
        ibfindA=SimpleCallable(lambda name: 4),
        ibsre=SimpleCallable(lambda ud, value: calls.append(("ibsre", ud, value))),
        ibonl=SimpleCallable(lambda ud, value: calls.append(("ibonl", ud, value))),
    )

    assert _ni4882_set_ren("GPIB1", True, loader=lambda name: library)
    assert calls == [("ibsre", 4, 1), ("ibonl", 4, 0)]


def test_release_gpib_remote_uses_both_backends(monkeypatch) -> None:
    calls = []
    monkeypatch.setattr(
        "kohdalab_iv.instruments.visa_base._release_gpib_remote_pyvisa",
        lambda board: calls.append(("pyvisa", board)),
    )
    monkeypatch.setattr(
        "kohdalab_iv.instruments.visa_base._ni4882_set_ren",
        lambda board, asserted: calls.append(("ni4882", board, asserted)),
    )

    release_gpib_remote("GPIB2")
    assert calls == [("pyvisa", "GPIB2"), ("ni4882", "GPIB2", False)]


def test_release_gpib_remote_pyvisa_closes_interface_and_manager(monkeypatch) -> None:
    calls = []

    class Interface:
        def send_command(self, command):
            calls.append(("command", command))

        def control_ren(self, mode):
            calls.append(("ren", mode))

        def close(self):
            calls.append(("interface-close",))

    class Manager:
        def open_resource(self, resource):
            calls.append(("open", resource))
            return Interface()

        def close(self):
            calls.append(("manager-close",))

    fake_pyvisa = SimpleNamespace(
        ResourceManager=lambda: Manager(),
        constants=SimpleNamespace(
            VI_GPIB_REN_DEASSERT_GTL=1,
            VI_GPIB_REN_DEASSERT=2,
        ),
    )
    monkeypatch.setitem(sys.modules, "pyvisa", fake_pyvisa)

    _release_gpib_remote_pyvisa("GPIB4")

    assert ("open", "GPIB4::INTFC") in calls
    assert ("interface-close",) in calls
    assert ("manager-close",) in calls


def test_release_gpib_remote_pyvisa_tolerates_backend_failures(monkeypatch) -> None:
    monkeypatch.setitem(
        sys.modules,
        "pyvisa",
        SimpleNamespace(
            ResourceManager=lambda: (_ for _ in ()).throw(
                RuntimeError("missing backend")
            ),
            constants=SimpleNamespace(),
        ),
    )
    _release_gpib_remote_pyvisa("GPIB0")

    class OpenFailureManager:
        def open_resource(self, resource):
            raise RuntimeError(resource)

        def close(self):
            pass

    monkeypatch.setitem(
        sys.modules,
        "pyvisa",
        SimpleNamespace(
            ResourceManager=lambda: OpenFailureManager(),
            constants=SimpleNamespace(
                VI_GPIB_REN_DEASSERT_GTL=1,
                VI_GPIB_REN_DEASSERT=2,
            ),
        ),
    )
    _release_gpib_remote_pyvisa("GPIB0")

    class BrokenInterface:
        def send_command(self, command):
            raise RuntimeError("command")

        def control_ren(self, mode):
            raise RuntimeError("ren")

        def close(self):
            raise RuntimeError("close")

    class BrokenManager:
        def open_resource(self, resource):
            return BrokenInterface()

        def close(self):
            raise RuntimeError("close")

    monkeypatch.setitem(
        sys.modules,
        "pyvisa",
        SimpleNamespace(
            ResourceManager=lambda: BrokenManager(),
            constants=SimpleNamespace(
                VI_GPIB_REN_DEASSERT_GTL=1,
                VI_GPIB_REN_DEASSERT=2,
            ),
        ),
    )
    _release_gpib_remote_pyvisa("GPIB0")


def test_common_visa_helpers_configure_and_close_resource_manager(monkeypatch) -> None:
    handle = SimpleNamespace(
        timeout=None, write_termination=None, read_termination=None
    )
    manager = SimpleNamespace(
        closed=False,
        open_resource=lambda resource: handle,
        list_resources=lambda: ("GPIB0::1::INSTR",),
    )

    def close():
        manager.closed = True

    manager.close = close
    monkeypatch.setitem(
        sys.modules, "pyvisa", SimpleNamespace(ResourceManager=lambda: manager)
    )

    assert common.open_visa("GPIB0::1::INSTR", timeout_ms=1234) is handle
    assert handle.timeout == 1234
    assert handle.write_termination == "\n"
    assert handle.read_termination == "\n"
    assert common.list_visa_resources() == ("GPIB0::1::INSTR",)
    assert manager.closed


def test_visa_identify_gpib_and_interface_failure_paths(monkeypatch) -> None:
    handle = Handle(["ACME,MODEL"])
    device = VisaDevice("GPIB0::8::INSTR", handle=handle)
    assert device.identify() == "ACME,MODEL"

    device.gpib_send_go_to_local()
    device.gpib_deassert_ren()

    non_gpib = VisaDevice("USB0::1::INSTR", handle=Handle())
    non_gpib.gpib_send_go_to_local()
    non_gpib.gpib_interface_go_to_local()

    invalid_address = VisaDevice("GPIB0::31::INSTR", handle=Handle())
    assert invalid_address._gpib_location() is None

    monkeypatch.setitem(
        sys.modules,
        "pyvisa",
        SimpleNamespace(
            ResourceManager=lambda: (_ for _ in ()).throw(RuntimeError("no manager"))
        ),
    )
    no_manager = VisaDevice("GPIB0::1::INSTR", handle=Handle())
    no_manager.gpib_interface_go_to_local()

    class Interface:
        def __init__(self):
            self.closed = False

        def send_command(self, command):
            assert command

        def close(self):
            self.closed = True

    interface = Interface()
    existing_manager_handle = Handle()
    existing_manager_handle._resource_manager = SimpleNamespace(
        open_resource=lambda _resource: interface
    )
    VisaDevice(
        "GPIB0::1::INSTR", handle=existing_manager_handle
    ).gpib_interface_go_to_local(release_ren=False)
    assert interface.closed

    open_failure_handle = Handle()
    open_failure_handle._resource_manager = SimpleNamespace(
        open_resource=lambda resource: (_ for _ in ()).throw(RuntimeError(resource))
    )
    VisaDevice(
        "GPIB0::1::INSTR", handle=open_failure_handle
    ).gpib_interface_go_to_local()

    class Interface:
        def send_command(self, command):
            raise RuntimeError("command failed")

        def close(self):
            raise RuntimeError("close failed")

    class Manager:
        def open_resource(self, resource):
            return Interface()

        def close(self):
            raise RuntimeError("close failed")

    no_manager_handle = Handle()
    no_manager_handle.get_visa_attribute = lambda attribute: 8
    monkeypatch.setitem(
        sys.modules,
        "pyvisa",
        SimpleNamespace(
            ResourceManager=lambda: Manager(),
            constants=SimpleNamespace(
                VI_ATTR_GPIB_PRIMARY_ADDR=1,
                VI_GPIB_REN_DEASSERT_GTL=2,
                VI_GPIB_REN_DEASSERT=3,
            ),
        ),
    )
    VisaDevice("GPIB0::8::INSTR", handle=no_manager_handle).gpib_interface_go_to_local(
        release_ren=True
    )


def test_ni4882_tolerates_call_and_cleanup_failures() -> None:
    missing_find = SimpleNamespace()
    assert not _ni4882_set_ren("GPIB0", False, loader=lambda name: missing_find)

    library = SimpleNamespace(
        ibfindA=SimpleCallable(lambda name: 2),
        ibconfig=SimpleCallable(
            lambda *args: (_ for _ in ()).throw(RuntimeError("config"))
        ),
        ibonl=SimpleCallable(
            lambda *args: (_ for _ in ()).throw(RuntimeError("close"))
        ),
    )
    assert not _ni4882_set_ren("GPIB0", False, loader=lambda name: library)


def test_gs210_supports_both_source_modes_and_readback_fallbacks() -> None:
    voltage_handle = Handle([RuntimeError("unsupported"), "2.5"])
    voltage = YokogawaGS210("USB0::1::INSTR", handle=voltage_handle)
    voltage.configure_source(
        source_function="voltage", source_range=10, hardware_compliance=0.01
    )
    assert voltage.read_level() == 2.5
    voltage.output_on()
    voltage.output_off()

    current_handle = Handle(["1"])
    current = YokogawaGS210("USB0::2::INSTR", handle=current_handle)
    current.configure_source(
        source_function="current", source_range=0.01, hardware_compliance=5
    )
    assert current.output_state() is True

    assert voltage_handle.commands[:4] == [
        ":SOUR:FUNC VOLT",
        ":SOUR:RANG 10",
        ":SOUR:PROT:CURR 0.01",
        ":SOUR:LEV:FIX 0",
    ]
    assert current_handle.commands[:4] == [
        ":SOUR:FUNC CURR",
        ":SOUR:RANG 0.01",
        ":SOUR:PROT:VOLT 5",
        ":SOUR:LEV:FIX 0",
    ]


def test_gs210_and_7651_reject_invalid_modes_and_responses() -> None:
    gs210 = YokogawaGS210("USB0::1::INSTR", handle=Handle(["bad", "bad", "bad", "bad"]))
    with pytest.raises(ValueError, match="Unsupported GS210"):
        gs210.configure_source(
            source_function="resistance", source_range=1, hardware_compliance=1
        )
    assert gs210.read_level() is None
    assert gs210.output_state() is None

    source_7651 = Yokogawa7651(
        "GPIB0::1::INSTR", handle=Handle([RuntimeError("no id")])
    )
    assert source_7651.identify() == "Yokogawa 7651"
    with pytest.raises(ValueError, match="Unsupported 7651"):
        source_7651.configure_source(
            source_function="bad", source_range=1, hardware_compliance=1
        )
    with pytest.raises(ValueError, match="range code"):
        source_7651._range_code(1.5)

    empty_identity = Yokogawa7651("GPIB0::2::INSTR", handle=Handle([""]))
    assert empty_identity.identify() == "Yokogawa 7651"
    bad_readback = Yokogawa7651(
        "GPIB0::3::INSTR",
        handle=Handle(["not numeric", "status unknown", RuntimeError("offline")]),
    )
    assert bad_readback.read_level() is None
    assert bad_readback.output_state() is None
    assert bad_readback.output_state() is None


def test_scpi_dmm_configures_modes_averages_and_reports_status() -> None:
    handle = Handle(["1", "2", '0,"No error"'])
    dmm = ScpiDMM("USB0::1::INSTR", handle=handle)
    dmm.configure_measurement(measure_function="dc_voltage", nplc=1, auto_range=True)
    dmm.configure_measurement(measure_function="dc_current", nplc=0.2, auto_range=False)
    assert dmm.read_average(2) == 1.5
    assert dmm.connect_status() == '0,"No error"'
    with pytest.raises(ValueError, match="Unsupported DMM"):
        dmm.configure_measurement(measure_function="frequency", nplc=1)


def test_scpi_dmm_covers_auto_range_and_best_effort_write() -> None:
    handle = Handle(["3"])
    dmm = ScpiDMM("USB0::1::INSTR", handle=handle)
    dmm.configure_measurement(measure_function="dc_voltage", nplc=2, auto_range=False)
    dmm.configure_measurement(measure_function="dc_current", nplc=3, auto_range=True)
    assert dmm.read_average(0) == 3.0
    dmm._try_write("TRIG:SOUR IMM")

    class BrokenWriteHandle(Handle):
        def write(self, command):
            raise RuntimeError("unsupported")

    ScpiDMM("USB0::2::INSTR", handle=BrokenWriteHandle())._try_write("OPTIONAL")
