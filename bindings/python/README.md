# Python / Notebook control

The application and instrument engine are Rust. This optional client uses Python
3.10+ and its standard library to control the portable `iv-cli` executable over
JSON lines. It does not need PyVISA, NumPy, or the former Python application.

Copy `kohdalab_iv_native.py` beside your script, or add this directory to
`sys.path`. Set `KOHDALAB_IV_EXE` to the absolute path of `iv-cli` (`iv-cli.exe`
on Windows). Keep the executable inside its portable folder so native helpers
can be found. Close the GUI before connecting to the same instruments.

```python
import os
from pathlib import Path
from kohdalab_iv_native import NativeIV

with NativeIV(os.environ["KOHDALAB_IV_EXE"]) as iv:
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
