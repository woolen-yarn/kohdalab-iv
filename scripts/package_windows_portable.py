"""Validate frozen Windows executables and create the English portable ZIP."""

from __future__ import annotations

import csv
import hashlib
import json
import math
import os
import platform
import shutil
import struct
import subprocess
import sys
import sysconfig
import tempfile
import tomllib
import zipfile
from importlib.metadata import distribution, version as dependency_version
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def verify_pe_x64(path: Path) -> None:
    data = path.read_bytes()
    if len(data) < 64 or data[:2] != b"MZ":
        raise RuntimeError(f"Not a Windows executable: {path}")
    offset = struct.unpack_from("<I", data, 0x3C)[0]
    if offset + 26 > len(data) or data[offset : offset + 4] != b"PE\0\0":
        raise RuntimeError(f"Invalid PE header: {path}")
    machine = struct.unpack_from("<H", data, offset + 4)[0]
    magic = struct.unpack_from("<H", data, offset + 24)[0]
    if machine != 0x8664 or magic != 0x20B:
        raise RuntimeError(f"Expected Windows x64 PE32+ executable: {path}")


def verify_simulated_csv(path: Path) -> None:
    with path.open(newline="", encoding="utf-8") as stream:
        rows = list(csv.DictReader(stream))
    if len(rows) != 3:
        raise RuntimeError("The frozen simulated sweep must produce three points")
    for row, expected_voltage in zip(rows, (-0.01, 0.0, 0.01), strict=True):
        voltage, current = float(row["voltage_V"]), float(row["current_A"])
        if not math.isclose(voltage, expected_voltage, abs_tol=1e-12):
            raise RuntimeError("Unexpected frozen simulated voltage")
        if not math.isclose(current, expected_voltage / 1000, abs_tol=1e-12):
            raise RuntimeError("Unexpected frozen simulated current")


def smoke_test(gui: Path, cli: Path, expected_version: str) -> dict[str, str]:
    with tempfile.TemporaryDirectory(prefix="kohdalab-portable-smoke-") as temporary:
        scratch = Path(temporary)
        config = json.loads((ROOT / "portable/configs/simulated.json").read_text())
        config["measurements"]["iv"]["output"].update(
            dir=str(scratch), filename="smoke", auto_timestamp_suffix=False
        )
        config_path = scratch / "simulated.json"
        config_path.write_text(json.dumps(config))
        env = {k: v for k, v in os.environ.items() if not k.startswith("KOHDALAB")}
        env.update(
            KOHDALAB_IV_CONFIG=str(config_path),
            KOHDALAB_IV_STATE_DIR=str(scratch / "state"),
            QT_QPA_PLATFORM="offscreen",
        )
        result = subprocess.run(
            [str(cli), "--version"],
            cwd=scratch,
            env=env,
            capture_output=True,
            text=True,
            check=True,
            timeout=30,
        )
        if result.stdout.strip() != f"kohdalab-iv {expected_version}":
            raise RuntimeError("Frozen CLI version does not match the source version")
        for operation in ("check-config", "measure"):
            subprocess.run(
                [str(cli), "--config", str(config_path), operation],
                cwd=scratch,
                env=env,
                check=True,
                timeout=60,
            )
        verify_simulated_csv(scratch / "smoke.csv")
        report = scratch / "gui-smoke.json"
        subprocess.run(
            [str(gui), "--smoke-test", str(report)],
            cwd=scratch,
            env=env,
            check=True,
            timeout=30,
        )
        if not report.is_file() or json.loads(report.read_text()) != {
            "qt_startup": "ok"
        }:
            raise RuntimeError("The frozen Qt GUI did not complete its startup test")
    return {
        "cli_version": "passed",
        "simulated_sweep": "passed",
        "qt_startup": "passed",
    }


def copy_tree(source: Path, destination: Path) -> None:
    shutil.copytree(
        source,
        destination,
        ignore=shutil.ignore_patterns(
            ".DS_Store", "__pycache__", "*.pyc", "*.egg-info", "build-kits"
        ),
    )


