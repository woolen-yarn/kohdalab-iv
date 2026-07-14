from __future__ import annotations

import datetime as dt
import runpy
from pathlib import Path

import pytest


ROOT = Path(__file__).resolve().parents[1]
BUMP_VERSION = runpy.run_path(str(ROOT / "scripts" / "bump_version.py"))["bump_version"]
VERIFY_RELEASE = runpy.run_path(str(ROOT / "scripts" / "verify_release.py"))[
    "verify_release"
]


def _write_project(root: Path, *, unreleased: str = "- New behavior") -> None:
    (root / "pyproject.toml").write_text(
        '[project]\nname = "example"\nversion = "1.2.3"\n', encoding="utf-8"
    )
    (root / "CITATION.cff").write_text(
        'version: "1.2.3"\ndate-released: "2025-01-01"\n', encoding="utf-8"
    )
    (root / "README.md").write_text(
        "The current development version is `1.2.3`.\n", encoding="utf-8"
    )
    (root / "ROADMAP.md").write_text(
        "# Roadmap\n\n## v1.2.3 - Current\n\n## v1.3.0 - Next\n",
        encoding="utf-8",
    )
    (root / "CHANGELOG.md").write_text(
        f"# Changelog\n\n## Unreleased\n\n{unreleased}\n\n"
        "## [1.2.3] - 2025-01-01\n\n- Previous release\n",
        encoding="utf-8",
    )


def test_dry_run_reports_changes_without_writing(tmp_path: Path) -> None:
    _write_project(tmp_path)
    before = {path.name: path.read_text() for path in tmp_path.iterdir()}

    result = BUMP_VERSION(
        tmp_path,
        "1.3.0",
        release_date=dt.date(2025, 2, 1),
        dry_run=True,
    )

    assert result.previous_version == "1.2.3"
    assert {path.name for path in result.changed_files} == {
        "pyproject.toml",
        "CITATION.cff",
        "README.md",
        "CHANGELOG.md",
    }
    assert {path.name: path.read_text() for path in tmp_path.iterdir()} == before


def test_bump_synchronizes_release_metadata(tmp_path: Path) -> None:
    _write_project(tmp_path, unreleased="### Added\n\n- New behavior")

    BUMP_VERSION(tmp_path, "1.3.0", release_date=dt.date(2025, 2, 1))

    assert 'version = "1.3.0"' in (tmp_path / "pyproject.toml").read_text()
    assert 'version: "1.3.0"' in (tmp_path / "CITATION.cff").read_text()
    assert "`1.3.0`" in (tmp_path / "README.md").read_text()
    changelog = (tmp_path / "CHANGELOG.md").read_text()
    assert "## Unreleased\n\n## [1.3.0] - 2025-02-01" in changelog
    assert "### Added\n\n- New behavior" in changelog
    metadata = VERIFY_RELEASE(tmp_path, tag="v1.3.0")
    assert metadata.version == "1.3.0"
    assert metadata.release_date == dt.date(2025, 2, 1)


@pytest.mark.parametrize("version", ["1.2.3", "1.2.2", "1.3", "v1.3.0"])
def test_rejects_non_increasing_or_unstable_version(
    tmp_path: Path, version: str
) -> None:
    _write_project(tmp_path)
    with pytest.raises(RuntimeError):
        BUMP_VERSION(tmp_path, version, release_date=dt.date(2025, 2, 1))


def test_rejects_missing_roadmap_or_changelog_entries(tmp_path: Path) -> None:
    _write_project(tmp_path, unreleased="No bullet entries")
    with pytest.raises(RuntimeError, match="Unreleased must contain"):
        BUMP_VERSION(tmp_path, "1.3.0", release_date=dt.date(2025, 2, 1))

    _write_project(tmp_path)
    (tmp_path / "ROADMAP.md").write_text("# Roadmap\n", encoding="utf-8")
    with pytest.raises(RuntimeError, match="heading for v1.3.0"):
        BUMP_VERSION(tmp_path, "1.3.0", release_date=dt.date(2025, 2, 1))


def test_rejects_future_release_date(tmp_path: Path) -> None:
    _write_project(tmp_path)
    tomorrow = dt.datetime.now(dt.timezone.utc).date() + dt.timedelta(days=1)
    with pytest.raises(RuntimeError, match="cannot be in the future"):
        BUMP_VERSION(tmp_path, "1.3.0", release_date=tomorrow)
