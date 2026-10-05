"""Build on the target OS with uv run --extra gui --with pyinstaller."""

from __future__ import annotations

import argparse
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--target", choices=("all", "gui", "cli"), default="all")
    parser.add_argument(
        "--native-gpib-helper",
        type=Path,
        help="Bundle the NI / Agilent USB-GPIB helper for this OS.",
    )
    parser.add_argument(
        "--native-usb-helper", type=Path, help="Bundle the GS210/34411A USBTMC helper."
    )
    args = parser.parse_args()
    targets = ("gui", "cli") if args.target == "all" else (args.target,)
    for target in targets:
        name = "KohdaLab IV" if target == "gui" else "kohdalab-iv"
        command = [
            sys.executable,
            "-m",
            "PyInstaller",
            "--noconfirm",
            "--clean",
            "--onedir",
            "--name",
            name,
            "--paths",
            str(ROOT / "src"),
            "--distpath",
            str(ROOT / "dist"),
            "--workpath",
            str(ROOT / "build" / "standalone" / target),
            "--specpath",
            str(ROOT / "build" / "standalone"),
            "--collect-data",
            "kohdalab_iv",
            "--copy-metadata",
            "kohdalab-iv",
            "--hidden-import",
            "pyvisa.ctwrapper",
        ]
        if target == "gui":
            command += ["--windowed"]
            if sys.platform == "darwin":
                command += ["--osx-bundle-identifier", "org.kohdalab.iv"]
        if args.native_gpib_helper:
            helper = args.native_gpib_helper.resolve(strict=True)
            command += ["--add-binary", f"{helper}:."]
        if args.native_usb_helper:
            helper = args.native_usb_helper.resolve(strict=True)
            command += ["--add-binary", f"{helper}:."]
        command.append(str(ROOT / "scripts" / f"standalone_{target}.py"))
        subprocess.run(command, cwd=ROOT, check=True)


if __name__ == "__main__":
    main()
