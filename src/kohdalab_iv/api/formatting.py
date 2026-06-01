from __future__ import annotations

import math
from typing import Any


_RESISTANCE_SCALES = [
    (1e12, "TOhm"),
    (1e9, "GOhm"),
    (1e6, "MOhm"),
    (1e3, "kOhm"),
    (1.0, "Ohm"),
    (1e-3, "mOhm"),
    (1e-6, "uOhm"),
    (1e-9, "nOhm"),
    (1e-12, "pOhm"),
]

_CONDUCTANCE_SCALES = [
    (1e12, "TS"),
    (1e9, "GS"),
    (1e6, "MS"),
    (1e3, "kS"),
    (1.0, "S"),
    (1e-3, "mS"),
    (1e-6, "uS"),
    (1e-9, "nS"),
    (1e-12, "pS"),
]


def _trim_fixed(value: float, decimals: int) -> str:
    text = f"{value:.{decimals}f}".rstrip("0").rstrip(".")
    return "0" if text in {"", "-0"} else text


def _format_scaled(value: Any, scales: list[tuple[float, str]], *, zero_unit: str, decimals: int = 3) -> str:
    if value is None:
        return "-"
    try:
        scaled_value = float(value)
    except (TypeError, ValueError):
        return str(value)
    if not math.isfinite(scaled_value):
        return str(scaled_value)
    if scaled_value == 0:
        return f"0 {zero_unit}"

    abs_value = abs(scaled_value)
    for factor, unit in scales:
        if abs_value >= factor:
            return f"{_trim_fixed(scaled_value / factor, decimals)} {unit}"

    factor, unit = scales[-1]
    return f"{_trim_fixed(scaled_value / factor, decimals)} {unit}"


def format_resistance(value: Any, *, decimals: int = 3) -> str:
    return _format_scaled(value, _RESISTANCE_SCALES, zero_unit="Ohm", decimals=decimals)


def format_conductance(value: Any, *, decimals: int = 3) -> str:
    return _format_scaled(value, _CONDUCTANCE_SCALES, zero_unit="S", decimals=decimals)
