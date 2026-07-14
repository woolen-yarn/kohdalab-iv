from __future__ import annotations

import argparse
import datetime as dt
import re
import tomllib
from dataclasses import dataclass
from pathlib import Path

import yaml


STABLE_VERSION = re.compile(r"[0-9]+\.[0-9]+\.[0-9]+")
FILES = ("pyproject.toml", "CITATION.cff", "README.md", "ROADMAP.md", "CHANGELOG.md")


@dataclass(frozen=True)
class VersionBump:
    previous_version: str
    version: str
    release_date: dt.date
    changed_files: tuple[Path, ...]


def _version_tuple(version: str) -> tuple[int, int, int]:
    if STABLE_VERSION.fullmatch(version) is None:
        raise RuntimeError("Version must use MAJOR.MINOR.PATCH.")
    return tuple(int(part) for part in version.split("."))  # type: ignore[return-value]


def _replace_once(text: str, pattern: str, replacement: str, *, label: str) -> str:
    updated, count = re.subn(pattern, replacement, text, count=1, flags=re.MULTILINE)
    if count != 1:
        raise RuntimeError(f"Could not locate the current version in {label}.")
    return updated


def _release_changelog(changelog: str, version: str, release_date: dt.date) -> str:
    matches = list(re.finditer(r"^## Unreleased\s*$", changelog, re.MULTILINE))
    if len(matches) != 1:
        raise RuntimeError("CHANGELOG.md must contain exactly one Unreleased section.")
    if re.search(rf"^## \[{re.escape(version)}\] - ", changelog, re.MULTILINE):
        raise RuntimeError(f"CHANGELOG.md already contains release {version}.")

    start = matches[0].start()
    next_heading = re.search(r"^## ", changelog[matches[0].end() :], re.MULTILINE)
    end = (
        matches[0].end() + next_heading.start()
        if next_heading is not None
        else len(changelog)
    )
    body = changelog[matches[0].end() : end].strip()
    if re.search(r"^\s*[-*+]\s+", body, re.MULTILINE) is None:
        raise RuntimeError(
            "CHANGELOG.md Unreleased must contain at least one change entry."
        )

    release = (
        f"## Unreleased\n\n## [{version}] - {release_date.isoformat()}\n\n{body}\n\n"
    )
    return changelog[:start] + release + changelog[end:].lstrip()


def bump_version(
    root: Path,
    version: str,
    *,
    release_date: dt.date | None = None,
    dry_run: bool = False,
) -> VersionBump:
    root = root.resolve()
    new_parts = _version_tuple(version)
    release_date = release_date or dt.datetime.now(dt.timezone.utc).date()
    if release_date > dt.datetime.now(dt.timezone.utc).date():
        raise RuntimeError("Release date cannot be in the future.")

    contents = {name: (root / name).read_text(encoding="utf-8") for name in FILES}
    project = tomllib.loads(contents["pyproject.toml"])["project"]
    previous = project.get("version")
    if not isinstance(previous, str) or STABLE_VERSION.fullmatch(previous) is None:
        raise RuntimeError("Current project version must use MAJOR.MINOR.PATCH.")
    if new_parts <= _version_tuple(previous):
        raise RuntimeError(f"New version {version} must be greater than {previous}.")

    citation = yaml.safe_load(contents["CITATION.cff"])
    if not isinstance(citation, dict) or str(citation.get("version")) != previous:
        raise RuntimeError("CITATION.cff version does not match pyproject.toml.")
    documented = re.findall(
        r"current development version is `([^`]+)`",
        contents["README.md"],
        re.IGNORECASE,
    )
    if documented != [previous]:
        raise RuntimeError("README.md must document the current project version once.")
    if (
        len(
            re.findall(
                rf"^## v{re.escape(version)}(?:\s|$)",
                contents["ROADMAP.md"],
                re.MULTILINE,
            )
        )
        != 1
    ):
        raise RuntimeError(f"ROADMAP.md must contain one heading for v{version}.")

    project_pattern = (
        rf'(^\[project\]\s*$(?s:.*?)^version\s*=\s*"){re.escape(previous)}("\s*$)'
    )
    updated = dict(contents)
    updated["pyproject.toml"] = _replace_once(
        contents["pyproject.toml"],
        project_pattern,
        rf"\g<1>{version}\g<2>",
        label="pyproject.toml",
    )
    updated["CITATION.cff"] = _replace_once(
        contents["CITATION.cff"],
        r"^version:.*$",
        f'version: "{version}"',
        label="CITATION.cff",
    )
    updated["CITATION.cff"] = _replace_once(
        updated["CITATION.cff"],
        r"^date-released:.*$",
        f'date-released: "{release_date.isoformat()}"',
        label="CITATION.cff",
    )
    updated["README.md"] = _replace_once(
        contents["README.md"],
        rf"(current development version is `){re.escape(previous)}(`)",
        rf"\g<1>{version}\g<2>",
        label="README.md",
    )
    updated["CHANGELOG.md"] = _release_changelog(
        contents["CHANGELOG.md"], version, release_date
    )

    changed = tuple(root / name for name in FILES if updated[name] != contents[name])
    if not dry_run:
        for path in changed:
            path.write_text(updated[path.name], encoding="utf-8")
    return VersionBump(previous, version, release_date, changed)


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Synchronize project metadata and promote Unreleased changes."
    )
    parser.add_argument("version", help="New stable version in MAJOR.MINOR.PATCH form")
    parser.add_argument("--date", type=dt.date.fromisoformat, dest="release_date")
    parser.add_argument("--project-root", type=Path, default=Path.cwd())
    parser.add_argument("--dry-run", action="store_true")
    args = parser.parse_args()
    result = bump_version(
        args.project_root,
        args.version,
        release_date=args.release_date,
        dry_run=args.dry_run,
    )
    action = "Would bump" if args.dry_run else "Bumped"
    print(
        f"{action} kohdalab-iv {result.previous_version} -> {result.version} "
        f"({result.release_date.isoformat()}); {len(result.changed_files)} files."
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
