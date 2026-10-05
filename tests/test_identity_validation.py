from __future__ import annotations

import copy

import pytest

from kohdalab_iv.api.config import DEFAULT_CONFIG
from kohdalab_iv.api.session import DeviceSession
from kohdalab_iv.instruments import visa_base


class Handle:
    def __init__(self, identity):
        self.identity = identity
        self.commands = []
        self.closed = False
        self.session = 1

    def query(self, command):
        self.commands.append(command)
        if isinstance(self.identity, Exception):
            raise self.identity
        return self.identity

    def write(self, command):
        self.commands.append(command)
        raise AssertionError("No configuration command before identity validation")

    def close(self):
        self.closed = True
        self.session = None


CASES = [
    ("source", "YOKOGAWA_GS210", "YOKOGAWA,GS210,SERIAL,2.01"),
    ("meter", "AGILENT_34411A", "Agilent Technologies,34411A,SERIAL,2.21"),
    ("meter", "KEYSIGHT_34411A", "Agilent Technologies,34411A,SERIAL,2.21"),
    ("meter", "AGILENT_34411A", "Keysight Technologies,34411A,SERIAL,2.21"),
    ("meter", "AGILENT_34401A", "HEWLETT-PACKARD,34401A,SERIAL,1.0"),
    ("meter", "KEYSIGHT_34465A", "Keysight Technologies,34465A,SERIAL,1.0"),
    ("meter", "ADCMT_7461A", "ADC,AD7461A,SERIAL,1.0"),
    ("source", "YOKOGAWA_7651", "7651 REV1.0\nF1R2S+0.000000E+00"),
]


def make_session(kind, model, resource):
    config = copy.deepcopy(DEFAULT_CONFIG)
    config["instruments"] = {kind: {"test": {"model": model, "resource": resource}}}
    return DeviceSession(config, auto_connect=False), f"{kind}.test"


@pytest.mark.parametrize(
    "resource", ["GPIB0::3::INSTR", "USB0::0x0957::0x0A07::SERIAL::INSTR"]
)
@pytest.mark.parametrize("kind,model,identity", CASES)
def test_matching_model_is_verified_before_registration(
    monkeypatch, resource, kind, model, identity
):
    handle = Handle(identity)
    monkeypatch.setattr(visa_base, "open_visa", lambda *args, **kwargs: handle)
    session, ref = make_session(kind, model, resource)
    device = session.connect_device(ref)
    assert device.inst is handle
    assert session.connected_devices() == {ref: True}
    assert handle.commands == (["OS;E"] if model == "YOKOGAWA_7651" else ["*IDN?"])
    assert not handle.closed
    handle.close()


@pytest.mark.parametrize(
    "resource", ["GPIB0::3::INSTR", "USB0::0x0957::0x0A07::SERIAL::INSTR"]
)
@pytest.mark.parametrize(
    "kind,model,identity",
    [
        ("source", "YOKOGAWA_GS210", "Agilent Technologies,34411A,SERIAL,1.0"),
        ("meter", "KEYSIGHT_34411A", "YOKOGAWA,GS210,SERIAL,1.0"),
        ("meter", "KEYSIGHT_34465A", "Keysight Technologies,34411A,SERIAL,1.0"),
        ("source", "YOKOGAWA_GS210", "YOKOGAWA,GS2100,SERIAL,1.0"),
        ("source", "YOKOGAWA_GS210", "OTHER,GS210,SERIAL,1.0"),
        ("meter", "KEYSIGHT_34411A", ""),
        ("meter", "KEYSIGHT_34411A", "34411A"),
        ("source", "YOKOGAWA_7651", ""),
        ("source", "YOKOGAWA_7651", "YOKOGAWA,GS210,SERIAL,1.0"),
        ("source", "YOKOGAWA_7651", "Agilent Technologies,34411A,7651,1.0"),
    ],
)
def test_mismatched_identity_closes_without_configuration(
    monkeypatch, resource, kind, model, identity
):
    handle = Handle(identity)
    monkeypatch.setattr(visa_base, "open_visa", lambda *args, **kwargs: handle)
    session, ref = make_session(kind, model, resource)
    with pytest.raises(ValueError, match="Instrument mismatch") as error:
        session.connect_device(ref)
    assert resource in str(error.value)
    assert handle.closed
    assert session.connected_devices() == {ref: False}
    assert handle.commands == (["OS;E"] if model == "YOKOGAWA_7651" else ["*IDN?"])


@pytest.mark.parametrize("model", ["YOKOGAWA_GS210", "YOKOGAWA_7651"])
def test_id_failure_does_not_fabricate_identity(monkeypatch, model):
    handle = Handle(TimeoutError("device did not answer"))
    monkeypatch.setattr(visa_base, "open_visa", lambda *args, **kwargs: handle)
    session, ref = make_session("source", model, "GPIB0::3::INSTR")
    with pytest.raises(RuntimeError, match="Cannot verify"):
        session.connect_device(ref)
    assert handle.closed
    assert session.connected_devices() == {ref: False}
    assert len(handle.commands) == 1


def test_reused_connection_is_reverified_and_removed_on_mismatch(monkeypatch):
    handle = Handle("YOKOGAWA,GS210,SERIAL,1.0")
    monkeypatch.setattr(visa_base, "open_visa", lambda *args, **kwargs: handle)
    session, ref = make_session("source", "YOKOGAWA_GS210", "GPIB0::3::INSTR")
    session.connect_device(ref)
    handle.identity = "Agilent Technologies,34411A,SERIAL,1.0"
    with pytest.raises(ValueError, match="Instrument mismatch"):
        session.connect_device(ref)
    assert handle.commands == ["*IDN?", "*IDN?"]
    assert handle.closed
    assert session.connected_devices() == {ref: False}
