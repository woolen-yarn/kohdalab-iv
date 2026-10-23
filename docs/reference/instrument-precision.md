# Instrument precision and display policy audit / 計測器精度と表示ポリシー監査

> **日本語の案内:** 精度、桁数、測定上の制約は機種・条件に依存します。数値と一次資料への参照は
> 英語原文のまま確認してください。

Reviewed on 2026-10-14. The authoritative per-model configuration is in
`specs/sources.toml` and
`specs/meters.toml`. Each model now has a
`display_precision` table with provenance, policy, review status and limitations.
Rust embeds those files; its setting-resolution table and display query
commands are no longer independently hardcoded. Parsing is cached.

## What the numbers mean

Keep these four concepts separate: source setting increments, configured
measurement resolution, measurement uncertainty/accuracy, and panel display
format. Neither advertised digit counts nor the length of a SCPI response
establishes measurement uncertainty. A resolution-based display is a conservative
formatting policy, not a guarantee of an identical front-panel string.

The native display rounds to a decimal place no finer than the available
resolution. For example, a resolution of 1.5 nA leads to the 10 nA decimal place;
it does not mean a 1.5 nA quantization lattice. Zero displays without a negative
sign. Unknown resolution is explicitly marked and does not borrow another
model's specification. Plot coordinates, fitting, protection comparisons and CSV
values retain the original floating-point values. Native CSV uses shortest-round-trip
scientific notation to preserve finite f64 values; this is numerical
persistence, not a claim of 17 physically meaningful digits.

## Reviewed models

| Model | Resolution policy | Evidence and caveats | Native desktop |
| --- | --- | --- | --- |
| Yokogawa GS210 | Match the queried source range to the existing TOML voltage/current tables | Source-setting increments verified against manual section 5.2; programmed level readback is not an independent measured output | Enabled; precision-query hardware check pending |
| Yokogawa 7651 | Match legacy range codes to its own TOML tables | All source increments agree with official specifications; range codes must not be treated as GS210 SCPI range values | Not enabled |
| Agilent 34401A | Read current DC range and configured resolution | Manual documents function-specific resolution queries in V/A and NPLC settings; panel formatting still separate | Not enabled |
| Agilent / Keysight 34411A | Read DC range and configured resolution after each reading | Shared SCPI policy for both brands; NPLC and autoranging affect resolution; queried resolution is not certified panel formatting | Enabled; precision-query hardware check pending |
| Keysight 34465A | Model-specific SCPI range/resolution policy | Integration-time capabilities depend on DIG option/firmware; existing legacy `nplc_values` must not be interpreted as universally available | Not enabled |
| ADCMT 7461A | Unknown until its programming protocol is reviewed | Official product specifications give best-case values and integration dependence; no assumed Keysight-compatible resolution query | Not enabled |
| Simulated source / meter | Explicit illustrative TOML increments | No hardware or uncertainty claim; simulator numbers are not real-instrument fallback values | Enabled |

### GS210

