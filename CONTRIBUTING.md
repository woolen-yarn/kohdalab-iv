# Contributing

Thank you for improving KohdaLab IV. This project controls real laboratory
instruments, so changes should favor safe behavior, reproducible measurements,
and clear operator feedback.

## Development Setup

Use `uv` from the repository root:

```powershell
uv sync --all-extras --group dev
```

Run the hardware-free test suite:

```powershell
uv run pytest
```

Check the active configuration without starting a measurement:

```powershell
uv run kohdalab-iv --config config/default.json check-config
```

## Change Guidelines

- Keep measurement workflow logic in `src/kohdalab_iv/api`.
- Keep instrument-specific command details in `src/kohdalab_iv/instruments`.
- Keep GUI code thin; it should call the public API instead of duplicating
  measurement behavior.
- Add or update tests for sweep generation, units, output rows, safety behavior,
  and device session behavior.
- Prefer hardware-free tests first. Use real instruments only for smoke tests.

## Hardware Changes

For new instruments, document:

- model and transport
- tested firmware or command mode if known
- supported source/measure modes
- range and resolution limits
- local/remote release behavior
- a small safe smoke-test procedure

## Pull Requests

Before opening a pull request:

```powershell
uv run pytest
uv run kohdalab-iv --config config/default.json check-config
```

If the change affects real hardware behavior, include the hardware used and the
smallest range tested.
