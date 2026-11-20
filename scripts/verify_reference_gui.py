"""Verify reference Meter geometry and independent DC operation in the simulated GUI."""
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import subprocess


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--version", required=True)
    parser.add_argument("--renderer", default="system OpenGL")
    args = parser.parse_args()
    binary = args.binary.resolve(strict=True)
    cli = binary.with_name("iv-cli.exe" if binary.suffix.lower() == ".exe" else "iv-cli")
    actual_version = subprocess.check_output([str(cli), "--version"], text=True, timeout=10).strip()
    if actual_version.split()[-1] != args.version:
        raise RuntimeError(f"Wrong GUI companion version: {actual_version}; expected {args.version}")
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    cases = []
    for label, flag, assigned in [
        ("source-only", "--gui-smoke", False),
        ("source-and-meter", "--gui-reference-smoke", True),
        ("independent-dc", "--gui-inventory-smoke", False),
        ("two-columns", "--gui-two-column-smoke", True),
        ("current-source", "--gui-current-source-smoke", False),
        ("held-dc", "--gui-dc-smoke", False),
        ("separate", "--gui-separate-smoke", True),
        ("separate-narrow", "--gui-separate-smoke", True),
        ("separate-linked", "--gui-separate-smoke", True),
    ]:
        screenshot = output / f"{label}.png"
        command = [str(binary), flag, "--screenshot", str(screenshot)]
        if label == "separate-linked":
            command.append("--gui-linked-axes-smoke")
        if label == "separate-narrow":
            command.append("--gui-narrow-smoke")
        if label == "held-dc":
            # The production smoke waits for three post-ON acquisitions in
            # the GUI history and captures while DC is still ON, then cleans up.
            command.append("--screenshot-dc-on")
        result = subprocess.run(
            command,
            capture_output=True, text=True, timeout=45,
        )
        (output / f"{label}.stdout.log").write_text(result.stdout)
        (output / f"{label}.stderr.log").write_text(result.stderr)
        if result.returncode:
            raise RuntimeError(f"{label}: GUI exited {result.returncode}: {result.stderr[-1500:]}")
        records = []
        for line in result.stdout.splitlines():
            try:
                value = json.loads(line)
            except json.JSONDecodeError:
                continue
            if isinstance(value, dict) and value.get("gui_smoke") == "passed":
                records.append(value)
        if len(records) != 1 or not screenshot.is_file():
            raise RuntimeError(f"{label}: GUI completion/screenshot missing")
        record = records[0]
        if record.get("reference_meter_assigned") != assigned or record.get("output") != "Off" or record.get("points", 0) <= 0:
            raise RuntimeError(f"{label}: invalid completion: {record}")
        if record.get("measurement_controls_visible") is not True:
            raise RuntimeError(f"Measurement controls overflow in {label}: {record.get("measurement_controls_bounds")}")
        if label == "two-columns" and not record.get("two_card_columns"):
            raise RuntimeError("The two-column device layout did not activate")
        bounds = record.get("plot_bounds")
        if not isinstance(bounds, list) or len(bounds) != 4:
            raise RuntimeError(f"{label}: no plot bounds")
        if label == "current-source":
            data_bounds = record.get("plot_data_bounds")
            if record.get("mode") != "dc_vi" or record["points"] != 11:
                raise RuntimeError(f"Current-source simulation did not complete: {record}")
            if not data_bounds or not (data_bounds[0] <= -0.0005 and 0.0005 <= data_bounds[2] < 0.01
                                       and data_bounds[1] <= -0.5 and data_bounds[3] >= 0.5):
                raise RuntimeError(f"Current-source plot did not fit the data: {data_bounds}")
        if label.startswith("separate"):
            if record.get("stop_visible") is not True:
                raise RuntimeError(f"Stop is clipped in {label}")
            upper, lower = record.get("separate_primary_bounds"), record.get("separate_reference_bounds")
            if (record.get("reference_display") != "Separate" or record.get("run_id") != 2
                    or record.get("history_points") != 11 or record.get("separate_points") != 11
                    or record.get("first_run_histories") != [11, 11]):
                raise RuntimeError(f"Separate histories did not reset after valid GUI Start: {record}")
            if not upper or not lower or abs(upper[0]-lower[0]) > 1e-12 or abs(upper[2]-lower[2]) > 1e-12:
                raise RuntimeError(f"Separate x bounds are not linked: {upper}, {lower}")
            if label == "separate-linked":
                if abs(upper[0]+2e-6)>1e-12 or abs(upper[2]-4e-6)>1e-12:
                    raise RuntimeError(f"Manual x zoom/pan did not propagate: {upper}, {lower}")
            elif not (-1e-5 < upper[0] <= -5e-6 and 5e-6 <= upper[2] < 1e-5):
                raise RuntimeError(f"Separate plot retained the first run bounds: {upper}")
        cases.append({"case": label, "screenshot": screenshot.name, **record})
    if any(abs(a - b) > 0.1 for a, b in zip(cases[0]["plot_bounds"], cases[1]["plot_bounds"])):
        raise RuntimeError("Assigning a Meter changed the plot geometry")
    report = {
        "version": args.version,
        "renderer": args.renderer,
        "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
        "cli_sha256": hashlib.sha256(cli.read_bytes()).hexdigest(),
        "normal_gui_rendered": True,
        "same_plot_bounds": True,
        "simulation_only": True,
        "physical_hardware_tested": False,
        "cases": cases,
    }
    (output / "gui-validation.json").write_text(json.dumps(report, indent=2) + "\n")
    print("All GUI cases passed and the reference Meter did not change plot geometry")


if __name__ == "__main__":
    main()
