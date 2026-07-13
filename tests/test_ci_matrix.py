from __future__ import annotations

from pathlib import Path

import yaml


def test_ci_covers_supported_python_baseline_and_current_runtime() -> None:
    workflow = yaml.safe_load(
        Path(".github/workflows/test.yml").read_text(encoding="utf-8")
    )
    matrix = workflow["jobs"]["test"]["strategy"]["matrix"]

    entries = matrix["include"]
    assert {(entry["os"], entry["python-version"]) for entry in entries} == {
        ("ubuntu-latest", "3.10"),
        ("ubuntu-latest", "3.13"),
        ("windows-latest", "3.10"),
        ("windows-latest", "3.13"),
    }
    required_names = {
        entry["check-name"]
        for entry in entries
        if entry["python-version"] == "3.13"
    }
    assert required_names == {"ubuntu-latest", "windows-latest"}
