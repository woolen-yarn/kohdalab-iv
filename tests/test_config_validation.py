from __future__ import annotations

import copy
from pathlib import Path

import pytest
from jsonschema import Draft202012Validator

from kohdalab_iv.api import config as config_module
from kohdalab_iv.api.config import (
    CONFIG_SCHEMA_VERSION,
    DEFAULT_CONFIG,
    load_config,
    load_config_schema,
    initialize_config,
    normalize_config,
    validate_config,
)


def test_schema_validates_packaged_default_and_simulated_profiles() -> None:
    schema = load_config_schema()
    Draft202012Validator.check_schema(schema)
    validator = Draft202012Validator(schema)

    validator.validate(load_config())
    validator.validate(
        load_config(config_module.PACKAGE_ROOT / "resources" / "simulated.json")
    )


def test_config_version_migrates_legacy_and_rejects_unsupported_values() -> None:
    legacy = copy.deepcopy(DEFAULT_CONFIG)
    legacy.pop("config_version")
    assert normalize_config(legacy)["config_version"] == CONFIG_SCHEMA_VERSION

    for version, message in (
        ("1", "must be an integer"),
        (True, "must be an integer"),
        (0, "must be >= 1"),
        (CONFIG_SCHEMA_VERSION + 1, "newer than supported"),
    ):
        config = copy.deepcopy(DEFAULT_CONFIG)
        config["config_version"] = version
        with pytest.raises(ValueError, match=message):
            validate_config(config)


def test_config_schema_rejects_non_object_packaged_data(monkeypatch, tmp_path) -> None:
    path = tmp_path / "schema.json"
    path.write_text("[]", encoding="utf-8")
    monkeypatch.setattr(config_module, "CONFIG_SCHEMA_PATH", path)

    with pytest.raises(RuntimeError, match="schema must be a JSON object"):
        load_config_schema()


def test_initialize_config_refuses_symlink_destination(monkeypatch, tmp_path) -> None:
    destination = tmp_path / "config.json"
    original = Path.is_symlink
    monkeypatch.setattr(
        Path,
        "is_symlink",
        lambda self: self == destination or original(self),
    )

    with pytest.raises(ValueError, match="through a symlink"):
        initialize_config(destination, overwrite=True)


def test_packaged_default_loader_rejects_non_object_data(monkeypatch, tmp_path) -> None:
    path = tmp_path / "default.json"
    path.write_text("[]", encoding="utf-8")
    monkeypatch.setattr(config_module, "DEFAULT_CONFIG_PATH", path)

    with pytest.raises(RuntimeError, match="default config must be a JSON object"):
        config_module._load_packaged_default()


def test_validation_rejects_missing_selected_instrument() -> None:
    config = copy.deepcopy(DEFAULT_CONFIG)
    config["measurements"]["iv"]["mode"] = "dc_iv"
    config["roles"]["iv"]["source"] = "source.missing"

    with pytest.raises(ValueError, match="Missing instrument ref"):
        validate_config(config)


def test_validation_rejects_instrument_without_resource() -> None:
    config = copy.deepcopy(DEFAULT_CONFIG)
    config["instruments"]["source"]["gs210"]["resource"] = ""

    with pytest.raises(ValueError, match="requires resource"):
        validate_config(config)


def test_validation_rejects_unsafe_cleanup_action() -> None:
    config = copy.deepcopy(DEFAULT_CONFIG)
    config["measurements"]["iv"]["safety"]["on_error"] = "leave_output_on"

    with pytest.raises(ValueError, match="safety.on_error"):
        validate_config(config)
