from __future__ import annotations

import csv
import time
from datetime import datetime
from pathlib import Path
from typing import Any, Callable

from kohdalab_iv.api.config import output_path
from kohdalab_iv.api.measurement_rows import IV_FIELDS, iv_row, output_row
from kohdalab_iv.api.models import MeasurementPoint
from kohdalab_iv.api.scan_plan import IvPlan, iv_plan_from_config
from kohdalab_iv.api.session import DeviceSession
from kohdalab_iv.api.status import (
    STATUS_COMPLIANCE,
    STATUS_CONFIGURING,
    STATUS_OUTPUT_OFF,
    STATUS_OUTPUT_ON,
    STATUS_READING,
    STATUS_RETURNING_TO_ZERO,
    STATUS_RUNNING,
    STATUS_SETTING_SOURCE,
    STATUS_STOPPED,
    STATUS_WAITING,
)

PointCallback = Callable[[MeasurementPoint], None]
StatusCallback = Callable[[str], None]
ContinueCallback = Callable[[], bool]


def _emit(on_status: StatusCallback | None, status: str) -> None:
    if on_status is not None:
        on_status(status)


def _continue(should_continue: ContinueCallback | None) -> bool:
    return True if should_continue is None else bool(should_continue())


def _sleep_interruptible(duration_s: float, should_continue: ContinueCallback | None) -> bool:
    deadline = time.monotonic() + max(0.0, float(duration_s))
    while time.monotonic() < deadline:
        if not _continue(should_continue):
            return False
        time.sleep(min(0.05, deadline - time.monotonic()))
    return _continue(should_continue)


def _ramp_to(
    source,
    *,
    current: float,
    target: float,
    step: float,
    wait_s: float,
    should_continue: ContinueCallback | None,
) -> float:
    step = abs(float(step))
    if step <= 0:
        raise ValueError("ramp step must be positive.")
    if current == target:
        source.set_level(target)
        return target
    direction = 1.0 if target > current else -1.0
    value = current
    while True:
        if not _continue(should_continue):
            break
        remaining = abs(target - value)
        if remaining <= step:
            value = target
        else:
            value += direction * step
        source.set_level(value)
        if value == target:
            return value
        if not _sleep_interruptible(wait_s, should_continue):
            break
    return value


def _is_compliance(plan: IvPlan, measured_v: float | None, measured_a: float | None) -> bool:
    if plan.mode == "dc_iv":
        return measured_a is not None and abs(measured_a) >= abs(plan.software_compliance)
    return measured_v is not None and abs(measured_v) >= abs(plan.software_compliance)


def _measured_from_row(row: dict[str, Any]) -> tuple[float | None, float | None]:
    return row.get("measured_V"), row.get("measured_A")


def _configure_devices(plan: IvPlan, source, meter) -> None:
    source.configure_source(
        source_function=plan.source_function,
        source_range=plan.source_range.command_value,
        hardware_compliance=plan.hardware_compliance,
    )
    meter.configure_measurement(
        measure_function=plan.measure_function,
        nplc=plan.nplc,
        auto_range=True,
    )


def _prepare_meter_for_reading(meter) -> None:
    prepare = getattr(meter, "prepare_for_reading", None)
    if prepare is not None:
        prepare()


