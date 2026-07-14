from __future__ import annotations

import runpy
import sys
from pathlib import Path

import pytest


ROOT = Path(__file__).resolve().parents[1]
CHECKER = runpy.run_path(str(ROOT / "scripts" / "check_project.py"))


def test_quality_checks_run_in_reproducible_order(tmp_path: Path) -> None:
    calls: list[tuple[tuple[str, ...], dict[str, str] | None]] = []

    def capture(
        command: tuple[str, ...], root: Path, env: dict[str, str] | None
    ) -> None:
        assert root == tmp_path
        calls.append((tuple(command), env))

    CHECKER["run_quality_checks"](tmp_path, runner=capture)

    assert [command for command, _ in calls] == [
        ("uv", "lock", "--check"),
        ("uv", "run", "ruff", "check", "."),
        ("uv", "run", "ruff", "format", "--check", "."),
        ("uv", "run", "mypy"),
        ("uv", "run", "pytest", "--cov", "--cov-branch", "-q"),
        (sys.executable, "scripts/verify_release.py"),
    ]
    assert calls[4][1]["QT_QPA_PLATFORM"] == "offscreen"


def test_package_checks_validate_built_artifacts(tmp_path: Path) -> None:
    distribution = tmp_path / "dist"
    calls: list[tuple[str, ...]] = []

    def capture(
        command: tuple[str, ...], root: Path, env: dict[str, str] | None
    ) -> None:
        del root, env
        calls.append(tuple(command))
        if tuple(command[:2]) == ("uv", "build"):
            (distribution / "example-1.0.0-py3-none-any.whl").touch()
            (distribution / "example-1.0.0.tar.gz").touch()

    CHECKER["run_package_checks"](
        tmp_path,
        distribution,
        tag="v1.0.0",
        runner=capture,
    )

    assert calls[0] == (
        sys.executable,
        "scripts/verify_release.py",
        "--tag",
        "v1.0.0",
    )
    assert calls[1][:4] == ("uv", "build", "--no-sources", "--out-dir")
    assert calls[2][:2] == (sys.executable, "scripts/verify_distributions.py")
    assert calls[3][1:4] == ("-m", "twine", "check")
    assert calls[4][0] == "check-wheel-contents"


def test_package_checks_reject_dirty_or_incomplete_directory(tmp_path: Path) -> None:
    distribution = tmp_path / "dist"
    distribution.mkdir()
    (distribution / "old.whl").touch()
    with pytest.raises(RuntimeError, match="must be empty"):
        CHECKER["run_package_checks"](tmp_path, distribution)

    (distribution / "old.whl").unlink()
    with pytest.raises(RuntimeError, match="exactly one wheel"):
        CHECKER["run_package_checks"](
            tmp_path,
            distribution,
            runner=lambda command, root, env: None,
        )


def test_audit_checks_export_locked_dependencies_before_scanning(
    tmp_path: Path,
) -> None:
    calls: list[tuple[str, ...]] = []

    def capture(
        command: tuple[str, ...], root: Path, env: dict[str, str] | None
    ) -> None:
        assert root == tmp_path
        assert env is None
        calls.append(tuple(command))
        if tuple(command[:2]) == ("uv", "export"):
            Path(command[-1]).touch()

    CHECKER["run_audit_checks"](tmp_path, runner=capture)

    assert calls[0] == ("uv", "lock", "--check")
    assert calls[1][:4] == ("uv", "export", "--format", "pylock.toml")
    assert "--all-extras" in calls[1]
    assert "--all-groups" in calls[1]
    assert calls[2][0] == "pip-audit"
    assert "--locked" in calls[2]
    assert "--strict" in calls[2]
