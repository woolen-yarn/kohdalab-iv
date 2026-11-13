# GUI API v2 validation

Validated on macOS arm64, 2026-10-28, using the development build. No physical
instrument was connected, configured or energized by these checks. Windows
runtime/hardware validation is still required before a release claim.

## Automated checks

- Rust workspace: 170 tests passed, including existing mock-transport measurement,
  source protection, meter limits, shutdown ordering, Dual planning and recording
  tests. Final GUI unit suite: 75 tests passed.
- Python client and Notebook suite: 42 tests passed against the built Rust CLI.
- `cargo fmt`, workspace/all-target Clippy with warnings denied, diff whitespace
  check, and documentation links passed.
- Independent review: ownership, tagged job completions, shutdown paths, UUID
  conflicts and engine-loss recovery were reviewed; identified material findings
  were fixed and rechecked.

## Executed GUI checks

Both scripts create an isolated user profile and select simulation devices only.
Their machine-specific outputs remain in the ignored `validation/` directory.

`verify_gui_control.py` verified exclusive control, expired-heartbeat Stop/OFF,
rejection of gate changes during IV, immediate Gate OFF while measurement is
busy, explicit Stop, two gate/IV sequences with 41 points each, normal gate-bias
retention between successful runs, all-output OFF on context exit, CSV row counts
and a GUI screenshot.

`verify_gui_api.py` verified explicit profile launch and PID-matched discovery,
read-only inspection/close, exclusive acquisition, staged device connection,
plan/output changes, same-UUID retry without a second measurement, conflicting
UUID rejection, exact lookup of an earlier result after a subsequent run, V-I to
I-V switching, three-point CSVs with the same 18 columns, voluntary safe release
and reacquisition, disconnect, configuration loading and profile replacement.

Session API job history is intentionally excluded from measurement `.run.json`
outcomes. It does not alter the CSV/raw CSV field contracts or add external
experimental parameters to the GUI or saved measurement schema.
