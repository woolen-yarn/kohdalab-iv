from __future__ import annotations

import json
import os
from copy import deepcopy
from dataclasses import dataclass
from datetime import datetime
from pathlib import Path
from typing import Any

PROJECT_ROOT = Path(__file__).resolve().parents[3]
DEFAULT_CONFIG_PATH = PROJECT_ROOT / "config" / "default.json"
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


def write_last_config_path(config_path: str | Path, path: str | Path | None = None) -> Path:
    state_path = Path(path) if path is not None else last_config_state_path()
    state_path.parent.mkdir(parents=True, exist_ok=True)
    state_path.write_text(json.dumps({"path": str(Path(config_path))}, indent=2), encoding="utf-8")
    return state_path


def _record_candidate(candidates: list[dict[str, str]], source: str, path: Path) -> None:
    candidates.append({"source": source, "path": str(path), "exists": str(path.exists())})


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
            return ConfigPathResolution(path=last_path, source="last", candidates=candidates)

    default_from_env = os.environ.get(DEFAULT_CONFIG_PATH_ENV)
    default_path = Path(default_from_env) if default_from_env else Path(lab_default_path or DEFAULT_CONFIG_PATH)
    _record_candidate(candidates, "lab_default", default_path)
    if default_path.exists():
        return ConfigPathResolution(path=default_path, source="lab_default", candidates=candidates)

    return ConfigPathResolution(path=None, source="none", candidates=candidates)


DEFAULT_CONFIG: dict[str, Any] = {
    "profile": {
        "name": "default",
        "description": "Default KohdaLab DC VI setup: GS210 current source and 34401A voltage measurement.",
    },
    "instruments": {
        "source": {
            "gs210": {
                "model": "YOKOGAWA_GS210",
                "transport": "visa",
                "resource": "GPIB0::2::INSTR",
                "timeout_ms": 5000,
            },
            "yokogawa_7651": {
                "model": "YOKOGAWA_7651",
                "transport": "visa",
                "resource": "GPIB0::1::INSTR",
                "timeout_ms": 5000,
            },
        },
        "meter": {
            "dmm_34401a": {
                "model": "AGILENT_34401A",
                "transport": "visa",
                "resource": "GPIB0::26::INSTR",
                "timeout_ms": 10000,
                "auto_range": True,
            },
            "dmm_agilent_34411a": {
                "model": "AGILENT_34411A",
                "transport": "visa",
                "resource": "USB0::0x0000::0x0000::INSTR",
                "timeout_ms": 10000,
                "auto_range": True,
            },
            "dmm_34411a": {
                "model": "KEYSIGHT_34411A",
                "transport": "visa",
                "resource": "USB0::0x0000::0x0000::INSTR",
                "timeout_ms": 10000,
                "auto_range": True,
            },
            "dmm_34465a": {
                "model": "KEYSIGHT_34465A",
                "transport": "visa",
                "resource": "USB0::0x0000::0x0000::INSTR",
                "timeout_ms": 10000,
                "auto_range": True,
            },
            "dmm_7461a": {
                "model": "ADCMT_7461A",
                "transport": "visa",
                "resource": "GPIB0::27::INSTR",
                "timeout_ms": 10000,
                "auto_range": True,
                "command_language": "scpi",
            }
        },
    },
    "roles": {
        "iv": {"source": "source.gs210", "measure": "meter.dmm_34401a"},
        "vi": {"source": "source.gs210", "measure": "meter.dmm_34401a"},
    },
    "measurements": {
        "iv": {
            "mode": "dc_vi",
            "signal": {"kind": "dc", "ac_enabled": False},
            "scan": {
                "pattern": "linear",
                "start": {"value": -100.0, "unit": "mA"},
                "stop": {"value": 100.0, "unit": "mA"},
                "step": {"value": 10.0, "unit": "mA"},
                "repeat": 1,
                "custom_points": [],
            },
            "timing": {
                "pre_delay_s": 0.1,
                "start_settle_s": 0.5,
                "settle_s": 0.2,
                "post_zero_delay_s": 0.1,
                "ramp_step_wait_s": 0.02,
                "nplc": 1.0,
                "average_count": 1,
                "measure_timeout_s": 10.0,
                "timing_mode": "software",
            },
            "measure": {"auto_range": True},
            "safety": {
                "max_abs_source": {"value": 100.0, "unit": "mA"},
                "compliance": {"value": 1.0, "unit": "V"},
                "stop_on_compliance": True,
                "ramp_step": {"value": 10.0, "unit": "mA"},
                "on_finish": "ramp_to_zero_then_off",
                "on_stop": "ramp_to_zero_then_off",
                "on_error": "output_off",
                "output_off_on_finish": True,
            },
            "output": {
                "dir": "results",
                "filename": "iv_run",
                "auto_timestamp_suffix": True,
            },
        }
    },
}


def _deep_defaults(value: dict[str, Any], defaults: dict[str, Any]) -> dict[str, Any]:
    merged = deepcopy(value)
    for key, default_value in defaults.items():
        if key not in merged:
            merged[key] = deepcopy(default_value)
        elif isinstance(merged[key], dict) and isinstance(default_value, dict):
            merged[key] = _deep_defaults(merged[key], default_value)
    return merged


def normalize_config(config: dict[str, Any]) -> dict[str, Any]:
    normalized = _deep_defaults(config, DEFAULT_CONFIG)
    normalized.setdefault("measurements", {})
    if "iv" not in normalized["measurements"]:
        normalized["measurements"]["iv"] = deepcopy(DEFAULT_CONFIG["measurements"]["iv"])
    return normalized


def load_config(path: str | Path = DEFAULT_CONFIG_PATH) -> dict[str, Any]:
    with Path(path).open("r", encoding="utf-8") as f:
        return normalize_config(json.load(f))


def save_config(config: dict[str, Any], path: str | Path) -> Path:
    output = Path(path)
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(normalize_config(config), indent=2), encoding="utf-8")
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
    key = "vi" if measurement_settings(config, measurement_name).get("mode") == "dc_vi" else "iv"
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
