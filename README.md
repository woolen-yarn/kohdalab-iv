# KohdaLab IV

[![Test](https://github.com/woolen-yarn/kohdalab-iv/actions/workflows/test.yml/badge.svg)](https://github.com/woolen-yarn/kohdalab-iv/actions/workflows/test.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)
![Python](https://img.shields.io/badge/python-3.10%2B-blue)

KohdaLab IV is a Python toolkit for DC I-V and V-I measurements from CLI, GUI, and Jupyter Notebook workflows.

## What It Does

- Controls source and meter instruments for DC sweep measurements.
- Supports I-V and V-I modes with shared measurement logic.
- Saves measurement results as CSV for simple analysis and archival.
- Provides GUI, command-line, and notebook entry points.

## Quick Start

For a new PC, start with the setup guide:

- [Initial setup](docs/initial_setup.md)

After setup:

```powershell
uv sync --all-extras --group dev --frozen
uv run kohdalab-iv check-config
uv run --extra gui kohdalab-iv-gui
```

The default configuration is installed inside the Python package. To use a
lab-specific file, pass `--config PATH` or set `KOHDALAB_IV_CONFIG`.

Run checks:

```powershell
uv lock --check
uv run ruff check .
uv run pytest --cov --cov-branch
```

## Documentation

- [Initial setup](docs/initial_setup.md): install Git, GitHub CLI, uv, clone the repository, and verify the environment.
- [Usage guide](docs/usage.md): detailed GUI, CLI, notebook, instrument, and safety behavior notes.
- [Windows setup](docs/windows_setup.md): Windows instrument-PC preparation notes.
- [Roadmap](ROADMAP.md): planned milestones.
- [Safety notes](SAFETY.md): safety assumptions and operator responsibilities.
- [Contributing](CONTRIBUTING.md): development workflow and pull request expectations.

## Project Status

`v0.1.0` is the repository baseline: licensing, CI, branch protection, dependency maintenance, and documentation structure are in place. Measurement-core hardening and simulated drivers are planned next.

## License

MIT. See [LICENSE](LICENSE).
