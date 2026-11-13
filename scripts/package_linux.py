#!/usr/bin/env python3
"""Package the simulation-only Ubuntu 24.04 x64 DEB and portable tarball.

Both packages carry the same payload: the two Rust binaries built with
``--features simulation-only``, Python control clients, complete corresponding
source and license material. No instrument helper, driver or udev rule is
included. The binaries are checked to refuse hardware before packaging.
"""

from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import tarfile
import tempfile
import time
import tomllib

from distribution_policy import validate_archive
from package_rust_preview import dependency_notices, digest, source_archive

ROOT = Path(__file__).resolve().parents[1]
PLATFORM = "linux-x64"
EDITION = "simulation"
TARGET = "x86_64-unknown-linux-gnu"
INSTALL_DIR = "opt/kohdalab-iv"
BINARIES = ("kohdalab-iv", "iv-cli")
SIMULATION_ONLY_MARKER = "simulation-only"
# Loaded through winit/glutin dlopen, so dpkg-shlibdeps cannot discover them.
DEPENDS = (
    "libc6 (>= 2.39), libgcc-s1, libegl1, libgl1, libxkbcommon0, "
    "libxkbcommon-x11-0, libwayland-client0, libwayland-cursor0, "
    "libwayland-egl1, libx11-6, libx11-xcb1, libxcursor1, libxi6, libxrandr2"
)
RECOMMENDS = "xdg-desktop-portal"
REPOSITORY = "https://github.com/woolen-yarn/kohdalab-iv"


def deb_version(version: str) -> str:
    # 1.0.1-alpha.1 must sort before 1.0.1 in dpkg.
    return version.replace("-", "~", 1)


def require_linux_x64(binary: Path) -> None:
    header = binary.read_bytes()[:20]
    if header[:4] != b"\x7fELF" or header[4] != 2 or int.from_bytes(header[18:20], "little") != 0x3E:
        raise SystemExit(f"{binary} is not a Linux x86-64 ELF executable")


def require_simulation_only(binary: Path) -> None:
    """The packaged GUI must refuse discovery even with helpers offered."""
    with tempfile.TemporaryDirectory(prefix="iv-sim-only-check-") as temp:
        marker = Path(temp) / "helper-started"
        for name in ("kohdalab-gpib-helper", "kohdalab-usbtmc-helper"):
            helper = Path(temp) / name
            helper.write_text(f"#!/bin/sh\ntouch '{marker}'\nexit 1\n")
            helper.chmod(0o700)
        result = subprocess.run(
            [str(binary), "--list"],
            capture_output=True,
            text=True,
            timeout=30,
            env={**os.environ, "HOME": temp, "KOHDALAB_IV_HELPER_DIR": temp},
        )
        if result.returncode == 0 or SIMULATION_ONLY_MARKER not in result.stderr or marker.exists():
            raise SystemExit(
                f"{binary} was not built with --features simulation-only "
                f"(exit {result.returncode}: {result.stderr.strip()!r})"
            )


def stage_payload(stage: Path, binaries: Path) -> None:
    stage.mkdir(parents=True)
    for name in BINARIES:
        shutil.copy2(binaries / name, stage / name)
        (stage / name).chmod(0o755)
    support = stage / "support"
    support.mkdir()
    shutil.copytree(ROOT / "native/licenses", support / "licenses")
    shutil.copy2(ROOT / "native/vendor/libwdi/COPYING", support / "licenses/libwdi-COPYING.txt")
    shutil.copy2(ROOT / "native/vendor/libwdi/COPYING-LGPL", support / "licenses/libwdi-LGPL.txt")
    shutil.copy2(ROOT / "LICENSE", support / "licenses/KohdaLab-IV-MIT.txt")
    shutil.copy2(ROOT / "native/REBUILDING.md", support / "REBUILDING.md")
    shutil.copy2(ROOT / "packaging/licenses/THIRD_PARTY_NOTICES.md", support / "THIRD_PARTY_NOTICES.md")
    source_archive(support / "SOURCE.zip")
    dependency_notices(support / "licenses/rust", TARGET)
    shutil.copy2(ROOT / "REDISTRIBUTION.md", stage / "REDISTRIBUTION.md")
    shutil.copy2(ROOT / "README.md", stage / "README.md")
    shutil.copy2(ROOT / "SAFETY.md", stage / "SAFETY.md")
    shutil.copy2(ROOT / "packaging/linux/SIMULATION-ONLY.md", stage / "SIMULATION-ONLY.md")
    shutil.copy2(ROOT / "assets/kohdalab-iv.png", support / "kohdalab-iv.png")
    ignore = shutil.ignore_patterns("__pycache__", "*.pyc")
    shutil.copytree(ROOT / "bindings/python", stage / "python-control", ignore=ignore)
    shutil.copytree(ROOT / "bindings/control", stage / "python-control/common", ignore=ignore)
    for path in stage.rglob("*"):
        if path.is_dir():
            path.chmod(0o755)
        elif path.name not in BINARIES or path.parent != stage:
            path.chmod(0o644)
    files = {
        str(f.relative_to(stage)): digest(f) for f in sorted(stage.rglob("*")) if f.is_file()
    }
    (stage / "FILES.json").write_text(json.dumps(files, indent=2) + "\n", encoding="utf-8")
    (stage / "FILES.json").chmod(0o644)


