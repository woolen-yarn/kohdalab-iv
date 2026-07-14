from __future__ import annotations

import json
from pathlib import Path
from types import SimpleNamespace

import pytest

from kohdalab_iv.api.notebook import _format, format_point, make_iv_live_update


NOTEBOOK = Path("notebook/iv_notebook.ipynb")


def test_notebook_is_clean_and_compilable() -> None:
    notebook = json.loads(NOTEBOOK.read_text(encoding="utf-8"))

    assert notebook["nbformat"] == 4
    assert notebook["metadata"]["kernelspec"]["name"] == "python3"
    assert {cell.get("id") for cell in notebook["cells"]} == {
        "introduction",
        "setup",
        "resources",
        "measurement",
    }
    for cell in notebook["cells"]:
        if cell["cell_type"] != "code":
            continue
        source = "".join(cell["source"])
        compile(source, f"{NOTEBOOK}:{cell['id']}", "exec")
        assert cell["execution_count"] is None
        assert cell["outputs"] == []
        assert "sys.path" not in source
        assert "../config/default.json" not in source


def test_notebook_setup_uses_packaged_config_without_hardware() -> None:
    notebook = json.loads(NOTEBOOK.read_text(encoding="utf-8"))
    setup = next(cell for cell in notebook["cells"] if cell.get("id") == "setup")
    namespace: dict[str, object] = {}

    exec(compile("".join(setup["source"]), f"{NOTEBOOK}:setup", "exec"), namespace)

    experiment = namespace["experiment"]
    assert namespace["config_path"] is not None
    assert namespace["config"]["profile"]["name"] == "default"
    assert not any(experiment.connected_devices().values())


def test_notebook_value_and_point_formatting() -> None:
    assert _format(None, "V") == "-"
    assert _format(1.23456789, "V") == "1.23457 V"
    assert _format(2.0) == "2"
    assert _format("ready") == "ready"

    point = SimpleNamespace(
        index=2,
        total_points=5,
        row={
            "target_value": 0.001,
            "target_unit": "A",
            "voltage_V": 1.0,
            "current_A": 0.001,
            "resistance_Ohm": 1000.0,
            "status": "ok",
        },
    )

    assert format_point(point) == (
        "[2/5] target=0.001 A voltage=1 V current=0.001 A R=1 kOhm status=ok"
    )


def test_live_update_skips_missing_values_then_updates_display(monkeypatch) -> None:
    plt = pytest.importorskip("matplotlib.pyplot")
    displays = []

    class DisplayHandle:
        def __init__(self) -> None:
            self.updated = []

        def update(self, figure) -> None:
            self.updated.append(figure)

    handle = DisplayHandle()

    def fake_display(figure, *, display_id):
        displays.append((figure, display_id))
        return handle

    monkeypatch.setattr("IPython.display.display", fake_display)
    update = make_iv_live_update(
        xlabel="Voltage",
        ylabel="Current",
        title="Simulated I-V",
    )
    figure = plt.gcf()
    axis = figure.axes[0]

    update(SimpleNamespace(row={"voltage_V": 1.0, "current_A": None}))
    assert displays == []

    update(SimpleNamespace(row={"voltage_V": "1.0", "current_A": "0.001"}))
    update(SimpleNamespace(row={"voltage_V": 2.0, "current_A": 0.002}))

    assert displays == [(figure, True)]
    assert handle.updated == [figure]
    assert list(axis.lines[0].get_xdata()) == [1.0, 2.0]
    assert list(axis.lines[0].get_ydata()) == [0.001, 0.002]
    assert axis.get_xlabel() == "Voltage"
    assert axis.get_ylabel() == "Current"
    assert axis.get_title() == "Simulated I-V"
    plt.close(figure)

    make_iv_live_update()
    assert plt.gcf().axes[0].get_title() == ""
    plt.close(plt.gcf())
