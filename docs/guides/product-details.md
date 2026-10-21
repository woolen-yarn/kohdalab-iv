# KohdaLab IV

A Rust desktop for DC I-V and V-I measurements with GS210 and 34411A over direct
USB or USB-GPIB. The three-column interface keeps setup on the left, measurement
and the I-V graph in the center, and detailed values/logs on the right.

## Interface

![KohdaLab IV interface](../images/kohdalab-iv.png)

Simulation example: instrument setup and live readouts on the left, sweep
controls and plot in the center, and the saved-data inspector and log on the right.

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
[Python control client](../../bindings/python/README.md) supports scripts and Notebooks
by calling the same Rust engine; no measurement logic is duplicated.
Reviewed instrument profiles live in `specs/`. Native USB/GPIB helpers remain
separate processes with their own licenses and OS boundaries.

## Supported instruments

Stable **v1.2.0** supports GS210 and Agilent/Keysight 34411A. The current
**1.3 development source** also implements the additional models below. A protocol
implementation or successful mock test does not establish physical validation.
Both DC modes are available: source voltage / measure current and source current /
measure voltage. Model IDs are also used by the Notebook client.

| Instrument | Role / model ID | Direct USB | GPIB | Availability and validation |
| --- | --- | --- | --- | --- |
| Yokogawa GS210 | Source · `YOKOGAWA_GS210` | Control/communication mode | Yes | Stable + development; previous hardware operation retained |
| Yokogawa 7651 | Source · `YOKOGAWA_7651` | No | Legacy protocol | Development; protocol tests, physical validation pending |
| Agilent 34401A | Meter · `AGILENT_34401A` | No | Yes | Development; protocol tests, physical validation pending |
| Agilent / Keysight 34411A | Meter · `AGILENT_34411A`, `KEYSIGHT_34411A` | USBTMC | Yes | Stable + development; previous hardware operation retained |
| Keysight 34465A | Meter · `KEYSIGHT_34465A` | USBTMC | Yes | Development; protocol tests, physical validation pending; fast NPLC depends on options |
| ADCMT 7461A | Meter · `ADCMT_7461A` | Windows official 64-bit VISA, ADC mode | SCPI mode | Development; protocol tests, physical validation pending |
| Simulated source | Source · `SIMULATED_SOURCE` | Not needed | Not needed | Built-in, automated simulation tests |
| Simulated meter | Meter · `SIMULATED_METER` | Not needed | Not needed | Built-in, automated simulation tests |

GS210 USB must use communication mode, rather than storage mode. Native WinUSB
ADCMT USB and a separate GS200 driver are not implemented. Discovery and model
selection are separate: a discovered resource still needs a matching model.

| USB-GPIB adapter | Requirement |
| --- | --- |
| NI GPIB-USB-HS+ | Supported native helper or an available Windows official VISA runtime |
| Agilent / Keysight 82357B | Must already be initialized using separately authorized software/firmware |

The distribution never obtains or uploads 82357B firmware. Windows prefers an
available official 64-bit VISA runtime and otherwise uses supported native
WinUSB devices. macOS Apple Silicon uses the native USB/GPIB helpers. Multiple
simultaneous GPIB controllers are not supported. See
[instrument precision and limitations](../reference/instrument-precision.md).

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
See [Windows Setup distribution](application-guide.md#windows-setupexe-distribution).
Mac bundles are ad hoc signed, not Apple-notarized.

82357B is supported only after separate, authorized firmware initialization.
This release never downloads or uploads its firmware; NI HS+ and direct USB
remain supported. See [redistribution terms](../../REDISTRIBUTION.md) and the
[licensing evidence](../reference/redistribution-audit.md).

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

The **1.3.0-alpha.7** development line ports working Python protocols at
`b754cfa^` into the Rust engine. See the [supported-instrument matrix](#supported-instruments)
for model IDs, connection requirements and physical validation status. These
additional drivers are not included in the stable v1.2.0 downloads above.

The executables currently have no publisher Authenticode signature. See [Windows code signing and execution warnings](../development/windows-signing.md). Setup packaging alone does not remove SmartScreen or Smart App Control warnings.

## Code signing policy

Windows artifacts are currently **unsigned**. SignPath Foundation application
preparation is documented in [Code signing policy](../../CODE_SIGNING_POLICY.md),
[Privacy policy](../../PRIVACY.md) and the [application preparation guide](../../.signpath/README.md).
Acceptance and trusted publisher signatures are pending; this is not a sponsorship
claim. The manual [unsigned build workflow](../../.github/workflows/release-build.yml)
rebuilds Windows application/helper files from source on GitHub-hosted runners.
Upstream GPIB/libwdi signing eligibility and installer signing require review
before production signing can be enabled.

The desktop executable is `kohdalab-iv.exe` on Windows; the Mac bundle is
`KohdaLab IV.app`. The CLI remains `iv-cli`, and existing saved settings are retained.

Source and Meter connection settings, including Meter NPLC, are grouped in
collapsible Device sections. Live readouts remain visible when folded. Output
and Run have separate rounded outlines.

The latest development download is **v1.3.0-alpha.7**, including legacy protocol
ports, reduced zero-wait metadata traffic, signing preparation, automatic
Notebook application discovery and shared instrument/measurement contracts.
Versioned settings and Notebook negotiation retain legacy compatibility. Additional
instrument hardware validation is pending. The stable v1.2.0 links above remain
available.

- [Development downloads (Kohdalab)](https://github.com/Kohdalab/kohdalab-iv/releases/tag/preview-v1.3.0-alpha.7)
- [Development downloads (personal)](https://github.com/woolen-yarn/kohdalab-iv/releases/tag/preview-v1.3.0-alpha.7)

## Build and validation

Install the Rust toolchain declared in `rust-toolchain.toml`.

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo build --release --locked
```

See [native build/setup guide](application-guide.md) for platform packaging. Optional
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

- [Instrument precision and limitations](../reference/instrument-precision.md)
- [CSV column audit](../reference/csv-format.md)
- [Safety](../../SAFETY.md)
- [Native helper sources and licenses](../../native/README.md)

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
See [Python/Notebook instructions](../../bindings/python/README.md).
