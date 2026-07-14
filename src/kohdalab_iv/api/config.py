from __future__ import annotations

import json
import os
from copy import deepcopy
from dataclasses import dataclass
from datetime import datetime
from pathlib import Path
from typing import Any

PACKAGE_ROOT = Path(__file__).resolve().parent.parent
DEFAULT_CONFIG_PATH = PACKAGE_ROOT / "resources" / "default.json"
CONFIG_SCHEMA_PATH = PACKAGE_ROOT / "resources" / "config.schema.json"
CONFIG_SCHEMA_VERSION = 1
CONFIG_PATH_ENV = "KOHDALAB_IV_CONFIG"
DEFAULT_CONFIG_PATH_ENV = "KOHDALAB_IV_DEFAULT_CONFIG"
CONFIG_STATE_DIR_ENV = "KOHDALAB_IV_STATE_DIR"
LAST_CONFIG_STATE_PATH_ENV = "KOHDALAB_IV_LAST_CONFIG_STATE_PATH"


@dataclass(frozen=True)
class ConfigPathResolution:
    path: Path | None
    source: str
    candidates: list[dict[str, str]]


def config_state_dir() -> Path:
    configured = os.environ.get(CONFIG_STATE_DIR_ENV)
    if configured:
        return Path(configured)
    return Path.home() / ".kohdalab-iv"


def last_config_state_path() -> Path:
    configured = os.environ.get(LAST_CONFIG_STATE_PATH_ENV)
    if configured:
        return Path(configured)
    return config_state_dir() / "last_config.json"


def read_last_config_path(path: str | Path | None = None) -> Path | None:
    state_path = Path(path) if path is not None else last_config_state_path()
    if not state_path.exists():
        return None
    try:
        data = json.loads(state_path.read_text(encoding="utf-8"))
        value = data.get("path") if isinstance(data, dict) else data
    except json.JSONDecodeError:
        value = state_path.read_text(encoding="utf-8").strip()
    if not value:
        return None
    return Path(str(value))


def write_last_config_path(
    config_path: str | Path, path: str | Path | None = None
) -> Path:
    state_path = Path(path) if path is not None else last_config_state_path()
    state_path.parent.mkdir(parents=True, exist_ok=True)
    state_path.write_text(
        json.dumps({"path": str(Path(config_path))}, indent=2), encoding="utf-8"
    )
    return state_path


def _record_candidate(
    candidates: list[dict[str, str]], source: str, path: Path
) -> None:
    candidates.append(
        {"source": source, "path": str(path), "exists": str(path.exists())}
    )


def resolve_config_path(
    explicit_path: str | Path | None = None,
    *,
    env_var: str = CONFIG_PATH_ENV,
    last_state_path: str | Path | None = None,
    lab_default_path: str | Path | None = None,
) -> ConfigPathResolution:
    candidates: list[dict[str, str]] = []
    if explicit_path:
        path = Path(explicit_path)
        _record_candidate(candidates, "explicit", path)
        return ConfigPathResolution(path=path, source="explicit", candidates=candidates)

    env_path = os.environ.get(env_var)
    if env_path:
        path = Path(env_path)
        _record_candidate(candidates, env_var, path)
        return ConfigPathResolution(path=path, source=env_var, candidates=candidates)

    last_path = read_last_config_path(last_state_path)
    if last_path is not None:
        _record_candidate(candidates, "last", last_path)
        if last_path.exists():
            return ConfigPathResolution(
                path=last_path, source="last", candidates=candidates
            )

    default_from_env = os.environ.get(DEFAULT_CONFIG_PATH_ENV)
    default_path = (
        Path(default_from_env)
        if default_from_env
        else Path(lab_default_path or DEFAULT_CONFIG_PATH)
    )
    _record_candidate(candidates, "lab_default", default_path)
    if default_path.exists():
        return ConfigPathResolution(
            path=default_path, source="lab_default", candidates=candidates
        )

    return ConfigPathResolution(path=None, source="none", candidates=candidates)


def _load_packaged_default() -> dict[str, Any]:
    with DEFAULT_CONFIG_PATH.open("r", encoding="utf-8") as f:
        config = json.load(f)
    if not isinstance(config, dict):
        raise RuntimeError("Packaged default config must be a JSON object.")
    return config


DEFAULT_CONFIG: dict[str, Any] = _load_packaged_default()


def _deep_defaults(value: dict[str, Any], defaults: dict[str, Any]) -> dict[str, Any]:
    merged = deepcopy(value)
    for key, default_value in defaults.items():
        if key not in merged:
            merged[key] = deepcopy(default_value)
        elif isinstance(merged[key], dict) and isinstance(default_value, dict):
            merged[key] = _deep_defaults(merged[key], default_value)
    return merged


def normalize_config(config: dict[str, Any]) -> dict[str, Any]:
    return _deep_defaults(config, DEFAULT_CONFIG)


