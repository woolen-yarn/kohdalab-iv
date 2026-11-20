# Reference display / 参照計器の表示

**Introduced in 1.0.6. Package publication is tracked in the release notes.**
**1.0.6で追加。パッケージの公開状況はリリースノートを参照してください。**

## Sweep view / 掃引表示

Assign the optional reference Meter in Settings. In the central plot header,
choose **Reference display: Overlay / Separate**. Existing profiles default to
**Overlay**. With no reference assigned, the main plot remains a single panel.
Separate uses the same source reading/setpoint for both x axes; x zoom and pan
are linked, while y ranges and SI prefixes are independent. Assigned references
with no readings show a waiting panel; missing readings are gaps, never zero.

Settingsで参照Meterを割り当て、中央ヘッダーの **Overlay / Separate** を選びます。
既存設定の初期値はOverlayです。未割当なら主プロット一段のままです。
Separateでは上下の横軸に同じソース読値・設定値を使い、ズームとパンを同期します。
縦軸の範囲と単位接頭辞は独立です。未取得は待機表示、欠測は線の切れ目として扱い、0で埋めません。

| Mode / モード | Shared x / 共通横軸 | Upper / 上段 | Lower / 下段 |
|---|---|---|---|
| I–V, source I | `I_source_A` | Voltage + device label, `V_meter_V` | Current + device label, `I_meter_A` |
| V–I, source V | `V_source_V` | Current + device label, `I_meter_A` | Voltage + device label, `V_meter_V` |

For I–V, explicitly choose **Gate current** only when the reference measures a
separate gate circuit (**Gate circuit** purpose). Y-axis names always use the
quantity and the device card label: a reference labeled `Gate` becomes
`Current Gate (µA)`, and a primary labeled `Channel` becomes `Voltage Channel (mV)`.
Renaming either card updates its axis, legend and tooltip. Models, resource addresses and a connected bias source do
not establish that role. Separate never derives R/G from the reference reading.
After completion, Channel R/G uses a free-intercept fit of the primary meter
against the source. For I–V: `V_meter = V_offset + R_2T × I_source`; `G_2T = 1/R_2T`
when the reciprocal is valid. Every finite acquired primary pair contributes,
even if drawing history is later reduced. Dual V–I keeps its existing plot.

I–Vで別回路のゲートリークを測る場合だけ、用途を明示的に **Gate circuit** にします。Y軸は用途名ではなく装置ラベルを使い、
`Current Gate (µA)`・`Voltage Channel (mV)`のように表示します。
カードの改名は軸・凡例・ツールチップにも反映します。
型番・アドレス・バイアス源の接続からは推定しません。参照値からR/Gを作りません。
完了後のChannel R/Gは主計器とソースの自由切片フィットです。
描画が間引かれても、取得した全ての有効な主計器の点を計算に使います。
Dual V–Iの表示は既存仕様を維持します。

A new accepted run ID clears both sweep histories and their bounds, whether
started in the GUI or API. A rejected Start preserves the previous result.
Stopped runs retain their saved points until the next accepted run.

GUI・APIのどちらから開始しても、新しい有効なランIDで両段の履歴と範囲をリセットします。
拒否されたStartでは直前の結果を保持し、停止ランの保存済み点も次の有効なランまで残します。

## Gate summaries / ゲート別集計

In Separate I–V, **Load gate summary…** explicitly selects the Gate circuit purpose and loads
an existing CSV. Choose **R₂T / G₂T** for the upper panel and source-zero mean gate
current below, both against recorded `V_G`. This is a read-only display, not an
automatic hardware gate sweep. Revert to **Sweep** for the acquired I–V points.

SeparateのI–Vで **Load gate summary…** を押すと、用途をGate circuitとして明示し、
既存CSVを読み込みます。**R₂T / G₂T** を選ぶと上段に抵抗・コンダクタンス、下段に
ソース0 Aのゲート電流平均を表示します。共通横軸は記録された`V_G`です。
読み取り専用の表示で、自動ゲート掃引は追加しません。**Sweep** でI–Vの点に戻れます。

| CSV column / 列 | Meaning / 意味 |
|---|---|
| `V_G` (alias `gate_voltage_V`) | Explicit finite gate voltage in V / 明示されたゲート電圧 |
| `R_2T_ohm` (alias `resistance_ohm`) | Finite free-intercept fitted resistance in Ω / 自由切片フィットの抵抗 |
| `status` | Only `completed` or `Completed` is included / 完了ランだけを採用 |
| `I_gate_zero_mean_A` (alias `gate_current_zero_mean_A`) | Mean finite reference reading at exact source 0 A / ソース0 Aの有効読値の平均 |
| `zero_current_samples` | Positive count required to display that mean / 平均の表示には正の有効点数が必要 |
| `origin` | Optional run label / 任意のラン名 |

Missing gate values exclude that row; stopped/incomplete rows are excluded from
fits. Missing zero-current readings/counts leave a gap in leakage, while a valid
R/G point remains. Distinct runs at the same gate stay distinct. Missing or
non-finite resistance in a completed row is reported as an import error.
The acquisition CSV columns, values, protection limits and output cleanup are
unchanged. Existing per-point `R_meter_ohm` / `G_meter_S` ratios are not the
free-intercept channel fit and must not be interpreted as a resistance of a
separate reference circuit. Local experimental CSVs are not shipped as test fixtures.

ゲート値が欠けた行と途中停止・未完了の行はフィット集計に含めません。
ソース0 Aの有効点・件数がなければリークは欠測とし、有効なR/Gの点は残します。
同じゲート電圧の別ランを勝手に統合しません。完了行の抵抗が欠測・非有限なら読込エラーです。
取得CSVの列・数値、保護上限、出力終了処理は変更しません。
既存の各点の`R_meter_ohm`・`G_meter_S`は自由切片の主回路フィットではなく、
別回路の参照計器の抵抗として解釈しません。実験CSVはテスト配布物に含めません。

## Developer verification / 開発時の確認

```sh
cargo test --workspace --locked
python3 scripts/verify_reference_gui.py /path/to/kohdalab-iv --output /tmp/iv-gui --version 1.0.6
python3 scripts/verify_separate_reference_api.py /path/to/kohdalab-iv --output-dir /tmp/iv-api
```

These are Simulation checks, not physical acceptance measurements. The GUI
script covers Overlay, unassigned reference, two accepted GUI runs, rejected
Start retention, narrow-window Stop visibility, and manually changed linked x
bounds. The API script checks two accepted runs, rejected-plan retention, partial
software-limit data, and verified Output OFF.

これらはSimulationの検証で、実機受入測定ではありません。GUIでは従来表示、参照未割当、
連続2ラン、拒否Startでの保持、狭い画面のStop、手動変更した横軸範囲の同期を確認します。
APIでは連続2ラン、無効設定の拒否、ソフトウェア上限停止の部分データ、Output OFFを確認します。

For an interactive, read-only acceptance preview, add `--replay-keep-open` to
`--gui-separate-smoke --gui-reference-replay <CSV> --replay-status completed|stopped`.
Replay dispatch is disabled; this preview cannot connect or apply output.
Close the preview normally when finished.

対話的な読み取り専用の表示確認では、上記のCSV再生指定に`--replay-keep-open`を付けます。
この画面は機器への指令を無効にしており、接続・印加はできません。確認後は通常どおり閉じます。
