# KohdaLab IV

A Rust desktop for DC I-V and V-I measurements with GS210 and 34411A over direct
USB or USB-GPIB. The three-column interface keeps setup on the left, measurement
and the I-V graph in the center, and detailed values/logs on the right.

## Native version: 1.2.0

The refreshed charcoal interface preserves the original three-column layout,
with aligned sweep controls, distinct inputs and actions, readable path/resource
summaries and explicit instrument connection/output states. Data and logs remain
visible together on the right.

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

## Supported instruments

The v1.2.0 Rust desktop and automation engine support these instruments in both DC modes (source V / measure I and source I / measure V):

| Role | Instrument | Connection |
| --- | --- | --- |
| Source | Yokogawa GS210 | Direct USB in communication mode, or GPIB |
| Meter | Agilent / Keysight 34411A | Direct USB, or GPIB |
| Simulation | Built-in source and meter | No hardware required |

Supported USB-GPIB adapters are **NI GPIB-USB-HS+** and **Agilent 82357B**. The 82357B must already be initialized using separately authorized firmware/software; the distribution does not acquire or upload firmware. Windows can use an available official 64-bit VISA runtime or supported native WinUSB devices. Native USB/GPIB is available on macOS Apple Silicon. Multiple simultaneous GPIB controllers are not supported.

Yokogawa 7651 / GS200, Agilent 34401A, Keysight 34465A and ADCMT 7461A are **not implemented as instrument drivers in this Rust release**. Some have retained specification metadata or disabled UI entries; that does not provide measurement support. Refresh discovery alone does not establish model compatibility. See [instrument precision and limitations](docs/instrument_precision.md).

## Downloads

The current stable release is **v1.2.0**. Public downloads are available from
the personal mirror; Kohdalab links may require organization repository access.

Run uses **Start / Stop / Save**. Measurements are still saved automatically;
Save exports a complete CSV copy and its run metadata without overwriting data.
Output-off control remains in the engine and automation API; the GUI uses Stop
and automatic cleanup. Repeat averages N meter readings at each fixed target;
it does not repeat the sweep. Source range, effective step and ramp are automatic,
and invalid Start requests explain the problem in an English dialog.

