#!/usr/bin/env python3
"""Extract and smoke-check the simulation-only Linux DEB and portable tarball.

Static checks (any host with ``dpkg-deb``): redistribution policy, payload
hashes against FILES.json, identical DEB/tarball payloads, DEB metadata and
layout, and the absence of any instrument helper, driver or extra executable.

Runtime checks (Linux x86-64): the extracted binaries report the expected
version, refuse every hardware route even when fake helpers are offered through
``KOHDALAB_IV_HELPER_DIR``, the working directory and the payload ``support``
folder, and complete a simulated measurement to CSV.
"""

from __future__ import annotations

import argparse
import csv
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import tarfile
import tempfile

from distribution_policy import validate_archive

INSTALL_DIR = "opt/kohdalab-iv"
EXECUTABLES = {"kohdalab-iv", "iv-cli"}
FORBIDDEN = (
    "kohdalab-gpib-helper",
    "kohdalab-usbtmc-helper",
    "kohdalab-visa-helper",
    "kohdalab-instrument-hub",
    "usb-setup",
    "libwdi",
    "gpib-probe",
    "usbtmc-probe",
)
REQUIRED = (
    "kohdalab-iv",
    "iv-cli",
    "README.md",
    "SAFETY.md",
    "SIMULATION-ONLY.md",
    "REDISTRIBUTION.md",
    "FILES.json",
    "support/SOURCE.zip",
    "support/THIRD_PARTY_NOTICES.md",
    "support/REBUILDING.md",
    "support/licenses/KohdaLab-IV-MIT.txt",
    "support/licenses/rust/DEPENDENCIES.json",
    "python-control/kohdalab_iv_native.py",
    "python-control/common/kohdalab_control.py",
)
REFUSAL = "simulation-only"


def digest(path: Path) -> str:
    with path.open("rb") as handle:
        return hashlib.file_digest(handle, "sha256").hexdigest()


def fail(message: str) -> None:
    raise SystemExit(f"FAIL: {message}")


def payload_files(root: Path) -> dict[str, str]:
    return {
        f.relative_to(root).as_posix(): digest(f)
        for f in sorted(root.rglob("*"))
        if f.is_file() and not f.is_symlink()
    }


def check_payload(root: Path, label: str) -> dict[str, str]:
    for name in REQUIRED:
        if not (root / name).is_file():
            fail(f"{label}: missing {name}")
    files = payload_files(root)
    recorded = json.loads((root / "FILES.json").read_text(encoding="utf-8"))
    actual = {k: v for k, v in files.items() if k != "FILES.json"}
    if recorded != actual:
        missing = sorted(recorded.keys() - actual.keys())
        extra = sorted(actual.keys() - recorded.keys())
        changed = sorted(k for k in recorded.keys() & actual.keys() if recorded[k] != actual[k])
        fail(f"{label}: FILES.json mismatch (missing={missing}, extra={extra}, changed={changed})")
    for relative in files:
        path = root / relative
        lower = path.name.lower()
        if lower.startswith(FORBIDDEN) or lower.endswith((".exe", ".dll", ".so", ".rules", ".ko")):
            fail(f"{label}: hardware helper or driver must not ship: {relative}")
        with path.open("rb") as handle:
            elf = handle.read(4) == b"\x7fELF"
        if elf and relative not in EXECUTABLES:
            fail(f"{label}: unexpected native executable {relative}")
        executable = os.access(path, os.X_OK)
        if (relative in EXECUTABLES) != executable:
            fail(f"{label}: wrong executable bit on {relative}")
    for name in EXECUTABLES:
        header = (root / name).read_bytes()[:20]
        if header[:4] != b"\x7fELF" or int.from_bytes(header[18:20], "little") != 0x3E:
            fail(f"{label}: {name} is not an x86-64 ELF executable")
    return files


def extract_tarball(archive: Path, destination: Path, version: str) -> Path:
    with tarfile.open(archive) as bundle:
        top = {Path(member.name).parts[0] for member in bundle.getmembers()}
        expected = f"kohdalab-iv-{version}-linux-x64-simulation"
        if top != {expected}:
            fail(f"tarball must contain only {expected}/, found {sorted(top)}")
        for member in bundle.getmembers():
            if member.uid or member.gid:
                fail(f"tarball member is not root-owned: {member.name}")
        bundle.extractall(destination, filter="data")
    return destination / expected


def dpkg(*args: str) -> str:
    return subprocess.run(["dpkg-deb", *args], check=True, capture_output=True, text=True).stdout


