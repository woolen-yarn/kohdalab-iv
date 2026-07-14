from __future__ import annotations

import tomllib
from functools import lru_cache
from importlib.resources import files
from typing import Any


def _load_toml_resource(kind: str) -> dict[str, Any]:
    path = (
        files("kohdalab_iv")
        .joinpath("instruments")
        .joinpath(kind)
        .joinpath("specs.toml")
    )
    with path.open("rb") as f:
        return tomllib.load(f)


@lru_cache(maxsize=1)
def source_specs() -> dict[str, Any]:
    return _load_toml_resource("sources")


@lru_cache(maxsize=1)
def meter_specs() -> dict[str, Any]:
    return _load_toml_resource("meters")


MODEL_ALIASES: dict[str, dict[str, str]] = {
    "source": {},
    "meter": {},
}


def spec_for(kind: str, model: str) -> dict[str, Any]:
    normalized = model.strip().upper()
    normalized = MODEL_ALIASES.get(kind, {}).get(normalized, normalized)
    specs = source_specs() if kind == "source" else meter_specs()
    try:
        return specs[normalized]
    except KeyError as e:
        raise ValueError(f"Unsupported {kind} model: {model}") from e
