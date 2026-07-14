# Changelog

## Unreleased

## [0.2.1] - 2026-07-14

### Added

- Added public Python API examples and a conservative real-hardware smoke-test guide.
- Added GUI run-state, measurement-boundary, instrument-edge, and compatibility regression tests.
- Added versioned config files and a packaged JSON Schema for editor and external-tool validation.
- Added a guarded version-bump command with dry-run support and release-metadata synchronization.
- Added a tag-gated release workflow that verifies, archives, and attaches distributions to a draft GitHub Release.
- Added one cross-platform project-check command shared by local development, CI, and release builds.
- Added `init-config` for safely creating an editable local config from the packaged default.
- Added CLI version reporting and text/JSON `doctor` diagnostics for config and VISA troubleshooting.
- Added shared Ruff formatting, EditorConfig, and commit/push hooks for consistent local and CI checks.
- Added locked dependency auditing and a weekly security workflow; updated Pillow to the patched release identified by the first audit.
- Added Mypy source checking to local quality checks, commit hooks, and CI.

### Changed

- Raised the supported Python baseline to 3.13 and removed the Python 3.10 compatibility layer and CI jobs.
- Expanded the README with configuration resolution, supported instruments, local installation, and development verification.
- Updated setup, usage, measurement-specification, and contribution guides to use the packaged default and explicit local configs.
- Removed the duplicated Python default so normalization, GUI, CLI, Notebook, and tests share the packaged JSON as the single source of truth.
- Raised enforced statement and branch coverage to 100%.
- Simplified unreachable configuration, scan, GUI, ADCMT, Keysight, and VISA branches without changing supported behavior.

## [0.2.0] - 2026-07-13

### Added

- Added simulated source and meter drivers for hardware-free measurements.
- Added config preflight validation and full-provenance CSV output.
- Added end-to-end measurement and safety-cleanup regression tests.
- Added hardware-free CLI and Notebook regression tests.
- Added release-metadata and distribution-artifact verification.
- Added CLI and packaging smoke tests to CI.
- Added package metadata, linting, coverage, and packaging-focused regression tests.
- Added project governance and safety baseline files.
- Added GitHub issue and pull request templates.
- Added CI and Dependabot configuration.

### Changed

- Unified CLI, GUI, and Notebook configuration-path resolution.
- Expanded CI across Python 3.10 and 3.13 on Ubuntu and Windows.
- Moved the default configuration into the installable package.
- Derived the runtime version from installed package metadata and displayed it in the GUI title.
