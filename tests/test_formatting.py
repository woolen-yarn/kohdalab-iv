from kohdalab_iv.api.formatting import format_conductance, format_resistance


def test_format_resistance_scales_low_values():
    assert format_resistance(0.12) == "120 mOhm"
    assert format_resistance(0.00123) == "1.23 mOhm"


def test_format_resistance_scales_high_values():
    assert format_resistance(1200.0) == "1.2 kOhm"
    assert format_resistance(2.5e6) == "2.5 MOhm"


def test_format_resistance_handles_none_and_zero():
    assert format_resistance(None) == "-"
    assert format_resistance(0.0) == "0 Ohm"


def test_format_conductance_scales_values():
    assert format_conductance(0.0012) == "1.2 mS"
    assert format_conductance(2.5e-6) == "2.5 uS"
    assert format_conductance(None) == "-"
