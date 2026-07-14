from __future__ import annotations

import argparse
import csv
import io
import tarfile
import tomllib
import zipfile
from email.parser import BytesParser
from pathlib import Path, PurePosixPath

FORBIDDEN_PARTS = {".DS_Store", "__pycache__", ".git", ".pytest_cache"}
REQUIRED_RESOURCES = {
    "kohdalab_iv/resources/config.schema.json",
    "kohdalab_iv/resources/default.json",
    "kohdalab_iv/resources/simulated.json",
    "kohdalab_iv/instruments/meters/specs.toml",
    "kohdalab_iv/instruments/sources/specs.toml",
}


def _single(directory: Path, pattern: str) -> Path:
    matches = sorted(directory.glob(pattern))
    if len(matches) != 1:
        raise RuntimeError(
            f"Expected exactly one {pattern!r} in {directory}, found {len(matches)}."
        )
    return matches[0]


def _is_forbidden(path: str) -> bool:
    parts = PurePosixPath(path).parts
    return any(part in FORBIDDEN_PARTS for part in parts) or path.endswith(
        (".pyc", ".pyo")
    )


def _wheel_payloads(path: Path) -> dict[str, bytes]:
    with zipfile.ZipFile(path) as archive:
        bad = archive.testzip()
        if bad is not None:
            raise RuntimeError(f"Wheel CRC verification failed for {bad}.")
        return {
            info.filename: archive.read(info)
            for info in archive.infolist()
            if not info.is_dir()
        }


def _sdist_names(path: Path) -> set[str]:
    with tarfile.open(path, "r:gz") as archive:
        return {
            PurePosixPath(*PurePosixPath(member.name).parts[1:]).as_posix()
            for member in archive.getmembers()
            if member.isfile()
        }


def _verify_record(payloads: dict[str, bytes]) -> None:
    records = [name for name in payloads if name.endswith(".dist-info/RECORD")]
    if len(records) != 1:
        raise RuntimeError("Wheel must contain exactly one RECORD file.")
    rows = list(csv.reader(io.StringIO(payloads[records[0]].decode("utf-8"))))
    if {row[0] for row in rows} != set(payloads):
        raise RuntimeError("Wheel RECORD paths do not match archive members.")


def verify_build(directory: Path, root: Path) -> tuple[Path, Path]:
    wheel = _single(directory, "*.whl")
    sdist = _single(directory, "*.tar.gz")
    payloads = _wheel_payloads(wheel)
    sdist_names = _sdist_names(sdist)
    forbidden = [name for name in (*payloads, *sdist_names) if _is_forbidden(name)]
    if forbidden:
        raise RuntimeError(f"Forbidden files found in distributions: {forbidden}")
    if not REQUIRED_RESOURCES <= set(payloads):
        raise RuntimeError("Wheel is missing required runtime resources.")
    for prefix in ("tests/", "scripts/", "docs/", "notebook/"):
        if not any(name.startswith(prefix) for name in sdist_names):
            raise RuntimeError(f"sdist is missing {prefix} content.")

    metadata_names = [name for name in payloads if name.endswith(".dist-info/METADATA")]
    if len(metadata_names) != 1:
        raise RuntimeError("Wheel must contain exactly one METADATA file.")
    project = tomllib.loads((root / "pyproject.toml").read_text(encoding="utf-8"))[
        "project"
    ]
    metadata = BytesParser().parsebytes(payloads[metadata_names[0]])
    if metadata["Name"] != project["name"] or metadata["Version"] != project["version"]:
        raise RuntimeError("Wheel metadata does not match pyproject.toml.")
    _verify_record(payloads)
    return wheel, sdist


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Verify Python distribution artifacts."
    )
    parser.add_argument("directory", type=Path)
    parser.add_argument("--project-root", type=Path, default=Path.cwd())
    args = parser.parse_args()
    wheel, sdist = verify_build(args.directory, args.project_root.resolve())
    print(f"Verified {wheel.name} and {sdist.name}.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
