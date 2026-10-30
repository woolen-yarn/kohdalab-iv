# KohdaLab IV 1.3.0-alpha.21 preview

## Source and Meter values

Settings can now assign Source, Meter (V), and Meter (I) independently. V-I and
Dual V-I may use an optional voltage Meter; I-V may use an optional current
Meter. Existing single-Meter operation remains available. The central plot keeps
one shared SI coordinate scale, with Source on the lower axis and Meter on the
reserved upper axis. Source- and Meter-based traces overlay, the legend shows
only forward/backward direction, and the one-line status display reports their
independent fitted R/G values.

The main CSV is now 26-column `analysis_v4`; the original 18 fields are retained
in order and eight Source/Meter V/I and per-point R/G fields are appended. Raw
audit CSV remains 18-column `raw_csv_v1`. Settings schema is 4 and run metadata
schema is 9. Missing optional Meter values remain blank; no terminal measurement
is inferred from a source setting. The bundled Python client reads both
`analysis_v4` and legacy `analysis_v3`.

## Compatibility and safety

The canonical application-owned layout is
`~/.kohdalab/iv/config/config.json`, with sibling `run`, `results`, and `logs`
directories (Windows: `%USERPROFILE%\.kohdalab\iv\...`). A missing default is
created atomically on first normal GUI launch; existing, invalid, foreign, and
unsupported profiles are preserved and reported rather than replaced. Old
profiles are not imported automatically. Device connection and source output are
not enabled by configuration creation or loading.

Source protection, Meter limit, fixed-range behavior, GUI ownership and cleanup
policies are unchanged. The additional same-quantity Meter is read sequentially,
not as a hardware-synchronized acquisition. A failed assigned Meter does not
silently fall back to a Source value and follows the existing safety-stop path.

## Distribution and qualification

Windows x64 Setup/portable, macOS Apple-silicon portable, and the corresponding
source build kit are distributed with this preview. Windows binaries are
unsigned. macOS bundles are ad hoc signed and are not Apple-notarized. This
release has software, simulation, packaging, and documentation checks; it does
not claim a new physical-device qualification for the Source/Meter feature.

## 日本語

### Source/Meter値

SettingsでSource、Meter (V)、Meter (I)を別々に割り当てられます。V-I／Dual V-Iでは任意の電圧Meter、I-Vでは任意の電流Meterを使え、従来のMeter 1台だけの運用も維持します。中央プロットは同じSIスケールで下軸にSource、上軸にMeterを表示します。Source/Meter由来の線は重ねて表示され、凡例はforward/backwardのみ、1行のstatusにはそれぞれの掃引フィットによるR/Gを表示します。

main CSVは26列の`analysis_v4`です。元の18列を順序どおり維持し、Source/MeterのV/Iと点ごとのR/Gの8列を末尾に追加します。raw監査CSVは18列の`raw_csv_v1`のままです。settings schemaは4、run metadata schemaは9です。任意Meterが未割当または取得失敗なら値は空欄で、Source設定値から端子実測を推定して書き込みません。同梱Pythonクライアントは`analysis_v4`と旧`analysis_v3`を読めます。

### 互換性と安全性

標準のアプリ専用保存先は`~/.kohdalab/iv/config/config.json`で、同階層に`run`、`results`、`logs`があります（Windowsでは`%USERPROFILE%\.kohdalab\iv\...`）。初回の通常GUI起動時に設定がなければ原子的に作成します。既存・不正・別アプリ・未対応の設定は置換せずエラー表示します。旧設定は自動移行しません。設定の作成・読込だけでは装置接続やSource出力ONは行いません。

Source保護、Meter limit、固定レンジ、GUIの制御権、終了処理の安全方針は変更しません。追加Meterの取得は順次であり、ハードウェア同期ではありません。割り当てたMeterが失敗した場合はSource値へ黙って置換せず、既存の安全停止処理に従います。

### 配布と検証範囲

このpreviewにはWindows x64 Setup/portable、macOS Apple silicon portable、対応するsource build kitを用意します。Windowsバイナリは未署名です。macOSバンドルはad hoc署名で、Apple公証は受けていません。今回の確認はソフトウェア・simulation・パッケージ・ドキュメントです。Source/Meter機能について、新たな実機適合確認は主張しません。
