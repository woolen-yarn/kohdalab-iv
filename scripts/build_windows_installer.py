#!/usr/bin/env python3
"""Build a reproducible Windows Setup.exe from a prepared portable directory.

NSIS 3.12 is downloaded only when no explicit/local compiler is available. The
ZIP is the official NSIS binary archive, distributed under the zlib license.
The primary source is:
https://sourceforge.net/projects/nsis/files/NSIS%203/3.12/nsis-3.12.zip/download
If a managed network replaces its redirect with HTML, a public archive mirror
is retried and accepted only when it has the same pinned hash.
SHA-256: 56581f90db321581c5381193d796fffcf2d24b2f8fed2160a6c6a3baa67f2c4f

The compiler archive remains under build/tools/nsis-3.12; this script does not
install NSIS into Windows or modify PATH. Setup.exe contains NSIS-generated
bootstrap code, but never the NSIS compiler archive or its build tools.
"""

from __future__ import annotations

import argparse
import hashlib
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
from urllib.request import urlretrieve
from zipfile import ZipFile

from windows_version_resource import version_words


ROOT = Path(__file__).resolve().parents[1]
NSIS_VERSION = "3.12"
NSIS_URLS = (
    # Official NSIS project download. Some managed networks replace this
    # redirect with HTML, so a hash-verified archive mirror follows.
    "https://prdownloads.sourceforge.net/nsis/nsis-3.12.zip",
    "https://mirrors.mit.edu/macports/distfiles/nsis/nsis-3.12.zip",
)
NSIS_SHA256 = "56581f90db321581c5381193d796fffcf2d24b2f8fed2160a6c6a3baa67f2c4f"


def sha256(path: Path) -> str:
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def must_file(path: Path, description: str) -> Path:
    if not path.is_file():
        raise SystemExit(f"{description} does not exist: {path}")
    return path.resolve()


def must_directory(path: Path, description: str) -> Path:
    if not path.is_dir():
        raise SystemExit(f"{description} does not exist: {path}")
    return path.resolve()


def portable_compiler() -> Path:
    tools = ROOT / "build" / "tools"
    archive = tools / f"nsis-{NSIS_VERSION}.zip"
    destination = tools / f"nsis-{NSIS_VERSION}"
    binary = destination / "Bin" / "makensis.exe"
    if binary.is_file():
        return binary
    tools.mkdir(parents=True, exist_ok=True)
    if not archive.is_file() or sha256(archive) != NSIS_SHA256:
        archive.unlink(missing_ok=True)
        failures: list[str] = []
        for url in NSIS_URLS:
            print(f"Downloading pinned NSIS {NSIS_VERSION} compiler from {url}...", file=sys.stderr)
            try:
                urlretrieve(url, archive)
                actual = sha256(archive)
                if actual == NSIS_SHA256:
                    break
                failures.append(f"{url}: SHA-256 {actual}")
            except OSError as error:
                failures.append(f"{url}: {error}")
            archive.unlink(missing_ok=True)
        else:
            raise SystemExit(
                "Could not obtain the pinned NSIS archive (expected "
                f"{NSIS_SHA256}): " + "; ".join(failures)
            )
    with ZipFile(archive) as bundle:
        names = bundle.namelist()
        prefix = f"nsis-{NSIS_VERSION}/"
        if not names or any(not name.startswith(prefix) for name in names):
            raise SystemExit("NSIS archive has an unexpected layout")
        bundle.extractall(tools)
    if not binary.is_file():
        raise SystemExit("NSIS archive did not contain Bin/makensis.exe")
    return binary


def find_compiler(explicit: Path | None) -> Path:
    if explicit is not None:
        return must_file(explicit, "NSIS compiler")
    for name in ("makensis", "makensis.exe"):
        found = shutil.which(name)
        if found:
            return Path(found).resolve()
    return portable_compiler()


def windows_file_version(version: str) -> str:
    """Use exactly the same numeric PE version as the application/helpers."""
    try:
        return version_words(version).replace(",", ".")
    except ValueError as error:
        raise SystemExit(str(error)) from error


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--payload-dir", required=True, type=Path)
    parser.add_argument("--wrapper", type=Path, default=ROOT / "portable/windows/Install-Installer.ps1")
    parser.add_argument("--version", required=True)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--makensis", type=Path, help="Use this existing makensis compiler")
    args = parser.parse_args()

    payload = must_directory(args.payload_dir, "Prepared portable payload")
    wrapper = must_file(args.wrapper, "Installer wrapper")
    if not (payload / "kohdalab-iv-next.exe").is_file():
        raise SystemExit("Prepared portable payload lacks kohdalab-iv-next.exe")
    if not (payload / "iv-cli.exe").is_file():
        raise SystemExit("Prepared portable payload lacks iv-cli.exe")
    if not (payload / "FILES.json").is_file():
        raise SystemExit("Prepared portable payload lacks FILES.json")
    file_version = windows_file_version(args.version)

    output = args.output.resolve()
    output.parent.mkdir(parents=True, exist_ok=True)
    compiler = find_compiler(args.makensis)
    script = ROOT / "portable/windows/KohdaLab-IV.nsi"
    command = [
        str(compiler),
        "/V3",
        f"/DPAYLOAD_DIR={payload}",
        f"/DWRAPPER_PATH={wrapper}",
        f"/DOUTPUT_FILE={output}",
        f"/DVERSION={args.version}",
        f"/DVERSION_FILE={file_version}",
        str(script),
    ]
    print("Building Windows installer with", compiler)
    subprocess.run(command, cwd=ROOT, check=True)
    if not output.is_file() or output.stat().st_size == 0:
        raise SystemExit(f"NSIS did not create the expected installer: {output}")
    print(f"{output}  {sha256(output)}")


if __name__ == "__main__":
    main()
