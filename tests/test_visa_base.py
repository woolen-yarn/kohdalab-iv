from pyvisa import constants

from kohdalab_iv.instruments.meters.adcmt_7461a import ADCMT7461A
from kohdalab_iv.instruments.meters.agilent_34401a import Agilent34401A
from kohdalab_iv.instruments.meters.keysight_34411a import Keysight34411A
from kohdalab_iv.instruments.visa_base import VisaDevice, _ni4882_set_ren


class FakeFunction:
    def __init__(self, callback):
        self.callback = callback
        self.argtypes = None
        self.restype = None

    def __call__(self, *args):
        return self.callback(*args)


class FakeNi4882Library:
    def __init__(self):
        self.calls = []
        self.ibfindA = FakeFunction(self._ibfind_a)
        self.ibconfig = FakeFunction(self._ibconfig)
        self.ibonl = FakeFunction(self._ibonl)

    def _ibfind_a(self, name):
        self.calls.append(("ibfindA", name))
        return 7

    def _ibconfig(self, ud, option, value):
        self.calls.append(("ibconfig", ud, option, value))
        return 0

    def _ibonl(self, ud, value):
        self.calls.append(("ibonl", ud, value))
        return 0


class FakeVisaLib:
    def __init__(self):
        self.gpib_commands = []

    def gpib_command(self, session, command):
        self.gpib_commands.append((session, command))


class FakeGPIBInterface:
    def __init__(self):
        self.commands = []
        self.ren_modes = []
        self.closed = False

    def send_command(self, command):
        self.commands.append(command)

    def control_ren(self, mode):
        self.ren_modes.append(mode)

    def close(self):
        self.closed = True


class FakeResourceManager:
    def __init__(self):
        self.opened_resources = []
        self.interface = FakeGPIBInterface()

    def open_resource(self, resource):
        self.opened_resources.append(resource)
        return self.interface


class FakeVisaHandle:
    def __init__(self):
        self.commands = []
        self.ren_modes = []
        self.usb_control_outs = []
        self.read_responses = ["0"]
        self.session = object()
        self.invalid_session = False
        self.visalib = FakeVisaLib()
        self._resource_manager = FakeResourceManager()

    def write(self, command):
        self.commands.append(command)

    def query(self, command):
        self.commands.append(command)
        return "0,No error"

    def control_ren(self, mode):
        self.ren_modes.append(mode)

    def get_visa_attribute(self, attribute):
        if self.invalid_session and attribute == constants.VI_ATTR_RSRC_CLASS:
            raise RuntimeError("Invalid session handle")
        if attribute == constants.VI_ATTR_GPIB_PRIMARY_ADDR:
            return 26
        if attribute == constants.VI_ATTR_USB_INTFC_NUM:
            return 3
        return 3

    def control_out(self, request_type, request_id, request_value, index, data=b""):
        self.usb_control_outs.append((request_type, request_id, request_value, index, data))

    def read(self):
        return self.read_responses.pop(0)

    def close(self):
        self.session = None


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
    device = Keysight34411A("GPIB0::26::INSTR", handle=handle)

    device.local()

    assert handle.commands == []
    assert handle._resource_manager.opened_resources == ["GPIB0::INTFC"]
    assert handle._resource_manager.interface.commands == [
        bytes([0x3F, 0x20 + 26, 0x01, 0x3F]),
    ]
    assert handle._resource_manager.interface.ren_modes == [
        constants.VI_GPIB_REN_DEASSERT_GTL,
        constants.VI_GPIB_REN_DEASSERT,
    ]
    assert handle._resource_manager.interface.closed is True
    assert handle.visalib.gpib_commands == [
        (handle.session, bytes([0x3F, 0x20 + 26, 0x01, 0x3F])),
    ]
    assert handle.ren_modes == [
        constants.VI_GPIB_REN_ADDRESS_GTL,
        constants.VI_GPIB_REN_DEASSERT_GTL,
        constants.VI_GPIB_REN_DEASSERT,
    ]
    assert handle.usb_control_outs == [
        (0x21, 0xA1, 0, 3, b""),
        (0x21, 0xA0, 0, 3, b""),
    ]


def test_adcmt_7461a_configures_dc_voltage_and_reads_with_adc_trigger():
    handle = FakeVisaHandle()
    handle.read_responses = ["+1.234500E+00"]
    device = ADCMT7461A("GPIB0::27::INSTR", handle=handle)
    device.READ_DELAY_S = 0

    device.configure_measurement(measure_function="dc_voltage", nplc=1.234, auto_range=True)
    value = device.read_once()

    assert handle.commands == [
        "*RST",
        "H0",
        "F1",
        "R0",
        "ITP1.234",
    ]
    assert value == 1.2345


def test_adcmt_7461a_configures_dc_current_without_auto_range():
    handle = FakeVisaHandle()
    device = ADCMT7461A("GPIB0::27::INSTR", handle=handle)

    device.configure_measurement(measure_function="dc_current", nplc=10, auto_range=False)

    assert handle.commands == [
        "*RST",
        "H0",
        "F5",
        "ITP10",
    ]


def test_adcmt_7461a_rejects_unexpected_measurement_response():
    handle = FakeVisaHandle()
    handle.read_responses = ["ERR"]
    device = ADCMT7461A("GPIB0::27::INSTR", handle=handle)
    device.READ_DELAY_S = 0

    try:
        device.read_once()
    except RuntimeError as e:
        message = str(e)
    else:
        raise AssertionError("Expected ADCMT response error")

    assert "Unexpected ADCMT 7461A measurement response" in message
    assert "ERR" in message


def test_adcmt_7461a_discards_first_reading_after_settle():
    handle = FakeVisaHandle()
    handle.read_responses = ["0.0", "1.2345"]
    device = ADCMT7461A("GPIB0::27::INSTR", handle=handle)
    device.READ_DELAY_S = 0

    device.prepare_for_reading()
    value = device.read_once()

    assert value == 1.2345


def test_adcmt_7461a_connect_status_uses_adc_error_query():
    handle = FakeVisaHandle()
    device = ADCMT7461A("USB0::1::INSTR", handle=handle)
    device.USB_QUERY_DELAY_S = 0

    status = device.connect_status()

    assert status == "0,No error"
    assert handle.commands == ["*CLS", "ERR?"]


def test_visa_device_is_connected_detects_closed_handle():
    handle = FakeVisaHandle()
    device = VisaDevice("GPIB0::26::INSTR", handle=handle)

    assert device.is_connected() is True
    handle.close()

    assert device.is_connected() is False


def test_visa_device_is_connected_detects_invalid_session_handle():
    handle = FakeVisaHandle()
    device = VisaDevice("GPIB0::26::INSTR", handle=handle)

    handle.invalid_session = True

    assert device.is_connected() is False


def test_ni4882_set_ren_deasserts_ren_and_closes_board_handle():
    library = FakeNi4882Library()

    result = _ni4882_set_ren("GPIB0", False, loader=lambda _name: library)

    assert result is True
    assert library.calls == [
        ("ibfindA", b"gpib0"),
        ("ibconfig", 7, 0x000B, 0),
        ("ibonl", 7, 0),
    ]