def assemble_layout(package: Path, gui_folder: Path) -> Path:
    """Keep one launchable application and one support folder at the top level."""
    copy_tree(gui_folder, package)
    support = package / "support"
    support.mkdir()
    copy_tree(ROOT / "portable/configs", support / "configs")
    for item in (ROOT / "portable/windows").iterdir():
        if item.is_file():
            shutil.copy2(item, support / item.name)
    copy_tree(ROOT / "portable/licenses", support / "licenses")
    shutil.copy2(ROOT / "LICENSE", support / "licenses/LICENSE-MIT.txt")
    for license_file in (ROOT / "native/licenses").glob("*.txt"):
        shutil.copy2(license_file, support / "licenses" / license_file.name)
    diagnostics = support / "diagnostics"
    diagnostics.mkdir()
    for name in ("gpib-probe.exe", "usbtmc-probe.exe"):
        shutil.copy2(ROOT / "portable/windows/native" / name, diagnostics / name)
    for name in ("USB-Setup.exe", "libwdi.dll"):
        shutil.copy2(ROOT / "portable/windows/native" / name, support / name)
    return support


def archive_corresponding_source(source: Path, archive: Path) -> None:
    with zipfile.ZipFile(archive, "w", zipfile.ZIP_DEFLATED) as zipped:
        for path in sorted(source.rglob("*")):
            if path.is_file():
                relative = path.relative_to(source.parent).as_posix()
                if not relative.isascii():
                    raise RuntimeError(f"Non-English source filename: {relative}")
                zipped.write(path, relative)
    with zipfile.ZipFile(archive) as zipped:
        if zipped.testzip() is not None:
            raise RuntimeError("Corresponding-source ZIP integrity check failed")


def verify_release_layout(package: Path) -> None:
    expected = {"KohdaLab IV.exe", "README.txt", "_internal", "support"}
    if {path.name for path in package.iterdir()} != expected:
        raise RuntimeError("The portable release has unexpected top-level files")
    for relative in (
        "KohdaLab IV.exe",
        "README.txt",
        "support/USB-Setup.exe",
        "support/libwdi.dll",
        "support/SOURCE.zip",
        "support/DRIVERS.md",
        "support/BUILD_INFO.json",
        "_internal/kohdalab-gpib-helper.exe",
        "_internal/kohdalab-usbtmc-helper.exe",
    ):
        if not (package / relative).is_file():
            raise RuntimeError(f"Portable release is missing {relative}")


