from __future__ import annotations

from pathlib import Path

try:
    import tomllib
except ModuleNotFoundError:
    import tomli as tomllib

from kohdalab_iv.api import cli
from kohdalab_iv.apps import iv_gui


def test_entrypoints_target_callable_main_functions() -> None:
    project = tomllib.loads(Path("pyproject.toml").read_text(encoding="utf-8"))[
        "project"
    ]

    assert project["scripts"] == {
        "kohdalab-iv": "kohdalab_iv.api.cli:main",
        "kohdalab-iv-gui": "kohdalab_iv.apps.iv_gui:main",
    }
    assert callable(cli.main)
    assert callable(iv_gui.main)
