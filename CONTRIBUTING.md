# Contributing

Thank you for improving KohdaLab IV. This project controls real laboratory instruments, so changes should favor safe behavior, reproducible measurements, explicit configuration, and clear operator feedback.

## Development Setup

From the repository root:

```powershell
uv sync --all-extras --group dev --frozen
uv run pre-commit install --install-hooks
```

Run the complete hardware-free checks:

```powershell
uv run python scripts/check_project.py quality
uv run --group audit python scripts/check_project.py audit
```

Coverage is enforced at 100% statement and branch coverage. New behavior requires tests for success, failure, and cleanup paths.

The commit hook checks changed Python files with Ruff, runs Mypy across the
source package, and verifies release metadata when relevant files change. The
push hook runs the complete shared
quality gate. Run every commit-stage hook manually with
`uv run pre-commit run --all-files`.

## Architecture Boundaries

- `src/kohdalab_iv/api`: configuration, scan planning, sessions, measurement workflow, rows, and public orchestration.
- `src/kohdalab_iv/instruments`: instrument-specific commands, ranges, transports, and local/remote behavior.
- `src/kohdalab_iv/apps`: GUI presentation and run-state coordination. Do not duplicate measurement logic here.
- `src/kohdalab_iv/resources`: packaged defaults and simulated profiles. The packaged default is the standard source of truth.
- `tests`: hardware-free unit, integration, GUI-offscreen, safety, packaging, and compatibility tests.
- `scripts`: release and distribution verification.

The CLI, GUI, and Notebook must call the same public measurement core. Keep config resolution, safety cleanup, and version reporting consistent across entry points.

## Configuration Changes

- Preserve compatibility with normalized existing config files where practical.
- Add defaults in `api/config.py` and update packaged JSON together.
- Validate unsafe or unsupported values before opening hardware.
- Never add a second repository-level default config.
- Document new keys in [Usage](docs/usage.md) and test missing, invalid, and valid values.

## Instrument Changes

Document and test:

- model and transport;
- known resource-string forms;
- supported source or measurement functions;
- range, resolution, NPLC, and compliance limits;
- command language or tested firmware when relevant;
- output-off, zero, local/remote, and close behavior;
- a conservative procedure in the [Hardware smoke-test guide](docs/hardware_smoke_test.md).

Prefer fake VISA handles and simulated devices for automated tests. Real instruments are for a final smoke test, not the primary regression suite.

## Documentation Changes

- Commands must work from the repository root.
- Use `src/kohdalab_iv/resources/default.json` only when explaining repository development; installed users should copy a config to a local path.
- Keep README concise and route operational details to `docs/`.
- Update `CHANGELOG.md` when behavior, supported hardware, config, or user workflow changes.

## Release Checks

Preview and apply a synchronized version update. This updates project and citation
metadata, README, and CHANGELOG only after validating the version, date, Roadmap,
and Unreleased entries:

```powershell
uv run python scripts/bump_version.py 0.3.0 --date YYYY-MM-DD --dry-run
uv run python scripts/bump_version.py 0.3.0 --date YYYY-MM-DD
```

Then run the release checks:

```powershell
uv run --group package python scripts/check_project.py all
```

The version in `pyproject.toml`, runtime package metadata, `CITATION.cff`, changelog, and Git tag must agree.

After the release commit is on `main`, create and push the matching tag:

```powershell
git tag v0.3.0
git push origin v0.3.0
```

The Release workflow repeats the quality and package checks, stores the wheel and
source archive for 30 days, and creates a draft GitHub Release with both files.
Review and publish that draft manually. PyPI publishing is intentionally not
automated.

## Pull Requests

When a PR is requested, include:

- user-visible behavior and motivation;
- tests and commands run;
- config or CSV compatibility impact;
- hardware used and smallest tested range, if hardware behavior changed;
- screenshots only when GUI behavior materially changed.

Do not include generated results, local configs, credentials, or machine-specific VISA details.
