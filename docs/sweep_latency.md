# Zero-wait sweep latency

`Wait time = 0` removes the programmed per-point settling delay. It does not remove instrument conversion time, NPLC, per-point averaging (Repeat), source ramping, first-point settling, communication or CSV persistence.

The optimization in 1.3.0-alpha.2 preserves each acquired sample, each-sample compliance checking, source readback, per-point CSV flush, and autorange tracking. After every conversion it still queries the meter range. It reuses the queried measurement resolution only while that range remains unchanged within one configured run. The cache is discarded on range changes, unknown metadata and failures, and is never carried from idle monitoring or a preceding sweep. NPLC/function stay fixed during a sweep. Changes at the instrument front panel while a run owns the devices are outside this cache's operating assumptions.

For 21 points with Repeat=10 in a stable range, the previous implementation issued 210 range and 210 resolution queries. The revised implementation issues 210 range queries and one resolution query. READ?, source readback, safety checks and CSV writes remain unchanged. A range change refreshes resolution, so precision remains conservative when averaging conversions from different ranges.

Protocol integration tests verify four actual READ? requests, four range queries, one resolution query, preserved CSV resolution and verified output OFF for a fixed-target Repeat=4 run. Separate tests cover range changes, cache invalidation, new-run reset and zero-wait cancellation without an extra live conversion.

These are communication-count results, not a physical-instrument speed measurement. Hardware GPIB/USB timing was not measured for this change. Lowering Repeat or NPLC can further change acquisition time but also changes the measurement; no saved scientific settings are changed by this optimization.
