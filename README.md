# KohdaLab IV

A Rust desktop for DC I-V and V-I measurements with GS210 and 34411A over direct
USB or USB-GPIB. The three-column interface keeps setup on the left, measurement
and the I-V graph in the center, and detailed values/logs on the right.

## Native version: 0.4.1

Config stays at the top of the left panel. Source and Meter each contain a compact
resolution-aware readout, zero-centered level bar and filled time-history trend,
including while connected before measurement. The channel follows the actual
source function: V at Source / I at Meter, or I at Source / V at Meter.
Source level and meter readings are clearly distinguished. A single measurement
engine owns the devices; live monitoring runs between source steps during settling and uses each
meter conversion during averaging. Monitoring never enables the source output.

The application, planning, IPC and UI are Rust. The old Python application,
compatibility CSV API and legacy Python CI/builds are removed. A lightweight
[Python control client](bindings/python/README.md) supports scripts and Notebooks
by calling the same Rust engine; no measurement logic is duplicated.
Reviewed instrument profiles live in `next/specs/`. Native USB/GPIB helpers remain
separate processes with their own licenses and OS boundaries.

## Published portable downloads

The current published release is **v0.4.1**.

| Platform | Kohdalab | Personal mirror |
| --- | --- | --- |
| macOS Apple Silicon | [Mac portable](https://github.com/Kohdalab/kohdalab-iv/releases/download/portable-v0.4.1/kohdalab-iv-0.4.1-macos-arm64-portable.zip) | [Mac portable](https://github.com/woolen-yarn/kohdalab-iv/releases/download/portable-v0.4.1/kohdalab-iv-0.4.1-macos-arm64-portable.zip) |
| Windows x64 | [Windows portable](https://github.com/Kohdalab/kohdalab-iv/releases/download/portable-v0.4.1/kohdalab-iv-0.4.1-windows-x64-portable.zip) | [Windows portable](https://github.com/woolen-yarn/kohdalab-iv/releases/download/portable-v0.4.1/kohdalab-iv-0.4.1-windows-x64-portable.zip) |

[Source build kit (Kohdalab)](https://github.com/Kohdalab/kohdalab-iv/releases/download/portable-v0.4.1/kohdalab-iv-0.4.1-build-kit.zip) ·
[Source build kit (personal)](https://github.com/woolen-yarn/kohdalab-iv/releases/download/portable-v0.4.1/kohdalab-iv-0.4.1-build-kit.zip) ·
[SHA-256 checksums](https://github.com/Kohdalab/kohdalab-iv/releases/download/portable-v0.4.1/SHA256SUMS.txt)

Extract the whole ZIP. Mac: open `KohdaLab IV Next.app`; Windows: open
`kohdalab-iv-next.exe`. No Python, Rust or manufacturer VISA installation is
needed to run it. Windows may need the included `support/USB-Setup.exe` for WinUSB.
Mac bundles are ad hoc signed, not Apple-notarized.

## Build and validation

Install the Rust toolchain declared in `next/rust-toolchain.toml`.

```sh
cd next
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo build --release --locked
```

See [native build/setup guide](next/README.md) for platform packaging. Optional
host-only packaging/validation tools use Python standard-library scripts; the
application and its CI do not depend on the former Python toolkit.

## Data and instrument semantics

The native CSV has 14 columns: timestamp, elapsed time, point/repeat/sweep
indices, direction, target and unit, voltage/current, origins, limit flag and
status. Numeric values use shortest round-trip scientific notation. Display
rounding does not change saved values or protection comparisons. Keep the CSV
and `.run.json` together; settings are saved before output is enabled.

The GS210 level query is programmed source level, not an independent measurement
of the voltage/current delivered to a load. Display resolution is not a
measurement-uncertainty guarantee.

- [Instrument precision and limitations](docs/instrument_precision.md)
- [CSV column audit](docs/csv_column_audit.md)
- [Safety](SAFETY.md)
- [Native helper sources and licenses](native/README.md)

Historical benchmark reports and prior GitHub releases remain available as
evidence of the migration. MIT covers the Rust application; retain all native
helper and dependency source/license notices when distributing portable bundles.

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

Voltage uses amber; current uses teal. Forward/backward sweeps use blue/purple
with a legend. Changing Mode while idle switches live source/meter functions
after verifying output OFF and zero level. The GUI applies the chosen Mode on
source connection; changing Mode never enables output.

Forward curves are blue solid lines with filled circles; backward curves are
purple dashed lines with open diamonds, so overlapping curves remain distinct.
Field/Value shows the exact 14-column CSV schema and saved values; live readouts
and plot tooltips continue using instrument-resolution display precision.
