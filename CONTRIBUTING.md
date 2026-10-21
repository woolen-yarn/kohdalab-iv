# Contributing

Use the pinned Rust toolchain under `next/`. Run Cargo format, Clippy and the
workspace tests before proposing a change. Architecture: `iv-core` owns planning,
CSV and instrument specifications; `iv-transport` owns exclusive device leases
and helper IPC; `iv-desktop` owns measurement, monitoring and UI.

## Where changes belong

| Area | Location | Responsibility |
| --- | --- | --- |
| Sweep rules and saved data | `next/crates/iv-core/` | Planning, validation, SI values, CSV and reviewed instrument specs |
| Instrument communication | `next/crates/iv-transport/` | Resource routing, helper processes, device leases and instrument commands |
| Measurement execution | `next/crates/iv-desktop/src/engine.rs` | Serialized acquisition, monitoring, cancellation and output cleanup |
| Engine interface | `next/crates/iv-desktop/src/protocol.rs`, `client.rs` | Commands, events and the desktop client |
| Application UI | `next/crates/iv-desktop/src/ui.rs` | Application state, event synchronization and three-panel layout |
| UI settings | `next/crates/iv-desktop/src/ui/settings.rs` | GUI settings schema, defaults and persistence |
| Display formatting | `next/crates/iv-desktop/src/ui/formatting.rs` | Engineering prefixes, tick labels and resolution-aware readouts |
| Drawing history | `next/crates/iv-desktop/src/ui/history.rs` | Bounded live history, direction segments and turning-point anchors |
| Shared UI components | `next/crates/iv-desktop/src/ui/widgets.rs` | Palette, style and common input widgets |
| Notebook control | `bindings/python/` | Application discovery and the thin JSON-lines client |
| Packaging | `scripts/`, `portable/windows/` | Portable archives, source kits and installation lifecycle |

## Adding an instrument or measurement

Register a model once in `iv-core/src/catalog.rs`, including its role,
capabilities and protocol dialect. Keep numeric limits, ranges, resolution and
NPLC rules in `next/specs/`; `instrument_profile` reads these same resources.
The GUI obtains model choices and operation availability from the catalog.
Add the model to the README matrix with connection requirements and physical
validation status. The catalog contract tests check that every model is listed.

Implement protocol exchanges in `iv-transport/src/drivers.rs` through
`InstrumentIo`. DevicePair retains ownership of sessions, exclusive leases,
absolute deadlines and emergency recovery. New drivers must pass the shared
identity/configuration contracts and exercise timeout, disconnect and malformed
responses. Preserve ADC discard ordering and the single 7651 panel deadline.

Measurement labels, required roles, source/meter functions, mode-switch
defaults, axis labels and sweep choices belong in `iv-core/src/measurements.rs`.
Planning and validation still belong in `iv-core`; a descriptor alone must never
enable a device or bypass a scientific limit. Use `engine/workflow.rs` for source
ramps and completion precedence. Keep the existing wait/acquisition routines and
verified output shutdown on the device-owning worker. Emergency output-off must
not wait behind a normal source ramp.

`state.rs` defines typed phases, execution activities, output state and failures.
Serialized phase/output labels and the `error` message remain compatible with
older clients. `error_kind` is supplementary diagnostic information; legacy
helper strings are classified at that boundary and must never control shutdown.

`compatibility.rs` owns protocol/settings/metadata/CSV contract versions. The
Python client negotiates from the first ordinary engine state and accepts older
engines without a declaration. Never probe old engines with a new command-line
flag: an unknown flag can launch their GUI. Unversioned and v0 GUI settings
migrate to v1 without changing values; future versioned settings are refused on
load and protected from auto-save. The existing CSV schema and metadata version
are retained. Increment versions only when their compatibility contract changes.

Keep UI helper modules private to the UI. Their unit tests live with the code;
`next/crates/iv-desktop/tests/live_engine.rs` exercises the process interface and
output cleanup. Move the existing tests with a refactor and retain their
assertions. Compare the GUI simulation and saved records when reorganizing UI
code; a successful build alone does not establish unchanged behavior.

Display formatting does not define the saved precision. Keep CSV serialization
in `iv-core`, and keep drawing anchors out of measurement records. Existing GUI
settings use `.kohdalab/config/iv-next.json`; the filename is a compatibility
contract despite the current application name. Preserve its fields, defaults
and location when reorganizing settings code.

## Measurement and distribution requirements

Never relax scientific limits, golden data or output-shutdown requirements to
make tests pass. Live acquisition shares the same device-owning worker as sweeps.
Keep GUI rendering free of instrument I/O and bound display history.

Instrument data belongs in `next/specs/`, with primary references and review
status. Do not infer physically meaningful digits from serialized float length.
Native C/C++ helper changes require their source and license notices to accompany
portable binaries. Optional packaging utilities are host tools, not a runtime.
