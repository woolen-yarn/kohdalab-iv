# KohdaLab IV Native Desktop

Development preview: **1.2.0-alpha.1** adds per-device official VISA preference
on Windows. The published v1.1.0 behavior described below remains archived.

Windows discovery first asks the installed 64-bit system VISA runtime for USB0
and GPIB0 instruments. These entries appear as `VISA::<resource>` in the app;
`VISA::` is an app routing prefix, not part of the vendor VISA resource string.
Old saved resource strings automatically choose VISA when it exposes the same
device. Other devices use the existing native WinUSB route. No runtime is
downloaded or bundled, and the app never replaces a driver during Refresh.
A selected VISA connection does not silently fall back after a lock/timeout/error.
Manufacturer drivers without a usable 64-bit VISA runtime are retained and need
their vendor runtime configured. The updated USB setup preserves all existing
non-WinUSB assignments, including third-party drivers.
USB0 and GPIB0 primary-address resources are supported. GPIB0 uses one backend
at a time: when VISA owns it, native controller probing is skipped and saved
GPIB0 addresses also use VISA. Multiple simultaneous GPIB controllers are not
supported. Official-VISA and native direct-USB instruments may be mixed.


Version 1.1.0 refreshes the existing three-column interface with quiet charcoal
cards, distinct input fields and action buttons, aligned sweep controls, readable
path/resource summaries, and explicit connection/output states. Paths and resource
strings remain editable: click a summary to edit the full value, or hover to view it.
Windows Program Files installation and safe updates remain available.
It keeps Run focused on Start / Stop / Save, with internal output-off
control and automatic cleanup. It includes complete-data Save, automatic source
setup and per-point Repeat averaging. It keeps unverified 82357B firmware acquisition/upload excluded and supplies
explicit redistribution terms, native licensing evidence and relinking steps.
Blue/purple forward/backward styles and the CSV-matched inspector are retained.

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

This update reuses CSV formatting buffers, builds sweep plans without
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

Extract the whole ZIP. On Windows run `Install-Application.cmd`, approve the
administrator prompt, and use the **KohdaLab IV** desktop shortcut. You can also
open `kohdalab-iv-next.exe` directly for portable use. On Apple Silicon open
`KohdaLab IV Next.app`. No Python, Rust, or vendor VISA installation is
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
   its own unit. Timing / Compliance opens a dialog with OK / Cancel;
   extra timing settings and Readings per point (Repeat) are under Advanced.
   Repeat holds each source target, averages N meter readings and saves one
   CSV row; it does not repeat the sweep. NPLC stays under Meter.
   Source range and ramp step are automatic. Ramp step is the larger of the
   sweep step and 1% of the nominal source range, capped to the selected range.
   The requested Step magnitude rounds
   up to a whole source-resolution increment (1 uA for GS210 sweeps reaching
   100 mA); the effective step and ramp are shown below Start. Start / End must
   match that resolution. Invalid settings show a reason below Start. Limits
   are never increased automatically. Press Start to see an English dialog explaining
   any problem and how to fix it. Every real sweep verifies source output OFF
   then resets the source level to zero before configuration; failure prevents Start.
   The run JSON retains both requested and
   effective settings; CSV targets use the effective sweep.
5. Choose Directory with Browse, enter File, and choose Auto suffix. Run has
   Start / Stop / Save. Measurements are automatically saved as they are acquired.
   After completion or Stop cleanup, Save opens a file picker for a complete CSV
   copy and companion .run.json. It never exports a decimated graph or overwrites
   existing files. There is no separate Output Off button. Stop and shutdown
   retain the source cleanup and output-off verification. The output_off IPC
   command remains available to CLI and Python/Notebook clients. If cleanup
   reports Failed or output Unknown, check the instrument front panel and use
   Disconnect to retry cleanup before reconnecting; do not treat an unverified
   output state as safe.
6. The status strip shows progress, resistance and conductance. The right log
   remains visible during discovery and measurement. Field / Value toggles the
   exact 14 saved CSV fields; hover truncated entries for their full values.
7. Config Browse selects a JSON file. Load / Save use the displayed path. The
   Rust desktop config format is native Rust format.

Direct USB requires GS210 TMC mode (0B21:0039) or 34411A (0957:0A07).
Native USB/GPIB connections require the WinUSB assignments used by the existing portable; the
bundled `support/USB-Setup.exe` can install them. Setup is currently an explicit
support action in this release, rather than an automatic startup step. Administrator
approval may be required. The application never changes unrelated devices.
NI GPIB-USB-HS+ is supported. Agilent 82357B is supported only when already
initialized (0957:0718) through a separately authorized workflow. Boot ID
0957:0518 is refused without opening it or replacing its boot driver. This
release does not download, cache or upload firmware. Do not add a firmware file
to the portable or source build kit. See REDISTRIBUTION.md for licensing terms.

