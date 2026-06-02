from __future__ import annotations

from dataclasses import dataclass
from decimal import Decimal
from typing import Any

from kohdalab_iv.api.config import instrument_config, measurement_settings, role_refs
from kohdalab_iv.api.specs import spec_for
from kohdalab_iv.api.units import Quantity, parse_quantity, quantity_float


@dataclass(frozen=True)
class SourceRange:
    name: str
    command_value: float
    max_abs: float
    resolution: float


@dataclass(frozen=True)
class SweepPoint:
    repeat_index: int
    sweep_index: int
    point_index: int
    total_points: int
    direction: str
    target: float
    target_unit: str


@dataclass(frozen=True)
class IvPlan:
    measurement_name: str
    mode: str
    source_ref: str
    measure_ref: str
    source_model: str
    measure_model: str
    source_function: str
    measure_function: str
    sweep_pattern: str
    points: list[SweepPoint]
    source_range: SourceRange
    safety_limit: float
    compliance: float
    hardware_compliance: float
    software_compliance: float
    stop_on_compliance: bool
    ramp_step: float
    ramp_step_wait_s: float
    pre_delay_s: float
    start_settle_s: float
    settle_s: float
    post_zero_delay_s: float
    nplc: float
    average_count: int
    measure_timeout_s: float
    on_finish: str
    on_stop: str
    on_error: str
    output_off_on_finish: bool
    summary: str

    @property
    def total_points(self) -> int:
        return len(self.points)

    @property
    def source_unit(self) -> str:
        return "V" if self.source_function == "voltage" else "A"


def _decimal_range(start: Decimal, stop: Decimal, step: Decimal) -> list[Decimal]:
    if step == 0:
        raise ValueError("Step must be non-zero.")
    if step > 0 and start > stop:
        raise ValueError("Positive step cannot reach stop from start.")
    if step < 0 and start < stop:
        raise ValueError("Negative step cannot reach stop from start.")

    points: list[Decimal] = []
    current = start
    tolerance = abs(step) / Decimal("1000000000")
    if step > 0:
        while current <= stop + tolerance:
            points.append(current)
            current += step
    else:
        while current >= stop - tolerance:
            points.append(current)
            current += step
    if not points:
        raise ValueError("No scan points generated.")
    return points


def _forward_points(start: Decimal, stop: Decimal, step: Decimal) -> list[Decimal]:
    magnitude = abs(step)
    signed = magnitude if stop >= start else -magnitude
    return _decimal_range(start, stop, signed)


def _without_duplicate_join(left: list[Decimal], right: list[Decimal]) -> list[Decimal]:
    if left and right and left[-1] == right[0]:
        return left + right[1:]
    return left + right


def _directions_from_points(points: list[Decimal]) -> list[str]:
    if not points:
        return []
    if len(points) == 1:
        return ["forward"]

    directions: list[str] = []
    last_direction = "forward"
    for index, point in enumerate(points):
        if index == 0:
            delta = points[1] - point
        else:
            delta = point - points[index - 1]
        if delta > 0:
            last_direction = "forward"
        elif delta < 0:
            last_direction = "backward"
        directions.append(last_direction)
    return directions


