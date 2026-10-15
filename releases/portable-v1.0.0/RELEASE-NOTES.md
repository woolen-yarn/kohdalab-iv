KohdaLab IV v1.0.0

The first 1.0 release keeps Run focused on Start / Stop / Save. The separate
Output Off GUI button is removed; internal output_off control, priority handling
and automatic source cleanup on Stop, disconnect and shutdown are retained.
The automation API still supports explicit output_off commands.

Pre-release fixes: every real sweep now verifies source output OFF and zero
before changing configuration, including after a previous failure. If this
cannot be confirmed, Start is refused without enabling output. Python/Notebook
control returns immediately when a run ends with a compliance stop.

Included capabilities:
- Rust desktop and one engine for source/meter ownership, live monitoring,
  I-V / V-I sweeps, automatic source range/step/ramp setup and English validation.
- Repeat means N meter readings averaged at each fixed source target, one
  settling wait and one saved CSV row; it does not repeat the sweep.
- Individual conversion limit checks preserve the triggering reading and partial
  count. Limits are never raised automatically.
- Streaming autosave and Save of a complete, exact CSV plus run metadata under
  a new filename, with no overwrite and no export of decimated plot data.
- Scientific CSV notation without rounding measurement values, range-aware
  display precision, forward/backward colors and CSV-matched Field / Value.
- Python/Notebook control through the same Rust engine.

Packages: macOS Apple Silicon portable, Windows x64 portable, source build kit,
and SHA256SUMS.txt. No Python/Rust/VISA installation is required by the GUI.
Windows uses the included WinUSB setup. Mac is ad hoc signed, not Apple-notarized.
82357B support requires separate authorized firmware initialization; no firmware
acquisition/upload is included. Complete native corresponding source and
GPL/LGPL notices, library replacement/relinking and rebuild instructions remain
included.

Pre-release validation covers Rust formatting/lints/tests, native helper mocks,
Python IPC, complete-data Save/no-overwrite cases, both portable simulations,
Stop, output_off IPC and lost-GUI cleanup. Mac/Windows/Linux CI and dependency
audit are required before publication. Physical hardware timing and every
instrument/firmware variant are not re-certified by these simulation checks.
