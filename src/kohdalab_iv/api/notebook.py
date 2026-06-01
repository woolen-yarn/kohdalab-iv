from __future__ import annotations

from typing import Any, Callable

from kohdalab_iv.api.formatting import format_resistance


def _format(value: Any, unit: str = "") -> str:
    if value is None:
        return "-"
    if isinstance(value, float):
        return f"{value:.6g}{(' ' + unit) if unit else ''}"
    return str(value)


def format_point(point: Any) -> str:
    row = point.row
    return (
        f"[{point.index}/{point.total_points}] "
        f"target={_format(row.get('target_value'), row.get('target_unit') or '')} "
        f"voltage={_format(row.get('voltage_V'), 'V')} "
        f"current={_format(row.get('current_A'), 'A')} "
        f"R={format_resistance(row.get('resistance_Ohm'))} "
        f"status={row.get('status', '-')}"
    )


def make_iv_live_update(
    *,
    x_key: str = "voltage_V",
    y_key: str = "current_A",
    xlabel: str | None = None,
    ylabel: str | None = None,
    title: str | None = None,
) -> Callable[[Any], None]:
    from IPython.display import display
    import matplotlib.pyplot as plt

    xs: list[float] = []
    ys: list[float] = []
    fig, ax = plt.subplots(figsize=(7, 4))
    (line,) = ax.plot([], [], marker="o", ms=3)
    ax.grid(True, alpha=0.3)
    ax.set_xlabel(xlabel or x_key)
    ax.set_ylabel(ylabel or y_key)
    if title:
        ax.set_title(title)
    display_handle = None

    def update(point: Any) -> None:
        nonlocal display_handle
        row = point.row
        if row.get(x_key) is None or row.get(y_key) is None:
            return
        xs.append(float(row[x_key]))
        ys.append(float(row[y_key]))
        line.set_data(xs, ys)
        ax.relim()
        ax.autoscale_view()
        if display_handle is None:
            display_handle = display(fig, display_id=True)
        else:
            display_handle.update(fig)

    return update