| Platform | Kohdalab | Personal mirror |
| --- | --- | --- |
| macOS Apple Silicon | [Mac portable](https://github.com/Kohdalab/kohdalab-iv/releases/download/portable-v1.2.0/kohdalab-iv-1.2.0-macos-arm64-portable.zip) | [Mac portable](https://github.com/woolen-yarn/kohdalab-iv/releases/download/portable-v1.2.0/kohdalab-iv-1.2.0-macos-arm64-portable.zip) |
| Windows x64 installer | [Windows Setup](https://github.com/Kohdalab/kohdalab-iv/releases/download/portable-v1.2.0/kohdalab-iv-1.2.0-windows-x64-Setup.exe) | [Windows Setup](https://github.com/woolen-yarn/kohdalab-iv/releases/download/portable-v1.2.0/kohdalab-iv-1.2.0-windows-x64-Setup.exe) |
| Windows x64 | [Windows portable](https://github.com/Kohdalab/kohdalab-iv/releases/download/portable-v1.2.0/kohdalab-iv-1.2.0-windows-x64-portable.zip) | [Windows portable](https://github.com/woolen-yarn/kohdalab-iv/releases/download/portable-v1.2.0/kohdalab-iv-1.2.0-windows-x64-portable.zip) |

[Source build kit (Kohdalab)](https://github.com/Kohdalab/kohdalab-iv/releases/download/portable-v1.2.0/kohdalab-iv-1.2.0-build-kit.zip) ·
[Source build kit (personal)](https://github.com/woolen-yarn/kohdalab-iv/releases/download/portable-v1.2.0/kohdalab-iv-1.2.0-build-kit.zip) ·
[SHA-256 checksums](https://github.com/Kohdalab/kohdalab-iv/releases/download/portable-v1.2.0/SHA256SUMS.txt)

Windows: run **Setup.exe**, approve administrator access, then start **KohdaLab IV** from the desktop or Start menu. It installs to `C:\Program Files\Kohdalab\KohdaLab IV`. Run a newer Setup to update; uninstall from Windows Settings. Close IV and automation clients first. Settings, results and instrument drivers are retained.

For portable use, extract the whole ZIP. Mac: open `KohdaLab IV.app`. Windows: run `kohdalab-iv.exe` directly. No Python or Rust installation is needed. Instrument communication uses an available official 64-bit VISA runtime or the native WinUSB helpers. App installation does not change drivers; `support/USB-Setup.exe` is a separate action for devices that need WinUSB.
See [Windows Setup distribution](next/README.md#windows-setupexe-distribution).
Mac bundles are ad hoc signed, not Apple-notarized.

82357B is supported only after separate, authorized firmware initialization.
This release never downloads or uploads its firmware; NI HS+ and direct USB
remain supported. See [redistribution terms](REDISTRIBUTION.md) and the
[licensing evidence](docs/redistribution_audit.md).

## Windows installer and official VISA preference

Version 1.2.0 adds an English Setup.exe with Windows app registration,
updates and data-preserving removal, while retaining the portable ZIP. It prefers an installed 64-bit NI/Keysight-compatible
VISA runtime for each resource it enumerates. Existing WinUSB devices continue
through the native USB/GPIB helpers. VISA selection is retained for the lifetime
of a connection: busy devices, timeouts and identity failures do not trigger a
silent retry through a different backend. No manufacturer runtime is bundled.
The USB setup tool preserves already assigned non-WinUSB drivers; only missing
drivers or an incomplete existing WinUSB assignment are configured.


## Development: legacy instrument drivers

The next **1.3.0-alpha.6** build ports the working Python drivers at `b754cfa^` into the Rust engine. This development work is not included in the v1.2.0 downloads above.

| Instrument | Implemented development connection | Validation |
| --- | --- | --- |
| Yokogawa GS210 | Direct USB / GPIB | Existing support retained |
| Yokogawa 7651 | GPIB, legacy commands | Protocol mock tests; hardware pending |
| Agilent 34401A | GPIB | Protocol port; hardware pending |
| Agilent / Keysight 34411A | Direct USB / GPIB | Existing support retained |
| Keysight 34465A | USBTMC / GPIB | Protocol port; hardware pending; fast NPLC depends on options |
| ADCMT 7461A | GPIB in SCPI mode; Windows official VISA USB in ADC mode | Protocol mock tests; hardware pending |

7461A direct USB through the native WinUSB helper is not implemented. GS200 has no separate legacy driver to port and is not newly enabled. Device selection and resource discovery are separate; a discovered resource still needs a matching model.

The executables currently have no publisher Authenticode signature. See [Windows code signing and execution warnings](docs/windows_code_signing.md). Setup packaging alone does not remove SmartScreen or Smart App Control warnings.

## Code signing policy

Windows artifacts are currently **unsigned**. SignPath Foundation application
preparation is documented in [Code signing policy](CODE_SIGNING_POLICY.md),
[Privacy policy](PRIVACY.md) and the [application preparation guide](.signpath/README.md).
Acceptance and trusted publisher signatures are pending; this is not a sponsorship
claim. The manual [unsigned build workflow](.github/workflows/release-build.yml)
rebuilds Windows application/helper files from source on GitHub-hosted runners.
Upstream GPIB/libwdi signing eligibility and installer signing require review
before production signing can be enabled.

The desktop executable is `kohdalab-iv.exe` on Windows; the Mac bundle is
`KohdaLab IV.app`. The CLI remains `iv-cli`, and existing saved settings are retained.

Source and Meter connection settings, including Meter NPLC, are grouped in
collapsible Device sections. Live readouts remain visible when folded. Output
and Run have separate rounded outlines.

The latest development download is **v1.3.0-alpha.6**, including legacy protocol
ports, reduced zero-wait metadata traffic, signing preparation and automatic
Notebook application discovery with a migration tutorial. Additional
instrument hardware validation is pending. The stable v1.2.0 links above remain
available.

- [Development downloads (Kohdalab)](https://github.com/Kohdalab/kohdalab-iv/releases/tag/preview-v1.3.0-alpha.6)
- [Development downloads (personal)](https://github.com/woolen-yarn/kohdalab-iv/releases/tag/preview-v1.3.0-alpha.6)

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

## Python and Jupyter

Install the desktop/portable Rust application first. In a checkout containing the
new client packaging, run `python -m pip install ./bindings/python` (or
`uv pip install ./bindings/python`). `NativeIV()` finds the Windows installation
or a macOS application in `~/Applications` or `/Applications` automatically.
For a portable package elsewhere, pass its folder or the `iv-cli` path, or set
`KOHDALAB_IV_EXE`. The Python package contains only a small client; Git clone alone does not install the Rust application.
See [Python/Notebook instructions](bindings/python/README.md).
