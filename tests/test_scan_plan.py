import copy

import pytest

from kohdalab_iv.api.config import DEFAULT_CONFIG
from kohdalab_iv.api.scan_plan import iv_plan_from_config


def test_default_plan_uses_gs210_100ma_range():
    config = copy.deepcopy(DEFAULT_CONFIG)

    plan = iv_plan_from_config(config)

    assert plan.mode == "dc_vi"
    assert plan.source_function == "current"
    assert plan.measure_function == "dc_voltage"
    assert plan.source_range.name == "100mA"
    assert plan.total_points == 21
    assert plan.points[0].target == pytest.approx(-0.1)
    assert plan.points[-1].target == pytest.approx(0.1)


def test_plan_rejects_step_below_gs210_resolution():
    config = copy.deepcopy(DEFAULT_CONFIG)
    scan = config["measurements"]["iv"]["scan"]
    scan["pattern"] = "linear"
    scan["start"] = {"value": 0.0, "unit": "A"}
    scan["stop"] = {"value": 1.0, "unit": "uA"}
    scan["step"] = {"value": 1.0, "unit": "nA"}

    with pytest.raises(ValueError, match="source resolution"):
        iv_plan_from_config(config)


def test_plan_rejects_safety_limit_exceeded():
    config = copy.deepcopy(DEFAULT_CONFIG)
    config["measurements"]["iv"]["safety"]["max_abs_source"] = {"value": 50.0, "unit": "mA"}

    with pytest.raises(ValueError, match="exceeds effective limit"):
        iv_plan_from_config(config)


def test_vi_plan_uses_current_source_and_voltage_meter():
    config = copy.deepcopy(DEFAULT_CONFIG)
    settings = config["measurements"]["iv"]
    settings["mode"] = "dc_vi"
    settings["scan"]["pattern"] = "linear"
    settings["scan"]["start"] = {"value": 0.0, "unit": "uA"}
    settings["scan"]["stop"] = {"value": 10.0, "unit": "uA"}
    settings["scan"]["step"] = {"value": 1.0, "unit": "uA"}
    settings["safety"]["max_abs_source"] = {"value": 100.0, "unit": "uA"}
    settings["safety"]["compliance"] = {"value": 1.0, "unit": "V"}
    settings["safety"]["ramp_step"] = {"value": 1.0, "unit": "uA"}

    plan = iv_plan_from_config(config)

    assert plan.mode == "dc_vi"
    assert plan.source_function == "current"
    assert plan.measure_function == "dc_voltage"
    assert plan.source_range.name == "1mA"
    assert plan.total_points == 11
    assert {point.direction for point in plan.points} == {"forward"}


def test_plan_accepts_keysight_34465a_meter():
    config = copy.deepcopy(DEFAULT_CONFIG)
    config["roles"]["iv"]["measure"] = "meter.dmm_34465a"
    config["roles"]["vi"]["measure"] = "meter.dmm_34465a"
    config["measurements"]["iv"]["timing"]["nplc"] = 0.001

    plan = iv_plan_from_config(config)

    assert plan.measure_model == "KEYSIGHT_34465A"
    assert plan.nplc == 0.001


def test_plan_accepts_agilent_34411a_meter():
    config = copy.deepcopy(DEFAULT_CONFIG)
    config["roles"]["iv"]["measure"] = "meter.dmm_agilent_34411a"
    config["roles"]["vi"]["measure"] = "meter.dmm_agilent_34411a"

    plan = iv_plan_from_config(config)

    assert plan.measure_model == "AGILENT_34411A"


def test_plan_accepts_adcmt_7461a_meter_nplc_range():
    config = copy.deepcopy(DEFAULT_CONFIG)
    config["roles"]["iv"]["measure"] = "meter.dmm_7461a"
    config["roles"]["vi"]["measure"] = "meter.dmm_7461a"
    config["measurements"]["iv"]["timing"]["nplc"] = 1.234

    plan = iv_plan_from_config(config)

    assert plan.measure_model == "ADCMT_7461A"
    assert plan.nplc == 1.234


def test_plan_accepts_yokogawa_7651_source():
    config = copy.deepcopy(DEFAULT_CONFIG)
    settings = config["measurements"]["iv"]
    config["roles"]["iv"]["source"] = "source.yokogawa_7651"
    settings["mode"] = "dc_iv"
    settings["scan"]["pattern"] = "linear"
    settings["scan"]["start"] = {"value": 0.0, "unit": "mV"}
    settings["scan"]["stop"] = {"value": 1.0, "unit": "mV"}
    settings["scan"]["step"] = {"value": 0.1, "unit": "mV"}
    settings["safety"]["max_abs_source"] = {"value": 1.0, "unit": "mV"}
    settings["safety"]["compliance"] = {"value": 10.0, "unit": "uA"}
    settings["safety"]["ramp_step"] = {"value": 0.1, "unit": "mV"}

    plan = iv_plan_from_config(config)

    assert plan.source_model == "YOKOGAWA_7651"
    assert plan.source_range.name == "10mV"
    assert plan.source_range.command_value == 2.0


def test_plan_rejects_adcmt_7461a_nplc_below_range():
    config = copy.deepcopy(DEFAULT_CONFIG)
    config["roles"]["iv"]["measure"] = "meter.dmm_7461a"
    config["roles"]["vi"]["measure"] = "meter.dmm_7461a"
    config["measurements"]["iv"]["timing"]["nplc"] = 0.001

    with pytest.raises(ValueError, match="ADCMT_7461A does not support nplc"):
        iv_plan_from_config(config)


def test_decreasing_linear_scan_is_backward():
    config = copy.deepcopy(DEFAULT_CONFIG)
    settings = config["measurements"]["iv"]
    settings["scan"]["pattern"] = "linear"
    settings["scan"]["start"] = {"value": 20.0, "unit": "mA"}
    settings["scan"]["stop"] = {"value": -20.0, "unit": "mA"}
    settings["scan"]["step"] = {"value": 10.0, "unit": "mA"}
    settings["safety"]["max_abs_source"] = {"value": 20.0, "unit": "mA"}
    settings["safety"]["ramp_step"] = {"value": 10.0, "unit": "mA"}

    plan = iv_plan_from_config(config)

    assert [point.target for point in plan.points] == pytest.approx([0.02, 0.01, 0.0, -0.01, -0.02])
    assert {point.direction for point in plan.points} == {"backward"}


def test_zero_centered_uses_forward_backward_directions():
    config = copy.deepcopy(DEFAULT_CONFIG)
    settings = config["measurements"]["iv"]
    settings["scan"]["pattern"] = "zero_centered"
    settings["scan"]["start"] = {"value": -20.0, "unit": "mA"}
    settings["scan"]["stop"] = {"value": 20.0, "unit": "mA"}
    settings["scan"]["step"] = {"value": 10.0, "unit": "mA"}
    settings["safety"]["max_abs_source"] = {"value": 20.0, "unit": "mA"}
    settings["safety"]["ramp_step"] = {"value": 10.0, "unit": "mA"}

    plan = iv_plan_from_config(config)

    assert {point.direction for point in plan.points} == {"forward", "backward"}
    assert [point.target for point in plan.points] == pytest.approx(
        [0.0, 0.01, 0.02, 0.01, 0.0, -0.01, -0.02, -0.01, 0.0]
    )
    assert [point.direction for point in plan.points] == [
        "forward",
        "forward",
        "forward",
        "backward",
        "backward",
        "backward",
        "backward",
        "forward",
        "forward",
    ]
