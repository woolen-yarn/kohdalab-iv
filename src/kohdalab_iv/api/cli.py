from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

from kohdalab_iv import __version__
from kohdalab_iv.api.config import (
    initialize_config,
    load_config,
    output_path,
    resolve_config_path,
)
from kohdalab_iv.api.diagnostics import collect_diagnostics
from kohdalab_iv.api.experiment import Experiment
from kohdalab_iv.api.notebook import format_point
from kohdalab_iv.api.scan_plan import iv_plan_from_config
from kohdalab_iv.interfaces.common import list_visa_resources


def _status(status: str) -> None:
    print(f"status: {status}", flush=True)


def _point(point) -> None:
    print(format_point(point), flush=True)


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        prog="kohdalab-iv", description="Run KohdaLab IV/VI measurements."
    )
    parser.add_argument(
        "--version", action="version", version=f"%(prog)s {__version__}"
    )
    parser.add_argument(
        "--config",
        type=Path,
        help="Config path. Defaults to environment, last-used, then packaged config.",
    )
    subparsers = parser.add_subparsers(dest="command", required=True)
    subparsers.add_parser("list-resources", help="List VISA resources.")
    initialize = subparsers.add_parser(
        "init-config", help="Create an editable local config from the packaged default."
    )
    initialize.add_argument("path", type=Path, help="Destination JSON path.")
    initialize.add_argument(
        "--force", action="store_true", help="Replace an existing destination."
    )
    doctor = subparsers.add_parser(
        "doctor", help="Report runtime, config, and VISA diagnostics without measuring."
    )
    doctor.add_argument(
        "--json", action="store_true", help="Print machine-readable JSON."
    )
    subparsers.add_parser("check-config", help="Load config and print IV plan summary.")
    measure = subparsers.add_parser("measure", help="Run a measurement.")
    measure.add_argument(
        "name", nargs="?", default="iv", help="Measurement name, default: iv"
    )
    return parser


def main(argv: list[str] | None = None) -> int:
    args = build_parser().parse_args(argv)
    try:
        if args.command == "list-resources":
            for resource in list_visa_resources():
                print(resource)
            return 0

        if args.command == "init-config":
            destination = initialize_config(args.path, overwrite=args.force)
            print(f"Created config: {destination}")
            return 0

        if args.command == "doctor":
            report = collect_diagnostics(args.config)
            if args.json:
                print(json.dumps(report, indent=2))
            else:
                config = report["config"]
                visa = report["visa"]
                print(f"KohdaLab IV {report['version']}")
                print(f"Python: {report['python']}")
                print(f"Platform: {report['platform']}")
                if config["ok"]:
                    print(
                        f"Config: OK - {config['path']} ({config['source']}) - "
                        f"{config['plan']}"
                    )
                else:
                    print(f"Config: ERROR - {config['error']}")
                if visa["ok"]:
                    print(f"VISA: OK - {len(visa['resources'])} resource(s)")
                    for resource in visa["resources"]:
                        print(f"  {resource}")
                else:
                    print(f"VISA: ERROR - {visa['error']}")
                print(f"Overall: {'OK' if report['ok'] else 'FAILED'}")
            return 0 if report["ok"] else 1

        resolution = resolve_config_path(args.config)
        if resolution.path is None:
            raise FileNotFoundError("No configuration file could be resolved.")
        config = load_config(resolution.path)
        if args.command == "check-config":
            plan = iv_plan_from_config(config)
            print(f"config: {resolution.path} ({resolution.source})")
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
            rows = experiment.run_iv(
                plan=plan, output=out, on_status=_status, on_point=_point
            )
            print(f"Saved {len(rows)} rows -> {out}", flush=True)
            return 0
    except KeyboardInterrupt:
        print("Interrupted.", file=sys.stderr, flush=True)
        return 130
    except Exception as e:
        print(f"Error: {e}", file=sys.stderr, flush=True)
        return 1
    raise ValueError(f"Unsupported command: {args.command}")


if __name__ == "__main__":  # pragma: no cover - console script calls main
    raise SystemExit(main())
