"""Check Separate plot reset and partial-stop contracts in the Simulation GUI API."""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "bindings/python"))
from kohdalab_iv_native import GuiIV, GuiIVError


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=Path)
    parser.add_argument("--output-dir", type=Path, required=True)
    args = parser.parse_args()
    args.output_dir.mkdir(parents=True, exist_ok=True)
    profile_dir = Path(tempfile.mkdtemp(prefix="iv-separate-api-"))
    # Scope discovery and all settings/data to this child test process.
    os.environ["USERPROFILE"] = str(profile_dir)
    os.environ["HOME"] = str(profile_dir)
    binary = args.binary.resolve(strict=True)
    plan = json.loads(subprocess.check_output([str(binary), "--default-config"], text=True))
    plan.update(
        mode="dc_vi", pattern="linear", start=-1e-5, stop=1e-5, step=1e-6,
        source_limit=1e-4, compliance=3.0, ramp_step=1e-6,
        start_settle_s=0.02, settle_s=0.02, ramp_wait_s=0,
    )
    profile = {
        "simulation": True,
        "config": plan,
        "reference_display": "Separate",
        "reference_purpose": "Gate",
        "reference_meter_id": 3,
        "sources": [{"id": 1, "model": "SIMULATED_SOURCE", "resource": ""}],
        "meters": [
            {"id": 2, "model": "SIMULATED_METER", "resource": ""},
            {"id": 3, "model": "SIMULATED_METER", "resource": "SIMULATED_REFERENCE_METER"},
        ],
        "source1_id": 1,
        "meter_id": 2,
        "output_dir": str(profile_dir / "data"),
    }
    path = profile_dir / "config.json"
    path.write_text(json.dumps(profile), encoding="utf-8")
    app = GuiIV.launch(path, binary=binary)
    pid = json.loads(Path(app.endpoint).read_text(encoding="utf-8"))["pid"]
    report = []
    try:
        app.wait(app.connect()["id"])
        # Retain the exact connected reference role while editing sweep values.
        plan = app.plan()
        for limit in (1e-5, 1e-6):
            plan.update(start=-limit, stop=limit, step=limit / 10)
            app.set_plan(plan)
            receipt = app.start()
            app.wait(receipt["id"])
            status = app.status()
            snapshot, history = status["snapshot"], status["plot_history"]
            assert snapshot["output"] == "Off" and snapshot["points"] == 21, status
            assert history["primary_points"] == history["reference_points"] == 21, status
            assert history["run_id"] == snapshot["run_id"], status
            deadline = time.monotonic() + 2
            while True:
                history = app.status()["plot_history"]
                upper, lower = history["primary_bounds"], history["reference_bounds"]
                if (upper and lower and upper[0] < -limit
                        and limit < upper[2] < limit * 1.2
                        and abs(upper[0] - lower[0]) < 1e-15
                        and abs(upper[2] - lower[2]) < 1e-15):
                    break
                assert time.monotonic() < deadline, history
                time.sleep(0.05)
            status["plot_history"] = history
            report.append(status)

        # Rejecting a plan must preserve the completed histories and view.
        before = app.status()["plot_history"]
        try:
            app.set_plan({**plan, "step": 0})
        except GuiIVError:
            pass
        else:
            raise AssertionError("Invalid plan was accepted")
        assert app.status()["plot_history"] == before

        # Exercise the real software-limit path, partial CSV and verified OFF.
        plan.update(start=-1e-5, stop=1e-5, step=1e-6, compliance=0.003)
        app.set_plan(plan)
        receipt = app.start()
        try:
            app.wait(receipt["id"])
        except GuiIVError:
            pass
        stopped = app.status()
        snapshot, history = stopped["snapshot"], stopped["plot_history"]
        assert snapshot["phase"] == "Compliance stop", stopped
        assert snapshot["output"] == "Off", stopped
        assert history["primary_points"] == snapshot["points"] > 0, stopped
        assert history["reference_points"] == snapshot["points"], stopped
        time.sleep(0.2)
        assert app.status()["plot_history"] == history
        report.append(stopped)
        (args.output_dir / "api.json").write_text(
            json.dumps(report, indent=2) + "\n", encoding="utf-8"
        )
        print("Two API runs, rejected plan retention, software-limit partial history and Output OFF passed")
    finally:
        app.close()
        os.kill(pid, 15)


if __name__ == "__main__":
    main()