def validate_config(config: dict[str, Any]) -> dict[str, Any]:
    if not isinstance(config, dict):
        raise ValueError("Config root must be a JSON object.")
    config_version = config.get("config_version")
    if type(config_version) is not int:
        raise ValueError("config_version must be an integer.")
    if config_version > CONFIG_SCHEMA_VERSION:
        raise ValueError(
            f"Config version {config_version} is newer than supported version "
            f"{CONFIG_SCHEMA_VERSION}."
        )
    if config_version < 1:
        raise ValueError("config_version must be >= 1.")
    measurements = config.get("measurements")
    if not isinstance(measurements, dict) or not measurements:
        raise ValueError("Config must define at least one measurement.")
    instruments = config.get("instruments")
    roles = config.get("roles")
    if not isinstance(instruments, dict) or not isinstance(roles, dict):
        raise ValueError("Config must define instruments and roles objects.")

    allowed_actions = {"ramp_to_zero_then_off", "output_off"}
    for name, settings in measurements.items():
        if not isinstance(settings, dict):
            raise ValueError(f"measurements.{name} must be an object.")
        mode = str(settings.get("mode", "dc_iv")).strip().lower()
        if mode not in {"dc_iv", "dc_vi"}:
            raise ValueError(f"measurements.{name}.mode must be 'dc_iv' or 'dc_vi'.")
        role_name = "vi" if mode == "dc_vi" else "iv"
        role = roles.get(role_name)
        if not isinstance(role, dict):
            raise ValueError(f"Missing roles.{role_name} object.")
        for field in ("source", "measure"):
            ref = role.get(field)
            if not isinstance(ref, str) or "." not in ref:
                raise ValueError(f"Missing roles.{role_name}.{field} instrument ref.")
            instrument = instrument_config(config, ref)
            for required in ("model", "resource"):
                if not str(instrument.get(required, "")).strip():
                    raise ValueError(f"Instrument {ref} requires {required}.")
        safety = settings.get("safety", {})
        for field in ("on_finish", "on_stop"):
            action = str(safety.get(field, "output_off"))
            if action not in allowed_actions:
                raise ValueError(
                    f"measurements.{name}.safety.{field} must be one of "
                    f"{sorted(allowed_actions)}."
                )
        if str(safety.get("on_error", "output_off")) != "output_off":
            raise ValueError(
                f"measurements.{name}.safety.on_error must be 'output_off'."
            )
    return config


def load_config_schema() -> dict[str, Any]:
    with CONFIG_SCHEMA_PATH.open("r", encoding="utf-8") as f:
        schema = json.load(f)
    if not isinstance(schema, dict):
        raise RuntimeError("Packaged config schema must be a JSON object.")
    return schema


def load_config(path: str | Path = DEFAULT_CONFIG_PATH) -> dict[str, Any]:
    with Path(path).open("r", encoding="utf-8") as f:
        loaded = json.load(f)
    if not isinstance(loaded, dict):
        raise ValueError("Config root must be a JSON object.")
    return validate_config(normalize_config(loaded))


def initialize_config(path: str | Path, *, overwrite: bool = False) -> Path:
    """Copy the validated packaged default to an editable local path."""
    load_config(DEFAULT_CONFIG_PATH)
    output = Path(path)
    if output.is_symlink():
        raise ValueError(f"Refusing to initialize config through a symlink: {output}")
    output.parent.mkdir(parents=True, exist_ok=True)
    mode = "w" if overwrite else "x"
    try:
        with output.open(mode, encoding="utf-8", newline="") as destination:
            destination.write(DEFAULT_CONFIG_PATH.read_text(encoding="utf-8"))
    except FileExistsError as error:
        raise FileExistsError(
            f"Config already exists: {output}. Use --force to replace it."
        ) from error
    return output


def save_config(config: dict[str, Any], path: str | Path) -> Path:
    output = Path(path)
    output.parent.mkdir(parents=True, exist_ok=True)
    normalized = validate_config(normalize_config(config))
    output.write_text(json.dumps(normalized, indent=2), encoding="utf-8")
    return output


def measurement_settings(config: dict[str, Any], name: str = "iv") -> dict[str, Any]:
    try:
        return config["measurements"][name]
    except KeyError as e:
        raise ValueError(f"Missing measurements.{name} in config.") from e


def instrument_config(config: dict[str, Any], ref: str) -> dict[str, Any]:
    kind, key = ref.split(".", 1)
    try:
        return config["instruments"][kind][key]
    except KeyError as e:
        raise ValueError(f"Missing instrument ref: {ref}") from e


def role_refs(config: dict[str, Any], measurement_name: str = "iv") -> tuple[str, str]:
    roles = config.get("roles", {})
    key = (
        "vi"
        if measurement_settings(config, measurement_name).get("mode") == "dc_vi"
        else "iv"
    )
    try:
        role = roles[key]
        return str(role["source"]), str(role["measure"])
    except KeyError as e:
        raise ValueError(f"Missing roles.{key}.source/measure in config.") from e


def with_csv_suffix(filename: str) -> str:
    path = Path(filename.strip() or "iv_run.csv")
    return str(path) if path.suffix.lower() == ".csv" else f"{path}.csv"


def with_auto_suffix(filename: str) -> str:
    path = Path(with_csv_suffix(filename))
    stamp = datetime.now().strftime("%Y%m%d_%H%M%S")
    return f"{path.stem}_{stamp}{path.suffix}"


def output_path(config: dict[str, Any], measurement_name: str = "iv") -> Path:
    settings = measurement_settings(config, measurement_name)
    output = settings.get("output", {})
    output_dir = Path(str(output.get("dir") or "results"))
    filename = str(output.get("filename") or "iv_run")
    if output.get("auto_timestamp_suffix", True):
        filename = with_auto_suffix(filename)
    else:
        filename = with_csv_suffix(filename)
    return output_dir / filename