Simulation is a 1 kOhm circuit and never opens physical devices. CSV contains
every saved measurement point; the live graph can coalesce updates or reduce
its display cache during large runs. Existing result files are not overwritten.
Settings use `~/.kohdalab/config/iv-next.json`.

If communication fails, emergency cleanup attempts a fresh source helper and
verifies the source identity before retrying output off. If output cannot be
verified, the application reports Unknown: check the source front panel.
Hardware compliance and physical output controls remain essential.

## Windows installation and updates

1. Extract the complete Windows ZIP to a normal local folder, such as Downloads.
2. Close every running KohdaLab IV application. Double-click
   `Install-Application.cmd` and approve the Windows administrator prompt.
3. Start **KohdaLab IV** from its desktop shortcut. The installer does not start
   the application with administrator privileges or access any instrument.

The application is installed under `C:\Program Files\Kohdalab\KohdaLab IV`
(or the Windows Program Files location). The desktop contains a shortcut.
The shortcut uses `%USERPROFILE%\.kohdalab` as its working folder so relative
output paths are writable. User files remain separate:

| Content | Location |
| --- | --- |
| Settings | `%USERPROFILE%\.kohdalab\config\iv-next.json` |
| Default measurements | `%USERPROFILE%\.kohdalab\results` |
| Driver setup/cache | `%LOCALAPPDATA%\KohdaLab IV` |
| Previous installation backup | `KohdaLab IV\archive` inside your Windows Documents folder |

Existing absolute measurement paths remain unchanged. Installing the app does
not install/remove instrument drivers, edit the saved configuration or migrate
measurement files. Updates check the package files against the included `FILES.json`, stage the new files,
and preserve a verified backup of an existing installation before replacement.
Existing installation access controls are retained. If setup fails, keep the
extracted folder and the displayed backup path, and review the English error.
It does not change the PowerShell execution policy. Use the official GitHub
download links in the root README. The included hashes check internal file
consistency; they are not a publisher signature.

To update, download and extract the new ZIP, close IV and run its
`Install-Application.cmd` again. After success, the extracted folder can be
deleted; the installed copy owns all required support files. Direct portable
use remains available without this app installation step. Driver setup may
still require administrator approval separately.

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
support licenses, dependency notices, `REDISTRIBUTION.md` and `SOURCE.zip` when
redistributing. The combined GPIB executable is GPLv2, not MIT or GPLv3. Complete
source and LGPL replacement/relinking instructions are in
`SOURCE.zip/native/REBUILDING.md`. Windows uses the OS inbox WinUSB rather than
a bundled Microsoft driver binary.
SOURCE.zip includes the Rust, helper, shared specification and build sources.

Official downloads are available at
https://github.com/Kohdalab/kohdalab-iv/releases/tag/portable-v1.1.0 and the
personal mirror at https://github.com/woolen-yarn/kohdalab-iv/releases/tag/portable-v1.1.0.
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

### Sweep speed

Start with Repeat = 1. Reduce Wait time only as far as your circuit can settle;
First wait and Ramp wait are independent and still apply. Lower NPLC gives
faster conversions at the cost of resolution and mains-noise rejection (see
[34410A/34411A User's Guide](https://www.keysight.com/us/en/assets/9018-05586/user-manuals/9018-05586.pdf), Integration Time and Resolution).
At 50 Hz, 1 PLC is 20 ms and 0.2 PLC is 4 ms of integration, before autozero,
autoranging and communication overhead. The default 0.2 s per-point wait alone
limits the sweep to fewer than 5 points/s. Defaults are not reduced silently.
Per-point repeats share one source-setting operation and one settling wait.
Each individual conversion is checked against the software limit, so averaging
cannot hide an over-limit reading; partial accumulation is recorded on cutoff.
Legacy `average_count` is accepted when `repeat` is 1; otherwise `repeat` wins.
`repeat_index` remains 1 in the CSV; requested/effective counts are in run JSON.

Planning benchmark note: `repeat` now controls per-point accumulation. The
`benchmark_plan` example therefore builds 10,001 source targets with 99 readings
per target (990,099 meter conversions), not 990,099 source targets. Historical
planning timings in the optimization report used the earlier sweep-repeat
meaning and are not directly comparable to this corrected benchmark.
