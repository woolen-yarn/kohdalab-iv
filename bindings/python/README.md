# Python / Notebook control

The application and instrument engine are Rust. This optional client uses Python
3.10+ and its standard library to control the installed or portable `iv-cli`
executable over JSON lines. It does not need PyVISA, NumPy, or the former Python
application.

## Install the client

Install the Windows or macOS KohdaLab IV application first. This package is only
a client: it never downloads, builds, or replaces the Rust measurement engine.

For a checked-out repository:

```text
python -m pip install ./bindings/python
uv pip install ./bindings/python
```

Install directly from the matching GitHub release:

```text
python -m pip install "kohdalab-iv-native @ git+https://github.com/woolen-yarn/kohdalab-iv.git@preview-v1.3.0-alpha.6#subdirectory=bindings/python"
uv add "kohdalab-iv-native @ git+https://github.com/woolen-yarn/kohdalab-iv.git@preview-v1.3.0-alpha.6#subdirectory=bindings/python"
```

The discovery enhancements below are included in v1.3.0-alpha.6 and newer.
Install the matching client from the release tag or its bundled `python-control`
folder. Release tags are frozen snapshots.

The client resolves `iv-cli` in this order:

1. The explicit `NativeIV(executable=...)` argument.
2. `KOHDALAB_IV_EXE`.
3. An `iv-cli` or `iv-cli.exe` already on `PATH`.
4. On Windows, the registered installer location (both 32-bit and 64-bit
   registry views), then the native Program Files installation.
5. On macOS, `~/Applications/KohdaLab IV.app`, then
   `/Applications/KohdaLab IV.app`.

Discovery only checks known paths. It does not scan hardware or recursively
search Downloads. Explicit arguments and `KOHDALAB_IV_EXE` take priority;
invalid overrides raise an error instead of silently selecting another app.
Use `resolve_executable()` to inspect the selected path without starting it.
For a portable installation elsewhere, pass the CLI path, its containing folder,
or the macOS `.app` folder. Keep the complete portable package together.
Close the GUI's hardware connections before using the same devices in a Notebook.

```python
from pathlib import Path
from kohdalab_iv_native import NativeIV

with NativeIV() as iv:  # Finds an installed Windows or macOS application automatically.
    iv.connect(simulation=True)
    print(iv.live())  # no output enabled, no CSV created
    result = iv.run(Path("example.csv"), {
        "start": -0.001, "stop": 0.001, "step": 0.001,
        "pattern": "linear",
    })
    print(result["points"], result["output"])
```

Use a new output filename for every run. Rust refuses to overwrite a CSV.
Configuration defaults are queried from Rust, rather than maintained separately
in Python. `snapshot` returns the latest state; `live()` waits for a new sample.
`stop()`, `output_off()` and `disconnect()` send the respective engine commands.
`run()` waits for completion and sends stop/OFF on timeout or interruption.
The context manager requests shutdown and waits for cleanup; it never kills a
hardware engine whose output cleanup is still pending.

For hardware, explicitly use `iv.connect("GPIB0::5::INSTR", "GPIB0::9::INSTR")`,
or the direct USB resource strings returned by `iv-cli --list`. Choose the actual
addresses and verify the permitted voltage/current limits before calling `run`.
A source-level reading is the programmed source value, not an independent output
measurement. Continuous acquisition configures the meter but does not enable
source output. During a sweep, the Rust engine serializes monitoring and sweep I/O.

Open [the simulation tutorial](examples/native-control.ipynb) in Jupyter.
No hardware commands are executed by that example. Direct CLI alternatives:

```text
iv-cli --default-config
iv-cli --list
iv-cli --measure --simulate --output example.csv
```

After connecting, `iv.set_mode("dc_vi")` selects source current / measure voltage;
`iv.set_mode("dc_iv")` selects source voltage / measure current. Rust verifies
output OFF and zero level before switching. Use these calls while idle.

## Development model selection

The 1.3 development engine accepts explicit models (both the client and engine must be the matching development version):

```python
iv.connect("GPIB0::3::INSTR", "GPIB0::7::INSTR",
           source_model="YOKOGAWA_7651", meter_model="ADCMT_7461A")
```

The client carries those models into each run. The engine rejects a run whose model settings disagree with the connected devices. 7461A GPIB uses SCPI; Windows official VISA USB uses ADC commands and stream reads. Native ADCMT USB and a separate GS200 driver are not implemented. Hardware validation of the newly ported models is pending.

## Move an existing Notebook

See [the migration tutorial](examples/migrate-to-native.ipynb). Keep analysis and
plotting cells; replace legacy Python instrument objects / PyVISA calls with one
`NativeIV` context. This is a new API, not a drop-in replacement. Rust owns
communication, limits, CSV persistence and output cleanup. `iv.live()` returns a
sample dictionary; `iv.run()` returns run status, while the saved CSV holds the
measurement rows. Use `start`, `stop` (the former End field), and `step` in SI
units, `settle_s` for wait time, `nplc` for integration time and `repeat` for
meter readings averaged at each unchanged source level. Query
`iv.default_config` for the complete current schema. Use the context manager
inside a single cell or function so interruptions pass through cleanup.

The package installation must target your Notebook kernel's Python environment.
In Jupyter, `%pip install /path/to/kohdalab-iv/bindings/python` installs this client
into the active kernel. Restart the kernel after upgrading an imported client.
No Python measurement engine or VISA dependency is installed by this package.
