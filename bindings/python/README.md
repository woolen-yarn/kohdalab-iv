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
python -m pip install "kohdalab-iv-native @ git+https://github.com/woolen-yarn/kohdalab-iv.git@portable-v1.2.0#subdirectory=bindings/python"
uv add "kohdalab-iv-native @ git+https://github.com/woolen-yarn/kohdalab-iv.git@portable-v1.2.0#subdirectory=bindings/python"
```

The client resolves `iv-cli` in this order:

1. The explicit `NativeIV(executable=...)` argument.
2. `KOHDALAB_IV_EXE`.
3. An `iv-cli` or `iv-cli.exe` already on `PATH`.
4. On Windows, `C:\\Program Files\\Kohdalab\\KohdaLab IV\\iv-cli.exe` from the
   desktop installer.

Use an explicit path if several installations exist. On macOS, pass the portable
`iv-cli` path or set `KOHDALAB_IV_EXE`. Keep a portable executable in its package
folder so its native helpers can be found. Close the GUI before connecting to the
same hardware.

```python
from pathlib import Path
from kohdalab_iv_native import NativeIV

with NativeIV() as iv:  # Finds the Windows desktop installation automatically.
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
