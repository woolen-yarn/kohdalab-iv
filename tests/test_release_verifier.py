from __future__ import annotations

import runpy
from pathlib import Path

import pytest


ROOT = Path(__file__).resolve().parents[1]
VERIFY_RELEASE = runpy.run_path(str(ROOT / "scripts" / "verify_release.py"))[
    "verify_release"
]


def _write_project(
    root: Path,
    *,
    version: str = "1.2.3",
    citation_version: str | None = None,
    citation_date: str = "2025-01-01",
    unreleased: str = "- Work in progress",
    release_body: str = "- Stable change",
) -> None:
    (root / "pyproject.toml").write_text(
        f'[project]\nname = "example"\nversion = "{version}"\n', encoding="utf-8"
    )
    (root / "CITATION.cff").write_text(
        f'version: "{citation_version or version}"\ndate-released: "{citation_date}"\n',
        encoding="utf-8",
    )
    (root / "README.md").write_text(
        f"The current development version is `{version}`.\n", encoding="utf-8"
    )
    (root / "ROADMAP.md").write_text(
        f"# Roadmap\n\n## v{version} - Current\n", encoding="utf-8"
    )
    (root / "CHANGELOG.md").write_text(
        f"# Changelog\n\n## Unreleased\n\n{unreleased}\n"
        f"\n## [{version}] - {citation_date}\n\n{release_body}\n",
        encoding="utf-8",
    )


def test_repository_development_metadata_is_consistent() -> None:
    assert VERIFY_RELEASE(ROOT).version == "0.2.0"


def test_rejects_citation_version_mismatch(tmp_path: Path) -> None:
    _write_project(tmp_path, citation_version="1.2.4")
    with pytest.raises(RuntimeError, match="CITATION.cff version"):
        VERIFY_RELEASE(tmp_path)


def test_tag_requires_exact_version_and_empty_unreleased(tmp_path: Path) -> None:
    _write_project(tmp_path)
    with pytest.raises(RuntimeError, match="Git tag must be exactly"):
        VERIFY_RELEASE(tmp_path, tag="1.2.3")
    with pytest.raises(RuntimeError, match="Unreleased must contain no change entries"):
        VERIFY_RELEASE(tmp_path, tag="v1.2.3")


def test_tag_accepts_matching_release_metadata(tmp_path: Path) -> None:
    _write_project(tmp_path, unreleased="")
    metadata = VERIFY_RELEASE(tmp_path, tag="v1.2.3")
    assert metadata.release_date is not None
    assert metadata.release_date.isoformat() == "2025-01-01"
