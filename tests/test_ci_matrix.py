from __future__ import annotations

from pathlib import Path

import yaml


def test_ci_covers_supported_python_baseline_and_current_runtime() -> None:
    workflow = yaml.safe_load(
        Path(".github/workflows/test.yml").read_text(encoding="utf-8")
    )
    matrix = workflow["jobs"]["test"]["strategy"]["matrix"]

    assert matrix["os"] == ["ubuntu-latest", "windows-latest"]
    assert matrix["python-version"] == ["3.10", "3.13"]