def extract_deb(deb: Path, destination: Path, version: str) -> Path:
    fields = {
        key: dpkg("--field", str(deb), key).strip()
        for key in ("Package", "Version", "Architecture", "Depends", "Description")
    }
    if fields["Package"] != "kohdalab-iv":
        fail(f"DEB package name {fields['Package']!r}")
    if fields["Version"] != version.replace("-", "~", 1):
        fail(f"DEB version {fields['Version']!r} does not match {version!r}")
    if fields["Architecture"] != "amd64":
        fail(f"DEB architecture {fields['Architecture']!r}")
    if "simulation only" not in fields["Description"].lower():
        fail("DEB description must state simulation only")
    contents = dpkg("--contents", str(deb))
    for line in contents.splitlines():
        if not line.startswith(("drwxr-xr-x root/root", "-rw-r--r-- root/root", "-rwxr-xr-x root/root", "lrwxrwxrwx root/root")):
            fail(f"DEB entry has unexpected mode/owner: {line}")
    with tempfile.TemporaryDirectory() as temp:
        control = Path(temp) / "DEBIAN"
        dpkg("--control", str(deb), str(control))
        scripts = sorted(set(os.listdir(control)) - {"control", "md5sums"})
        if scripts:
            fail(f"DEB must not run maintainer scripts: {scripts}")
    dpkg("-x", str(deb), str(destination))
    for name in EXECUTABLES:
        link = destination / "usr/bin" / name
        if not link.is_symlink() or os.readlink(link) != f"/{INSTALL_DIR}/{name}":
            fail(f"DEB /usr/bin/{name} must link to /{INSTALL_DIR}/{name}")
    for name in (
        "usr/share/applications/kohdalab-iv.desktop",
        "usr/share/pixmaps/kohdalab-iv.png",
        "usr/share/doc/kohdalab-iv/copyright",
    ):
        if not (destination / name).is_file():
            fail(f"DEB missing /{name}")
    desktop = (destination / "usr/share/applications/kohdalab-iv.desktop").read_text()
    if f"Exec=/{INSTALL_DIR}/kohdalab-iv" not in desktop or "Simulation" not in desktop:
        fail("DEB desktop entry must launch the simulation build")
    outside = sorted(
        str(p.relative_to(destination))
        for p in destination.rglob("*")
        if p.is_file() and not p.is_symlink()
        and not p.is_relative_to(destination / INSTALL_DIR)
        and not str(p.relative_to(destination)).startswith("usr/share/")
    )
    if outside:
        fail(f"DEB installs files outside /{INSTALL_DIR} and /usr/share: {outside}")
    return destination / INSTALL_DIR


def run(binary: Path, *args: str, env: dict[str, str], cwd: Path, timeout: int = 60):
    return subprocess.run(
        [str(binary), *args], capture_output=True, text=True, timeout=timeout, env=env, cwd=cwd
    )


def offer_fake_helpers(directory: Path, marker: Path) -> None:
    directory.mkdir(parents=True, exist_ok=True)
    for name in ("kohdalab-gpib-helper", "kohdalab-usbtmc-helper", "kohdalab-visa-helper", "kohdalab-instrument-hub"):
        helper = directory / name
        helper.write_text(f"#!/bin/sh\necho started >> '{marker}'\nexit 1\n")
        helper.chmod(0o755)


