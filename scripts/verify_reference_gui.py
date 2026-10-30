"""Run device-free GUI cases and verify identical source/Meter plot geometry."""
from __future__ import annotations

import argparse
import json
from pathlib import Path
import subprocess


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--version", required=True)
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
    ]:
        screenshot = output / f"{label}.png"
        result = subprocess.run(
            [str(binary), flag, "--screenshot", str(screenshot)],
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
        bounds = record.get("plot_bounds")
        if not isinstance(bounds, list) or len(bounds) != 4:
            raise RuntimeError(f"{label}: no plot bounds")
        cases.append({"case": label, "screenshot": screenshot.name, **record})
    if any(abs(a - b) > 0.1 for a, b in zip(cases[0]["plot_bounds"], cases[1]["plot_bounds"])):
        raise RuntimeError("Assigning a Meter changed the plot geometry")
    report = {
        "version": args.version,
        "normal_gui_rendered": True,
        "same_plot_bounds": True,
        "simulation_only": True,
        "physical_hardware_tested": False,
        "cases": cases,
    }
    (output / "gui-validation.json").write_text(json.dumps(report, indent=2) + "\n")
    print("Both GUI cases passed with identical plot geometry")


if __name__ == "__main__":
    main()
