# Zero-wait sweep latency / 待機時間ゼロのスイープ遅延

> **日本語の案内:** この短い記録の計測条件と値は、後から変更しない検証証跡です。

`Wait time = 0` removes the programmed per-point settling delay. It does not remove instrument conversion time, NPLC, per-point averaging (Repeat), source ramping, first-point settling, communication or CSV persistence.

The optimization in 1.3.0-alpha.2 preserves each acquired sample, each-sample compliance checking, source readback, per-point CSV flush, and autorange tracking. After every conversion it still queries the meter range. It reuses the queried measurement resolution only while that range remains unchanged within one configured run. The cache is discarded on range changes, unknown metadata and failures, and is never carried from idle monitoring or a preceding sweep. NPLC/function stay fixed during a sweep. Changes at the instrument front panel while a run owns the devices are outside this cache's operating assumptions.

For 21 points with Repeat=10 in a stable range, the previous implementation issued 210 range and 210 resolution queries. The revised implementation issues 210 range queries and one resolution query. READ?, source readback, safety checks and CSV writes remain unchanged. A range change refreshes resolution, so precision remains conservative when averaging conversions from different ranges.

Protocol integration tests verify four actual READ? requests, four range queries, one resolution query, preserved CSV resolution and verified output OFF for a fixed-target Repeat=4 run. Separate tests cover range changes, cache invalidation, new-run reset and zero-wait cancellation without an extra live conversion.

These are communication-count results, not a physical-instrument speed measurement. Hardware GPIB/USB timing was not measured for this change. Lowering Repeat or NPLC can further change acquisition time but also changes the measurement; no saved scientific settings are changed by this optimization.

---

# 待機時間ゼロのスイープ遅延

`Wait time = 0` はプログラムされた点ごとの整定遅延を除きます。計測器変換時間、NPLC、点ごとの平均化 (Repeat)、source ramping、first-point settling、通信、CSV 永続化を除くものではありません。

1.3.0-alpha.2 の最適化は、各取得 sample、各 sample の compliance check、source readback、点ごとの CSV flush、autorange tracking を保持します。各 conversion 後も meter range を問い合わせます。問い合わせた measurement resolution の再利用は、一回の configured run 内でその range が不変の間だけです。range change、unknown metadata、failure で cache を破棄し、idle monitoring や前の sweep から持ち越しません。sweep 中 NPLC/function は固定です。run が devices を所有中の instrument front panel 変更は、この cache の operating assumptions 外です。

stable range で Repeat=10、21 points の場合、以前の実装は 210 range queries と 210 resolution queries を発行しました。改訂実装は 210 range queries と一回の resolution query を発行します。READ?、source readback、safety checks、CSV writes は変わりません。range change は resolution を更新するため、異なる ranges の conversion を平均しても precision は保守的です。

protocol integration tests は fixed-target Repeat=4 run について、4 回の実 READ? requests、4 range queries、1 resolution query、保存した CSV resolution、検証済み output OFF を確認します。別テストは range changes、cache invalidation、new-run reset、extra live conversion のない zero-wait cancellation を扱います。

これらは communication-count の結果であり、physical-instrument speed measurement ではありません。この変更で hardware GPIB/USB timing は計測していません。Repeat または NPLC を下げれば acquisition time はさらに変わり得ますが、測定自体も変わります。この最適化で保存済み scientific settings を変更しません。