def runtime_smoke(payload: Path, version: str, label: str) -> dict[str, object]:
    with tempfile.TemporaryDirectory(prefix="iv-linux-smoke-") as temp:
        temp_path = Path(temp)
        # Work on a copy so fake helpers can sit inside the payload's own
        # support/ folder without altering the extracted artifact.
        app = temp_path / "app"
        shutil.copytree(payload, app, symlinks=True)
        home = temp_path / "home"
        home.mkdir()
        marker = temp_path / "helper-started"
        offered = temp_path / "offered"
        offer_fake_helpers(offered, marker)
        offer_fake_helpers(app / "support", marker)
        offer_fake_helpers(temp_path / "build/native-gpib", marker)
        offer_fake_helpers(temp_path / "build/usbtmc-probe", marker)
        endpoint = temp_path / "hub.json"
        endpoint.write_text("{}")
        env = {
            **os.environ,
            "HOME": str(home),
            "XDG_CONFIG_HOME": str(home / ".config"),
            "XDG_DATA_HOME": str(home / ".local/share"),
            "KOHDALAB_IV_HELPER_DIR": str(offered),
            "KOHDALAB_INSTRUMENT_HUB_CLIENT": str(offered / "kohdalab-instrument-hub"),
            "KOHDALAB_INSTRUMENT_HUB_CONFIG": str(endpoint),
        }
        gui, cli = app / "kohdalab-iv", app / "iv-cli"

        version_line = run(cli, "--version", env=env, cwd=temp_path)
        if version_line.returncode or version_line.stdout.strip() != f"KohdaLab IV {version}":
            fail(f"{label}: iv-cli --version returned {version_line.stdout!r} {version_line.stderr!r}")
        default = run(gui, "--default-config", env=env, cwd=temp_path)
        if default.returncode:
            fail(f"{label}: --default-config failed: {default.stderr}")
        json.loads(default.stdout)

        hardware_attempts = {
            "list": ("--list",),
            "identify": ("--identify", "--source", "GPIB0::1::INSTR", "--meter", "GPIB0::22::INSTR"),
            "measure": (
                "--measure", "--confirm-open-circuit", "--source", "GPIB0::1::INSTR",
                "--meter", "GPIB0::22::INSTR", "--output", str(temp_path / "hardware.csv"),
            ),
        }
        refusals = {}
        for name, arguments in hardware_attempts.items():
            for binary in (gui, cli):
                result = run(binary, *arguments, env=env, cwd=temp_path)
                if result.returncode == 0 or REFUSAL not in result.stderr:
                    fail(
                        f"{label}: {binary.name} {name} was not refused as simulation-only "
                        f"(exit {result.returncode}, stderr {result.stderr.strip()!r})"
                    )
                refusals[f"{binary.name} {name}"] = result.stderr.strip().splitlines()[-1]
        if (temp_path / "hardware.csv").exists():
            fail(f"{label}: hardware measurement wrote a CSV")

        output = temp_path / "simulated.csv"
        simulated = run(cli, "--measure", "--simulate", "--output", str(output), env=env, cwd=temp_path, timeout=120)
        if simulated.returncode or not output.is_file():
            fail(f"{label}: simulated measurement failed: {simulated.stderr}")
        state = json.loads(simulated.stdout.strip().splitlines()[-1])
        with output.open(newline="", encoding="utf-8") as handle:
            rows = [row for row in csv.reader(handle) if row and not row[0].startswith("#")]
        if state.get("phase") != "Completed" or len(rows) < 2:
            fail(f"{label}: simulated run incomplete: phase={state.get('phase')!r}, rows={len(rows)}")

        if marker.exists():
            fail(f"{label}: an instrument helper was started: {marker.read_text()!r}")
        return {"refusals": refusals, "simulated_points": state.get("points"), "csv_rows": len(rows)}


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--tarball", type=Path, required=True)
    parser.add_argument("--deb", type=Path, required=True)
    parser.add_argument("--version", required=True)
    parser.add_argument("--report", type=Path)
    parser.add_argument("--skip-runtime", action="store_true", help="Static checks only (non-Linux hosts)")
    args = parser.parse_args()

    report: dict[str, object] = {"version": args.version}
    for artifact in (args.tarball, args.deb):
        validate_archive(artifact)
        recorded = artifact.with_name(artifact.name + ".sha256")
        if recorded.is_file() and recorded.read_text().split()[0] != digest(artifact):
            fail(f"{artifact.name}: .sha256 does not match")
        report[artifact.name] = {"bytes": artifact.stat().st_size, "sha256": digest(artifact)}

    with tempfile.TemporaryDirectory(prefix="iv-linux-verify-") as temp:
        temp_path = Path(temp)
        portable = extract_tarball(args.tarball, temp_path / "tarball", args.version)
        installed = extract_deb(args.deb, temp_path / "deb", args.version)
        portable_files = check_payload(portable, "portable")
        deb_files = check_payload(installed, "DEB")
        if portable_files != deb_files:
            fail("DEB /opt payload differs from the portable tarball payload")
        report["payload_files"] = len(portable_files)
        report["executables"] = {name: portable_files[name] for name in sorted(EXECUTABLES)}

        runtime = not args.skip_runtime
        if runtime and (platform.system() != "Linux" or platform.machine() not in {"x86_64", "AMD64"}):
            fail("runtime smoke needs Linux x86-64; pass --skip-runtime for static checks")
        if runtime:
            report["portable_runtime"] = runtime_smoke(portable, args.version, "portable")
            report["deb_runtime"] = runtime_smoke(installed, args.version, "DEB")
        report["runtime_checked"] = runtime

    text = json.dumps(report, indent=2)
    if args.report:
        args.report.parent.mkdir(parents=True, exist_ok=True)
        args.report.write_text(text + "\n", encoding="utf-8")
    print(text)


if __name__ == "__main__":
    main()
