from __future__ import annotations

import importlib.metadata
import importlib
import json
import tomllib
from pathlib import Path

from kohdalab_iv import __version__
from kohdalab_iv.api import (
    CONFIG_SCHEMA_PATH,
    CONFIG_SCHEMA_VERSION,
    DEFAULT_CONFIG_PATH,
    load_config,
    load_config_schema,
)
from kohdalab_iv.api.config import DEFAULT_CONFIG


PROJECT_ROOT = Path(__file__).resolve().parents[1]


def test_runtime_version_matches_project_metadata() -> None:
    project = tomllib.loads(
        (PROJECT_ROOT / "pyproject.toml").read_text(encoding="utf-8")
    )["project"]

    assert project["version"] == __version__
    assert importlib.metadata.version("kohdalab-iv") == __version__


def test_runtime_version_falls_back_when_distribution_metadata_is_missing(
    monkeypatch,
) -> None:
    import kohdalab_iv

    real_version = importlib.metadata.version
    monkeypatch.setattr(
        importlib.metadata,
        "version",
        lambda _name: (_ for _ in ()).throw(importlib.metadata.PackageNotFoundError),
    )
    assert importlib.reload(kohdalab_iv).__version__ == "0.0.0"

    monkeypatch.setattr(importlib.metadata, "version", real_version)
    assert importlib.reload(kohdalab_iv).__version__ == __version__


def test_default_config_is_packaged_and_loadable() -> None:
    expected = PROJECT_ROOT / "src" / "kohdalab_iv" / "resources" / "default.json"

    assert DEFAULT_CONFIG_PATH == expected
    assert DEFAULT_CONFIG_PATH.is_file()
    config = load_config()
    assert config["profile"]["name"] == "default"
    assert config["config_version"] == CONFIG_SCHEMA_VERSION
    assert DEFAULT_CONFIG == json.loads(DEFAULT_CONFIG_PATH.read_text(encoding="utf-8"))


def test_config_schema_is_packaged_and_loadable() -> None:
    expected = PROJECT_ROOT / "src" / "kohdalab_iv" / "resources" / "config.schema.json"

    assert CONFIG_SCHEMA_PATH == expected
    assert CONFIG_SCHEMA_PATH.is_file()
    assert (
        load_config_schema()["properties"]["config_version"]["const"]
        == CONFIG_SCHEMA_VERSION
    )


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
    assert project["requires-python"] == ">=3.13"
    assert "Programming Language :: Python :: 3.13" in project["classifiers"]
    assert not any(
        dependency.startswith("tomli") for dependency in project["dependencies"]
    )
    assert project["urls"]["Repository"].endswith("Kohdalab/kohdalab-iv")
    json.loads(DEFAULT_CONFIG_PATH.read_text(encoding="utf-8"))