Voltage range / setting increment: 10 mV / 100 nV; 100 mV / 1 µV;
1 V / 10 µV; 10 V / 100 µV; 30 V / 1 mV.
Current range / setting increment: 1 mA / 10 nA; 10 mA / 100 nA;
100 mA / 1 µA; 200 mA / 1 µA. Protection-limit increments are separate.
These are output-setting increments, not the resolution of the optional /MON
measurement. The native engine reads the programmed level, not /MON.
Source: [GS200/GS210 user manual, section 5.2](https://www.yokogawa.com/pdf/provide/E/GW/IM/0000023941/0/IMGS210-01EN.pdf).

### 7651

Voltage increments match the GS210 list above, but the 7651 has its own range
codes (2–6), not the GS210 numeric range commands. Current ranges are 1, 10 and
100 mA with 10 nA, 100 nA and 1 µA increments; there is no GS210-style 200 mA
range. Source: [official 7651 specifications, page 1](https://cdn.tmi.yokogawa.com/7651spec.pdf).

### 34401A

The manual describes resolution in measurement units and supports DC NPLC
settings 0.02, 0.2, 1, 10 and 100. Query the selected function instead of deriving
an absolute current increment from an advertised 6½-digit capability.
Source: [34401A user guide, Measurement Configuration Commands](https://www.keysight.com/us/en/assets/9018-01063/user-manuals/9018-01063.pdf).

### 34411A: Agilent and Keysight

The reviewed commands are `SENS:VOLT:DC:RANG?`, `SENS:VOLT:DC:RES?`,
`SENS:CURR:DC:RANG?` and `SENS:CURR:DC:RES?`. Querying is read-only and must
follow `READ?` when autoranging is enabled. The engine queries after each sample;
when averaging, display precision uses the coarsest encountered resolution and
never adds digits merely because the average count increased.
Sources: [SCPI comparison, SENSE subsystem](https://www.keysight.com/us/en/assets/7018-02044/white-papers-archived/5990-3515.pdf)
and [34410A/34411A data sheet](https://www.keysight.com.cn/cn/zh/assets/7018-01326/data-sheets/5989-3738.pdf).

The data sheet's integration-time table defines resolution using typical
10 V DC RMS noise. Applying that ppm table indiscriminately to every current
range, or calling it a front-panel digit table, would be incorrect. Consequently,
the profile records the read-only configured-resolution queries and does not
invent a universal NPLC-to-display-digit table. Literal panel matching remains
unverified. The two metadata queries add instrument communication per sample;
correct autorange metadata takes priority over a speculative cached range.

### 34465A

Treat this as a separate profile, not a renamed 34411A. The official guide lists
integration-time settings dependent on model options; faster settings are not
unconditional. Native support is still disabled. Sources:
[Truevolt operating/service guide](https://www.keysight.com/us/en/assets/9018-03876/service-manuals/9018-03876.pdf)
and [Truevolt data sheet](https://www.keysight.com/gb/en/assets/7018-03846/data-sheets/5991-1983.pdf).

### ADCMT 7461A

The official product information gives best-case DCV resolution 100 nV and DCI
resolution 1 nA, but describes reduced display digits at high sampling speeds.
Those minima are not universal increments. Its programming manual download
requires a form; the command-level review is therefore explicitly pending.
The profile records no speculative `RES?` query or universal fixed digit count.
Source: [ADC 7461A product specifications](https://www.adcmt.com/en/products/dmm/7451).

## Validation and outstanding work

Tests cover the shared source tables, integer legacy range codes, both 34411A
brand profiles, current/voltage query selection, unknown-device behavior and
simulation-only policies. Existing tests preserve unrounded CSV values when
display metadata changes. Prior mock-instrument checks covered autoranging,
averaging, cancellation and verified output-off. Those checks are not hardware
validation or calibration.

Before claiming literal front-panel matching: compare queried ranges/resolutions
and the actual front-panel display on supported firmware at several NPLC/range
settings. For the ADCMT, first review its official programming manual. Do not
silently enable unsupported native devices because metadata has been documented.

## 日本語 — 全文対訳

2026-10-14に確認した記録です。機種別設定の正本は`specs/sources.toml`と`specs/meters.toml`です。各機種の`display_precision`に由来、方針、確認状態、制約を記録します。Rustはこのファイルを埋め込み、設定分解能表と照会命令を個別にhardcodeしません。解析結果はcacheします。

### 数値の意味

出力設定の刻み、設定された測定分解能、測定の不確かさ・確度、表示形式の4つを区別してください。広告の桁数やSCPI応答長は不確かさを確立しません。分解能に基づく表示は保守的な書式方針で、実機パネル文字列との一致保証ではありません。

表示は利用可能な分解能より細かくない小数位へ丸めます。1.5 nAの分解能なら10 nAの位であり、1.5 nA間隔の量子化ではありません。ゼロに負号を付けません。未知の分解能は明記し、他機種の仕様を借用しません。プロット、フィット、保護比較、CSVは元の浮動小数値を保持します。CSVの最短往復可能指数表記は有限f64の保存方式であり、17桁が物理的に有意という主張ではありません。

### 確認した機種（この監査時点）

| 機種 | 分解能の方針 | 根拠・制約 | ネイティブGUI |
| --- | --- | --- | --- |
| Yokogawa GS210 | 照会した出力rangeを既存TOML電圧・電流表へ対応 | 設定刻みをマニュアル5.2節で確認。programmed levelは独立出力測定ではない | 有効。分解能照会の実機確認は未実施 |
| Yokogawa 7651 | 旧range codeを専用TOMLへ対応 | 全設定刻みは公式仕様と一致。GS210のSCPI数値rangeと同一視しない | 未有効化 |
| Agilent 34401A | DC rangeと設定resolutionを読む | マニュアルに機能別V/A照会とNPLC。パネル書式は別 | 未有効化 |
| Agilent / Keysight 34411A | 各読取後にDC range・resolution照会 | 両ブランド共通SCPI。NPLC・autorangeが影響。パネル書式の認証ではない | 有効。分解能照会の実機確認は未実施 |
| Keysight 34465A | 機種専用SCPI range・resolution | 積分時間はDIG option・firmware次第。旧`nplc_values`を常に利用可と見なさない | 未有効化 |
| ADCMT 7461A | プログラミングprotocol確認まで不明 | 公式値は最良条件と積分依存。Keysight互換照会を推測しない | 未有効化 |
| 模擬電源・測定器 | 説明用の明示的TOML刻み | 実機・不確かさの主張なし。実機fallback値に使わない | 有効 |

### GS210

電圧range/設定刻みは10 mV/100 nV、100 mV/1 µV、1 V/10 µV、10 V/100 µV、30 V/1 mVです。電流は1 mA/10 nA、10 mA/100 nA、100 mA/1 µA、200 mA/1 µAです。保護制限の刻みは別です。これらは出力設定で、任意の/MON測定の分解能ではありません。エンジンはprogrammed levelを読み、/MONは読みません。一次資料は上記GS200/GS210 manual5.2節です。

### 7651

電圧刻みは上のGS210表と一致しますが、range codeは専用の2–6で、GS210数値range命令ではありません。電流rangeは1・10・100 mA、刻みは10 nA・100 nA・1 µAで、GS210のような200 mAはありません。一次資料は上記7651公式仕様1ページです。

### 34401A

マニュアルは測定単位でのresolutionとDC NPLCの0.02・0.2・1・10・100を記載します。6½桁という広告から絶対電流刻みを導かず、選択機能を照会します。一次資料は上記34401A user guideのMeasurement Configuration Commandsです。

### Agilent・Keysight 34411A

確認した命令は`SENS:VOLT:DC:RANG?`・`SENS:VOLT:DC:RES?`・`SENS:CURR:DC:RANG?`・`SENS:CURR:DC:RES?`です。読取専用で、autorange時は`READ?`後に行う必要があります。各点後に照会し、平均時は最も粗いresolutionを表示に使い、平均回数だけを理由に桁を増やしません。資料は上記SCPI比較SENSE節と34410A/34411A data sheetです。

data sheetの積分時間表は典型的10 V DC RMS noiseによるresolutionです。そのppm表を全電流rangeへ無条件適用したりパネル桁表と呼んだりするのは誤りです。このため読取専用の設定resolution照会を記録し、普遍的NPLC→表示桁表を創作しません。パネル完全一致は未検証です。点ごとにmetadata照会が2回増えますが、推測cacheより正しいautorange情報を優先します。

### 34465A

34411Aの改名扱いにせず別profileとします。公式guideの積分時間はmodel option依存で、高速設定は無条件ではありません。監査時点のnative対応は無効です。一次資料は上記Truevolt operating/service guideとdata sheetです。

### ADCMT 7461A

公式仕様の最良値はDCV100 nV、DCI1 nAですが、高速取得では表示桁が減ると記載しています。最小値は普遍的な刻みではありません。プログラミングmanual取得にformが必要なため、命令の確認は明示的に未了です。推測の`RES?`や普遍的固定桁数を記録しません。一次資料は上記ADC7461A製品仕様です。

### 検証と残件

共通出力表、旧整数range code、両34411Aブランド、電流・電圧照会選択、未知機器、simulation専用方針をテストします。表示metadata変更時も丸め前CSV値を保持する既存テストを維持します。以前の疑似機器確認はautorange、平均、キャンセル、出力OFFを扱いましたが、実機検証・校正ではありません。

パネル一致を主張する前に、対応firmwareで複数NPLC・rangeの照会値と実パネルを比較します。ADCMTは先に公式programming manualを確認します。metadataを文書化しただけで未対応native機器を有効化しません。
