# Native recording format / ネイティブ保存形式

> **日本語の案内:** 列名、単位、数値の直列化規則は互換性契約です。CSV の列名は翻訳せず、
> 本文の原表記を使用してください。

The native analysis CSV uses 18 columns (`analysis_v3`), with run metadata schema 6.
Each new run has a matching multiline-safe `<name>.raw.csv` and `<name>.run.json`.

## Three-file contract / 3ファイルの役割

New acquisitions produce the following files. Keep them together; the UUID joins
records and the finalized JSON stores SHA-256 checksums of both CSV files.
新しい測定は次の3ファイルを生成します。同じUUIDで対応付け、確定したJSONに両CSVのSHA-256を保存します。

| File / ファイル | Contents / 内容 |
| --- | --- |
| `<name>.csv` | One row per saved analysis point / 解析点ごとに1行 |
| `<name>.raw.csv` | Individual readings, commands/replies, configuration and cleanup events, errors and host timing / 個々の読取・コマンドと応答・設定と終了処理・エラー・ホスト時刻 |
| `<name>.run.json` | Requested/effective configuration, device identities/resources, observed display settings, optional `experiment` annotations, application version/available commit, outcome and checksums / 要求・実効設定、機器と接続先、取得した表示設定、任意の実験情報、アプリ版・取得可能なコミット、結果とチェックサム |

### Main CSV: all 18 columns / 解析CSVの全18列

| Columns / 列 | Meaning / 意味 |
| --- | --- |
| `run_id`, `point_index` | Run UUID and 1-based point number / 実行UUIDと1始まりの点番号 |
| `timestamp`, `elapsed_s` | Host completion of the last accepted conversion, UTC and monotonic seconds / 最後に受理した変換のホスト観測完了時刻（UTC・単調時計秒） |
| `sweep_index`, `direction` | Sweep number and forward/backward direction / スイープ番号と往復方向 |
| `target_value`, `target_unit` | Requested source level and V/A / 出力目標とV・A |
| `voltage_V`, `current_A` | Analysis values in SI units / SI単位の解析値 |
| `voltage_origin`, `current_origin` | `meter`, `source_readback`, or `source_setpoint` / 実測・設定読返し・目標値の由来 |
| `readings_requested`, `readings_completed` | Requested and accepted physical reading counts; completed is blank for simulation / 要求回数と受理した実機読取回数。simulationのcompletedは空欄 |
| `meter_stddev`, `meter_unit` | Sample standard deviation (ddof=1) of accepted readings and unit; blank when fewer than two readings or simulation / 受理した読取の標本標準偏差と単位。2回未満・simulationでは空欄 |
| `software_limit_hit`, `quality_flags` | Software limit and `normal`, `partial_average`, `software_limit_hit`, or `simulated` qualification / ソフトウェア制限と通常・部分平均・制限・合成の区別 |

`software_limit_hit` is not a queried hardware compliance flag. A limited partial
average retains its requested/completed counts even when its quality flag reports
the limit. Settings successfully sent are not automatically verified by readback.
`software_limit_hit`は機器から照会したコンプライアンス状態ではありません。制限による途中平均でも要求・完了回数を保持します。設定送信の成功を読返し検証済みとは扱いません。

### Raw CSV: shared 18-column contract / 共通raw CSVの全18列

| Columns / 列 | Meaning / 意味 |
| --- | --- |
| `run_id`, `event_index`, `point_index`, `reading_index` | Join key, event sequence, optional point/reading indices / 対応UUID、イベント連番、該当する点・読取番号 |
| `event_type`, `device_role`, `quantity` | Event, source/meter role and quantity / イベント、source・meterの役割、物理量 |
| `command`, `raw_response` | Original available decoded command/reply; CSV quoting preserves CR/LF, quotes and trailing zeroes / 取得できた原コマンド・デコード済み応答。改行・引用符・末尾ゼロを保持 |
| `value`, `unit` | Parsed individual value and unit when available / 取得できた個別値と単位 |
| `host_started_at_utc`, `host_completed_at_utc` | Host-observed logical operation window / 論理操作のホスト観測開始・終了UTC |
| `started_elapsed_s`, `completed_elapsed_s` | Corresponding monotonic seconds / 対応する単調時計秒 |
| `result`, `error` | Available result/error / 取得できた結果・エラー |
| `details_json` | Structured event-specific information (averaging windows, settings and output verification); not a duplicate main CSV row / 平均区間・設定・出力確認などの構造化情報。解析CSV行の複製ではない |

Blank means unavailable or not applicable, not zero. Raw events cover audited
operations, not every background transport operation. No instrument reply is
fabricated in simulation. Legacy exports keep their `.readings.jsonl` companion.
空欄は未取得・非該当であり0ではありません。rawは監査対象操作を記録し、全バックグラウンド通信を記録するものではありません。simulationで機器応答を捏造しません。旧形式の保存では`.readings.jsonl`を維持します。


| Former fields | Retained information |
| --- | --- |
| timestamp, elapsed_s | CSV |
| point_index, readings_completed, meter_stddev, quality_flags | CSV; retain accepted-reading count, ddof=1 dispersion, and quality state for each saved point |
| target_value, target_unit | CSV; requested source target, V for IV or A for VI |
| voltage_V, current_A | CSV; original finite f64 values in shortest round-trip scientific notation |
| voltage_origin, current_origin | CSV; distinguish meter, programmed source-level readback and source-setpoint fallback |
| status | IPC/metadata only; per-point CSV quality uses `quality_flags` and `software_limit_hit` |
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