def main() -> None:
    if sys.platform != "win32" or platform.machine().upper() not in ("AMD64", "X86_64"):
        raise SystemExit("Build and validate the Windows x64 portable on Windows x64")
    version = tomllib.loads((ROOT / "pyproject.toml").read_text())["project"]["version"]
    gui_folder, cli_folder = ROOT / "dist/KohdaLab IV", ROOT / "dist/kohdalab-iv"
    gui, cli = gui_folder / "KohdaLab IV.exe", cli_folder / "kohdalab-iv.exe"
    for executable in (gui, cli):
        verify_pe_x64(executable)
    checks: dict[str, str] = {}
    for folder in (gui_folder, cli_folder):
        for name in ("kohdalab-gpib-helper.exe", "kohdalab-usbtmc-helper.exe"):
            helper = folder / "_internal" / name
            verify_pe_x64(helper)
            if "usbtmc" in name:
                subprocess.run([str(helper), "--self-test"], check=True, timeout=15)
            else:
                result = subprocess.run(
                    [str(helper), "--available"],
                    capture_output=True,
                    text=True,
                    check=True,
                    timeout=15,
                )
                if result.stdout.strip() not in ("0", "1"):
                    raise RuntimeError("Frozen native GPIB helper startup failed")
    checks["native_helpers"] = "x64/startup/self-tests passed; hardware pending"
    release = ROOT / "dist/releases" / f"v{version}" / "windows-x64"
    name = f"kohdalab-iv-{version}-windows-x64-portable"
    package = release / name
    if package.exists():
        shutil.rmtree(package)  # Only this generated staging directory.
    support = assemble_layout(package, gui_folder)
    checks.update(smoke_test(package / "KohdaLab IV.exe", cli, version))
    subprocess.run(
        [str(support / "USB-Setup.exe"), "--self-test"], check=True, timeout=15
    )
    for dependency in ("pyqtgraph", "pyvisa"):
        installed = distribution(dependency)
        for item in installed.files or []:
            if str(item).endswith(("licenses/LICENSE", "licenses/LICENSE.txt")):
                shutil.copy2(
                    installed.locate_file(item),
                    support / "licenses" / f"{dependency}-LICENSE.txt",
                )
    python_license = next(
        (
            p
            for p in (
                Path(sysconfig.get_path("stdlib")) / "LICENSE.txt",
                Path(sys.base_prefix) / "LICENSE.txt",
            )
            if p.is_file()
        ),
        Path(sys.base_prefix) / "LICENSE.txt",
    )
    if not python_license.is_file():
        raise RuntimeError("CPython license text not found")
    shutil.copy2(python_license, support / "licenses/PYTHON-LICENSE.txt")
    source = support / "sources/application"
    source.mkdir(parents=True)
    for directory in ("src", "scripts", "tests", "portable", "docs", "native"):
        copy_tree(ROOT / directory, source / directory)
    for filename in (
        "README.md",
        "LICENSE",
        "pyproject.toml",
        "uv.lock",
        "MANIFEST.in",
    ):
        shutil.copy2(ROOT / filename, source / filename)
    archive_corresponding_source(source, support / "SOURCE.zip")
    shutil.rmtree(source.parent)  # Only the temporary corresponding-source staging.
    documentation = (ROOT / "docs/windows_portable.md").read_text()
    (package / "README.txt").write_text(
        f"KohdaLab IV {version} - Windows x64 Portable\n\n"
        "1. Extract the entire ZIP to a writable folder.\n"
        "2. Connect your instruments and open KohdaLab IV.exe.\n"
        "3. Approve Windows USB driver prompts if requested.\n"
        "4. Wait for the USB check, click Refresh, select matching Device and\n"
        "   Resource entries for Source and Meter, then click Connect.\n\n"
        "Python, VISA and Zadig are not required for supported connections.\n"
        "GS210 direct USB must use TMC mode. Supported GPIB adapters are\n"
        "NI GPIB-USB-HS+ and Agilent 82357B; connect only one adapter at a time.\n"
        "First-time 82357B setup requires Internet access for firmware.\n\n"
        "Keep _internal and support beside the EXE. Move the whole folder\n"
        "when moving the app to another location.\n"
        "Settings and default results are stored in your user .kohdalab folder.\n"
        "You can choose a different result directory in the app.\n\n"
        "Troubleshooting: support/DRIVERS.md and support/diagnostics.\n"
        "Example profiles: support/configs (adapt addresses to your instruments).\n"
        "Licenses: support/licenses and bundled dependency folders.\n"
        "Complete corresponding source and rebuild scripts: support/SOURCE.zip.\n"
        "Build details and completed checks: support/BUILD_INFO.json.\n"
    )
    (support / "BUILD.md").write_text(documentation)
    (support / "BUILD_INFO.json").write_text(
        json.dumps(
            {
                "application_version": version,
                "platform": "windows-x64",
                "python": sys.version,
                "checks": checks,
                "dependencies": {
                    n: dependency_version(n)
                    for n in ("PySide6", "pyvisa", "pyqtgraph", "numpy")
                },
                "instrument_testing": {
                    "direct_usb": "user-reported working before latest GUI fixes",
                    "gpib": "user-reported working with NI HS+ and Agilent 82357B",
                    "latest_gui_changes": "pending Windows hardware validation",
                },
            },
            indent=2,
        )
        + "\n"
    )
    notes = (
        f"# KohdaLab IV {version} Windows x64 portable\n\n"
        "Single GUI entry point with bundled Python, English README, USB/GPIB profiles,\n"
        "licenses, native helpers, probes, and corresponding source in support.\n"
        "Supported native connections use WinUSB, without external VISA.\n"
        "Hardware checks remain pending on the Windows instrument PC.\n\n"
        f"Release assets: `{name}.zip` and `SHA256SUMS.txt`.\n"
    )
    (support / "RELEASE_NOTES.md").write_text(notes)
    verify_release_layout(package)
    (release / "RELEASE_NOTES.md").write_text(notes)
    archive = release / f"{name}.zip"
    with zipfile.ZipFile(archive, "w", zipfile.ZIP_DEFLATED) as zipped:
        for path in sorted(package.rglob("*")):
            if path.is_file():
                relative = path.relative_to(release).as_posix()
                if not relative.isascii():
                    raise RuntimeError(f"Non-English filename in package: {relative}")
                zipped.write(path, relative)
    with zipfile.ZipFile(archive) as zipped:
        if zipped.testzip() is not None:
            raise RuntimeError("Portable ZIP integrity check failed")
    digest = hashlib.sha256(archive.read_bytes()).hexdigest()
    (release / "SHA256SUMS.txt").write_text(f"{digest}  {archive.name}\n")
    print(f"Portable release ready: {archive}")


if __name__ == "__main__":
    main()
