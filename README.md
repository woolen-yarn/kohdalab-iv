# KohdaLab IV

[![Test](https://github.com/Kohdalab/kohdalab-iv/actions/workflows/test.yml/badge.svg)](https://github.com/Kohdalab/kohdalab-iv/actions/workflows/test.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)
![Python](https://img.shields.io/badge/python-3.13%2B-blue)

KohdaLab IV is a Python toolkit for reproducible DC I-V and V-I measurements from GUI, CLI, and Jupyter Notebook workflows.

## Portable desktop downloads

Download the portable ZIP for your computer (v0.2.3):

The Windows package was updated on 2026-10-07 with NI USB-HS+ initialization
and driver-removal recovery fixes. Full driver reinstallation took about
3 seconds on the test PC, down from about 70 seconds; timing varies by PC.

| Platform | Kohdalab download | Personal mirror |
| --- | --- | --- |
| macOS — Apple Silicon (arm64) | [Download Mac portable](https://github.com/Kohdalab/kohdalab-iv/releases/download/portable-v0.2.3/kohdalab-iv-0.2.3-macos-arm64-portable.zip) | [Download Mac portable](https://github.com/woolen-yarn/kohdalab-iv/releases/download/portable-v0.2.3/kohdalab-iv-0.2.3-macos-arm64-portable.zip) |
| Windows — 64-bit (x64) | [Download Windows portable](https://github.com/Kohdalab/kohdalab-iv/releases/download/portable-v0.2.3/kohdalab-iv-0.2.3-windows-x64-portable.zip) | [Download Windows portable](https://github.com/woolen-yarn/kohdalab-iv/releases/download/portable-v0.2.3/kohdalab-iv-0.2.3-windows-x64-portable.zip) |

For build kits, checksums and release notes, visit the
[Kohdalab release](https://github.com/Kohdalab/kohdalab-iv/releases/tag/portable-v0.2.3)
or the [personal release](https://github.com/woolen-yarn/kohdalab-iv/releases/tag/portable-v0.2.3).

Extract the entire ZIP and open the app. Supported native connections do not
require Python or VISA. Build kits are included in `portable/build-kits` and
as release assets. See [the portable guide](docs/portable_release.md) for
setup, supported connections and rebuild instructions.

## What It Does

- Controls supported source and meter instruments through a shared measurement API.
- Validates sweep targets, source resolution, ranges, NPLC, limits, and compliance before measurement.
- Supports linear, round-trip, zero-centered, and custom-point sweeps.
- Writes provenance-rich CSV rows continuously so partial results survive a controlled stop.
- Returns the source to zero and turns output off on normal finish, Stop, and configured error paths.
- Includes simulated instruments and hardware-free tests with 100% statement and branch coverage.
- Displays the installed package version in the GUI title.

## Quick Start

For a new PC, start with [Initial setup](docs/initial_setup.md).

For repository development:

```powershell
uv sync --all-extras --group dev --frozen
uv run pre-commit install --install-hooks
uv run kohdalab-iv init-config config/local.json
uv run kohdalab-iv --config config/local.json doctor
uv run --extra gui kohdalab-iv-gui
```

For a local installation without development tools:

```powershell
python -m pip install .
python -m pip install ".[gui]"
python -m pip install ".[notebook]"
```

The base package installs the API and CLI. GUI and Notebook dependencies are explicit extras, so headless instrument PCs and automation environments do not need Qt or Jupyter.

Use `kohdalab-iv --version` to report the installed version. The `doctor` command
reports the resolved config, scan plan, Python/platform details, and visible VISA
resources without starting a measurement. Add `--json` for support automation.

Run a complete hardware-free measurement:

```powershell
uv run kohdalab-iv --config src/kohdalab_iv/resources/simulated.json measure
```

The simulated profile models a 1 kOhm circuit and uses the same planning, session, cleanup, and CSV-writing path as a real measurement.

Run development checks:

```powershell
uv run python scripts/check_project.py quality
uv run --group package python scripts/check_project.py package
uv run --group audit python scripts/check_project.py audit
```

## Configuration

The packaged default is the single source of truth for the standard configuration. Resolution order is:

1. explicit `--config PATH`;
2. `KOHDALAB_IV_CONFIG`;
3. the last config selected in the GUI;
4. optional lab default from `KOHDALAB_IV_DEFAULT_CONFIG`;
5. packaged `src/kohdalab_iv/resources/default.json` during repository development.

Do not edit an installed package's bundled JSON. Create a local editable copy with `kohdalab-iv init-config config/local.json`, update VISA resource strings and safety limits, then select it with `--config` or the GUI. Existing files are never replaced unless `--force` is provided. See [Usage](docs/usage.md) for the schema and operating details.

Configs use `"config_version": 1`. Legacy files without the field are normalized to version 1; newer unsupported versions fail before hardware access. The packaged [JSON Schema](src/kohdalab_iv/resources/config.schema.json) supports editor completion and independent validation.

## Supported Instruments

Sources:

- Yokogawa GS210
- Yokogawa 7651
- simulated source

Meters:

- Agilent/HP 34401A
- Agilent 34411A
- Keysight 34411A and 34465A
- ADCMT 7461A using GPIB SCPI or USB/ADC commands
- simulated meter

Real hardware operation requires the checks in [Safety notes](SAFETY.md) and the [Hardware smoke-test guide](docs/hardware_smoke_test.md).

## Documentation

- [Initial setup](docs/initial_setup.md): install tools, clone, sync, and verify the environment.
- [Usage guide](docs/usage.md): GUI, CLI, config, instruments, CSV, and safety behavior.
- [API usage](docs/api_usage.md): public Python API and callback examples.
- [I-V measurement specification](docs/iv_measurement_spec.md): scan-plan and output-field details.
- [Hardware smoke test](docs/hardware_smoke_test.md): conservative first-run procedure for real instruments.
- [Windows setup](docs/windows_setup.md): VISA and instrument-PC preparation.
- [Standalone apps](docs/standalone.md): build Windows and Mac apps with Python included.
- [Roadmap](ROADMAP.md): planned milestones.
- [Safety notes](SAFETY.md): operator responsibilities and safety assumptions.
- [Contributing](CONTRIBUTING.md): architecture boundaries and development checks.
- [Changelog](CHANGELOG.md): released and upcoming changes.

## Project Status

The current development version is `0.2.3`. It includes simulated end-to-end measurements, configuration preflight validation, provenance-rich CSV output, GUI run-state protection, cross-platform Python 3.13 CI, release-artifact verification, and 100% statement and branch coverage.

## License

MIT. See [LICENSE](LICENSE).
