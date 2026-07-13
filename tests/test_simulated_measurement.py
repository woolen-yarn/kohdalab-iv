from __future__ import annotations

import csv
from importlib.resources import files
from pathlib import Path

import pytest

from kohdalab_iv.api import cli
from kohdalab_iv.api.config import load_config, save_config
from kohdalab_iv.api.measurement_rows import IV_FIELDS
from kohdalab_iv.instruments.simulated import _circuit, reset_simulated_circuits


@pytest.fixture(autouse=True)
def reset_simulation() -> None:
    reset_simulated_circuits()


def _simulated_config() -> dict:
    path = files("kohdalab_iv").joinpath("resources", "simulated.json")
    return load_config(Path(str(path)))


def test_cli_runs_complete_hardware_free_measurement(tmp_path: Path) -> None:
    config = _simulated_config()
    output = config["measurements"]["iv"]["output"]
    output["dir"] = str(tmp_path)
    output["filename"] = "simulation.csv"
    output["auto_timestamp_suffix"] = False
    config_path = save_config(config, tmp_path / "config.json")

    assert cli.main(["--config", str(config_path), "measure"]) == 0

    output_path = tmp_path / "simulation.csv"
    with output_path.open(newline="", encoding="utf-8") as stream:
        rows = list(csv.DictReader(stream))
    assert len(rows) == 3
    assert list(rows[0]) == IV_FIELDS
    assert [float(row["voltage_V"]) for row in rows] == [-0.01, 0.0, 0.01]
    assert [float(row["current_A"]) for row in rows] == [-1e-5, 0.0, 1e-5]
    assert all(float(row["resistance_Ohm"]) == 1000.0 for row in (rows[0], rows[2]))
    assert all(row["source_model"] == "SIMULATED_SOURCE" for row in rows)
    assert all(row["measure_model"] == "SIMULATED_METER" for row in rows)
    assert _circuit("SIM::IV").output_enabled is False
    assert _circuit("SIM::IV").level == 0.0


def test_simulated_vi_mode_obeys_ohms_law(tmp_path: Path) -> None:
    config = _simulated_config()
    settings = config["measurements"]["iv"]
    settings["mode"] = "dc_vi"
    settings["scan"].update(
        {
            "start": {"value": -10.0, "unit": "uA"},
            "stop": {"value": 10.0, "unit": "uA"},
            "step": {"value": 10.0, "unit": "uA"},
        }
    )
    settings["safety"].update(
        {
            "max_abs_source": {"value": 100.0, "unit": "uA"},
            "compliance": {"value": 1.0, "unit": "V"},
            "ramp_step": {"value": 10.0, "unit": "uA"},
        }
    )
    settings["output"].update(
        {"dir": str(tmp_path), "filename": "vi.csv", "auto_timestamp_suffix": False}
    )
    config_path = save_config(config, tmp_path / "vi-config.json")

    assert cli.main(["--config", str(config_path), "measure"]) == 0
    with (tmp_path / "vi.csv").open(newline="", encoding="utf-8") as stream:
        rows = list(csv.DictReader(stream))
    assert [float(row["current_A"]) for row in rows] == [-1e-5, 0.0, 1e-5]
    assert [float(row["voltage_V"]) for row in rows] == [-0.01, 0.0, 0.01]
