# Changelog

## Unreleased

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
