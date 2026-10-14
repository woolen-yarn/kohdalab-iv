# KohdaLab IV Native Desktop

Version 0.3.0 is the Rust desktop release with the KohdaLab IV app icon. It uses the same
egui and eframe GUI foundation as KohdaLab Camera Native, with a separate Rust
measurement engine and the established USB and GPIB helper processes.

This desktop supports Yokogawa GS210 and Agilent or Keysight 34411A. It includes
IV and VI modes, linear, round trip, zero centered and custom sweeps, limits,
settling, averaging, NPLC, streamed CSV, run metadata, and an explicit simulator.
The existing Python edition remains available for other models and notebooks.
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
   Rust desktop config format is separate from the Python edition's format.

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
Settings use `~/.kohdalab/config/iv-next.json` independently of the Python GUI.

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
https://github.com/Kohdalab/kohdalab-iv/releases/tag/portable-v0.3.0 and the
personal mirror at https://github.com/woolen-yarn/kohdalab-iv/releases/tag/portable-v0.3.0.
The Python desktop remains available as portable-v0.2.3. Release evidence and
known limitations are recorded in `docs/rust_preview.md` in the source project.
