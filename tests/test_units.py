from decimal import Decimal

import pytest

from kohdalab_iv.api.units import parse_quantity


def test_parse_voltage_quantity_to_si():
    quantity = parse_quantity({"value": 100, "unit": "mV"}, dimension="voltage")

    assert quantity.si_unit == "V"
    assert quantity.si_value == Decimal("0.100")


def test_parse_micro_alias():
    quantity = parse_quantity({"value": 2.5, "unit": "microA"}, dimension="current")

    assert quantity.si_value == Decimal("0.0000025")


def test_reject_wrong_dimension():
    with pytest.raises(ValueError):
        parse_quantity({"value": 1, "unit": "mA"}, dimension="voltage")
