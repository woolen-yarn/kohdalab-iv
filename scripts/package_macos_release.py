"""Package the current macOS app with the same simple layout as Windows."""

from __future__ import annotations

import hashlib
import json
import os
import shutil
import subprocess
import tempfile
import tomllib
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def copy_tree(source: Path, destination: Path) -> None:
    shutil.copytree(
        source,
        destination,
        symlinks=True,
        ignore=shutil.ignore_patterns(
            ".DS_Store", "__pycache__", "*.pyc", "*.egg-info", ".git", "build-kits"
        ),
    )


def main() -> None:
    version = tomllib.loads((ROOT / "pyproject.toml").read_text())["project"]["version"]
    release = ROOT / "dist/distribution" / f"v{version}" / "macos-arm64"
    name = f"kohdalab-iv-{version}-macos-arm64-portable"
    package = release / name
    if package.exists():
        shutil.rmtree(package)  # Only generated staging for this platform/version.
    copy_tree(ROOT / "dist/KohdaLab IV.app", package / "KohdaLab IV.app")
    support = package / "support"
    support.mkdir()
    for directory in ("configs", "licenses"):
        copy_tree(ROOT / "portable" / directory, support / directory)
    shutil.copy2(ROOT / "LICENSE", support / "licenses/LICENSE-MIT.txt")
    for path in (ROOT / "native/licenses").glob("*.txt"):
        shutil.copy2(path, support / "licenses" / path.name)
    from importlib.metadata import distribution
    import sysconfig

    for dependency in ("pyqtgraph", "pyvisa"):
        installed = distribution(dependency)
        for item in installed.files or []:
            if str(item).endswith(("licenses/LICENSE", "licenses/LICENSE.txt")):
                shutil.copy2(
                    installed.locate_file(item),
                    support / "licenses" / f"{dependency}-LICENSE.txt",
                )
    shutil.copy2(
        Path(sysconfig.get_path("stdlib")) / "LICENSE.txt",
        support / "licenses/PYTHON-LICENSE.txt",
    )
    diagnostics = support / "diagnostics"
    diagnostics.mkdir()
    shutil.copy2(
        ROOT / "build/native-gpib/kohdalab-gpib-helper", diagnostics / "gpib-probe"
    )
    shutil.copy2(ROOT / "build/usbtmc-probe/usbtmc-probe", diagnostics / "usbtmc-probe")
    shutil.copy2(
        ROOT / "scripts/setup-82357b-firmware.command",
        support / "setup-82357b-firmware.command",
    )
    source = support / "sources"
    application = source / "application"
    application.mkdir(parents=True)
    for directory in ("src", "scripts", "tests", "docs", "native", "portable"):
        copy_tree(ROOT / directory, application / directory)
    for filename in (
        "README.md",
        "LICENSE",
        "pyproject.toml",
        "uv.lock",
        "MANIFEST.in",
    ):
        shutil.copy2(ROOT / filename, application / filename)
    with zipfile.ZipFile(
        support / "SOURCE.zip", "w", zipfile.ZIP_DEFLATED
    ) as source_zip:
        for path in sorted(source.rglob("*")):
            if path.is_file():
                source_zip.write(path, path.relative_to(support))
    with zipfile.ZipFile(support / "SOURCE.zip") as source_zip:
        if source_zip.testzip() is not None:
            raise RuntimeError("Corresponding-source archive is invalid")
    shutil.rmtree(source)
    details = (ROOT / "docs/portable_release.md").read_text()
    (support / "DETAILS.md").write_text(details)
    (package / "README.txt").write_text(
        f"KohdaLab IV {version} - macOS Apple Silicon Portable\n\n"
        "1. Extract the entire ZIP.\n"
        "2. Connect your instruments and open KohdaLab IV.app.\n"
        "3. Click Refresh, choose matching Device and Resource entries for\n"
        "   Source and Meter, then click Connect.\n\n"
        "Python and VISA are not required for supported native connections.\n"
        "GS210 direct USB must use TMC mode. Supported GPIB adapters are\n"
        "NI GPIB-USB-HS+ and Agilent 82357B; connect only one adapter at a time.\n"
        "For 82357B, run support/setup-82357b-firmware.command once online.\n\n"
        "Apple Silicon and macOS 13 or later are required. This app is ad-hoc\n"
        "signed, not notarized. If macOS blocks launch, use Open Anyway in\n"
        "System Settings > Privacy & Security after attempting to open it.\n\n"
        "Settings and default results are stored in your user .kohdalab folder.\n"
        "You can choose a different result directory in the app.\n\n"
        "Troubleshooting: support/DETAILS.md and support/diagnostics.\n"
        "Example profiles: support/configs (adapt addresses to your instruments).\n"
        "Licenses: support/licenses and bundled dependency folders.\n"
        "Complete corresponding source and rebuild scripts: support/SOURCE.zip.\n"
        "Build details and completed checks: support/BUILD_INFO.json.\n"
    )
    app = package / "KohdaLab IV.app"
    subprocess.run(["codesign", "--verify", "--deep", "--strict", str(app)], check=True)
    with tempfile.TemporaryDirectory(prefix="kohdalab-mac-release-") as temporary:
        scratch = Path(temporary)
        config = scratch / "simulated.json"
        shutil.copy2(support / "configs/simulated.json", config)
        report = scratch / "smoke.json"
        env = {
            key: value
            for key, value in os.environ.items()
            if not key.startswith("KOHDALAB")
        }
        env.update(
            KOHDALAB_IV_CONFIG=str(config),
            KOHDALAB_IV_STATE_DIR=str(scratch / "state"),
            QT_QPA_PLATFORM="offscreen",
        )
        subprocess.run(
            [str(app / "Contents/MacOS/KohdaLab IV"), "--smoke-test", str(report)],
            env=env,
            cwd=scratch,
            check=True,
            timeout=60,
        )
        if json.loads(report.read_text()) != {"qt_startup": "ok"}:
            raise RuntimeError("Packaged GUI startup failed")
    (support / "BUILD_INFO.json").write_text(
        json.dumps(
            {
                "application_version": version,
                "platform": "macos-arm64",
                "checks": {"signature": "passed", "packaged_qt_startup": "passed"},
                "instrument_testing": "Native USB and both GPIB adapters reported working by the user; automated checks do not verify physical measurement accuracy",
            },
            indent=2,
        )
        + "\n"
    )
    if {path.name for path in package.iterdir()} != {
        "KohdaLab IV.app",
        "README.txt",
        "support",
    }:
        raise RuntimeError("Unexpected macOS release layout")
    archive = release / f"{name}.zip"
    subprocess.run(
        ["ditto", "--norsrc", "-c", "-k", "--keepParent", str(package), str(archive)],
        check=True,
    )
    with zipfile.ZipFile(archive) as zipped:
        if zipped.testzip() is not None:
            raise RuntimeError("Invalid release archive")
        for entry in zipped.namelist():
            if not entry.isascii() or ".DS_Store" in entry or "__pycache__" in entry:
                raise RuntimeError(f"Unexpected archive entry: {entry}")
    digest = hashlib.sha256(archive.read_bytes()).hexdigest()
    (release / "SHA256SUMS.txt").write_text(f"{digest}  {archive.name}\n")
    print(f"Created {archive} ({archive.stat().st_size / 1024 / 1024:.2f} MiB)")


if __name__ == "__main__":
    main()
