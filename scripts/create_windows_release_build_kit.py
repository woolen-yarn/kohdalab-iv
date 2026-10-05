"""Create the English Windows build kit; user distributions are built on Windows."""

from __future__ import annotations

import hashlib
import shutil
import tomllib
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def launcher(version: str) -> str:
    return rf"""@echo off
setlocal
cd /d "%~dp0project"
powershell -NoProfile -ExecutionPolicy Bypass -File ".\scripts\build_windows_portable.ps1"
set "TaskExit=%ERRORLEVEL%"
if not "%TaskExit%"=="0" goto build_failed
cd /d "%~dp0"
if not exist "Release" mkdir "Release"
copy /y "project\dist\releases\v{version}\windows-x64\kohdalab-iv-{version}-windows-x64-portable.zip" "Release\" >nul
if errorlevel 1 goto copy_failed
copy /y "project\dist\releases\v{version}\windows-x64\SHA256SUMS.txt" "Release\" >nul
if errorlevel 1 goto copy_failed
echo Portable ZIP and checksum are ready in Release.
start "" explorer "%~dp0Release"
exit /b 0
:copy_failed
set "TaskExit=1"
:build_failed
echo Build did not complete. See project\build\windows-portable-build.log.
pause
exit /b %TaskExit%
"""


def main() -> None:
    version = tomllib.loads((ROOT / "pyproject.toml").read_text())["project"]["version"]
    base = ROOT / "dist/windows-release-build-kit"
    kit = base / "KohdaLab-IV-Windows-Release-BuildKit"
    project = kit / "project"
    if kit.exists():
        shutil.rmtree(kit)  # Generated build-kit staging only.
    project.mkdir(parents=True, exist_ok=True)
    ignore = shutil.ignore_patterns(
        "__pycache__", "*.pyc", ".DS_Store", ".git", "build-kits"
    )
    for name in (
        "src",
        "scripts",
        "tests",
        "docs",
        "portable",
        "native",
        "notebook",
        ".github",
    ):
        shutil.copytree(ROOT / name, project / name, dirs_exist_ok=True, ignore=ignore)
    for name in (
        "README.md",
        "pyproject.toml",
        "uv.lock",
        "LICENSE",
        "MANIFEST.in",
        "CITATION.cff",
        "CONTRIBUTING.md",
        "ROADMAP.md",
        "SAFETY.md",
        "SECURITY.md",
        ".gitignore",
        "CHANGELOG.md",
    ):
        shutil.copy2(ROOT / name, project / name)
    (kit / "Build-Windows.cmd").write_bytes(
        launcher(version).replace("\n", "\r\n").encode("ascii")
    )
    (kit / "README.txt").write_text(
        f"KohdaLab IV {version} - Windows Release Builder\n\n"
        "This is the build kit. Do not distribute this folder as the portable app.\n\n"
        "1. Extract the entire ZIP on your Windows x64 build PC.\n"
        "2. Double-click Build-Windows.cmd.\n"
        "3. After success, the Release folder opens automatically.\n"
        "4. Distribute the portable ZIP in Release. Keep SHA256SUMS.txt with it.\n\n"
        "If uv is missing on the build PC, install it with:\n"
        "  winget install --id astral-sh.uv -e\n"
        "Then close the terminal and run Build-Windows.cmd again.\n\n"
        "The user distribution has only four top-level entries:\n"
        "  KohdaLab IV.exe\n  README.txt\n  _internal/\n  support/\n\n"
        "The macOS distribution uses the same layout, with KohdaLab IV.app and\n"
        "its runtime inside the app instead of a separate _internal folder.\n\n"
        "Users extract the whole portable ZIP and open the application.\n"
        "Python, uv and VISA are not required on the instrument PC for supported\n"
        "native connections. Windows driver approval can be required on first setup.\n\n"
        "USB/GPIB communication was reported working on the Windows instrument PC.\n"
        "The final portable ZIP was confirmed working by the user.\n\n"
        "Technical build documentation and complete sources are inside project.\n"
    )
    archive = base / f"kohdalab-iv-{version}-windows-release-build-kit.zip"
    with zipfile.ZipFile(archive, "w", zipfile.ZIP_DEFLATED) as zipped:
        for path in sorted(kit.rglob("*")):
            if path.is_file():
                relative = path.relative_to(base).as_posix()
                if not relative.isascii():
                    raise RuntimeError(f"Unexpected filename: {relative}")
                zipped.write(path, relative)
    with zipfile.ZipFile(archive) as zipped:
        if zipped.testzip() is not None:
            raise RuntimeError("Invalid build kit archive")
        command = zipped.read(kit.name + "/Build-Windows.cmd")
        if any(c < 32 and c not in (10, 13) for c in command):
            raise RuntimeError("Invalid Windows batch file")
        if {n.split("/")[1] for n in zipped.namelist()} != {
            "Build-Windows.cmd",
            "README.txt",
            "project",
        }:
            raise RuntimeError("Unexpected build kit layout")
    digest = hashlib.sha256(archive.read_bytes()).hexdigest()
    (base / "SHA256SUMS.txt").write_text(f"{digest}  {archive.name}\n")
    print(f"Created {archive} ({archive.stat().st_size / 1024 / 1024:.2f} MiB)")


if __name__ == "__main__":
    main()
