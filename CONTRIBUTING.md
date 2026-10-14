# Contributing

Use the pinned Rust toolchain under `next/`. Run Cargo format, Clippy and the
workspace tests before proposing a change. Architecture: `iv-core` owns planning,
CSV and instrument specifications; `iv-transport` owns exclusive device leases
and helper IPC; `iv-desktop` owns measurement, monitoring and UI.

Never relax scientific limits, golden data or output-shutdown requirements to
make tests pass. Live acquisition shares the same device-owning worker as sweeps.
Keep GUI rendering free of instrument I/O and bound display history.

Instrument data belongs in `next/specs/`, with primary references and review
status. Do not infer physically meaningful digits from serialized float length.
Native C/C++ helper changes require their source and license notices to accompany
portable binaries. Optional packaging utilities are host tools, not a runtime.
