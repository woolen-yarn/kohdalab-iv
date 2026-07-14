from __future__ import annotations

from pathlib import Path
from typing import Any

import yaml


def _workflow(name: str) -> dict[str, Any]:
    workflow = yaml.safe_load(
        Path(".github/workflows", name).read_text(encoding="utf-8")
    )
    assert isinstance(workflow, dict)
    return workflow


def _triggers(workflow: dict[str, Any]) -> dict[str, Any]:
    # PyYAML follows YAML 1.1 and parses the unquoted key `on` as True.
    triggers = workflow.get("on", workflow.get(True))
    assert isinstance(triggers, dict)
    return triggers


def test_ci_covers_supported_operating_systems_on_python_313() -> None:
    workflow = _workflow("test.yml")
    matrix = workflow["jobs"]["test"]["strategy"]["matrix"]

    entries = matrix["include"]
    assert {(entry["os"], entry["python-version"]) for entry in entries} == {
        ("ubuntu-latest", "3.13"),
        ("windows-latest", "3.13"),
    }
    required_names = {
        entry["check-name"] for entry in entries if entry["python-version"] == "3.13"
    }
    assert required_names == {"ubuntu-latest", "windows-latest"}


def test_release_workflow_is_tag_only_and_guarded() -> None:
    workflow = _workflow("release.yml")
    triggers = _triggers(workflow)
    assert triggers["push"] == {"tags": ["v*"]}
    assert "branches" not in triggers["push"]

    quality = workflow["jobs"]["quality"]
    package = workflow["jobs"]["package"]
    assert quality["if"] == "startsWith(github.ref, 'refs/tags/v')"
    assert package["needs"] == "quality"
    assert package["permissions"] == {"contents": "write"}

    quality_commands = "\n".join(step.get("run", "") for step in quality["steps"])
    assert "scripts/check_project.py quality" in quality_commands

    package_commands = "\n".join(step.get("run", "") for step in package["steps"])
    assert (
        'scripts/check_project.py package --tag "${{ github.ref_name }}" '
        "--dist-dir dist"
    ) in package_commands
    assert "gh release create" in package_commands
    assert "--draft" in package_commands
    assert "gh release upload" in package_commands
    assert "--clobber" in package_commands

    actions = {step.get("uses") for step in package["steps"]}
    assert "actions/upload-artifact@v4" in actions
    upload = next(
        step
        for step in package["steps"]
        if step.get("uses") == "actions/upload-artifact@v4"
    )
    assert upload["with"]["if-no-files-found"] == "error"
    assert upload["with"]["retention-days"] == 30
    assert "pypi" not in str(workflow).lower()


def test_regular_ci_does_not_handle_release_tags() -> None:
    workflow = _workflow("test.yml")
    triggers = _triggers(workflow)
    assert triggers["push"] == {"branches": ["main"]}
    assert "verify_release.py --tag" not in str(workflow)


def test_security_workflow_audits_lock_changes_and_runs_weekly() -> None:
    workflow = _workflow("security.yml")
    triggers = _triggers(workflow)
    assert {"pyproject.toml", "uv.lock"} <= set(triggers["push"]["paths"])
    assert triggers["schedule"] == [{"cron": "17 3 * * 1"}]
    assert workflow["permissions"] == {"contents": "read"}

    commands = "\n".join(
        step.get("run", "") for step in workflow["jobs"]["audit"]["steps"]
    )
    assert "uv sync --only-group audit --frozen" in commands
    assert "scripts/check_project.py audit" in commands
