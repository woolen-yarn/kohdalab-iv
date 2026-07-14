import pytest

from kohdalab_iv import __version__
from kohdalab_iv.apps.iv_gui import (
    _device_key_for_selection,
    _model_config_for_selection,
    _quantity_value,
    _resistance_from_rows,
    _set_quantity,
    _short,
    _unit_family,
    _window_title,
)


def test_window_title_includes_package_version():
    assert _window_title() == f"KohdaLab IV v{__version__}"


def test_meter_selection_prefers_canonical_34411a_key_over_stale_7461a_entry():
    config = {
        "instruments": {
            "meter": {
                "dmm_7461a": {
                    "model": "KEYSIGHT_34411A",
                    "resource": "GPIB0::27::INSTR",
                },
                "dmm_34411a": {
                    "model": "KEYSIGHT_34411A",
                    "resource": "USB0::1::INSTR",
                },
            }
        }
    }

    key = _device_key_for_selection(config, "meter", "KEYSIGHT_34411A", "dmm_7461a")
    selected_key, cfg = _model_config_for_selection(config, "meter", "KEYSIGHT_34411A")

    assert key == "dmm_34411a"
    assert selected_key == "dmm_34411a"
    assert cfg["resource"] == "USB0::1::INSTR"


def test_meter_selection_prefers_canonical_agilent_34411a_key():
    config = {
        "instruments": {
            "meter": {
                "dmm_7461a": {
                    "model": "AGILENT_34411A",
                    "resource": "GPIB0::27::INSTR",
                },
                "dmm_agilent_34411a": {
                    "model": "AGILENT_34411A",
                    "resource": "USB0::2::INSTR",
                },
            }
        }
    }

    key = _device_key_for_selection(config, "meter", "AGILENT_34411A", "dmm_7461a")

    assert key == "dmm_agilent_34411a"


def test_model_selection_falls_back_to_matching_noncanonical_entry():
    config = {
        "instruments": {
            "source": {
                "lab_source": {
                    "model": " yokogawa_gs210 ",
                    "resource": "GPIB0::1::INSTR",
                }
            }
        }
    }

    key, selected = _model_config_for_selection(config, "source", "YOKOGAWA_GS210")

    assert key == "lab_source"
    assert selected == config["instruments"]["source"]["lab_source"]


def test_model_selection_handles_invalid_or_missing_device_maps():
    assert _model_config_for_selection(
        {"instruments": {"meter": []}}, "meter", "ADCMT_7461A"
    ) == (None, None)
    assert _model_config_for_selection({}, "meter", "ADCMT_7461A") == (None, None)
    assert _device_key_for_selection({}, "meter", "ADCMT_7461A", "current") == "current"


def test_quantity_helpers_round_trip_and_support_legacy_scalars():
    target = {}
    _set_quantity(target, "compliance", 0.125, "mA")

    assert target == {"compliance": {"value": 0.125, "unit": "mA"}}
    assert _quantity_value(target, "compliance", 1.0, "A") == (0.125, "mA")
    assert _quantity_value({"step": 2}, "step", 1.0, "V") == (2.0, "V")
    assert _quantity_value({}, "step", 1.0, "V") == (1.0, "V")


def test_short_and_unit_family_format_display_values():
    assert _short(None) == "-"
    assert _short(0.000001234567, "A") == "1.23457e-06 A"
    assert _short("ready") == "ready"
    assert _unit_family("mA") == "current"
    assert _unit_family("uV") == "voltage"
    assert _unit_family("kOhm") is None


def test_resistance_estimate_uses_linear_regression_slope():
    rows = [
        {"measured_A": -0.001, "measured_V": -1.0},
        {"measured_A": 0.0, "measured_V": 0.0},
        {"measured_A": 0.002, "measured_V": 2.0},
        {"measured_A": None, "measured_V": 99.0},
    ]

    assert _resistance_from_rows(rows) == pytest.approx(1000.0)


def test_resistance_estimate_rejects_insufficient_or_constant_current():
    assert _resistance_from_rows([]) is None
    assert _resistance_from_rows([{"measured_A": 1.0, "measured_V": 2.0}]) is None
    assert (
        _resistance_from_rows(
            [
                {"measured_A": 1.0, "measured_V": 2.0},
                {"measured_A": 1.0, "measured_V": 3.0},
            ]
        )
        is None
    )
