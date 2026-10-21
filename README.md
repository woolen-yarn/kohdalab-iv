<img src="assets/kohdalab-iv.png" width="64" height="64" align="right" alt="KohdaLab IV icon">

# KohdaLab IV

[![CI](https://github.com/woolen-yarn/kohdalab-iv/actions/workflows/test.yml/badge.svg)](https://github.com/woolen-yarn/kohdalab-iv/actions/workflows/test.yml)
[![Release](https://img.shields.io/github/v/release/woolen-yarn/kohdalab-iv)](https://github.com/woolen-yarn/kohdalab-iv/releases/latest)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

Rust desktop application for DC I-V and V-I measurements, live source/meter monitoring, and USB/GPIB instrument control.

[Download](https://github.com/woolen-yarn/kohdalab-iv/releases) · [Documentation](docs/README.md) · [Report an issue](https://github.com/woolen-yarn/kohdalab-iv/issues)

- Voltage-source/current-measure and current-source/voltage-measure sweeps.
- Live monitoring, source protection, CSV recording, and run metadata.
- One Rust engine for the desktop GUI and optional Python/Jupyter client.

## Interface

![KohdaLab IV interface](docs/images/kohdalab-iv.png)

Simulation example: instrument setup and live signals on the left, sweep and plot in the centre, and saved values/logs on the right.

## Downloads

| Channel | Version | Platforms |
| --- | --- | --- |
| Stable | [1.2.0](https://github.com/woolen-yarn/kohdalab-iv/releases/tag/portable-v1.2.0) | Windows x64 Setup / portable ZIP; macOS Apple silicon portable ZIP |
| Preview | [1.3.0-alpha.7](https://github.com/woolen-yarn/kohdalab-iv/releases/tag/preview-v1.3.0-alpha.7) | Windows x64 Setup / preview ZIP; macOS Apple silicon preview ZIP |

Rust and Python are not required to run the packaged application. Current Windows
artifacts are unsigned; download packages and checksums together from Releases.
macOS bundles use ad hoc signing and are not Apple-notarized.

## Supported hardware

| Role | Model / API identifier | Availability |
| --- | --- | --- |
| Source | Yokogawa GS210 · `YOKOGAWA_GS210` | Stable + preview |
| Source | Yokogawa 7651 · `YOKOGAWA_7651` | Preview; physical validation pending |
| Meter | Agilent / Keysight 34411A · `AGILENT_34411A`, `KEYSIGHT_34411A` | Stable + preview |
| Meter | Agilent 34401A · `AGILENT_34401A` | Preview; physical validation pending |
| Meter | Keysight 34465A · `KEYSIGHT_34465A` | Preview; physical validation pending |
| Meter | ADCMT 7461A · `ADCMT_7461A` | Preview; physical validation pending |
| Simulation | `SIMULATED_SOURCE`, `SIMULATED_METER` | Built in |

See [hardware and transport](docs/reference/hardware.md) for USB/USBTMC/GPIB, official VISA, adapter requirements, and model limitations. Stable downloads do not include the additional preview drivers.

## Quick start

1. Choose the stable release or explicitly opt into the preview. Install Setup or extract the complete portable package.
2. Start with the simulated source and meter to explore the interface.
3. Review source/meter identities, connection routes, ranges, and protection settings before connecting a load.
4. Choose a sweep and output location, then Start. Monitoring and mode selection do not enable source output.

Python/Jupyter access: [client guide](bindings/python/README.md). Detailed measurement semantics: [application guide](docs/guides/application-guide.md).

## Development

Run commands from the repository root. The root `Cargo.toml` and `Cargo.lock`
are the workspace entry point; product packages live under `crates/`.

```sh
cargo fmt --all --check
cargo test --workspace --locked
cargo build --release --locked
cargo run --locked --bin kohdalab-iv
```

```text
crates/       Rust desktop, measurement core, and transport
bindings/     Thin Python/Jupyter client
native/       Isolated native helpers and upstream notices
packaging/    Windows installer and driver-setup assets
scripts/      Build, package, and validation entry points
specs/        Reviewed instrument profiles and measurement specifications
tests/        Packaging and distribution-policy tests
docs/         Guides, development notes, references, validation, images
```

See [repository layout](docs/development/repository-layout.md) and
[CONTRIBUTING.md](CONTRIBUTING.md) before changing device or build boundaries.
Tests must not connect hardware, move stages, enable outputs, or alter drivers.

## Documentation

- [Documentation index](docs/README.md): the entry point for all guides and references.
- `docs/guides/`: installation and everyday operation.
- `docs/development/`: architecture, extension points, and build/release maintenance.
- `docs/reference/`: hardware, protocols, formats, and compatibility contracts.
- `docs/validation/`: versioned evidence and measured limitations.
- `docs/images/`: repository screenshots and figures.

## KohdaLab applications

[IV](https://github.com/woolen-yarn/kohdalab-iv) · [TRKR](https://github.com/woolen-yarn/kohdalab-trkr) · [Camera](https://github.com/woolen-yarn/kohdalab-camera)

The applications share a root Cargo workspace, `crates/` source packages, focused
build scripts, and the same documentation categories. Hardware backends and
qualification remain product-specific.

## License

Project code is [MIT licensed](LICENSE). Native helpers and redistributed
dependencies retain their own licenses; keep the accompanying notices and source.
See [redistribution terms](REDISTRIBUTION.md), [code signing](CODE_SIGNING_POLICY.md), and [privacy](PRIVACY.md).