def normalized(mtime: int):
    def apply(info: tarfile.TarInfo) -> tarfile.TarInfo:
        info.uid = info.gid = 0
        info.uname = info.gname = "root"
        info.mtime = mtime
        return info

    return apply


def build_tarball(stage: Path, output: Path, mtime: int) -> None:
    with tarfile.open(output, "w:gz", format=tarfile.PAX_FORMAT) as archive:
        for path in [stage, *sorted(stage.rglob("*"))]:
            archive.add(
                path,
                arcname=str(Path(stage.name) / path.relative_to(stage)),
                recursive=False,
                filter=normalized(mtime),
            )


def build_deb(stage: Path, output: Path, version: str) -> None:
    with tempfile.TemporaryDirectory(prefix="kohdalab-iv-deb-") as temp:
        root = Path(temp) / "root"
        shutil.copytree(stage, root / INSTALL_DIR, symlinks=True)
        (root / "usr/bin").mkdir(parents=True)
        for name in BINARIES:
            (root / "usr/bin" / name).symlink_to(f"/{INSTALL_DIR}/{name}")
        applications = root / "usr/share/applications"
        applications.mkdir(parents=True)
        shutil.copy2(ROOT / "packaging/linux/kohdalab-iv.desktop", applications)
        pixmaps = root / "usr/share/pixmaps"
        pixmaps.mkdir(parents=True)
        shutil.copy2(ROOT / "assets/kohdalab-iv.png", pixmaps / "kohdalab-iv.png")
        doc = root / "usr/share/doc/kohdalab-iv"
        doc.mkdir(parents=True)
        (doc / "copyright").write_text(
            "Format: https://www.debian.org/doc/packaging-manuals/copyright-format/1.0/\n"
            "Upstream-Name: KohdaLab IV\n"
            f"Source: {REPOSITORY}\n\n"
            "Files: *\n"
            "License: MIT\n"
            " The full project license, third-party notices, Rust dependency\n"
            " licenses and the complete corresponding source are installed in\n"
            f" /{INSTALL_DIR}/support (licenses/, THIRD_PARTY_NOTICES.md, SOURCE.zip).\n",
            encoding="utf-8",
        )
        installed_kib = sum(f.stat().st_size for f in root.rglob("*") if f.is_file() and not f.is_symlink()) // 1024
        control = root / "DEBIAN"
        control.mkdir()
        (control / "control").write_text(
            f"Package: kohdalab-iv\n"
            f"Version: {deb_version(version)}\n"
            f"Architecture: amd64\n"
            f"Maintainer: KohdaLab IV <{REPOSITORY}/issues>\n"
            f"Homepage: {REPOSITORY}\n"
            f"Installed-Size: {installed_kib}\n"
            f"Depends: {DEPENDS}\n"
            f"Recommends: {RECOMMENDS}\n"
            f"Section: science\n"
            f"Priority: optional\n"
            f"Description: KohdaLab IV measurement GUI (simulation only)\n"
            f" Simulation-only Linux build of KohdaLab IV. Instrument discovery and\n"
            f" hardware connections are compiled out; no helper, driver or udev\n"
            f" rule is installed. Use the Windows or macOS release for instruments.\n",
            encoding="utf-8",
        )
        for path in [root, *root.rglob("*")]:
            if path.is_dir() and not path.is_symlink():
                path.chmod(0o755)
        (control / "control").chmod(0o644)
        (doc / "copyright").chmod(0o644)
        (applications / "kohdalab-iv.desktop").chmod(0o644)
        (pixmaps / "kohdalab-iv.png").chmod(0o644)
        subprocess.run(
            ["dpkg-deb", "--root-owner-group", "-Zxz", "--build", str(root), str(output)],
            check=True,
        )


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binaries-dir", type=Path, default=ROOT / "target/release")
    parser.add_argument("--output-dir", type=Path, default=ROOT / "dist")
    args = parser.parse_args()
    version = tomllib.loads((ROOT / "Cargo.toml").read_text())["workspace"]["package"]["version"]
    for name in BINARIES:
        require_linux_x64(args.binaries_dir / name)
    require_simulation_only(args.binaries_dir / "kohdalab-iv")

    name = f"kohdalab-iv-{version}-{PLATFORM}-{EDITION}"
    output = args.output_dir.resolve()
    output.mkdir(parents=True, exist_ok=True)
    stage = output / name
    if stage.exists():
        shutil.rmtree(stage)
    stage_payload(stage, args.binaries_dir)
    mtime = int(os.environ.get("SOURCE_DATE_EPOCH", time.time()))

    tarball = output / f"{name}.tar.gz"
    deb = output / f"{name}.deb"
    build_tarball(stage, tarball, mtime)
    build_deb(stage, deb, version)
    results = []
    for artifact in (tarball, deb):
        try:
            validate_archive(artifact)
        except Exception:
            artifact.unlink()
            raise
        checksum = digest(artifact)
        artifact.with_name(artifact.name + ".sha256").write_text(
            f"{checksum}  {artifact.name}\n", encoding="utf-8"
        )
        results.append({"artifact": str(artifact), "bytes": artifact.stat().st_size, "sha256": checksum})
    print(json.dumps(results, indent=2))


if __name__ == "__main__":
    main()
