#!/usr/bin/env python3
"""Reproducible macOS simulation benchmark: Python source APIs versus Rust engine.

This deliberately compares development Python APIs with the compiled Rust engine on
one Mac.  It is not a frozen/shipped portable or GUI benchmark.
"""

from __future__ import annotations

import argparse
import csv
import hashlib
import json
import os
from pathlib import Path
import queue
import resource
import subprocess
import sys
import threading
import time
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
FIELDS = [
    "timestamp",
    "measurement",
    "elapsed_s",
    "mode",
    "sweep_pattern",
    "repeat_index",
    "sweep_index",
    "point_index",
    "total_points",
    "direction",
    "source_function",
    "measure_function",
    "target_value",
    "target_unit",
    "source_target_value",
    "source_target_unit",
    "target_V",
    "target_A",
    "source_set_V",
    "source_set_A",
    "source_readback_value",
    "source_readback_unit",
    "source_readback_V",
    "source_readback_A",
    "meter_value",
    "meter_unit",
    "voltage_V",
    "current_A",
    "measured_V",
    "measured_A",
    "voltage_origin",
    "current_origin",
    "resistance_Ohm",
    "conductance_S",
    "compliance",
    "software_limit_hit",
    "status",
    "instrument_status",
    "source_ref",
    "measure_ref",
    "source_model",
    "measure_model",
    "start_settle_s",
    "settle_s",
    "nplc",
    "average_count",
]


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def cpu_children() -> dict[str, float]:
    usage = resource.getrusage(resource.RUSAGE_CHILDREN)
    return {"user_s": usage.ru_utime, "system_s": usage.ru_stime}


def cpu_delta(before: dict[str, float]) -> dict[str, float]:
    after = cpu_children()
    return {key: after[key] - before[key] for key in before}


def targets(count: int) -> list[float]:
    """Return the source-resolution-aligned linear +/-1 mV sequence."""
    return [
        float(f"{-0.001 + 0.002 * index / (count - 1):.12g}") for index in range(count)
    ]


def configured_profile(template: Path, count: int) -> dict[str, Any]:
    config = json.loads(template.read_text(encoding="utf-8"))
    for group in config["instruments"].values():
        for instrument in group.values():
            if instrument.get("transport") != "simulated":
                raise ValueError(
                    "Benchmark template must use simulated transport for every instrument"
                )
    iv = config["measurements"]["iv"]
    iv["scan"].update(
        {
            "pattern": "linear",
            "start": {"value": -1.0, "unit": "mV"},
            "stop": {"value": 1.0, "unit": "mV"},
            "step": {"value": 2.0 / (count - 1), "unit": "mV"},
            "repeat": 1,
            "custom_points": [],
        }
    )
    iv["timing"].update(
        {
            "pre_delay_s": 0.0,
            "start_settle_s": 0.0,
            "settle_s": 0.0,
            "post_zero_delay_s": 0.0,
            "ramp_step_wait_s": 0.0,
            "nplc": 1.0,
            "average_count": 1,
        }
    )
    iv["safety"].update(
        {
            "max_abs_source": {"value": 1.0, "unit": "mV"},
            "compliance": {"value": 1.0, "unit": "mA"},
            "ramp_step": {"value": 1.0, "unit": "mV"},
        }
    )
    return config


def finite(value: str, label: str) -> float:
    try:
        result = float(value)
    except ValueError as error:
        raise AssertionError(f"{label} is not numeric: {value!r}") from error
    if result != result or abs(result) == float("inf"):
        raise AssertionError(f"{label} is not finite: {value!r}")
    return result


def close(actual: float, expected: float, tolerance: float, label: str) -> None:
    if abs(actual - expected) > tolerance:
        raise AssertionError(f"{label}: {actual} != {expected} +/- {tolerance}")


