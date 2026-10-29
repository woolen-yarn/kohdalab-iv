"""Exercise the common SDK against the real IV GUI using isolated simulation only."""

import argparse
import csv
import json
import os
import signal
import subprocess
import sys
import tempfile
import uuid
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path[:0] = [str(ROOT / "bindings/python"), str(ROOT / "bindings/control")]
from kohdalab_control import ControlClient, ControlError, discover
from kohdalab_iv_native import GuiIV


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=Path)
    parser.add_argument("--output-dir", type=Path, required=True)
    args = parser.parse_args()
    args.output_dir.mkdir(parents=True, exist_ok=True)
    profile = Path(tempfile.mkdtemp(prefix="iv-common-simulation-"))
    os.environ["USERPROFILE"] = str(profile)
    plan = json.loads(
        subprocess.check_output(
            [str(args.binary.resolve()), "--default-config"], text=True
        )
    )
    plan.update(
        pattern="linear",
        start=-0.001,
        stop=0.001,
        step=0.001,
        start_settle_s=0,
        settle_s=0,
        ramp_wait_s=0,
    )
    config = profile / "config.json"
    config.write_text(
        json.dumps(
            {"simulation": True, "config": plan, "output_dir": str(profile / "data")}
        )
    )
    native = GuiIV.launch(config, binary=args.binary.resolve())
    client = ControlClient(endpoint=native.endpoint)
    try:
        assert (
            len(
                discover(
                    "kohdalab-iv",
                    directory=Path(native.endpoint).parent,
                    probe_timeout=2,
                )
            )
            == 1
        )
        description = client.describe()
        assert "control.start" in description["methods"]
        assert not client.status()["ownership"]["owned"]
        assert "start" in client.invoke("plan")
        assert not client.status()["ownership"]["owned"]
        client.acquire(label="Common SDK simulation")
        rival = ControlClient(endpoint=native.endpoint)
        try:
            rival.acquire()
            raise AssertionError("Second owner admitted")
        except ControlError as error:
            assert error.code in ("ownership", "busy"), error.code
        connected = client.invoke("connect")
        client.wait(connected["job_id"])
        assert native.snapshot["simulation"]
        request_id = str(uuid.uuid4())
        first = client.invoke("start", request_id=request_id)
        duplicate = client.invoke("start", request_id=request_id)
        assert first["job_id"] == duplicate["job_id"]
        result = client.wait(first["job_id"])
        with Path(result["native"]["output"]).open() as stream:
            rows = list(csv.DictReader(stream))
        assert len(rows) == 3 and len(rows[0]) == 18
        stopped = client.stop_and_wait()
        assert stopped["cleanup"] == "verified"
        assert client.release()["released"]
        assert not client.status()["ownership"]["owned"]
        client.acquire()
        assert client.release()["released"]
        report = {
            "ok": True,
            "simulation": True,
            "rows": len(rows),
            "csv_columns": len(rows[0]),
            "common_protocol": description["control_protocol"],
            "duplicate_receipt": True,
            "exclusive_ownership": True,
            "stop_cleanup": "verified",
            "reacquire": True,
        }
        (args.output_dir / "results.json").write_text(
            json.dumps(report, indent=2) + "\n"
        )
        print(json.dumps(report))
    finally:
        try:
            client.close()
            native.close()
        finally:
            os.kill(native._endpoint["pid"], signal.SIGTERM)


if __name__ == "__main__":
    main()