def _target_decimals(scan: dict[str, Any], source_dimension: str) -> tuple[list[Decimal], list[str], str]:
    pattern = str(scan.get("pattern", "linear")).strip().lower()
    repeat = int(scan.get("repeat", 1))
    if repeat < 1:
        raise ValueError("repeat must be >= 1.")

    if pattern == "custom_list":
        raw_points = scan.get("custom_points", [])
        if not raw_points:
            raise ValueError("custom_list requires custom_points.")
        points = [parse_quantity(item, dimension=source_dimension).si_value for item in raw_points]
    else:
        start = parse_quantity(scan["start"], dimension=source_dimension).si_value
        stop = parse_quantity(scan["stop"], dimension=source_dimension).si_value
        step = parse_quantity(scan["step"], dimension=source_dimension).si_value
        if step == 0:
            raise ValueError("step must be non-zero.")
        if pattern == "linear":
            points = _forward_points(start, stop, step)
        elif pattern == "round_trip":
            forward = _forward_points(start, stop, step)
            backward = _forward_points(stop, start, step)
            points = _without_duplicate_join(forward, backward)
        elif pattern == "zero_centered":
            max_abs = max(abs(start), abs(stop))
            step_abs = abs(step)
            p1 = _forward_points(Decimal("0"), max_abs, step_abs)
            p2 = _forward_points(max_abs, -max_abs, step_abs)
            p3 = _forward_points(-max_abs, Decimal("0"), step_abs)
            points = []
            for segment in (p1, p2, p3):
                start_index = 1 if points and segment and points[-1] == segment[0] else 0
                points.extend(segment[start_index:])
        else:
            raise ValueError(f"Unsupported sweep pattern: {pattern}")

    all_points: list[Decimal] = []
    for _ in range(repeat):
        all_points.extend(points)
    return all_points, _directions_from_points(all_points), pattern


def _range_entries(spec: dict[str, Any], source_function: str) -> list[dict[str, Any]]:
    return list(spec["voltage_ranges" if source_function == "voltage" else "current_ranges"])


def _select_source_range(spec: dict[str, Any], source_function: str, max_abs_target: float) -> SourceRange:
    key = "max_abs_V" if source_function == "voltage" else "max_abs_A"
    resolution_key = "resolution_V" if source_function == "voltage" else "resolution_A"
    for entry in sorted(_range_entries(spec, source_function), key=lambda item: float(item[key])):
        if max_abs_target <= float(entry[key]) + 1e-18:
            return SourceRange(
                name=str(entry["name"]),
                command_value=float(entry["command_value"]),
                max_abs=float(entry[key]),
                resolution=float(entry[resolution_key]),
            )
    raise ValueError(f"Source targets exceed {spec.get('display_name', 'source')} range.")


def _validate_resolution(values: list[Decimal], resolution: float, label: str) -> None:
    res = Decimal(str(resolution))
    for value in values:
        if value == 0:
            continue
        ratio = value / res
        if ratio != ratio.to_integral_value():
            raise ValueError(f"{label} {value} is smaller than or not aligned to source resolution {res}.")


def _hardware_compliance(spec: dict[str, Any], source_function: str, requested: float) -> float:
    if source_function == "voltage":
        min_value = float(spec.get("current_limit_min_A", requested))
        max_value = float(spec.get("current_limit_max_A", requested))
    else:
        min_value = float(spec.get("voltage_limit_min_V", requested))
        max_value = float(spec.get("voltage_limit_max_V", requested))
    return min(max(requested, min_value), max_value)


def _nplc_supported(meter_spec: dict[str, Any], nplc: float) -> bool:
    if any(abs(float(value) - nplc) < 1e-12 for value in meter_spec.get("nplc_values", [])):
        return True
    if "nplc_min" in meter_spec and "nplc_max" in meter_spec:
        return float(meter_spec["nplc_min"]) <= nplc <= float(meter_spec["nplc_max"])
    return False


