# Native CSV column reduction audit

The native CSV uses 14 columns (`compact_v2`), with run metadata schema 3.
The 46-column compatibility API has been removed. `CsvWriter::create` writes the native 14-column schema.

| Former fields | Retained information |
| --- | --- |
| timestamp, elapsed_s | CSV |
| point_index, repeat_index, sweep_index, direction | CSV; retain repeat/branch identity for fitting and hysteresis |
| target_value, target_unit | CSV; requested source target, V for IV or A for VI |
| voltage_V, current_A | CSV; original finite f64 values in shortest round-trip scientific notation |
| voltage_origin, current_origin | CSV; distinguish meter, programmed source-level readback and source-setpoint fallback |
| software_limit_hit, status | CSV; point-level limit and status information |
| measurement, mode, sweep_pattern, source_function, measure_function | JSON config.mode/config.pattern; functions follow IV/VI mode |
| total_points | JSON outcome.total, including the initial pre-output metadata |
| source_target_value/unit, target_V/A | Duplicates or mode-specific projections of CSV target_value/unit |
| source_set_V/A | In the native engine source_set equals point.target after ramping; source-setting value is recoverable from target and mode |
| source_readback_value/unit, source_readback_V/A | Recover from voltage/current and origins according to mode; source_setpoint origin means readback was unavailable |
| meter_value/unit | Recover from current_A in IV or voltage_V in VI |
| measured_V/A | Duplicates of voltage_V/current_A in the former export; names did not imply two independent meter measurements |
| resistance_Ohm, conductance_S | Recompute V/I and I/V; undefined denominators remain undefined, not zero. These point ratios are distinct from the run's fitted resistance in JSON outcome.resistance |
| compliance | Previously duplicated software_limit_hit; it was not an independently queried hardware-compliance flag |
| instrument_status | Empty in the current backend; any future nonempty per-point values are retained in JSON instrument_status_events |
| source_ref, measure_ref, source_model, measure_model | JSON identities, resource addresses, simulation flags and last record; actual instrument identity/resource is retained rather than only the generic legacy alias |
| start_settle_s, settle_s, nplc, average_count | JSON config; immutable within one run |

## Source provenance matters

A GS210 programmed-level query is not an independent measurement of voltage or
current delivered to the load. The origin columns describe this explicitly:
`meter`, `source_readback`, or `source_setpoint`. Consumers must not treat both
voltage/current columns as independent DMM measurements. Source-setting
resolution is not /MON resolution or measurement uncertainty.

## Metadata durability

The CSV header and the JSON run settings are created before output is enabled.
Failure to create the settings file aborts preparation. Initial JSON includes
configuration, identities, resource addresses, expected point count, source
range setting and hardware compliance setting. `in_progress: true` means the
metadata has not been finalized; it does not indicate current physical output.

Final metadata is written to a separate temporary file, synchronized and renamed
over the initial JSON after output cleanup. If final writing fails, initial
settings remain and a warning is reported. Both CSV and JSON should be retained
together for reproducibility. Copying only the CSV omits configuration context.

CSV continues to flush each point. Range/resolution changes are indexed by point
in final JSON. An abrupt process/system failure can lose the final change history,
final outcome and latest diagnostics even though initial settings and flushed CSV
rows exist; this is not a crash-proof per-point metadata journal.

## Validation

Tests cover compact IV/VI column order, units, setpoint fallback origins,
negative-zero and finite f64 round trips, and limit/status preservation. A JSON
replacement-failure test verifies that initial settings survive a failed final
write. Integration checks cover initial metadata, completed/stopped runs,
autorange history and output cleanup. Full legacy CSV tests remain intact.
