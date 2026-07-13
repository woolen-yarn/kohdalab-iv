import copy

from kohdalab_iv.api.config import DEFAULT_CONFIG
from kohdalab_iv.api.measurement_rows import IV_FIELDS, iv_row, output_row
from kohdalab_iv.api.scan_plan import iv_plan_from_config


def _iv_config():
    config = copy.deepcopy(DEFAULT_CONFIG)
    settings = config["measurements"]["iv"]
    settings["mode"] = "dc_iv"
    settings["scan"]["pattern"] = "linear"
    settings["scan"]["start"] = {"value": -100.0, "unit": "mV"}
    settings["scan"]["stop"] = {"value": 100.0, "unit": "mV"}
    settings["scan"]["step"] = {"value": 10.0, "unit": "mV"}
    settings["safety"]["max_abs_source"] = {"value": 100.0, "unit": "mV"}
    settings["safety"]["compliance"] = {"value": 10.0, "unit": "uA"}
    settings["safety"]["ramp_step"] = {"value": 10.0, "unit": "mV"}
    return config


def test_iv_row_uses_measured_values_for_resistance():
    plan = iv_plan_from_config(_iv_config())
    point = plan.points[0]

    row = iv_row(
        timestamp="2026-01-01T00:00:00",
        elapsed_s=0.1,
        plan=plan,
        point=point,
        source_set=-0.1,
        source_readback=-0.099,
        meter_value=-1e-6,
        compliance=False,
        status="running",
    )

    assert row["measured_V"] == -0.099
    assert row["measured_A"] == -1e-6
    assert row["voltage_origin"] == "source_readback"
    assert row["current_origin"] == "meter"
    assert row["resistance_Ohm"] == 99000.0


def test_vi_row_maps_meter_voltage_and_source_current():
    config = copy.deepcopy(DEFAULT_CONFIG)
    settings = config["measurements"]["iv"]
    settings["mode"] = "dc_vi"
    settings["scan"]["pattern"] = "linear"
    settings["scan"]["start"] = {"value": 0.0, "unit": "uA"}
    settings["scan"]["stop"] = {"value": 1.0, "unit": "uA"}
    settings["scan"]["step"] = {"value": 1.0, "unit": "uA"}
    settings["safety"]["max_abs_source"] = {"value": 10.0, "unit": "uA"}
    settings["safety"]["compliance"] = {"value": 1.0, "unit": "V"}
    settings["safety"]["ramp_step"] = {"value": 1.0, "unit": "uA"}
    plan = iv_plan_from_config(config)
    point = plan.points[1]

    row = iv_row(
        timestamp="2026-01-01T00:00:00",
        elapsed_s=0.1,
        plan=plan,
        point=point,
        source_set=1e-6,
        source_readback=0.99e-6,
        meter_value=0.123,
        compliance=False,
        status="running",
    )

    assert row["measured_V"] == 0.123
    assert row["measured_A"] == 0.99e-6
    assert row["voltage_origin"] == "meter"
    assert row["current_origin"] == "source_readback"


def test_output_row_uses_stable_provenance_csv_fields():
    plan = iv_plan_from_config(DEFAULT_CONFIG)
    point = plan.points[0]
    row = iv_row(
        timestamp="2026-01-01T00:00:00",
        elapsed_s=0.1,
        plan=plan,
        point=point,
        source_set=-0.1,
        source_readback=-0.099,
        meter_value=-1e-3,
        compliance=False,
        status="running",
    )

    assert "source_model" in row
    assert list(output_row(row)) == IV_FIELDS
    assert "source_model" in output_row(row)
    assert "compliance" in output_row(row)
    assert "status" in output_row(row)
    assert "resistance_Ohm" in output_row(row)
    assert "conductance_S" in output_row(row)
    assert output_row(row)["target_value"] == "-1.000000000000e-01"
    assert output_row(row)["voltage_V"] == "-1.000000000000e-03"
    assert output_row(row)["current_A"] == "-9.900000000000e-02"
