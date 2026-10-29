# Manual DC range selection — alpha.16 validation

This note separates implementation evidence from physical-test assumptions for the
`1.3.0-alpha.16` preview. It is not a calibration record and does not claim
new independent measurements.

## Scope

The preview replaces automatic manual-DC range changes in the GUI with an
explicit documented source range. The selected range determines the accepted
level and ramp grid, the displayed unit/precision, and the programmed native
range command. While output is ON, a request to select another explicit range
is rejected and the output path remains fail-safe.

The GUI presents the range selector with left/right arrows around its centred
display in the DC controls. Source and Meter cards retain the compact Device → live → settings
flow. Output and Zero are equal-width controls; Zero is an ordinary in-range
level request and does not itself mean Output OFF.

## Automated evidence

The repository's mock transport and simulation tests cover command ordering
and safety boundaries without opening a physical instrument or changing a
driver. In particular, the alpha.16 regression coverage checks that:

- a physical-mock cross-range request verifies OFF before the native range
  write and enables output only afterward;
- a value change within a held range, including Zero, does not rewrite OFF,
  ON, or the range command;
- an explicit range change while ON is rejected and fails safe;
- an unsupported range, an off-grid level, or a GS210 divider range that would
  weaken requested protection does not enable output;
- legacy commands without an explicit `range` continue to use the existing
  compatibility path.

These are automated mock/simulation results. They verify software behavior and
recorded command order, not electrical accuracy, terminal voltage, or an
instrument's calibration.

## Physical verification scope

At the user's direction, publication proceeds without a new all-device physical
retest. Physical operation across supported devices is assumed for this release;
it is not an independently verified pass recorded by this validation. No new
per-model settings, readings, ranges, or calibration results were collected.

Supported-model scope remains the catalog in `specs/sources.toml` and
`specs/meters.toml`; availability and model limitations are listed in the
root [README](../../README.md#supported-hardware--対応機器). Follow the
[manual DC operation reference](../reference/manual-dc-output.md), verify
wiring and protection at the instrument, and start with a safe unloaded or
known load condition appropriate to the experiment.
