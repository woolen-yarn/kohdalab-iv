"""Simulation-only v2 API acceptance: launch, configuration, jobs and ownership.

Requires a visible desktop. Uses an isolated user profile; never opens hardware.
"""
import argparse
import csv
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import time
import uuid
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "bindings/python"))
from kohdalab_iv_native import GuiIV, GuiIVError


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=Path)
    parser.add_argument("--output-dir", required=True, type=Path)
    args = parser.parse_args()
    report = args.output_dir.resolve()
    report.mkdir(parents=True, exist_ok=True)
    profile = Path(tempfile.mkdtemp(prefix="iv-api-v2-"))
    os.environ["USERPROFILE"] = str(profile)
    config = profile / "profile.json"
    plan_defaults = json.loads(subprocess.check_output([str(args.binary.resolve()), "--default-config"], text=True))
    plan_defaults.update({"pattern": "linear", "start": -0.001, "stop": 0.001,
        "step": 0.001, "start_settle_s": 0, "settle_s": 0, "ramp_wait_s": 0})
    config.write_text(json.dumps({"simulation": True, "config": plan_defaults, "output_dir": str(profile / "data")}))
    client = GuiIV.launch(config, binary=args.binary.resolve())
    pid = client._endpoint["pid"]
    (report / "process.json").write_text(json.dumps({"pid": pid, "profile": str(profile)}))
    try:
        assert client.capabilities()["api_version"] == 2
        assert client._lease is None
        observer = GuiIV(client.endpoint)
        # Rust canonicalization keeps the Windows \\?\ prefix; Python resolve
        # may omit it. Compare file identity rather than path spelling.
        assert Path(observer.config()["config_path"]).samefile(config)
        observer.close()
        assert not client.status()["control"]["owned"]
        client.acquire(label="API validation")
        assert client.status()["control"]["owned"]
        other = GuiIV(client.endpoint)
        try:
            other.acquire()
            raise AssertionError("Second owner acquired GUI")
        except GuiIVError:
            pass
        finally:
            other.close()
        connected = client.connect()
        client.wait(connected["id"])
        assert client.snapshot["simulation"]
        assert client.snapshot["source_simulation"] and client.snapshot["meter_simulation"]
        plan = client.plan()
        plan["settle_s"] = 0.005
        client.set_plan(plan)
        client.set_output(directory=profile / "data", filename="api.CSV", auto_timestamp_suffix=False)
        assert client.output()["filename"] == "api"
        request_id = str(uuid.uuid4())
        first = client.start(request_id=request_id)
        second = client.start(request_id=request_id)
        assert first["id"] == second["id"]
        result = client.wait(first["id"])
        assert result["status"] == "completed" and result["rows"] == 3
        rows = list(csv.DictReader(Path(result["output"]).open()))
        assert len(rows) == 3 and len(rows[0]) == 18
        try:
            client._request("connect", require_lease=True, request_id=request_id)
            raise AssertionError("Conflicting request id accepted")
        except GuiIVError as error:
            assert error.code == "request_id_conflict", error
        plan["mode"] = "dc_vi"
        # Simulation's resistance is 1 kOhm. Keep this independent current
        # source test well within the unchanged 1 V voltage-stop setting.
        plan.update(start=-1e-6, stop=1e-6, step=1e-6)
        plan["compliance"] = 1.0
        client.set_plan(plan)
        assert client.plan()["mode"] == "dc_vi"
        client.set_output(filename="current-source")
        state = client.run()
        assert state["phase"] == "Completed" and state["points"] == 3
        # Old receipts remain addressable after a later run.
        assert client.result(first["id"])["output"] == result["output"]
        client.release()
        deadline = time.monotonic() + 5
        while client.status()["control"]["status"] != "idle":
            assert time.monotonic() < deadline
            time.sleep(0.05)
        client.acquire()
        disconnected = client.disconnect()
        client.wait(disconnected["id"])
        assert not client.snapshot["source_connected"] and not client.snapshot["meter_connected"]
        client.load_config(config)
        client.configure(client.config()["profile"])
        client.close()
        (report / "results.json").write_text(json.dumps({"ok": True,"simulation": True,"pid":pid,"rows":len(rows),"csv_columns":len(rows[0]),"first_job":result["id"]}, indent=2))
        print((report / "results.json").read_text())
    finally:
        try:
            client.close()
        finally:
            os.kill(pid, signal.SIGTERM)


if __name__ == "__main__":
    main()
