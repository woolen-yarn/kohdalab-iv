from __future__ import annotations

import argparse
import sys
from pathlib import Path

from kohdalab_iv.api.config import load_config, output_path, resolve_config_path
from kohdalab_iv.api.experiment import Experiment
from kohdalab_iv.api.notebook import format_point
from kohdalab_iv.api.scan_plan import iv_plan_from_config
from kohdalab_iv.interfaces.common import list_visa_resources


def _status(status: str) -> None:
    print(f"status: {status}", flush=True)


def _point(point) -> None:
    print(format_point(point), flush=True)


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="Run KohdaLab IV/VI measurements.")
    parser.add_argument(
        "--config",
        type=Path,
        help="Config path. Defaults to environment, last-used, then packaged config.",
    )
    subparsers = parser.add_subparsers(dest="command", required=True)
    subparsers.add_parser("list-resources", help="List VISA resources.")
    subparsers.add_parser("check-config", help="Load config and print IV plan summary.")
    measure = subparsers.add_parser("measure", help="Run a measurement.")
    measure.add_argument("name", nargs="?", default="iv", help="Measurement name, default: iv")
    return parser


def main(argv: list[str] | None = None) -> int:
    args = build_parser().parse_args(argv)
    try:
        if args.command == "list-resources":
            for resource in list_visa_resources():
                print(resource)
            return 0

        resolution = resolve_config_path(args.config)
        if resolution.path is None:
            raise FileNotFoundError("No configuration file could be resolved.")
        config = load_config(resolution.path)
        if args.command == "check-config":
            plan = iv_plan_from_config(config)
            print(plan.summary)
            print(f"source: {plan.source_ref} ({plan.source_model})")
            print(f"measure: {plan.measure_ref} ({plan.measure_model})")
            print(f"output: {output_path(config, plan.measurement_name)}")
            return 0

        if args.command == "measure":
            plan = iv_plan_from_config(config, args.name)
            out = output_path(config, args.name)
            print(f"Starting {plan.summary} -> {out}", flush=True)
            experiment = Experiment(config, auto_connect=True)
            rows = experiment.run_iv(plan=plan, output=out, on_status=_status, on_point=_point)
            print(f"Saved {len(rows)} rows -> {out}", flush=True)
            return 0
    except KeyboardInterrupt:
        print("Interrupted.", file=sys.stderr, flush=True)
        return 130
    except Exception as e:
        print(f"Error: {e}", file=sys.stderr, flush=True)
        return 1
    raise ValueError(f"Unsupported command: {args.command}")


if __name__ == "__main__":
    raise SystemExit(main())
