#!/usr/bin/env python3
"""Build and verify the macOS drag-to-Applications DMG from the portable payload.

The DMG is made from the same ``KohdaLab IV.app`` directory that
``package_rust_preview.py`` signed and zipped; nothing is rebuilt or re-signed.
After creation the image is checksum-verified, mounted read-only, and the
mounted app must pass ``codesign --verify --deep --strict`` and contain
byte-identical files to the payload app (and, when given, the portable ZIP).
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import subprocess
import tempfile
from zipfile import ZipFile

APP_NAME = "KohdaLab IV.app"


def digest_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def digest(path: Path) -> str:
    sha = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1 << 20), b""):
            sha.update(chunk)
    return sha.hexdigest()


def app_files(app: Path) -> dict[str, str]:
    return {
        f.relative_to(app).as_posix(): digest(f)
        for f in sorted(app.rglob("*"))
        if f.is_file() and not f.is_symlink()
    }


def zip_app_files(archive: Path) -> dict[str, str]:
    files: dict[str, str] = {}
    with ZipFile(archive) as bundle:
        for info in bundle.infolist():
            if info.is_dir():
                continue
            parts = PurePosixPath(info.filename).parts
            if APP_NAME not in parts:
                continue
            relative = PurePosixPath(*parts[parts.index(APP_NAME) + 1 :])
            files[relative.as_posix()] = digest_bytes(bundle.read(info))
    return files


def run(*command: str) -> str:
    return subprocess.run(
        command, check=True, capture_output=True, text=True
    ).stdout


def compare(label: str, expected: dict[str, str], actual: dict[str, str]) -> None:
    if not expected:
        raise SystemExit(f"{label}: no app files found")
    missing = sorted(expected.keys() - actual.keys())
    extra = sorted(actual.keys() - expected.keys())
    changed = sorted(k for k in expected.keys() & actual.keys() if expected[k] != actual[k])
    if missing or extra or changed:
        raise SystemExit(
            f"{label}: DMG app differs (missing={missing}, extra={extra}, changed={changed})"
        )


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--payload-dir", type=Path, required=True)
    parser.add_argument("--version", required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--zip", type=Path, help="Portable ZIP whose app bytes must match")
    args = parser.parse_args()

    app = args.payload_dir / APP_NAME
    if not app.is_dir():
        raise SystemExit(f"Missing app bundle: {app}")
    plist = app / "Contents/Info.plist"
    bundle_version = run(
        "/usr/libexec/PlistBuddy", "-c", "Print :KohdaLabApplicationVersion", str(plist)
    ).strip()
    if bundle_version != args.version:
        raise SystemExit(f"App version {bundle_version!r} != {args.version!r}")
    run("codesign", "--verify", "--deep", "--strict", str(app))
    expected = app_files(app)
    executables = sorted(
        k for k in expected if os.access(app / k, os.X_OK)
    )
    if "Contents/MacOS/kohdalab-iv" not in executables:
        raise SystemExit("Main executable missing from payload app")
    if args.zip:
        compare("portable ZIP", expected, zip_app_files(args.zip))

    output = args.output.resolve()
    output.parent.mkdir(parents=True, exist_ok=True)
    if output.exists():
        output.unlink()
    with tempfile.TemporaryDirectory(prefix="kohdalab-iv-dmg-") as temp:
        temp_path = Path(temp)
        source = temp_path / "source"
        source.mkdir()
        # ditto preserves the signature, extended attributes and permissions.
        run("ditto", str(app), str(source / APP_NAME))
        (source / "Applications").symlink_to("/Applications")
        run(
            "hdiutil", "create",
            "-volname", f"KohdaLab IV {args.version}",
            "-srcfolder", str(source),
            "-fs", "HFS+",
            "-format", "UDZO",
            "-imagekey", "zlib-level=9",
            "-ov", str(output),
        )
        try:
            run("hdiutil", "verify", str(output))
            mountpoint = temp_path / "mount"
            mountpoint.mkdir()
            run(
                "hdiutil", "attach", str(output),
                "-readonly", "-nobrowse", "-noautoopen",
                "-mountpoint", str(mountpoint),
            )
            try:
                mounted = mountpoint / APP_NAME
                link = mountpoint / "Applications"
                if not link.is_symlink() or os.readlink(link) != "/Applications":
                    raise SystemExit("DMG is missing the /Applications link")
                run("codesign", "--verify", "--deep", "--strict", "--verbose=2", str(mounted))
                compare("payload app", expected, app_files(mounted))
            finally:
                run("hdiutil", "detach", str(mountpoint))
        except BaseException:
            output.unlink(missing_ok=True)
            raise

    checksum = digest(output)
    output.with_name(output.name + ".sha256").write_text(
        f"{checksum}  {output.name}\n", encoding="utf-8"
    )
    print(
        json.dumps(
            {
                "dmg": str(output),
                "bytes": output.stat().st_size,
                "sha256": checksum,
                "verified_files": len(expected),
                "executables": {k: expected[k] for k in executables},
            },
            indent=2,
        )
    )


if __name__ == "__main__":
    main()
