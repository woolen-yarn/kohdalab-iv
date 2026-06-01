from __future__ import annotations

from dataclasses import dataclass
from decimal import Decimal, InvalidOperation
from typing import Any


@dataclass(frozen=True)
class Quantity:
    value: Decimal
    unit: str
    si_value: Decimal
    si_unit: str

    @property
    def si_float(self) -> float:
        return float(self.si_value)


UNIT_FACTORS: dict[str, tuple[str, Decimal]] = {
    "nv": ("V", Decimal("1e-9")),
    "uv": ("V", Decimal("1e-6")),
    "microv": ("V", Decimal("1e-6")),
    "mv": ("V", Decimal("1e-3")),
    "v": ("V", Decimal("1")),
    "pa": ("A", Decimal("1e-12")),
    "na": ("A", Decimal("1e-9")),
    "ua": ("A", Decimal("1e-6")),
    "microa": ("A", Decimal("1e-6")),
    "ma": ("A", Decimal("1e-3")),
    "a": ("A", Decimal("1")),
    "ms": ("s", Decimal("1e-3")),
    "s": ("s", Decimal("1")),
}

DIMENSION_UNITS = {
    "voltage": {"V"},
    "current": {"A"},
    "time": {"s"},
}


def normalize_unit(unit: str) -> str:
    normalized = str(unit).strip().replace("\u00b5", "u").replace("\u03bc", "u")
    return normalized.lower()


def _decimal(value: Any) -> Decimal:
    try:
        return Decimal(str(value))
    except (InvalidOperation, ValueError) as e:
        raise ValueError(f"Invalid numeric value: {value!r}") from e


def parse_quantity(value: Any, *, dimension: str, default_unit: str | None = None) -> Quantity:
    """Parse a JSON quantity and return an SI-normalized value.

    Values should normally be objects like ``{"value": 1, "unit": "mV"}``.
    Numeric values are accepted only when a default unit is supplied by the
    caller for compatibility with compact timing fields.
    """

    if isinstance(value, dict):
        if "value" not in value or "unit" not in value:
            raise ValueError("Quantity objects must contain 'value' and 'unit'.")
        raw_value = value["value"]
        unit = str(value["unit"])
    elif isinstance(value, (int, float)) and not isinstance(value, bool) and default_unit is not None:
        raw_value = value
        unit = default_unit
    else:
        raise ValueError(f"Expected quantity object for {dimension}, got {value!r}.")

    unit_key = normalize_unit(unit)
    try:
        si_unit, factor = UNIT_FACTORS[unit_key]
    except KeyError as e:
        raise ValueError(f"Unsupported unit: {unit!r}") from e

    allowed = DIMENSION_UNITS.get(dimension)
    if allowed is not None and si_unit not in allowed:
        raise ValueError(f"Unit {unit!r} is not valid for {dimension}.")

    number = _decimal(raw_value)
    return Quantity(value=number, unit=unit, si_value=number * factor, si_unit=si_unit)


def quantity_float(value: Any, *, dimension: str, default_unit: str | None = None) -> float:
    return parse_quantity(value, dimension=dimension, default_unit=default_unit).si_float


def format_si(value: float | None, unit: str) -> str:
    if value is None:
        return "-"
    return f"{value:.6g} {unit}"
