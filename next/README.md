# KohdaLab IV Native Desktop

Version 0.4.0 adds nonnegative timing inputs and automatic engineering units on
the graph axes, including mV and µA. Zooming also updates the displayed units.
Plot coordinates and CSV values remain in SI units. CSV numbers use the shortest round-trip scientific notation without fixed
trailing zero padding. This preserves finite 64-bit floating-point values and
does not claim that all saved digits have measurement significance.
Screen readings now follow GS210 source-range resolution and the 34411A queried
DC range/resolution. Autoranging is queried after each reading; averaging uses
the coarsest resolution encountered and does not add display digits. Graph
hover labels retain each point's resolution. Unknown resolution is marked
explicitly. Derived resistance and conductance are not instrument readings and
are not certified significant figures. The native CSV now contains 14 analysis columns; configuration and instrument
metadata are kept in its companion `.run.json` (schema version 3). The old compatibility export has been removed. Simulation resolution is illustrative, not a hardware claim.

References: [GS210 manual, section 5.2](https://www.yokogawa.com/pdf/provide/E/GW/IM/0000023941/0/IMGS210-01EN.pdf),
[34411A SCPI comparison](https://www.keysight.com/us/en/assets/7018-02044/white-papers-archived/5990-3515.pdf).

Sweep targets can still be
negative; the measurement engine continues to validate all limits before a run.

The Rust desktop includes the KohdaLab IV app icon. It uses the same
egui and eframe GUI foundation as KohdaLab Camera Native, with a separate Rust
measurement engine and the established USB and GPIB helper processes.

This desktop supports Yokogawa GS210 and Agilent or Keysight 34411A. It includes
IV and VI modes, linear, round trip, zero centered and custom sweeps, limits,
settling, averaging, NPLC, streamed CSV, run metadata, and an explicit simulator.
Hardware accuracy and all firmware variants are not certified by this release.

This update reuses CSV formatting buffers, builds repeated sweep plans without
duplicate intermediate arrays, updates display state only when needed, and
appends to cached graph series. CSV still flushes every point; settling, NPLC,
limits and verified output shutdown are unchanged. See
`docs/rust_optimization.md` in the included source for the measured comparison.

The blue and orange I-V sweep icon appears in the Windows executables, Mac app
bundle and application window. Its editable source and platform formats are
included under `next/assets/`. To rebuild them, install Pillow and run
`python3 scripts/build_iv_icons.py` from the source root. Windows builds also
require MinGW `windres` or Windows SDK `rc.exe`; these are build tools only.

## Run

Extract the whole ZIP. On Windows open `kohdalab-iv-next.exe`; on Apple Silicon
open `KohdaLab IV Next.app`. No Python, Rust, or vendor VISA installation is
required for supported native connections. Keep the support files with the app.
The Mac app is ad hoc signed and is not notarized.

1. The original IV layout is reproduced: Config / Source / Meter on the left,
   Measurement controls and graph in the center, and log / Field-Value table on
   the right. The tall `<` / `>` buttons collapse either side column.
2. Choose a Device in each instrument group. For a dry run, select
   SIMULATED_SOURCE and SIMULATED_METER. Other unimplemented models from the
   original UI are visible but disabled in this edition.
3. For real devices, click Refresh, choose or type each Resource, then use the
   independent Connect buttons or All Connect. Identity checks reject an
   incompatible instrument. Only one GPIB adapter may be attached; both roles
   share its session. Disconnecting the source verifies output OFF first.
4. Review Mode, Sweep and Start / End / Step in Measurement. Each quantity has
   its own unit. Timing / Compliance opens a dialog with OK / Cancel; additional
   ramp, range and repeat settings are under Advanced. NPLC stays under Meter.
5. Choose Directory with Browse, enter File, and choose Auto suffix. Start runs
   the measurement; Stop requests cleanup; Output Off requests shutdown before
   resetting the source level. Existing CSV files are never overwritten.
6. The status strip shows progress, resistance and conductance. The right log
   remains visible during discovery and measurement. Field / Value toggles the
   original 15 snapshot fields; hover truncated entries for their full values.
7. Config Browse selects a JSON file. Load / Save use the displayed path. The
   Rust desktop config format is native Rust format.

Direct USB requires GS210 TMC mode (0B21:0039) or 34411A (0957:0A07).
Windows requires the WinUSB assignments used by the existing portable; the
bundled `support/USB-Setup.exe` can install them. Setup is currently an explicit
support action in this release, rather than an automatic startup step. Administrator
approval may be required. The application never changes unrelated devices.
The supported GPIB adapters are NI GPIB-USB-HS+ and Agilent 82357B. Initial
82357B firmware acquisition still requires its established setup procedure;
restricted firmware is not included.

Simulation is a 1 kOhm circuit and never opens physical devices. CSV contains
every saved measurement point; the live graph can coalesce updates or reduce
its display cache during large runs. Existing result files are not overwritten.
Settings use `~/.kohdalab/config/iv-next.json`.

If communication fails, emergency cleanup attempts a fresh source helper and
verifies the source identity before retrying output off. If output cannot be
verified, the application reports Unknown: check the source front panel.
Hardware compliance and physical output controls remain essential.

## Developer build

Install rustup and the pinned Rust toolchain, then run from the source root:

```sh
cargo test --manifest-path next/Cargo.toml --workspace --locked
cargo clippy --manifest-path next/Cargo.toml --workspace --all-targets --locked -- -D warnings
cargo build --manifest-path next/Cargo.toml --release --locked
```

The native helpers can be rebuilt with `scripts/build_native_gpib.sh` and
`scripts/build_usbtmc_probe.sh` on Mac. Windows helper rebuilds use the existing
MSYS2 scripts. Set `KOHDALAB_IV_HELPER_DIR` to a directory containing both helpers,
or use the bundled support folder. New instrument PCs do not need these tools.

Windows cross builds use the `x86_64-pc-windows-gnu` target and MinGW, with a
static compiler runtime. The packager expects Windows binaries under
`next/target-windows/x86_64-pc-windows-gnu/release`. Mac binaries use
`next/target/release`. Package from the source root:

```sh
python3 scripts/package_rust_preview.py --platform macos-arm64
python3 scripts/package_rust_preview.py --platform windows-x64
python3 scripts/package_rust_build_kit.py
```

The source project also contains `next/build-macos.command` and
`next/build-windows.ps1`. The Windows script needs the Rust MSVC development
toolchain, Visual C++ Build Tools, and Python 3.11 or newer on the build PC.
It packages the native Cargo output with the hash checked existing helpers.
These development tools are not needed on instrument PCs. The build scripts
and packager are included in SOURCE.zip.
For a Windows build from SOURCE.zip alone, place the original bundled support
binaries in `portable/windows/native` before packaging, or rebuild the native
helpers and setup tool using the existing C/C++ build instructions. A source
archive deliberately contains source rather than a second copy of the EXEs.

For command output on Windows, use `iv-cli.exe`. Developer checks include
`--list`, `--identify --source RESOURCE --meter RESOURCE`, and
`--measure --simulate --output NEW_CSV_PATH`. Hardware CLI smoke tests require
`--confirm-open-circuit` and are limited to a three point +/-1 mV sweep with
1 mA hardware compliance. `--gui-smoke --screenshot PNG_PATH` checks the real
window and the same engine, CSV, and simulator path. It does not test hardware.

`python3 scripts/verify_rust_preview.py BINARY --output-dir NEW_DIRECTORY`
checks the complete engine process, the existing CSV columns, Stop, and loss
of the GUI heartbeat using simulation. Hardware GUI smoke uses
`--gui-hardware-smoke --confirm-open-circuit --source RESOURCE --meter RESOURCE`
and the same three point limits as the hardware CLI smoke.

## Source and licenses

Rust application code is MIT licensed and forbids unsafe code. Dependencies
and native OS boundaries have their own memory safety and license properties.
The native helpers contain GPL components and LGPL libraries. Retain the
support licenses, dependency notices, and `SOURCE.zip` when redistributing.
SOURCE.zip includes the Rust, helper, shared specification and build sources.

Official downloads are available at
https://github.com/Kohdalab/kohdalab-iv/releases/tag/portable-v0.4.0 and the
personal mirror at https://github.com/woolen-yarn/kohdalab-iv/releases/tag/portable-v0.4.0.
Earlier releases remain archived on GitHub. Historical preview validation is
recorded in `docs/rust_preview.md`; current behavior and limitations are described here.

Per-model display policies, primary sources and audit limitations are documented
in `docs/instrument_precision.md` and the shared source/meter `specs.toml` files.

Native CSV columns: `timestamp`, `elapsed_s`, `point_index`, `repeat_index`,
`sweep_index`, `direction`, `target_value`, `target_unit`, `voltage_V`,
`current_A`, `voltage_origin`, `current_origin`, `software_limit_hit`, `status`. Target units are V in IV mode
and A in VI mode. Voltage/current use source level readback (or the setpoint
when unavailable) and the meter reading according to mode. Source level
readback is not an independent voltage/current measurement.

The run JSON also records range/resolution changes with their point indices,
so autorange history is retained without duplicating metadata in every CSV row.

Run settings are saved before output is enabled. Final metadata is written to
a separate temporary file and renamed over the initial JSON after output
cleanup. If final metadata writing fails, initial settings remain available
and the run reports a warning. `in_progress: true` identifies an unfinished
metadata record; it does not establish the current physical output state.

## Live channels

Config stays at the top of the left panel. Source and Meter each embed compact
I/V values, zero-centered level bars and bounded filled time-history trends.
Each instrument displays its own channel according to the actual source function.
Live readings start after both instruments connect,
including before a sweep. The monitor uses the actual GS210 function and the
opposite 34411A DC measurement function, configures the meter at 1 NPLC, and
never enables the source output. During a sweep, settling waits also acquire live samples and each averaging
conversion updates the monitor. Live reads never reconfigure the sweep NPLC or
create CSV points. They can lengthen a wait by transport/conversion overhead;
no competing background transfers run. Monitor readings are not
written to measurement CSVs. Historical values are held after disconnect.
GS210 readings are programmed source level, not an independent /MON measurement.
A live-transfer failure stops the monitor and requests verified output shutdown.

This repository contains a Rust application and isolated native helper sources.
Legacy Python application code, notebook API, VISA bindings, PyInstaller builds and
Python CI have been removed. Optional packaging/validation/icon utilities use
Python's standard library (Pillow only for regenerating icons) on a build host;
they are not included as an application runtime or compatibility layer.

## Python and Notebook control

Use `python-control/kohdalab_iv_native.py` from the portable package, or
[the source client](../bindings/python/README.md). It uses only the Python standard
library and invokes `iv-cli --engine`. The engine owns devices and safety cleanup;
its defaults are available through `iv-cli --default-config`.

Live acquisition targets a 50 ms start-to-start interval (up to 20 samples/s),
with a 33 ms display refresh interval. Actual acquisition rate depends on the
instrument, NPLC and USB/GPIB latency; no synthetic measured samples are added.
All floating CSV columns (`elapsed_s`, `target_value`, `voltage_V`, `current_A`)
use shortest round-trip scientific notation, including zero (`0e0`).

The central I-V plot ignores ordinary wheel scrolling. Drag, boxed zoom,
modifier-wheel zoom and double-click reset remain available. Direction colors
share measured turning points, so forward/backward lines meet at the endpoints.
Excel may reformat numeric CSV cells on import; choose Scientific number format
in Excel to display all electrical values as exponents. CSV text remains scientific.

## Live mode and palette

Mode changes switch GS210 voltage/current sourcing and the opposite 34411A
measurement function. Output OFF and zero are verified before the switch; zero
is verified again for the new function. Live history clears on a GUI Mode change.
Voltage is amber, current is teal, and forward/backward curves are blue/purple
with a legend. Mode changes never enable output.
