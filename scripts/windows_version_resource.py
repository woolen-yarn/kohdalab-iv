#!/usr/bin/env python3
"""Generate a Windows VERSIONINFO resource for a KohdaLab-owned executable."""

from __future__ import annotations

import argparse
import os
import re
import sys
import tomllib
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
TEMPLATE = ROOT / "native" / "windows_version.rc"
SEMVER = re.compile(
    r"^(?P<major>0|[1-9]\d*)\.(?P<minor>0|[1-9]\d*)\.(?P<patch>0|[1-9]\d*)"
    r"(?:-(?P<pre>[0-9A-Za-z.-]+))?(?:\+[0-9A-Za-z.-]+)?$"
)


def package_version() -> str:
    with (ROOT / "next" / "Cargo.toml").open("rb") as source:
        return tomllib.load(source)["workspace"]["package"]["version"]


def version_words(version: str) -> str:
    match = SEMVER.fullmatch(version)
    if match is None:
        raise ValueError(f"unsupported Cargo package version: {version!r}")
    major, minor, patch = (int(match.group(key)) for key in ("major", "minor", "patch"))
    pre = match.group("pre")
    revision = 0
    if pre:
        label, separator, number = pre.partition(".")
        base = {"alpha": 1000, "beta": 2000, "rc": 3000}.get(label)
        if base is not None and separator and number.isdecimal():
            revision = base + int(number)
        else:
            revision = 1
    words = (major, minor, patch, revision)
    if any(value > 65535 for value in words):
        raise ValueError(f"Windows version components must be below 65536: {version!r}")
    return ",".join(str(value) for value in words)


def resource_string(value: str) -> str:
    if any(ord(character) < 32 for character in value):
        raise ValueError("resource strings must not contain control characters")
    return value.replace("\\", "\\\\").replace('"', '\\"')


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--description", required=True)
    parser.add_argument("--original-filename", required=True)
    parser.add_argument("--internal-name")
    parser.add_argument("--version", default=os.environ.get("CARGO_PKG_VERSION") or package_version())
    parser.add_argument("--icon", type=Path)
    args = parser.parse_args()

    try:
        version_commas = version_words(args.version)
        original = args.original_filename
        substitutions = {
            "@FILEVERSION_COMMAS@": version_commas,
            "@PRODUCTVERSION_COMMAS@": version_commas,
            "@COMPANY_NAME@": "Kohdalab",
            "@FILE_DESCRIPTION@": resource_string(args.description),
            "@FILE_VERSION@": resource_string(args.version),
            "@INTERNAL_NAME@": resource_string(args.internal_name or Path(original).stem),
            "@ORIGINAL_FILENAME@": resource_string(original),
            "@PRODUCT_NAME@": "KohdaLab IV",
            "@PRODUCT_VERSION@": resource_string(args.version),
        }
        resource = TEMPLATE.read_text(encoding="utf-8")
        for needle, replacement in substitutions.items():
            resource = resource.replace(needle, replacement)
        if "@" in resource:
            raise ValueError("unexpanded placeholder in Windows resource template")
        if args.icon:
            icon = args.icon.resolve()
            resource = f'#pragma code_page(65001)\n1 ICON "{resource_string(icon.as_posix())}"\n\n{resource}'
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(resource, encoding="utf-8", newline="\n")
    except (OSError, ValueError, KeyError) as error:
        print(f"windows version resource: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