def iv_plan_from_config(config: dict[str, Any], measurement_name: str = "iv") -> IvPlan:
    settings = measurement_settings(config, measurement_name)
    mode = str(settings.get("mode", "dc_iv")).strip().lower()
    if mode not in {"dc_iv", "dc_vi"}:
        raise ValueError("mode must be 'dc_iv' or 'dc_vi'.")
    source_function = "voltage" if mode == "dc_iv" else "current"
    measure_function = "dc_current" if mode == "dc_iv" else "dc_voltage"
    source_dimension = "voltage" if source_function == "voltage" else "current"
    compliance_dimension = "current" if source_function == "voltage" else "voltage"

    source_ref, measure_ref = role_refs(config, measurement_name)
    source_config = instrument_config(config, source_ref)
    measure_config = instrument_config(config, measure_ref)
    source_model = str(source_config["model"]).strip().upper()
    measure_model = str(measure_config["model"]).strip().upper()
    source_spec = spec_for("source", source_model)
    meter_spec = spec_for("meter", measure_model)

    scan = settings.get("scan", {})
    target_values, directions, pattern = _target_decimals(scan, source_dimension)
    max_abs_target = max(abs(float(value)) for value in target_values + [Decimal("0")])
    source_range = _select_source_range(source_spec, source_function, max_abs_target)
    _validate_resolution(target_values, source_range.resolution, "target")

    timing = settings.get("timing", {})
    nplc = float(timing.get("nplc", 1.0))
    if not _nplc_supported(meter_spec, nplc):
        raise ValueError(f"{measure_model} does not support nplc={nplc}.")
    average_count = int(timing.get("average_count", 1))
    if average_count < 1:
        raise ValueError("average_count must be >= 1.")

    safety = settings.get("safety", {})
    safety_limit = quantity_float(safety["max_abs_source"], dimension=source_dimension)
    hard_limit = float(source_spec["max_abs_V" if source_function == "voltage" else "max_abs_A"])
    effective_limit = min(safety_limit, hard_limit)
    if max_abs_target > effective_limit:
        raise ValueError(f"Source target {max_abs_target:g} exceeds effective limit {effective_limit:g}.")

    compliance = quantity_float(safety["compliance"], dimension=compliance_dimension)
    hardware_compliance = _hardware_compliance(source_spec, source_function, compliance)
    ramp_step_q = parse_quantity(safety["ramp_step"], dimension=source_dimension)
    ramp_step = abs(ramp_step_q.si_float)
    if ramp_step <= 0:
        raise ValueError("ramp_step must be positive.")
    _validate_resolution([ramp_step_q.si_value], source_range.resolution, "ramp_step")

    total = len(target_values)
    repeat_len = total // int(scan.get("repeat", 1))
    points = [
        SweepPoint(
            repeat_index=(index // repeat_len) + 1 if repeat_len else 1,
            sweep_index=1,
            point_index=index + 1,
            total_points=total,
            direction=directions[index],
            target=float(target),
            target_unit="V" if source_function == "voltage" else "A",
        )
        for index, target in enumerate(target_values)
    ]

    summary = (
        f"{mode} {pattern}: {total} points, {source_range.name} source range, "
        f"settle={float(timing.get('settle_s', 0.1)):.6g}s"
    )
    return IvPlan(
        measurement_name=measurement_name,
        mode=mode,
        source_ref=source_ref,
        measure_ref=measure_ref,
        source_model=source_model,
        measure_model=measure_model,
        source_function=source_function,
        measure_function=measure_function,
        sweep_pattern=pattern,
        points=points,
        source_range=source_range,
        safety_limit=effective_limit,
        compliance=compliance,
        hardware_compliance=hardware_compliance,
        software_compliance=compliance,
        stop_on_compliance=bool(safety.get("stop_on_compliance", True)),
        ramp_step=ramp_step,
        ramp_step_wait_s=float(timing.get("ramp_step_wait_s", 0.02)),
        pre_delay_s=float(timing.get("pre_delay_s", 0.0)),
        start_settle_s=float(timing.get("start_settle_s", 0.5)),
        settle_s=float(timing.get("settle_s", 0.1)),
        post_zero_delay_s=float(timing.get("post_zero_delay_s", 0.0)),
        nplc=nplc,
        average_count=average_count,
        measure_timeout_s=float(timing.get("measure_timeout_s", 10.0)),
        on_finish=str(safety.get("on_finish", "ramp_to_zero_then_off")),
        on_stop=str(safety.get("on_stop", "ramp_to_zero_then_off")),
        on_error=str(safety.get("on_error", "output_off")),
        output_off_on_finish=bool(safety.get("output_off_on_finish", True)),
        summary=summary,
    )
