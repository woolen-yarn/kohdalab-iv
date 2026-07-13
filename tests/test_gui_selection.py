from kohdalab_iv import __version__
from kohdalab_iv.apps.iv_gui import (
    _device_key_for_selection,
    _model_config_for_selection,
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
