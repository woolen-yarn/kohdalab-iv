#!/usr/bin/env python3
"""Compare two released Rust engines on the same Mac with validated simulation CSVs."""

from __future__ import annotations

import argparse
import json
from pathlib import Path
import statistics
import sys
import time

from benchmark_iv_engines import ROOT, rust_run, sha256


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--before", type=Path, required=True)
    parser.add_argument("--after", type=Path, required=True)
    parser.add_argument(
        "--output-dir", type=Path, default=ROOT / "next/validation/optimization"
    )
    parser.add_argument("--repeats", type=int, default=7)
    args = parser.parse_args()
    if sys.platform != "darwin" or args.repeats < 1:
        parser.error("Use the same development Mac and at least one repeat.")
    binaries = {"before": args.before.resolve(), "after": args.after.resolve()}
    output = args.output_dir / time.strftime("run-%Y%m%d-%H%M%S")
    output.mkdir(parents=True, exist_ok=False)
    report = {
        "purpose": "same-Mac release Rust before/after; simulated 1 kOhm, +/-1 mV, all waits zero, per-row CSV flush unchanged",
        "invocation": [sys.executable, *sys.argv],
        "binaries": {
            name: {"path": str(path), "sha256": sha256(path)}
            for name, path in binaries.items()
        },
        "counts": [1001, 10001],
        "repeats": args.repeats,
        "validation": "Every CSV checked for the established 46 fields, exact row count, targets, readback, current and resistance by benchmark_iv_engines.validate_csv; same tolerances as the original comparison.",
        "rows": [],
    }
    report_path = output / "benchmark.json"

    def save() -> None:
        report_path.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")

    def run(name: str, count: int, warmup: bool) -> None:
        row = rust_run(
            binaries[name], output, count, len(report["rows"]) + 1, warmup, 30
        )
        row["variant"] = name
        report["rows"].append(row)
        save()

    save()
    for name in binaries:
        run(name, 1001, True)
    for count in report["counts"]:
        for repeat in range(args.repeats):
            for name in ["before", "after"] if repeat % 2 == 0 else ["after", "before"]:
                run(name, count, False)
    report["summary"] = {}
    for count in report["counts"]:
        report["summary"][str(count)] = {
            name: {
                metric: statistics.median(
                    row[metric]
                    for row in report["rows"]
                    if not row["warmup"]
                    and row["count"] == count
                    and row["variant"] == name
                )
                for metric in (
                    "state_elapsed_s_measurement_loop",
                    "run_to_completed_state_s",
                    "process_lifecycle_s",
                )
            }
            for name in binaries
        }
    report["result"] = "passed"
    save()
    print(report_path)
    print(json.dumps(report["summary"], indent=2))


if __name__ == "__main__":
    main()
