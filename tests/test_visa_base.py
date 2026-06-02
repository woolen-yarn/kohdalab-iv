from pyvisa import constants

from kohdalab_iv.instruments.meters.agilent_dmm import Agilent34401A, Agilent34411A
from kohdalab_iv.instruments.visa_base import VisaDevice


class FakeVisaHandle:
    def __init__(self):
        self.commands = []
        self.ren_modes = []
        self.usb_control_outs = []

    def write(self, command):
        self.commands.append(command)

    def control_ren(self, mode):
        self.ren_modes.append(mode)

    def get_visa_attribute(self, attribute):
        return 3

    def control_out(self, request_type, request_id, request_value, index, data=b""):
        self.usb_control_outs.append((request_type, request_id, request_value, index, data))

    def close(self):
        pass


def test_local_sends_scpi_local_and_gpib_gtl():
    handle = FakeVisaHandle()
    device = VisaDevice("GPIB0::1::INSTR", handle=handle)

    device.local()

    assert handle.commands == ["SYST:LOC"]
    assert handle.ren_modes == [constants.VI_GPIB_REN_ADDRESS_GTL]


def test_34401a_local_uses_gpib_gtl_without_scpi_system_local():
    handle = FakeVisaHandle()
    device = Agilent34401A("GPIB0::26::INSTR", handle=handle)

    device.local()

    assert handle.commands == []
    assert handle.ren_modes == [constants.VI_GPIB_REN_ADDRESS_GTL]


def test_34411a_local_uses_gpib_gtl_with_ren_release():
    handle = FakeVisaHandle()
    device = Agilent34411A("GPIB0::26::INSTR", handle=handle)

    device.local()

    assert handle.commands == ["SYST:LOC"]
    assert handle.ren_modes == [
        constants.VI_GPIB_REN_ADDRESS_GTL,
        constants.VI_GPIB_REN_DEASSERT_GTL,
        constants.VI_GPIB_REN_DEASSERT,
    ]
    assert handle.usb_control_outs == [
        (0x21, 0xA1, 0, 3, b""),
        (0x21, 0xA0, 0, 3, b""),
    ]
