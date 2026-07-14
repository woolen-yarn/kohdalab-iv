# Public API Usage

KohdaLab IV exposes the same measurement core used by the CLI, GUI, and Notebook through `kohdalab_iv.api`.

## Load and Validate Configuration

```python
from kohdalab_iv.api import load_config, validate_config

config = load_config()  # packaged default
validate_config(config)
```

Pass a path to load a machine-specific configuration:

```python
config = load_config("C:/KohdaLab/config/iv.local.json")
```

`load_config` normalizes defaults and validates the document. Keep local VISA resource strings and safety limits outside the installed package.

All saved configs include `config_version`. Legacy documents without it are normalized to the current version. Use `load_config_schema()` when an editor or external tool needs the packaged JSON Schema:

```python
from kohdalab_iv.api import CONFIG_SCHEMA_VERSION, load_config_schema

schema = load_config_schema()
assert schema["properties"]["config_version"]["const"] == CONFIG_SCHEMA_VERSION
```

## Inspect the Plan Without Hardware

```python
from kohdalab_iv.api import iv_plan_from_config

plan = iv_plan_from_config(config)
print(plan.summary)
print(plan.source_ref, plan.source_range.name)
print(plan.measure_ref, plan.nplc)
print(plan.total_points)
```

Plan construction checks units, scan generation, source resolution and range, safety limits, compliance, and meter NPLC support before a device session is opened.

## Run a Simulated Measurement

```python
from pathlib import Path

from kohdalab_iv.api import Experiment, load_config

config = load_config("src/kohdalab_iv/resources/simulated.json")
experiment = Experiment(config)

rows = experiment.run_iv(output=Path("results/api_simulated.csv"))
print(f"collected {len(rows)} points")
```

The simulated profile uses a 1 kOhm circuit. It exercises the production scan, callback, CSV, compliance, and cleanup paths without VISA hardware.

## Progress and Stop Callbacks

```python
running = True


def on_status(status: str) -> None:
    print(status)


def on_point(point) -> None:
    row = point.row
    print(point.index, row["voltage_V"], row["current_A"])


def should_continue() -> bool:
    return running


rows = experiment.run_iv(
    on_status=on_status,
    on_point=on_point,
    should_continue=should_continue,
)
```

Set `running = False` from the controlling application to request a safe stop. Cleanup follows the configured `on_stop` policy.

## Explicit Connection Management

```python
experiment = Experiment(config, auto_connect=False)
experiment.connect_all()

try:
    print(experiment.connected_devices())
    rows = experiment.run_iv()
finally:
    experiment.disconnect_all()
```

The GUI uses an explicit session so devices can remain connected between runs. CLI automation normally uses `auto_connect=True` and still returns devices to a safe state during cleanup.

## Configuration Updates

```python
from copy import deepcopy

updated = deepcopy(experiment.config)
updated["measurements"]["iv"]["timing"]["average_count"] = 3
experiment.config = updated
```

Assigning `experiment.config` updates the active session configuration. Build a new plan after changing scan or safety settings.

## Notebook Live Update

```python
from kohdalab_iv.api.notebook import make_iv_live_update

live_plot = make_iv_live_update(
    x_key="voltage_V",
    y_key="current_A",
    xlabel="Voltage (V)",
    ylabel="Current (A)",
    title="I-V",
)

rows = experiment.run_iv(on_point=live_plot)
```

See [`notebook/iv_notebook.ipynb`](../notebook/iv_notebook.ipynb) for a clean, unexecuted example notebook.
