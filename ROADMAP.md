# Roadmap

This roadmap tracks practical milestones for making KohdaLab IV easier to run, test, and extend across lab instruments.

## v0.1.0 - Project Baseline

- MIT license, contribution guide, security policy, safety notes, and changelog.
- GitHub issue templates, pull request template, CODEOWNERS, Dependabot, and CI on Ubuntu and Windows.
- Windows setup documentation for instrument PCs.
- Repository topics, labels, branch protection, and release metadata.

## v0.2.0 - Measurement Core Hardening

- Add hardware-free simulated source and meter drivers for development and CI.
- Preserve measurement metadata, compliance state, ranges, and instrument status in CSV output.
- Clarify user-facing I-V and V-I mode labels across CLI, GUI, and notebook workflows.
- Add regression tests for sweep generation, unit parsing, CSV schema, and stop/ramp safety behavior.

## v0.2.1 - Project Foundation Modernization

- Standardize development and CI on Python 3.13 with shared quality, type, audit, and package checks.
- Add versioned configuration, JSON Schema validation, config initialization, and environment diagnostics.
- Strengthen GUI state handling, measurement safety, and hardware-free regression coverage to 100%.
- Synchronize release metadata and automate guarded version bumps, artifact verification, and draft releases.

## v0.3.0 - Instrument Coverage

- Expand source and meter driver coverage from shared instrument capability interfaces.
- Document validated resource strings and known-good settings for each supported instrument.
- Add example configs for common KohdaLab measurement setups.

## v0.4.0 - User Workflow

- Provide notebook examples for simulated and real-device measurements.
- Improve GUI session recovery, recent config handling, and live measurement diagnostics.
- Add optional post-measurement summary plots and metadata reports.

## Later

- Consider plugin-style instrument discovery.
- Consider structured exports alongside CSV, such as JSON metadata sidecars.
- Consider remote instrument-PC execution workflows after the local Windows flow is stable.