`<name>.raw.csv` is an append-only, flushed evidence table. It preserves raw physical replies, commands, individual pre-average readings, timing windows, errors and structured residual details. Its CSV quoting preserves multiline replies exactly. The analysis CSV stores one row per saved point; its `meter_stddev` is the ddof=1 dispersion of accepted physical readings and is blank for fewer than two readings or simulation. Keep the analysis CSV, `.raw.csv`, and `.run.json` together. Old runs retain their `.readings.jsonl` when copied.

`timestamp` and `elapsed_s` in each saved CSV row describe host-observed completion of the final accepted meter conversion for that point. They are captured before later audit writes, display range/resolution work, source readback, and CSV disk I/O. The `raw_csv_v1` audit records microsecond RFC3339 UTC and monotonic elapsed start/completion times for each audited logical query, the first-to-last conversion window of an average, and the post-flush CSV write window. These are host timing observations, not an instrument clock; `device_timestamp_utc` is null unless a future verified device-clock interface is added. Simulation marks its time as synthetic logical host time.

## Metadata durability

The CSV header, raw CSV audit and JSON run settings are created before output is enabled.
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

Tests cover analysis_v3 IV/VI column order, units, setpoint fallback origins,
negative-zero and finite f64 round trips, and limit/status preservation. A JSON
replacement-failure test verifies that initial settings survive a failed final
write. Integration checks cover initial metadata, completed/stopped runs,
autorange history and output cleanup. Full legacy CSV tests remain intact.

## 日本語 — 全文対訳

ネイティブCSVは18列の`analysis_v3`で、run metadata schemaは6です。新しいrunはmultiline-safeな`.raw.csv`を作成し、旧runの`.readings.jsonl`は保存時に維持します。

| 旧フィールド | 保持する情報 |
| --- | --- |
| timestamp, elapsed_s | CSV |
| point_index, readings_completed, meter_stddev, quality_flags | CSV。受理済みreading数、ddof=1分散、quality状態を保持 |
| target_value, target_unit | CSV。要求した出力目標で、IVはV、VIはA |
| voltage_V, current_A | CSV。有限f64の元の値を、最短の往復可能な指数表記で保存 |
| voltage_origin, current_origin | CSV。meter、プログラム済み出力値のreadback、目標値へのfallbackを区別 |
| status | IPC・metadataのみ。CSVの点ごとの品質は`quality_flags`・`software_limit_hit` |
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

新しいrunでは`<name>.raw.csv`も作成します。これはappend-onlyで各行をflushする監査CSVです。`transport_exchange` entryは、legacy/ADC driverによるheader stripやdiscarded ADC conversionの前に、保存対象の変換・レンジ/分解能・出力readback照会内の通信応答を保持します。reading entryは正規化した値を保持し、通信エラーと最終エラーを追跡できます。監査対象の設定処理と出力終了確認も記録しますが、待機モニターを含む全通信の完全なトレースではありません。利用可能なrange/resolution、source readback、identity/resource、partial/compliance状態も保持します。simulationはsyntheticと明記し、hardware replyを作りません。CSV・`.run.json`・`.raw.csv`は一緒に保管してください。旧CSVにはこのaudit streamがないため、元の応答表記やrepeatごとのprovenanceは復元できません。

各保存CSV行の`timestamp`と`elapsed_s`は、そのpointで受理された最終meter conversionのhost観測完了を表します。後続のaudit write、表示用range/resolution、source readback、CSV disk I/Oより前に取得します。`raw_csv_v1` auditは、各監査対象logical query、averageのfirst-to-last conversion window、flush後CSV write windowについて、microsecond RFC3339 UTCとmonotonic elapsedの開始・完了時刻を記録します。これはhostの時刻観測でありinstrument clockではありません。検証済みのdevice-clock interfaceが追加されるまで`device_timestamp_utc`はnullです。simulationではsynthetic logical host timeと明記します。

### Metadataの耐久性

出力ON前にCSVヘッダー、raw CSV audit、JSON設定を生成します。設定ファイルを作れなければ準備を中止します。初期JSONは設定、識別、リソース、予定点数、出力レンジ、機器コンプライアンス設定を含みます。`in_progress: true`はmetadataが未確定という意味で、実際の出力状態を示しません。

出力後処理後、最終metadataを別の一時ファイルへ書込み、同期して初期JSONへrenameします。最終書込みが失敗しても初期設定を残し警告します。再現性のためCSV・JSON・auditを一緒に保持してください。CSVだけでは設定・raw acquisition contextが欠けます。

CSVは各点でflushします。range・resolution変更は最終JSONに点番号付きで記録します。プロセス・OSの突然の停止では、初期設定とflush済みCSVがあっても、最終変更履歴・結果・最新診断を失う可能性があります。クラッシュに耐える点ごとのmetadataジャーナルではありません。

### 検証

analysis_v3の18列、単位、setpoint fallbackのorigin、負のゼロと有限f64の往復、制限・状態保持をテストします。JSON置換失敗テストで最終保存失敗時にも初期設定が残ることを確認します。統合テストは初期metadata、完了・停止、autorange履歴、出力後処理を確認します。旧CSVの全テストも維持しています。
