from pyvisa import constants

from kohdalab_iv.instruments.meters.agilent_dmm import Agilent34401A, Agilent34411A
from kohdalab_iv.instruments.visa_base import VisaDevice


class FakeVisaHandle:
    def __init__(self):
        self.commands = []
        self.ren_modes = []

    def write(self, command):
        self.commands.append(command)

    def control_ren(self, mode):
        self.ren_modes.append(mode)

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

    assert handle.commands == []
    assert handle.ren_modes == [constants.VI_GPIB_REN_DEASSERT_GTL]