def run_iv(
    config: dict[str, Any],
    *,
    plan: IvPlan | None = None,
    output: str | Path | None = None,
    on_point: PointCallback | None = None,
    on_status: StatusCallback | None = None,
    should_continue: ContinueCallback | None = None,
    session: DeviceSession | None = None,
) -> list[dict[str, Any]]:
    plan = plan or iv_plan_from_config(config)
    owns_session = session is None
    session = session or DeviceSession(config)
    out = Path(output) if output is not None else output_path(config, plan.measurement_name)
    out.parent.mkdir(parents=True, exist_ok=True)
    rows: list[dict[str, Any]] = []
    source = None
    meter = None
    current_level = 0.0
    start = time.monotonic()
    normal_cleanup = True
    cleanup_action = plan.on_finish

    try:
        source = session.require(plan.source_ref)
        meter = session.require(plan.measure_ref)
        _emit(on_status, STATUS_CONFIGURING)
        _configure_devices(plan, source, meter)
        _emit(on_status, STATUS_OUTPUT_ON)
        source.output_on()
        if not _sleep_interruptible(plan.pre_delay_s, should_continue):
            cleanup_action = plan.on_stop
            normal_cleanup = True
            return rows

        if plan.points:
            current_level = _ramp_to(
                source,
                current=current_level,
                target=plan.points[0].target,
                step=plan.ramp_step,
                wait_s=plan.ramp_step_wait_s,
                should_continue=should_continue,
            )
            _emit(on_status, STATUS_WAITING)
            if not _sleep_interruptible(plan.start_settle_s, should_continue):
                cleanup_action = plan.on_stop
                normal_cleanup = True
                return rows

        _emit(on_status, STATUS_RUNNING)
        with out.open("w", newline="", encoding="utf-8") as f:
            writer = csv.DictWriter(f, fieldnames=IV_FIELDS)
            writer.writeheader()
            for index, point in enumerate(plan.points, start=1):
                if not _continue(should_continue):
                    cleanup_action = plan.on_stop
                    break
                _emit(on_status, STATUS_SETTING_SOURCE)
                source.set_level(point.target)
                current_level = point.target
                _emit(on_status, STATUS_WAITING)
                if not _sleep_interruptible(plan.settle_s, should_continue):
                    cleanup_action = plan.on_stop
                    break
                _emit(on_status, STATUS_READING)
                _prepare_meter_for_reading(meter)
                meter_value = meter.read_average(plan.average_count)
                source_readback = source.read_level()
                provisional = iv_row(
                    timestamp=datetime.now().isoformat(timespec="milliseconds"),
                    elapsed_s=time.monotonic() - start,
                    plan=plan,
                    point=point,
                    source_set=point.target,
                    source_readback=source_readback,
                    meter_value=meter_value,
                    compliance=False,
                    status=STATUS_RUNNING,
                )
                measured_v, measured_a = _measured_from_row(provisional)
                compliance = _is_compliance(plan, measured_v, measured_a)
                row = dict(provisional)
                row["compliance"] = compliance
                row["software_limit_hit"] = compliance
                row["status"] = STATUS_COMPLIANCE if compliance else STATUS_RUNNING
                writer.writerow(output_row(row))
                f.flush()
                rows.append(row)
                if on_point is not None:
                    on_point(MeasurementPoint(index=index, total_points=plan.total_points, row=row))
                if compliance and plan.stop_on_compliance:
                    cleanup_action = plan.on_stop
                    _emit(on_status, STATUS_COMPLIANCE)
                    break
    except Exception:
        normal_cleanup = False
        if plan.on_error == "output_off" and source is not None:
            try:
                _emit(on_status, STATUS_OUTPUT_OFF)
                source.output_off()
            except Exception:
                pass
        raise
    finally:
        if normal_cleanup and plan.output_off_on_finish and source is not None:
            try:
                if cleanup_action == "ramp_to_zero_then_off":
                    _emit(on_status, STATUS_RETURNING_TO_ZERO)
                    _ramp_to(
                        source,
                        current=current_level,
                        target=0.0,
                        step=plan.ramp_step,
                        wait_s=plan.ramp_step_wait_s,
                        should_continue=None,
                    )
                    _sleep_interruptible(plan.post_zero_delay_s, None)
                if cleanup_action in {"ramp_to_zero_then_off", "output_off"}:
                    _emit(on_status, STATUS_OUTPUT_OFF)
                    source.output_off()
            finally:
                _emit(on_status, STATUS_STOPPED)
        if owns_session:
            session.disconnect_all()
    return rows
