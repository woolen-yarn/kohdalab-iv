"""Package the native desktop source and pinned Windows helpers for rebuilding."""

from __future__ import annotations

import argparse
import copy
import hashlib
import json
from pathlib import Path
import tomllib
from zipfile import ZIP_DEFLATED, ZipFile

from distribution_policy import validate_archive


def digest(path: Path) -> str:
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--native-dir", type=Path)
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    version = tomllib.loads((root / "next/Cargo.toml").read_text())["workspace"][
        "package"
    ]["version"]
    edition = "preview" if "-" in version else "portable"
    source = (
        root
        / "next/dist"
        / f"kohdalab-iv-{version}-windows-x64-{edition}/support/SOURCE.zip"
    )
    name = f"kohdalab-iv-{version}-build-kit"
    destination = root / "next/dist" / f"{name}.zip"
    native = args.native_dir or root / "portable/windows/native"
    known = json.loads((native / "SHA256.json").read_text())
    for filename, expected in known.items():
        if digest(native / filename) != expected:
            raise RuntimeError(f"Native helper hash mismatch: {filename}")
    with ZipFile(source) as original, ZipFile(destination, "w", ZIP_DEFLATED) as kit:
        included = set(original.namelist())
        for entry in original.infolist():
            if entry.filename == "portable/windows/native/SHA256.json":
                continue
            metadata = copy.copy(entry)
            metadata.filename = f"{name}/{entry.filename}"
            kit.writestr(metadata, original.read(entry.filename))
        for file in sorted(native.iterdir()):
            relative = "portable/windows/native/" + file.name
            if file.is_file() and (relative not in included or file.name == "SHA256.json"):
                kit.write(file, f"{name}/{relative}")
        kit.writestr(
            f"{name}/BUILD-README.txt",
            f"KohdaLab IV {version} build kit\n\n"
            "Start with next/README.md. Mac: next/build-macos.command. "
            "Windows: next/build-windows.ps1.\n"
            "Pinned Windows helpers, full native source, licenses and icons are included.\n"
            "Rust and platform compiler tools are needed only on the build PC.\n",
        )
    with ZipFile(destination) as kit:
        assert len(kit.namelist()) == len(set(kit.namelist()))
        assert kit.testzip() is None
    try:
        validate_archive(destination)
    except Exception:
        destination.unlink()
        raise
    destination.with_suffix(".zip.sha256").write_text(
        f"{digest(destination)}  {destination.name}\n"
    )
    print(
        json.dumps(
            {
                "archive": str(destination),
                "sha256": digest(destination),
                "bytes": destination.stat().st_size,
            }
        )
    )


if __name__ == "__main__":
    main()
