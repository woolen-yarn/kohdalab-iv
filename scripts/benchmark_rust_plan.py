#!/usr/bin/env python3
"""Measure standalone Rust sweep planners with macOS peak RSS counters."""

from __future__ import annotations

import argparse
import json
from pathlib import Path
import re
import statistics
import subprocess
import sys
import time

from benchmark_iv_engines import ROOT, sha256


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--before", type=Path, required=True)
    parser.add_argument("--after", type=Path, required=True)
    parser.add_argument("--repeats", type=int, default=5)
    args = parser.parse_args()
    if sys.platform != "darwin" or args.repeats < 1:
        parser.error("Requires macOS /usr/bin/time -l and positive repeats.")
    output = ROOT / "next/validation/optimization" / time.strftime("plan-%Y%m%d-%H%M%S")
    output.mkdir(exist_ok=False)
    binaries = {"before": args.before.resolve(), "after": args.after.resolve()}
    report = {
        "purpose": "standalone release planner; 990099 resolution-aligned points (10001-point linear sweep repeated99 times); no GUI, instrument I/O or CSV",
        "binaries": {
            name: {"path": str(path), "sha256": sha256(path)}
            for name, path in binaries.items()
        },
        "invocation": [sys.executable, *sys.argv],
        "rows": [],
    }
    expected = None
    report_path = output / "benchmark.json"
    for trial in range(args.repeats + 1):
        for name in ["before", "after"] if trial % 2 == 0 else ["after", "before"]:
            result = subprocess.run(
                ["/usr/bin/time", "-l", str(binaries[name])],
                text=True,
                capture_output=True,
                timeout=30,
            )
            stem = output / f"{trial}-{name}"
            stem.with_suffix(".stdout.log").write_text(result.stdout)
            stem.with_suffix(".stderr.log").write_text(result.stderr)
            if result.returncode:
                raise RuntimeError(f"Planning failed: see {stem}")
            data = json.loads(result.stdout)
            seconds = data.pop("planning_s")
            rss = re.search(r"(\d+)\s+maximum resident set size", result.stderr)
            if rss is None:
                raise RuntimeError("macOS peak RSS counter absent")
            if expected is None:
                expected = data
            if data != expected:
                raise AssertionError(
                    "Before/after point count, endpoints, direction counts, config or metadata differ"
                )
            report["rows"].append(
                {
                    "variant": name,
                    "trial": trial,
                    "warmup": trial == 0,
                    "planning_s": seconds,
                    "peak_rss_bytes": int(rss[1]),
                    "validation": data,
                }
            )
            report_path.write_text(json.dumps(report, indent=2) + "\n")
    report["summary"] = {
        name: {
            metric: statistics.median(
                row[metric]
                for row in report["rows"]
                if row["variant"] == name and not row["warmup"]
            )
            for metric in ("planning_s", "peak_rss_bytes")
        }
        for name in binaries
    }
    report["result"] = "passed"
    report_path.write_text(json.dumps(report, indent=2) + "\n")
    print(report_path)
    print(json.dumps(report["summary"], indent=2))


if __name__ == "__main__":
    main()
