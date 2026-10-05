"""Create a self-contained source build kit for the macOS portable."""

from __future__ import annotations

import hashlib
import shutil
import tomllib
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def main() -> None:
    version = tomllib.loads((ROOT / "pyproject.toml").read_text())["project"]["version"]
    base = ROOT / "dist/macos-release-build-kit"
    kit = base / "KohdaLab-IV-macOS-Release-BuildKit"
    if kit.exists():
        shutil.rmtree(kit)  # Generated staging only.
    project = kit / "project"
    project.mkdir(parents=True)
    ignore = shutil.ignore_patterns(
        "__pycache__", "*.pyc", ".DS_Store", ".git", "*.egg-info", "build-kits"
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
        shutil.copytree(ROOT / name, project / name, ignore=ignore)
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
    command = kit / "Build-macOS.command"
    command.write_text(
        '#!/bin/bash\nset -euo pipefail\ncd "$(dirname "$0")/project"\nbash scripts/build_macos_portable.command\n'
    )
    command.chmod(0o755)
    (kit / "README.txt").write_text(
        f"KohdaLab IV {version} - macOS Release Builder\n\n"
        "Extract the entire ZIP on an Apple Silicon Mac. Install Xcode Command\n"
        "Line Tools (xcode-select --install) and uv (https://docs.astral.sh/uv/).\n"
        "Double-click Build-macOS.command, or run bash Build-macOS.command.\n"
        "The final ZIP and checksum are created in project/dist/distribution.\n"
        "A build log is saved in project/build/macos-portable-build.log.\n\n"
        "The kit supplies native dependency sources and all rebuild scripts.\n"
        "Internet access is needed for the locked Python dependencies.\n"
        "Users of the finished portable do not need Python, uv or VISA.\n"
        "The application is ad-hoc signed; Apple notarization is not included.\n"
        "See project/docs/portable_release.md for supported connections.\n"
    )
    archive = base / f"kohdalab-iv-{version}-macos-release-build-kit.zip"
    with zipfile.ZipFile(archive, "w", zipfile.ZIP_DEFLATED) as zipped:
        for path in sorted(kit.rglob("*")):
            if path.is_file():
                zipped.write(path, path.relative_to(base))
    with zipfile.ZipFile(archive) as zipped:
        if zipped.testzip() is not None:
            raise RuntimeError("Invalid build kit archive")
    digest = hashlib.sha256(archive.read_bytes()).hexdigest()
    (base / "SHA256SUMS.txt").write_text(f"{digest}  {archive.name}\n")
    print(f"Created {archive}")


if __name__ == "__main__":
    main()
