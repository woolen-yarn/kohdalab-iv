# Changelog

## Unreleased

- Fix Windows USB installer log sharing so SetupAPI log replacement no longer causes repeated 34-second waits.

- Fix native NI GPIB-USB-HS+ initialization on Windows when only the communication interface has WinUSB. Omit the optional analyzer-interface request; retain device/LED initialization and all required-transfer error handling. macOS behavior is unchanged.
- Recheck Windows USB drivers before Refresh, recover missing NI communication device nodes after uninstall, and wait for driver activation before scanning. Failed setup does not start instrument discovery.

## Portable v0.2.3 (2026-10-05)

- Publish matching macOS arm64 and Windows x64 portable distributions, English documentation, source build kits and checksums. Preserve the original Python v0.2.3 release.

- Simplify the Windows user distribution to a root GUI executable, README.txt, runtime and support folders; archive corresponding source and keep diagnostics/licenses in support. Validate the relocated GUI before packaging.

- Verify actual instrument identity before accepting hardware connections, reject mismatched Device/Resource selections on USB and GPIB, and close unverified handles without model-specific output commands.

- Avoid external vendor VISA discovery in the native portable, and display Refresh phases and elapsed seconds. Verify timeout recovery and repeated scans.

### Added

- Add PyInstaller build scripts and instructions for standalone GUI and CLI apps with Python included, with a writable per-user working directory for the frozen GUI.
- Add an optional persistent NI USB-GPIB userspace helper for Apple Silicon builds, with shared device access, configured timeouts, addressed local control, and explicit source-distribution instructions.
- Add a bundled direct USBTMC helper for the Agilent 34411A, including serial selection and USB descriptor discovery.
- Discover unregistered GPIB listeners at primary addresses 1–30 through the NI helper, and run Refresh in a background thread with measurement/discovery coordination.
- Add an Agilent 82357B userspace transport for macOS with automatic adapter selection, checked FX2 firmware loading, primary listener discovery, and an initial firmware download script. Identification has been verified on a remote Apple Silicon Mac.
- Check physical ATN on 82357B and interpret write-completion counts without an undocumented response prefix; include raw status diagnostics for remote verification.
- Release the 82357B external TI reset before AUX register setup so reopening after close can initialize the GPIB controller; verify repeated reopen/query cycles with a reset-aware USB simulator.
- Accept the physical 82357B's six-byte transfer-status response while preserving completion and exact byte-count checks; reject responses that omit any byte of the count.
- Release and verify ATN after GPIB addressing and before Agilent NO_ADDRESS data send/read, matching the Linux-GPIB common-layer standby transition.

- Support direct USB for GS210/GS200 alongside 34411A, with per-device USB ID and serial matching, and verified two-session isolation in native mock tests.
- Prepare an English macOS arm64 portable release package with source, licenses, diagnostic tools, USB/GPIB profiles, release notes and SHA256 checksums.
- Add a Windows x64 native build kit with statically linked USBTMC/NI/82357B helpers, Windows pipe handling, SHA256-verified firmware setup, WinUSB instructions, corresponding source and build-time executable checks. Windows hardware validation remains pending.
- Add a dedicated Windows USB setup candidate using a replaceable libwdi DLL, fixed instrument/adapter selection, exact-instance driver assignment, automatic first-launch checks and 82357B firmware/staging. Real Windows installation remains unverified.

### Fixed

- Avoid register polling between 82357B standby and receive requests, resolving observed GS210 first-byte loss on the remote Mac.
- Widen all dropdown menus only while open, preserving original input and panel widths.

- Provide identification and connection status for simulated instruments so the GUI can connect them without errors.
- Populate resource selectors from native USB discovery even when no external VISA runtime is installed, merge available VISA resources, and retain manually entered addresses when refreshing.

## [0.2.3] - 2026-09-15

### Changed

- Move Wait time, Average count, and both Compliance limits into a popup opened by the Timing / Compliance button below Step, retaining the original dark theme.
- Apply popup edits with OK, restore values with Cancel or close, and prevent editing during measurements. Config Save persists the applied settings.
- Consolidate pending dependency updates: IPython 9.15.0, Matplotlib 3.11.1, pre-commit 4.6.1, Ruff 0.16.0, Twine 7.0.0, setuptools >=83.0.0, and actions/upload-artifact v7.

## [0.2.2] - 2026-09-15

### Added

- Added editable GUI current and voltage stop thresholds, defaulting to 1 mA for voltage sourcing and 1 V for current sourcing.
- Preserve both thresholds and their units across mode changes, config saves, and reloads; retain legacy compliance values.
- Validate positive finite compliance settings and test positive/negative threshold boundaries and output cleanup.

### Changed

- Store the editable GUI default at `~/.kohdalab/config/iv.json`, alongside TRKR configs with a separate filename.
- Remember the selected config in `~/.kohdalab/last_iv_config.json`, retain legacy-path fallback, and support shared `KOHDALAB_STATE_DIR` and home-directory expansion.
- Document averaged-reading software stop behavior and the distinction from instrument hardware protection.
- Update locked cryptography, JupyterLab, pip, and Tornado dependencies to address the release-time vulnerability audit.

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

- Show the Windows GUI before checking USB drivers, with asynchronous setup, phase messages and elapsed seconds. Keep the GUI open after setup failure or cancellation.
