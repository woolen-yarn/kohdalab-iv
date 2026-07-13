from __future__ import annotations

import copy

import pytest

from kohdalab_iv.api.config import DEFAULT_CONFIG, validate_config


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
