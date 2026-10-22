# Native CSV column reduction audit / ネイティブ CSV 列削減監査

> **日本語の案内:** 列名、単位、数値の直列化規則は互換性契約です。CSV の列名は翻訳せず、
> 本文の原表記を使用してください。

The native CSV uses 14 columns (`compact_v2`), with run metadata schema 4.
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

## Precision, resolution and uncertainty

The CSV serializes every finite in-memory `f64` using shortest round-trip scientific notation. This prevents formatting from discarding a value that the engine received or calculated; it does **not** claim that every displayed digit is experimentally significant. Instrument range/resolution, when an existing query provides it, is recorded as provenance. Resolution is not accuracy and neither it nor trailing zeroes in an instrument reply establishes uncertainty. Uncertainty remains explicitly unknown unless an instrument-specific, traceable uncertainty model is added.

Each new run also writes `<name>.readings.jsonl`. It is an append-only, flushed audit stream with the exact `READ?` response text for every acquired repeat, its parsed value or query/parse error, available range/resolution, raw source-level readback replies, identities/resources, and partial/compliance state. Simulation is labelled synthetic and never invents a hardware reply. Keep the CSV, `.run.json`, and `.readings.jsonl` together. Older CSV files lack this audit stream, so their original reply spelling and per-repeat provenance cannot be reconstructed.

## Metadata durability

The CSV header, readings audit and JSON run settings are created before output is enabled.
Failure to create the settings file aborts preparation. Initial JSON includes
configuration, identities, resource addresses, expected point count, source
range setting and hardware compliance setting. `in_progress: true` means the
metadata has not been finalized; it does not indicate current physical output.

Final metadata is written to a separate temporary file, synchronized and renamed
over the initial JSON after output cleanup. If final writing fails, initial
settings remain and a warning is reported. The CSV, JSON and audit should be retained
together for reproducibility. Copying only the CSV omits configuration and raw acquisition context.

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

## 日本語 — 全文対訳

ネイティブCSVは14列の`compact_v2`で、run metadata schemaは4です。46列の互換APIは削除済みで、`CsvWriter::create`はネイティブ14列を書き込みます。

| 旧フィールド | 保持する情報 |
| --- | --- |
| timestamp, elapsed_s | CSV |
| point_index, repeat_index, sweep_index, direction | CSV。フィット・ヒステリシス解析用の反復・分岐識別を保持 |
| target_value, target_unit | CSV。要求した出力目標で、IVはV、VIはA |
| voltage_V, current_A | CSV。有限f64の元の値を、最短の往復可能な指数表記で保存 |
| voltage_origin, current_origin | CSV。meter、プログラム済み出力値のreadback、目標値へのfallbackを区別 |
| software_limit_hit, status | CSV。点ごとの制限・状態 |
| measurement, mode, sweep_pattern, source_function, measure_function | JSONのconfig.mode/config.pattern。機能はIV/VIモードに従う |
| total_points | 出力開始前の初期metadataを含むJSONのoutcome.total |
| source_target_value/unit, target_V/A | CSV target_value/unitの重複またはモード別射影 |
| source_set_V/A | ネイティブではramp後のsource_setはpoint.targetに等しい。targetとmodeから設定値を復元可能 |
| source_readback_value/unit, source_readback_V/A | modeに応じてvoltage/currentとoriginから復元。source_setpointはreadback取得不能を示す |
| meter_value/unit | IVならcurrent_A、VIならvoltage_Vから復元 |
| measured_V/A | 旧形式でもvoltage_V/current_Aの重複で、独立した2つの測定を意味しない |
| resistance_Ohm, conductance_S | V/I・I/Vから再計算。分母が未定義なら0にせず未定義のまま。点ごとの比と、JSON outcome.resistanceのrun全体のフィットは別 |
| compliance | 以前はsoftware_limit_hitの重複で、独立照会した機器コンプライアンスフラグではない |
| instrument_status | 現backendでは空。将来の空でない点別値はJSON instrument_status_eventsに保持 |
| source_ref, measure_ref, source_model, measure_model | JSONに識別、リソース、simulationフラグ、最終recordを保持。旧汎用aliasだけでなく実際の機器・接続先を保持 |
| start_settle_s, settle_s, nplc, average_count | JSON config。1回のrun中は不変 |

### 値の由来

GS210のprogrammed-level照会は、負荷へ供給された電圧・電流の独立測定ではありません。origin列の`meter`・`source_readback`・`source_setpoint`で明示します。電圧と電流の両列を独立DMM測定として扱わないでください。出力設定分解能は/MONの分解能や測定不確かさとは異なります。

### 精度・分解能・不確かさ

CSVはengine内の有限`f64`を最短往復可能な指数表記で直列化します。これは取得・計算済みの値を表示書式で失わないためであり、全桁が実験的に有意であることを示すものではありません。既存の照会で得られる機器のrange・resolutionはprovenanceとして記録します。resolutionはaccuracyではなく、instrument replyの末尾zeroもuncertaintyを決めません。traceableな機器別uncertainty modelが追加されるまで、uncertaintyは明示的にunknownです。

新しいrunでは`<name>.readings.jsonl`も作成します。これはappend-onlyで各行をflushする監査streamで、各repeatの正確な`READ?`応答text、parsed valueまたはquery/parse error、利用可能なrange/resolution、source-level readbackのraw応答、identity/resource、partial/compliance状態を保持します。simulationはsyntheticと明記し、hardware replyを作りません。CSV・`.run.json`・`.readings.jsonl`は一緒に保管してください。旧CSVにはこのaudit streamがないため、元の応答表記やrepeatごとのprovenanceは復元できません。

### Metadataの耐久性

出力ON前にCSVヘッダー、readings audit、JSON設定を生成します。設定ファイルを作れなければ準備を中止します。初期JSONは設定、識別、リソース、予定点数、出力レンジ、機器コンプライアンス設定を含みます。`in_progress: true`はmetadataが未確定という意味で、実際の出力状態を示しません。

出力後処理後、最終metadataを別の一時ファイルへ書込み、同期して初期JSONへrenameします。最終書込みが失敗しても初期設定を残し警告します。再現性のためCSV・JSON・auditを一緒に保持してください。CSVだけでは設定・raw acquisition contextが欠けます。

CSVは各点でflushします。range・resolution変更は最終JSONに点番号付きで記録します。プロセス・OSの突然の停止では、初期設定とflush済みCSVがあっても、最終変更履歴・結果・最新診断を失う可能性があります。クラッシュに耐える点ごとのmetadataジャーナルではありません。

### 検証

compact IV/VIの列順、単位、setpoint fallbackのorigin、負のゼロと有限f64の往復、制限・状態保持をテストします。JSON置換失敗テストで最終保存失敗時にも初期設定が残ることを確認します。統合テストは初期metadata、完了・停止、autorange履歴、出力後処理を確認します。旧CSVの全テストも維持しています。
