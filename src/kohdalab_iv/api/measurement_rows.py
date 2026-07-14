from __future__ import annotations

from decimal import Decimal, InvalidOperation
from typing import Any

from kohdalab_iv.api.scan_plan import IvPlan, SweepPoint


IV_FIELDS = [
    "timestamp",
    "measurement",
    "elapsed_s",
    "mode",
    "sweep_pattern",
    "repeat_index",
    "sweep_index",
    "point_index",
    "total_points",
    "direction",
    "source_function",
    "measure_function",
    "target_value",
    "target_unit",
    "source_target_value",
    "source_target_unit",
    "target_V",
    "target_A",
    "source_set_V",
    "source_set_A",
    "source_readback_value",
    "source_readback_unit",
    "source_readback_V",
    "source_readback_A",
    "meter_value",
    "meter_unit",
    "voltage_V",
    "current_A",
    "measured_V",
    "measured_A",
    "voltage_origin",
    "current_origin",
    "resistance_Ohm",
    "conductance_S",
    "compliance",
    "software_limit_hit",
    "status",
    "instrument_status",
    "source_ref",
    "measure_ref",
    "source_model",
    "measure_model",
    "start_settle_s",
    "settle_s",
    "nplc",
    "average_count",
]


def _safe_div(numerator: float | None, denominator: float | None) -> float | None:
    if numerator is None or denominator is None or denominator == 0:
        return None
    try:
        return float(Decimal(str(numerator)) / Decimal(str(denominator)))
    except (InvalidOperation, ValueError, ZeroDivisionError):
        return numerator / denominator


def _format_value(value: Any) -> Any:
    if isinstance(value, float):
        return f"{value:.12e}"
    return value


def output_row(row: dict[str, Any]) -> dict[str, Any]:
    return {key: _format_value(row.get(key)) for key in IV_FIELDS}


def iv_row(
    *,
    timestamp: str,
    elapsed_s: float,
    plan: IvPlan,
    point: SweepPoint,
    source_set: float,
    source_readback: float | None,
    meter_value: float,
    compliance: bool,
    status: str,
    instrument_status: str | None = None,
) -> dict[str, Any]:
    if plan.mode == "dc_iv":
        source_set_v = source_set
        source_set_a = None
        source_readback_v = source_readback
        source_readback_a = None
        meter_unit = "A"
        measured_v = source_readback if source_readback is not None else source_set
        measured_a = meter_value
        voltage_origin = (
            "source_readback" if source_readback is not None else "source_setpoint"
        )
        current_origin = "meter"
        target_v = point.target
        target_a = None
    else:
        source_set_v = None
        source_set_a = source_set
        source_readback_v = None
        source_readback_a = source_readback
        meter_unit = "V"
        measured_v = meter_value
        measured_a = source_readback if source_readback is not None else source_set
        voltage_origin = "meter"
        current_origin = (
            "source_readback" if source_readback is not None else "source_setpoint"
        )
        target_v = None
        target_a = point.target

    resistance = _safe_div(measured_v, measured_a)
    conductance = _safe_div(measured_a, measured_v)
    row = {
        "timestamp": timestamp,
        "measurement": plan.measurement_name,
        "elapsed_s": elapsed_s,
        "mode": plan.mode,
        "sweep_pattern": plan.sweep_pattern,
        "repeat_index": point.repeat_index,
        "sweep_index": point.sweep_index,
        "point_index": point.point_index,
        "total_points": point.total_points,
        "direction": point.direction,
        "source_function": plan.source_function,
        "measure_function": plan.measure_function,
        "target_value": point.target,
        "target_unit": point.target_unit,
        "source_target_value": point.target,
        "source_target_unit": point.target_unit,
        "target_V": target_v,
        "target_A": target_a,
        "source_set_V": source_set_v,
        "source_set_A": source_set_a,
        "source_readback_value": source_readback,
        "source_readback_unit": point.target_unit,
        "source_readback_V": source_readback_v,
        "source_readback_A": source_readback_a,
        "meter_value": meter_value,
        "meter_unit": meter_unit,
        "voltage_V": measured_v,
        "current_A": measured_a,
        "measured_V": measured_v,
        "measured_A": measured_a,
        "voltage_origin": voltage_origin,
        "current_origin": current_origin,
        "resistance_Ohm": resistance,
        "conductance_S": conductance,
        "compliance": compliance,
        "software_limit_hit": compliance,
        "status": status,
        "instrument_status": instrument_status,
        "source_ref": plan.source_ref,
        "measure_ref": plan.measure_ref,
        "source_model": plan.source_model,
        "measure_model": plan.measure_model,
        "start_settle_s": plan.start_settle_s,
        "settle_s": plan.settle_s,
        "nplc": plan.nplc,
        "average_count": plan.average_count,
    }
    return {key: row.get(key) for key in IV_FIELDS}