def validate_csv(path: Path, count: int) -> dict[str, Any]:
    expected_targets = targets(count)
    with path.open(newline="", encoding="utf-8") as stream:
        reader = csv.DictReader(stream)
        if reader.fieldnames != FIELDS:
            raise AssertionError(
                f"CSV fields differ from the established 46 fields: {path}"
            )
        samples: list[dict[str, Any]] = []
        sample_indices = {0, (count - 1) // 2, count - 1}
        rows = 0
        final_elapsed = None
        for index, row in enumerate(reader):
            expected = expected_targets[index]
            for field in (
                "target_V",
                "source_set_V",
                "source_readback_V",
                "voltage_V",
                "current_A",
                "measured_V",
                "measured_A",
            ):
                finite(row[field], f"{field} row {index}")
            for field in (
                "target_V",
                "source_set_V",
                "source_readback_V",
                "voltage_V",
                "measured_V",
            ):
                close(
                    finite(row[field], f"{field} row {index}"),
                    expected,
                    1e-12,
                    f"{field} row {index}",
                )
            close(
                finite(row["current_A"], f"current_A row {index}"),
                expected / 1000.0,
                1e-14,
                f"current_A row {index}",
            )
            if abs(expected) > 1e-15:
                close(
                    finite(row["resistance_Ohm"], f"resistance_Ohm row {index}"),
                    1000.0,
                    1e-8,
                    f"resistance row {index}",
                )
            elif row["resistance_Ohm"].strip():
                finite(row["resistance_Ohm"], "zero-point resistance")
            if index in sample_indices:
                samples.append(
                    {
                        key: row[key]
                        for key in (
                            "point_index",
                            "target_V",
                            "source_readback_V",
                            "current_A",
                            "resistance_Ohm",
                        )
                    }
                )
            final_elapsed = finite(row["elapsed_s"], "final elapsed_s")
            rows += 1
    if rows != count:
        raise AssertionError(f"CSV has {rows} rows, expected {count}: {path}")
    return {
        "fields": len(FIELDS),
        "rows": rows,
        "samples": samples,
        "final_elapsed_s": final_elapsed,
    }


def python_child(args: argparse.Namespace) -> None:
    # Imports stay in the child so the orchestrator does not compare its own import time.
    from kohdalab_iv.api.experiment import Experiment
    from kohdalab_iv.api.scan_plan import iv_plan_from_config

    config = configured_profile(Path(args.template), args.count)
    planning_begin = time.perf_counter()
    plan = iv_plan_from_config(config, "iv")
    planning_s = time.perf_counter() - planning_begin
    output = Path(args.output)
    experiment = Experiment(config, auto_connect=True)
    loop_begin = time.perf_counter()
    rows = experiment.run_iv(plan=plan, output=output)
    loop_s = time.perf_counter() - loop_begin
    source = experiment.session.require(plan.source_ref)
    output_off = getattr(source, "output_state", lambda: None)()
    experiment.disconnect_all()
    print(
        json.dumps(
            {
                "planner_s": planning_s,
                "loop_s": loop_s,
                "rows_returned": len(rows),
                "source_output_off": output_off is False,
                "python_process_cpu_s": time.process_time(),
            }
        ),
        flush=True,
    )


class RustEngine:
    def __init__(self, executable: Path, stdout_path: Path, stderr_path: Path) -> None:
        self.process = subprocess.Popen(
            [str(executable), "--engine"],
            text=True,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            bufsize=1,
        )
        self.events: queue.Queue[dict[str, Any]] = queue.Queue()
        self.stdout_lines: list[str] = []
        self.stderr_lines: list[str] = []
        self.stdout_path, self.stderr_path = stdout_path, stderr_path
        self._threads = [
            threading.Thread(target=self._read_stdout, daemon=True),
            threading.Thread(target=self._read_stderr, daemon=True),
        ]
        for thread in self._threads:
            thread.start()

    def _read_stdout(self) -> None:
        assert self.process.stdout is not None
        for line in self.process.stdout:
            self.stdout_lines.append(line)
            try:
                self.events.put(json.loads(line))
            except json.JSONDecodeError:
                pass

    def _read_stderr(self) -> None:
        assert self.process.stderr is not None
        for line in self.process.stderr:
            self.stderr_lines.append(line)

    def send(self, command: str, **fields: Any) -> None:
        assert self.process.stdin is not None
        self.process.stdin.write(json.dumps({"command": command, **fields}) + "\n")
        self.process.stdin.flush()

    def wait_state(self, predicate, timeout_s: int) -> dict[str, Any]:
        deadline, next_heartbeat = time.monotonic() + timeout_s, 0.0
        while time.monotonic() < deadline:
            if self.process.poll() is not None:
                raise RuntimeError(
                    f"Rust engine exited unexpectedly: {self.process.returncode}"
                )
            if time.monotonic() >= next_heartbeat:
                self.send("heartbeat")
                next_heartbeat = time.monotonic() + 0.25
            try:
                event = self.events.get(timeout=0.03)
            except queue.Empty:
                continue
            if event.get("event") == "state":
                state = event["data"]
                if state.get("phase") == "Failed" or state.get("error"):
                    raise RuntimeError(
                        f"Rust engine reported {state.get('phase')}: {state.get('error')}"
                    )
                if predicate(state):
                    return state
        raise TimeoutError(
            f"Rust engine state timed out; PID {self.process.pid} left running for evidence"
        )

    def close(self, timeout_s: int) -> int:
        try:
            if self.process.poll() is None:
                self.send("shutdown")
                assert self.process.stdin is not None
                self.process.stdin.close()
                self.process.wait(timeout=timeout_s)
        finally:
            for thread in self._threads:
                thread.join(timeout=1)
            self.stdout_path.write_text("".join(self.stdout_lines), encoding="utf-8")
            self.stderr_path.write_text("".join(self.stderr_lines), encoding="utf-8")
        if self.process.returncode is None:
            raise TimeoutError(
                f"Rust engine shutdown timed out; PID {self.process.pid} left running for evidence"
            )
        return self.process.returncode


def rust_run(
    executable: Path,
    directory: Path,
    count: int,
    ordinal: int,
    warmup: bool,
    timeout_s: int,
) -> dict[str, Any]:
    stem = f"rust-{count}-{ordinal:02d}"
    csv_path, stdout_path, stderr_path = (
        directory / f"{stem}.csv",
        directory / f"{stem}.stdout.log",
        directory / f"{stem}.stderr.log",
    )
    cpu_before, lifecycle_begin = cpu_children(), time.perf_counter()
    engine = RustEngine(executable, stdout_path, stderr_path)
    try:
        engine.send("connect_source", simulation=True, resource="")
        engine.wait_state(lambda state: state["source_connected"], timeout_s)
        engine.send("connect_meter", simulation=True, resource="")
        engine.wait_state(lambda state: state["connected"], timeout_s)
        config = {
            "mode": "dc_iv",
            "pattern": "linear",
            "start": -0.001,
            "stop": 0.001,
            "step": 0.002 / (count - 1),
            "repeat": 1,
            "custom_points": [],
            "source_limit": 0.001,
            "compliance": 0.001,
            "settle_s": 0.0,
            "start_settle_s": 0.0,
            "nplc": 1.0,
            "average_count": 1,
            "ramp_step": 0.001,
            "ramp_wait_s": 0.0,
        }
        run_begin = time.perf_counter()
        engine.send("run", config=config, path=str(csv_path))
        completed = engine.wait_state(
            lambda state: state["phase"] == "Completed", timeout_s
        )
        run_to_state_s = time.perf_counter() - run_begin
        engine.send("disconnect")
        engine.wait_state(lambda state: not state["connected"], timeout_s)
        exit_code = engine.close(timeout_s)
        lifecycle_s = time.perf_counter() - lifecycle_begin
    except Exception:
        engine.close(timeout_s)
        raise
    if exit_code != 0:
        raise RuntimeError(f"Rust engine exit code {exit_code}; see {stderr_path}")
    validation_begin = time.perf_counter()
    validation = validate_csv(csv_path, count)
    return {
        "engine": "rust",
        "warmup": warmup,
        "count": count,
        "ordinal": ordinal,
        "pid": engine.process.pid,
        "exit_code": exit_code,
        "run_to_completed_state_s": run_to_state_s,
        "state_elapsed_s_measurement_loop": completed["elapsed"],
        "process_lifecycle_s": lifecycle_s,
        "csv_validation_s": time.perf_counter() - validation_begin,
        "child_cpu_s": cpu_delta(cpu_before),
        "csv": str(csv_path),
        "stdout": str(stdout_path),
        "stderr": str(stderr_path),
        "validation": validation,
    }


def python_run(
    python: Path,
    template: Path,
    directory: Path,
    count: int,
    ordinal: int,
    warmup: bool,
    timeout_s: int,
) -> dict[str, Any]:
    stem = f"python-{count}-{ordinal:02d}"
    csv_path, stdout_path, stderr_path = (
        directory / f"{stem}.csv",
        directory / f"{stem}.stdout.log",
        directory / f"{stem}.stderr.log",
    )
    command = [
        str(python),
        str(Path(__file__).resolve()),
        "--child-python",
        "--template",
        str(template),
        "--count",
        str(count),
        "--output",
        str(csv_path),
    ]
    env = {**os.environ, "PYTHONPATH": str(ROOT / "src")}
    cpu_before, wall_begin = cpu_children(), time.perf_counter()
    completed = subprocess.run(
        command, cwd=ROOT, text=True, capture_output=True, env=env, timeout=timeout_s
    )
    wall_s = time.perf_counter() - wall_begin
    stdout_path.write_text(completed.stdout, encoding="utf-8")
    stderr_path.write_text(completed.stderr, encoding="utf-8")
    if completed.returncode != 0:
        raise RuntimeError(
            f"Python child exit code {completed.returncode}; see {stderr_path}"
        )
    result = json.loads(completed.stdout.strip().splitlines()[-1])
    if not result["source_output_off"]:
        raise AssertionError(
            "Python simulated source did not report output off after run"
        )
    validation_begin = time.perf_counter()
    validation = validate_csv(csv_path, count)
    return {
        "engine": "python-source-api",
        "warmup": warmup,
        "count": count,
        "ordinal": ordinal,
        "exit_code": completed.returncode,
        "command": command,
        "process_wall_s": wall_s,
        "planner_s": result["planner_s"],
        "measurement_loop_s": result["loop_s"],
        "python_process_cpu_s": result["python_process_cpu_s"],
        "source_output_off": result["source_output_off"],
        "csv_validation_s": time.perf_counter() - validation_begin,
        "child_cpu_s": cpu_delta(cpu_before),
        "csv": str(csv_path),
        "stdout": str(stdout_path),
        "stderr": str(stderr_path),
        "validation": validation,
    }


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--child-python", action="store_true")
    parser.add_argument(
        "--template", type=Path, default=ROOT / "portable/configs/simulated.json"
    )
    parser.add_argument("--python", type=Path, default=ROOT / ".venv/bin/python")
    parser.add_argument(
        "--rust", type=Path, default=ROOT / "next/target/release/kohdalab-iv-next"
    )
    parser.add_argument(
        "--output-dir", type=Path, default=ROOT / "next/validation/performance-mac"
    )
    parser.add_argument("--count", type=int, default=1001)
    parser.add_argument("--output", type=Path)
    parser.add_argument(
        "--counts",
        type=int,
        nargs="+",
        default=[1001, 10001],
        help="Resolution- and IPC-valid linear counts (1001, 10001 by default).",
    )
    parser.add_argument("--repeats", type=int, default=5)
    parser.add_argument("--timeout-s", type=int, default=300)
    args = parser.parse_args()
    if args.child_python:
        if args.output is None:
            parser.error("--child-python requires --output")
        python_child(args)
        return
    if sys.platform != "darwin":
        raise SystemExit(
            "This benchmark is intentionally recorded on the development Mac."
        )
    for path in (args.template, args.python, args.rust):
        if not path.is_file():
            raise SystemExit(f"Missing required path: {path}")
    if any(count < 2 for count in args.counts) or args.repeats < 1:
        raise SystemExit("Counts must be at least two and repeats at least one.")
    run_dir = args.output_dir / time.strftime("run-%Y%m%d-%H%M%S")
    run_dir.mkdir(parents=True, exist_ok=False)
    report: dict[str, Any] = {
        "purpose": "same-Mac development Python source/API versus compiled Rust engine; not shipped GUI/frozen comparison",
        "invocation": [sys.executable, *sys.argv],
        "started_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "python_version": sys.version,
        "template": str(args.template.resolve()),
        "config_sha256": sha256(args.template),
        "python_executable": str(args.python.resolve()),
        "rust_executable": str(args.rust.resolve()),
        "rust_sha256": sha256(args.rust),
        "source_hashes": {
            str(path.relative_to(ROOT)): sha256(path)
            for path in [
                Path(__file__).resolve(),
                ROOT / "src/kohdalab_iv/api/experiment.py",
                ROOT / "src/kohdalab_iv/api/measurements.py",
                ROOT / "src/kohdalab_iv/api/scan_plan.py",
                ROOT / "next/crates/iv-desktop/src/engine.rs",
                ROOT / "next/crates/iv-core/src/lib.rs",
            ]
        },
        "counts": args.counts,
        "repeats": args.repeats,
        "warmup": "one 1001-point run per engine",
        "excluded_case": "100001 points over +/-1 mV requires 20 nV increments, below the 100 nV Rust source resolution; a custom-list workaround also exceeds the engine's 64 KiB IPC input frame.",
        "rows": [],
    }
    for count in args.counts:
        (run_dir / f"profile-{count}.json").write_text(
            json.dumps(configured_profile(args.template, count), indent=2) + "\n",
            encoding="utf-8",
        )
    ordinal = 0
    for engine in ("python", "rust"):
        ordinal += 1
        report["rows"].append(
            python_run(
                args.python, args.template, run_dir, 1001, ordinal, True, args.timeout_s
            )
            if engine == "python"
            else rust_run(args.rust, run_dir, 1001, ordinal, True, args.timeout_s)
        )
    for count in args.counts:
        for repeat in range(1, args.repeats + 1):
            order = ("python", "rust") if repeat % 2 else ("rust", "python")
            for engine in order:
                ordinal += 1
                report["rows"].append(
                    python_run(
                        args.python,
                        args.template,
                        run_dir,
                        count,
                        ordinal,
                        False,
                        args.timeout_s,
                    )
                    if engine == "python"
                    else rust_run(
                        args.rust, run_dir, count, ordinal, False, args.timeout_s
                    )
                )
    report["ended_utc"] = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())
    report_path = run_dir / "benchmark.json"
    report_path.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(report_path)


if __name__ == "__main__":
    main()
