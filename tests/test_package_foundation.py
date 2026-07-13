from __future__ import annotations

import importlib.metadata
import json
from pathlib import Path

try:
    import tomllib
except ModuleNotFoundError:
    import tomli as tomllib

from kohdalab_iv import __version__
from kohdalab_iv.api.config import DEFAULT_CONFIG_PATH, load_config


PROJECT_ROOT = Path(__file__).resolve().parents[1]


def test_runtime_version_matches_project_metadata() -> None:
    project = tomllib.loads(
        (PROJECT_ROOT / "pyproject.toml").read_text(encoding="utf-8")
    )["project"]

    assert project["version"] == __version__
    assert importlib.metadata.version("kohdalab-iv") == __version__


def test_default_config_is_packaged_and_loadable() -> None:
    expected = PROJECT_ROOT / "src" / "kohdalab_iv" / "resources" / "default.json"

    assert DEFAULT_CONFIG_PATH == expected
    assert DEFAULT_CONFIG_PATH.is_file()
    assert load_config()["profile"]["name"] == "default"


def test_simulated_config_is_packaged_and_loadable() -> None:
    path = PROJECT_ROOT / "src" / "kohdalab_iv" / "resources" / "simulated.json"

    assert path.is_file()
    assert load_config(path)["profile"]["name"] == "simulated"


def test_repository_config_is_not_a_second_source_of_truth() -> None:
    assert not (PROJECT_ROOT / "config" / "default.json").exists()


def test_project_metadata_is_consistent() -> None:
    project = tomllib.loads(
        (PROJECT_ROOT / "pyproject.toml").read_text(encoding="utf-8")
    )["project"]
    citation_text = (PROJECT_ROOT / "CITATION.cff").read_text(encoding="utf-8")

    assert project["version"] in citation_text
    assert project["name"] == "kohdalab-iv"
    assert project["urls"]["Repository"].endswith("Kohdalab/kohdalab-iv")
    json.loads(DEFAULT_CONFIG_PATH.read_text(encoding="utf-8"))
