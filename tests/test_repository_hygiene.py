from __future__ import annotations

from pathlib import Path

import yaml


def test_pre_commit_uses_shared_quality_checks() -> None:
    config = yaml.safe_load(Path(".pre-commit-config.yaml").read_text(encoding="utf-8"))
    hooks = {
        hook["id"]: hook
        for repository in config["repos"]
        for hook in repository["hooks"]
    }

    assert set(hooks) == {
        "ruff-check",
        "ruff-format",
        "mypy",
        "release-metadata",
        "project-quality",
    }
    assert hooks["project-quality"]["stages"] == ["pre-push"]
    assert "scripts/check_project.py quality" in hooks["project-quality"]["entry"]


def test_local_outputs_and_configs_are_ignored() -> None:
    ignored = Path(".gitignore").read_text(encoding="utf-8").splitlines()

    assert {".coverage", "htmlcov/", "build/", "dist/", "config/*.json"} <= set(ignored)
    assert Path(".editorconfig").read_text(encoding="utf-8").startswith("root = true")
