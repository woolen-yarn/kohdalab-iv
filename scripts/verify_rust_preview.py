"""Check Rust engine IPC, CSV compatibility, Stop, and GUI lease loss without hardware."""

from __future__ import annotations

import argparse
import ast
import csv
import json
from pathlib import Path
import queue
import subprocess
import threading
import time


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("binary", type=Path)
    parser.add_argument("--output-dir", type=Path, required=True)
    args = parser.parse_args()
    args.output_dir.mkdir(parents=True, exist_ok=True)
    process = subprocess.Popen(
        [str(args.binary.resolve()), "--engine"],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
    )
    events: queue.Queue = queue.Queue()
    reader = threading.Thread(
        target=lambda: [events.put(json.loads(line)) for line in process.stdout],
        daemon=True,
    )
    reader.start()

    def send(command: str, **fields) -> None:
        process.stdin.write(json.dumps({"command": command, **fields}) + "\n")
        process.stdin.flush()

    def wait_state(predicate, heartbeat=True, seconds=8):
        deadline = time.monotonic() + seconds
        next_heartbeat = 0.0
        while time.monotonic() < deadline:
            if heartbeat and time.monotonic() >= next_heartbeat:
                send("heartbeat")
                next_heartbeat = time.monotonic() + 0.25
            try:
                event = events.get(timeout=0.05)
            except queue.Empty:
                if process.poll() is not None:
                    raise AssertionError("Engine exited unexpectedly")
                continue
            if event["event"] == "state" and predicate(event["data"]):
                return event["data"]
        raise AssertionError("Engine state timed out")

    config = dict(
        mode="dc_iv",
        pattern="linear",
        start=-0.001,
        stop=0.001,
        step=0.001,
        repeat=1,
        custom_points=[],
        source_limit=0.001,
        compliance=0.001,
        settle_s=0.01,
        start_settle_s=0.02,
        nplc=1.0,
        average_count=1,
        ramp_step=0.0001,
        ramp_wait_s=0.02,
    )
    results = []
    try:
        send("connect_source", simulation=True, resource="")
        source_only = wait_state(lambda s: s["source_connected"])
        assert not source_only["connected"] and not source_only["meter_connected"]
        send("connect_meter", simulation=True, resource="")
        wait_state(lambda s: s["connected"])
        send("disconnect_source")
        detached = wait_state(lambda s: not s["source_connected"])
        assert detached["meter_connected"] and detached["output"] == "Off"
        send("connect_source", simulation=True, resource="")
        wait_state(lambda s: s["connected"])
        results.append("independent source/meter connect, detach, reconnect: passed")
        completed_path = args.output_dir / "completed.csv"
        send("run", config=config, path=str(completed_path.resolve()))
        completed = wait_state(lambda s: s["run_id"] == 1 and s["phase"] == "Completed")
        assert (
            completed["points"] == 3
            and completed["output"] == "Off"
            and not completed["error"]
        )
        assert abs(completed["resistance"] - 1000) < 1e-9
        root = Path(__file__).resolve().parents[1]
        tree = ast.parse((root / "src/kohdalab_iv/api/measurement_rows.py").read_text())
        expected = next(
            ast.literal_eval(node.value)
            for node in tree.body
            if isinstance(node, ast.Assign)
            and any(
                isinstance(t, ast.Name) and t.id == "IV_FIELDS" for t in node.targets
            )
        )
        with completed_path.open(newline="") as stream:
            rows = csv.DictReader(stream)
            assert rows.fieldnames == expected
            saved = list(rows)
        assert len(saved) == 3 and all(
            row["source_ref"] == "source.simulated" for row in saved
        )
        results.append(
            "completed: 3 points, 46 compatible CSV fields, 1 kOhm fit, output Off"
        )
        long_config = {**config, "start_settle_s": 10.0}
        send(
            "run",
            config=long_config,
            path=str((args.output_dir / "stopped.csv").resolve()),
        )
        wait_state(lambda s: s["run_id"] == 2 and s["phase"] == "Running")
        begin = time.monotonic()
        send("stop")
        stopped = wait_state(lambda s: s["run_id"] == 2 and s["phase"] == "Stopped")
        assert stopped["output"] == "Off" and stopped["points"] == 0
        results.append(f"Stop: output Off in {time.monotonic() - begin:.3f} s")
        send(
            "run",
            config=long_config,
            path=str((args.output_dir / "lease.csv").resolve()),
        )
        wait_state(lambda s: s["run_id"] == 3 and s["phase"] == "Running")
        begin = time.monotonic()
        lost = wait_state(
            lambda s: s["run_id"] == 3 and not s["connected"], heartbeat=False
        )
        assert lost["output"] == "Off" and lost["points"] == 0
        results.append(
            f"GUI lease loss: output Off and disconnected in {time.monotonic() - begin:.3f} s"
        )
        print(json.dumps({"checks": results, "result": "passed"}))
    finally:
        if process.poll() is None:
            send("shutdown")
        process.stdin.close()
        process.wait(timeout=10)
    assert process.returncode == 0


if __name__ == "__main__":
    main()
