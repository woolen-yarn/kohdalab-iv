"""Package the Rust IV desktop and its complete native helper source."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import plistlib
import re
import shutil
import subprocess
import tomllib
from zipfile import ZIP_DEFLATED, ZipFile

ROOT = Path(__file__).resolve().parents[1]
EXCLUDED = {
    "target",
    "target-windows",
    "dist",
    "build",
    "validation",
    "tools",
    ".git",
    "__pycache__",
}


def digest(path: Path) -> str:
    with path.open("rb") as handle:
        return hashlib.file_digest(handle, "sha256").hexdigest()


def mac_versions(version: str) -> tuple[str, str]:
    base, _, prerelease = version.partition("-")
    if not re.fullmatch(r"\d+\.\d+\.\d+", base):
        raise ValueError(f"Unsupported Mac version: {version}")
    if not prerelease:
        return base, base
    match = re.fullmatch(r"(alpha|beta|rc)\.(\d+)", prerelease)
    if not match or not 1 <= int(match[2]) <= 255:
        raise ValueError(f"Unsupported Mac prerelease: {version}")
    suffix = {"alpha": "a", "beta": "b", "rc": "fc"}[match[1]]
    return base, f"{base}{suffix}{match[2]}"


def source_archive(path: Path) -> None:
    with ZipFile(path, "w", ZIP_DEFLATED, strict_timestamps=False) as archive:
        for name in (
            "next",
            "native",
            "src",
            "scripts",
            "docs",
            "portable/windows",
            "portable/licenses",
            "README.md",
            "pyproject.toml",
            "uv.lock",
            "LICENSE",
            "SAFETY.md",
        ):
            base = ROOT / name
            for file in [base] if base.is_file() else base.rglob("*"):
                relative = file.relative_to(ROOT)
                if (
                    file.is_file()
                    and not any(part in EXCLUDED for part in relative.parts)
                    and file.suffix not in {".pyc", ".exe", ".dll"}
                ):
                    archive.write(file, relative)


def dependency_notices(destination: Path, target: str) -> None:
    cargo = os.environ.get("CARGO", "cargo")
    metadata = json.loads(
        subprocess.check_output(
            [
                cargo,
                "metadata",
                "--manifest-path",
                str(ROOT / "next/Cargo.toml"),
                "--locked",
                "--format-version",
                "1",
                "--filter-platform",
                target,
            ]
        )
    )
    records = []
    for package in metadata["packages"]:
        records.append(
            {
                key: package.get(key)
                for key in ("name", "version", "license", "repository", "source")
            }
        )
        directory = Path(package["manifest_path"]).parent
        candidates = list(directory.iterdir())
        for folder in ("fonts", "licenses", "license"):
            if (directory / folder).is_dir():
                candidates.extend((directory / folder).rglob("*"))
        for file in candidates:
            name = file.name.lower()
            if file.is_file() and (
                name.startswith(("license", "copying", "copyright", "notice"))
                or "license" in name
                or file.stem.lower() in {"ofl", "ufl"}
            ):
                target = (
                    destination
                    / f"{package['name']}-{package['version']}"
                    / file.relative_to(directory)
                )
                target.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(file, target)
    destination.mkdir(parents=True, exist_ok=True)
    (destination / "DEPENDENCIES.json").write_text(
        json.dumps(records, indent=2) + "\n", encoding="utf-8"
    )


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--platform", required=True, choices=("macos-arm64", "windows-x64")
    )
    parser.add_argument(
        "--binaries-dir", type=Path, help="Override the Cargo release binary directory"
    )
    args = parser.parse_args()
    version = tomllib.loads((ROOT / "next/Cargo.toml").read_text())["workspace"][
        "package"
    ]["version"]
    edition = "preview" if "-" in version else "portable"
    name = f"kohdalab-iv-{version}-{args.platform}-{edition}"
    stage = ROOT / "next/dist" / name
    if stage.exists():
        shutil.rmtree(stage)
    stage.mkdir(parents=True)
    windows = args.platform == "windows-x64"
    if windows:
        binaries = (
            args.binaries_dir
            or ROOT / "next/target-windows/x86_64-pc-windows-gnu/release"
        )
        shutil.copy2(binaries / "kohdalab-iv-next.exe", stage / "kohdalab-iv-next.exe")
        shutil.copy2(binaries / "iv-cli.exe", stage / "iv-cli.exe")
        support = stage / "support"
    else:
        binaries = args.binaries_dir or ROOT / "next/target/release"
        app = stage / "KohdaLab IV Next.app"
        contents = app / "Contents"
        macos = contents / "MacOS"
        macos.mkdir(parents=True)
        resources = contents / "Resources"
        resources.mkdir()
        shutil.copy2(
            ROOT / "next/assets/kohdalab-iv.icns", resources / "KohdaLabIV.icns"
        )
        shutil.copy2(binaries / "kohdalab-iv-next", macos / "kohdalab-iv-next")
        shutil.copy2(binaries / "iv-cli", macos / "iv-cli")
        support = macos / "support"
        short_version, build_version = mac_versions(version)
        (contents / "Info.plist").write_bytes(
            plistlib.dumps(
                {
                    "CFBundleName": "KohdaLab IV Next",
                    "CFBundleDisplayName": "KohdaLab IV Next",
                    "CFBundleExecutable": "kohdalab-iv-next",
                    "CFBundleIdentifier": "org.kohdalab.iv.next",
                    "CFBundlePackageType": "APPL",
                    "CFBundleVersion": build_version,
                    "CFBundleShortVersionString": short_version,
                    "CFBundleIconFile": "KohdaLabIV.icns",
                    "KohdaLabApplicationVersion": version,
                    "LSMinimumSystemVersion": "13.0",
                    "NSHighResolutionCapable": True,
                }
            )
        )
    support.mkdir(parents=True)
    # Keep documentation outside executable bundle locations. A dotted crate
    # license directory can otherwise be mistaken for a nested Mac code bundle.
    documentation = support if windows else stage / "support"
    documentation.mkdir(parents=True, exist_ok=True)
    if windows:
        helpers = ROOT / "portable/windows/native"
        known = json.loads((helpers / "SHA256.json").read_text())
        for filename in (
            "kohdalab-gpib-helper.exe",
            "kohdalab-usbtmc-helper.exe",
            "USB-Setup.exe",
            "libwdi.dll",
        ):
            if digest(helpers / filename) != known[filename]:
                raise SystemExit(f"Native helper checksum mismatch: {filename}")
            shutil.copy2(helpers / filename, support / filename)
        shutil.copy2(ROOT / "portable/windows/setup-82357b-firmware.ps1", support)
        (support / "NATIVE-SOURCE-HASHES.json").write_text(
            json.dumps(known, indent=2) + "\n"
        )
    else:
        shutil.copy2(ROOT / "build/native-gpib/kohdalab-gpib-helper", support)
        shutil.copy2(ROOT / "build/usbtmc-probe/kohdalab-usbtmc-helper", support)
        shutil.copy2(ROOT / "scripts/setup-82357b-firmware.command", documentation)
    shutil.copytree(
        ROOT / "native/licenses", documentation / "licenses", dirs_exist_ok=True
    )
    shutil.copy2(
        ROOT / "native/vendor/libwdi/COPYING",
        documentation / "licenses/libwdi-COPYING.txt",
    )
    shutil.copy2(
        ROOT / "native/vendor/libwdi/COPYING-LGPL",
        documentation / "licenses/libwdi-LGPL.txt",
    )
    shutil.copy2(ROOT / "LICENSE", documentation / "licenses/KohdaLab-IV-MIT.txt")
    source_archive(documentation / "SOURCE.zip")
    target = (
        ("x86_64-pc-windows-msvc" if os.name == "nt" else "x86_64-pc-windows-gnu")
        if windows
        else "aarch64-apple-darwin"
    )
    dependency_notices(documentation / "licenses/rust", target)
    shutil.copy2(ROOT / "next/README.md", stage / "README.md")
    if not windows:
        for binary in (
            support / "kohdalab-gpib-helper",
            support / "kohdalab-usbtmc-helper",
        ):
            subprocess.run(
                ["codesign", "--force", "--sign", "-", str(binary)], check=True
            )
        subprocess.run(
            ["codesign", "--force", "--deep", "--sign", "-", str(app)], check=True
        )
        subprocess.run(
            ["codesign", "--verify", "--deep", "--strict", str(app)], check=True
        )
    files = {
        str(f.relative_to(stage)): digest(f) for f in stage.rglob("*") if f.is_file()
    }
    (stage / "FILES.json").write_text(
        json.dumps(files, indent=2) + "\n", encoding="utf-8"
    )
    archive_path = stage.parent / f"{stage.name}.zip"
    with ZipFile(archive_path, "w", ZIP_DEFLATED, strict_timestamps=False) as archive:
        for file in stage.rglob("*"):
            if file.is_file():
                archive.write(file, Path(name) / file.relative_to(stage))
    archive_path.with_suffix(".zip.sha256").write_text(
        f"{digest(archive_path)}  {archive_path.name}\n", encoding="utf-8"
    )
    print(
        json.dumps(
            {
                "archive": str(archive_path),
                "bytes": archive_path.stat().st_size,
                "sha256": digest(archive_path),
            }
        )
    )


if __name__ == "__main__":
    main()
