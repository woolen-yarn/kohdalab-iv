"""Capture the fixed device footer in both layouts, with and without scrolling."""
import argparse
import json
from pathlib import Path
import subprocess


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    binary = args.binary.resolve(strict=True)
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)

    for columns in (1, 2):
        positions = []
        for scrolled in (False, True):
            suffix = "after" if scrolled else "before"
            screenshot = output / f"iv-{columns}-column-{suffix}.png"
            command = [str(binary), "--gui-footer-smoke", "--screenshot", str(screenshot)]
            if columns == 2:
                command.append("--footer-two-columns")
            if scrolled:
                command.append("--footer-scrolled")
            result = subprocess.run(command, capture_output=True, text=True, timeout=20)
            if result.returncode:
                raise RuntimeError(result.stderr)
            report = next(json.loads(line) for line in result.stdout.splitlines()
                          if '"footer_geometry"' in line)
            offset, top, bottom = report["footer_geometry"]
            if (report["two_columns"] != (columns == 2) or report["points"] != 0
                    or not screenshot.is_file() or not 0 <= report["window_bottom"] - bottom < 32
                    or bottom - top < 20 or (offset > 100) != scrolled):
                raise RuntimeError(f"Invalid device footer capture: {report}")
            positions.append((top, bottom))
            print(f"{screenshot.name}: scroll={offset:.1f}, footer={top:.1f}–{bottom:.1f}")
        if any(abs(before - after) > 1 for before, after in zip(*positions)):
            raise RuntimeError("The Add device footer moved with the cards")


if __name__ == "__main__":
    main()
