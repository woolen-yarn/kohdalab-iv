# Contributing

Use the pinned Rust toolchain in the repository root. Run Cargo format, Clippy and the
workspace tests before proposing a change. Architecture: `iv-core` owns planning,
CSV and instrument specifications; `iv-transport` owns exclusive device leases
and helper IPC; `iv-desktop` owns measurement, monitoring and UI.

## Where changes belong

| Area | Location | Responsibility |
| --- | --- | --- |
| Sweep rules and saved data | `crates/iv-core/` | Planning, validation, SI values, CSV and reviewed instrument specs |
| Instrument communication | `crates/iv-transport/` | Resource routing, helper processes, device leases and instrument commands |
| Measurement execution | `crates/iv-desktop/src/engine.rs` | Serialized acquisition, monitoring, cancellation and output cleanup |
| Engine interface | `crates/iv-desktop/src/protocol.rs`, `client.rs` | Commands, events and the desktop client |
| Application UI | `crates/iv-desktop/src/ui.rs` | Application state, event synchronization and three-panel layout |
| UI settings | `crates/iv-desktop/src/ui/settings.rs` | GUI settings schema, defaults and persistence |
| Display formatting | `crates/iv-desktop/src/ui/formatting.rs` | Engineering prefixes, tick labels and resolution-aware readouts |
| Drawing history | `crates/iv-desktop/src/ui/history.rs` | Bounded live history, direction segments and turning-point anchors |
| Shared UI components | `crates/iv-desktop/src/ui/widgets.rs` | Palette, style and common input widgets |
| Notebook control | `bindings/python/` | Application discovery and the thin JSON-lines client |
| Packaging | `scripts/`, `packaging/windows/` | Portable archives, source kits and installation lifecycle |

## Adding an instrument or measurement

Register a model once in `iv-core/src/catalog.rs`, including its role,
capabilities and protocol dialect. Keep numeric limits, ranges, resolution and
NPLC rules in `specs/`; `instrument_profile` reads these same resources.
The GUI obtains model choices and operation availability from the catalog.
Add the model to the README matrix with connection requirements and physical
validation status. The catalog contract tests check that every model is listed.

Implement protocol exchanges in `iv-transport/src/drivers.rs` through
`InstrumentIo`. DevicePair retains ownership of sessions, exclusive leases,
absolute deadlines and emergency recovery. New drivers must pass the shared
identity/configuration contracts and exercise timeout, disconnect and malformed
responses. Preserve ADC discard ordering and the single 7651 panel deadline.

Measurement labels, required roles, source/meter functions, mode-switch
defaults, axis labels and sweep choices belong in `iv-core/src/measurements.rs`.
Planning and validation still belong in `iv-core`; a descriptor alone must never
enable a device or bypass a scientific limit. Use `engine/workflow.rs` for source
ramps and completion precedence. Keep the existing wait/acquisition routines and
verified output shutdown on the device-owning worker. Emergency output-off must
not wait behind a normal source ramp.

`state.rs` defines typed phases, execution activities, output state and failures.
Serialized phase/output labels and the `error` message remain compatible with
older clients. `error_kind` is supplementary diagnostic information; legacy
helper strings are classified at that boundary and must never control shutdown.

`compatibility.rs` owns protocol/settings/metadata/CSV contract versions. The
Python client negotiates from the first ordinary engine state and accepts older
engines without a declaration. Never probe old engines with a new command-line
flag: an unknown flag can launch their GUI. Unversioned and v0 GUI settings
migrate to v1 without changing values; future versioned settings are refused on
load and protected from auto-save. The existing CSV schema and metadata version
are retained. Increment versions only when their compatibility contract changes.

Keep UI helper modules private to the UI. Their unit tests live with the code;
`crates/iv-desktop/tests/live_engine.rs` exercises the process interface and
output cleanup. Move the existing tests with a refactor and retain their
assertions. Compare the GUI simulation and saved records when reorganizing UI
code; a successful build alone does not establish unchanged behavior.

Display formatting does not define the saved precision. Keep CSV serialization
in `iv-core`, and keep drawing anchors out of measurement records. Existing GUI
settings use `.kohdalab/config/iv-next.json`; the filename is a compatibility
contract despite the current application name. Preserve its fields, defaults
and location when reorganizing settings code.

## Measurement and distribution requirements

Never relax scientific limits, golden data or output-shutdown requirements to
make tests pass. Live acquisition shares the same device-owning worker as sweeps.
Keep GUI rendering free of instrument I/O and bound display history.

Instrument data belongs in `specs/`, with primary references and review
status. Do not infer physically meaningful digits from serialized float length.
Native C/C++ helper changes require their source and license notices to accompany
portable binaries. Optional packaging utilities are host tools, not a runtime.

---

# コントリビュート

リポジトリルートの固定 Rust ツールチェーンを使用してください。変更を提案する前に Cargo の
フォーマット、Clippy、ワークスペーステストを実行します。アーキテクチャでは、`iv-core` が計画、
CSV、計測器仕様を、`iv-transport` が排他的デバイスリースとヘルパー IPC を、`iv-desktop` が
測定、監視、UI を担当します。

## 変更先

| 領域 | 場所 | 責務 |
| --- | --- | --- |
| スイープ規則と保存データ | `crates/iv-core/` | 計画、検証、SI 値、CSV、レビュー済み計測器仕様 |
| 計測器通信 | `crates/iv-transport/` | リソース経路、ヘルパープロセス、デバイスリース、計測器コマンド |
| 測定実行 | `crates/iv-desktop/src/engine.rs` | 直列化した取得、監視、キャンセル、出力クリーンアップ |
| エンジンインターフェース | `crates/iv-desktop/src/protocol.rs`, `client.rs` | コマンド、イベント、デスクトップクライアント |
| アプリケーション UI | `crates/iv-desktop/src/ui.rs` | アプリケーション状態、イベント同期、3 パネルレイアウト |
| UI 設定 | `crates/iv-desktop/src/ui/settings.rs` | GUI 設定スキーマ、既定値、永続化 |
| 表示書式 | `crates/iv-desktop/src/ui/formatting.rs` | 工学接頭辞、目盛ラベル、分解能対応の読み値 |
| 描画履歴 | `crates/iv-desktop/src/ui/history.rs` | 上限付きライブ履歴、方向セグメント、折返し点アンカー |
| 共通 UI 部品 | `crates/iv-desktop/src/ui/widgets.rs` | パレット、スタイル、共通入力ウィジェット |
| Notebook 制御 | `bindings/python/` | アプリケーション検出と薄い JSON-lines クライアント |
| パッケージング | `scripts/`, `packaging/windows/` | ポータブルアーカイブ、ソースキット、インストールライフサイクル |

## 計測器または測定を追加する

モデルは役割、機能、プロトコル方言を含めて `iv-core/src/catalog.rs` に一度だけ登録します。数値制限、
レンジ、分解能、NPLC 規則は `specs/` に保持し、`instrument_profile` は同じリソースを読みます。
GUI はカタログからモデル選択肢と操作可否を取得します。接続要件と実機検証状況を README の表へ
追加してください。カタログ契約テストはすべてのモデルが記載されていることを確認します。

プロトコル交換は `iv-transport/src/drivers.rs` で `InstrumentIo` を通して実装します。`DevicePair` は
セッション、排他的リース、絶対デッドライン、緊急復旧を所有します。新ドライバーは共有の
識別／設定契約を通過し、タイムアウト、切断、不正応答を検証する必要があります。ADC の破棄順序と
7651 の単一パネルデッドラインを維持してください。

測定ラベル、必要な役割、ソース／メーター機能、モード切替既定値、軸ラベル、スイープ選択肢は
`iv-core/src/measurements.rs` に属します。計画と検証も `iv-core` に残します。記述子だけで
デバイスを有効化したり科学的制限を迂回したりしてはいけません。ソースランプと完了優先順位には
`engine/workflow.rs` を使用します。既存の待機／取得ルーチンと、デバイス所有ワーカー上の検証済み
出力停止を維持します。緊急 output-off は通常のソースランプの後ろで待ってはいけません。

`state.rs` は型付きフェーズ、実行アクティビティ、出力状態、失敗を定義します。直列化される
フェーズ／出力ラベルと `error` メッセージは古いクライアントと互換に保ちます。`error_kind` は
補助診断情報です。レガシーヘルパー文字列はこの境界で分類し、停止を制御させてはいけません。

`compatibility.rs` はプロトコル／設定／メタデータ／CSV 契約バージョンを所有します。Python
クライアントは最初の通常エンジン状態からネゴシエートし、宣言のない古いエンジンを受け入れます。
古いエンジンを新しいコマンドラインフラグで試行してはいけません。未知のフラグは GUI を起動し得ます。
バージョンなしと v0 の GUI 設定は値を変えずに v1 へ移行します。将来のバージョン設定はロードを拒否し、
自動保存から保護します。既存 CSV スキーマとメタデータバージョンは保持します。契約が変わるときだけ
バージョンを増やします。

UI ヘルパーモジュールは UI 内で非公開に保ちます。ユニットテストはコードと共に置き、
`crates/iv-desktop/tests/live_engine.rs` はプロセスインターフェースと出力クリーンアップを検証します。
リファクタリングでは既存テストとアサーションを移し、GUI シミュレーションと保存記録を比較してください。
ビルド成功だけでは挙動不変を示しません。

表示書式は保存精度を定義しません。CSV 直列化は `iv-core` に残し、描画アンカーを測定記録へ入れません。
既存 GUI 設定は `.kohdalab/config/iv-next.json` を使用します。現在のアプリ名にかかわらず、ファイル名は
互換性契約です。設定コードを整理しても、そのフィールド、既定値、場所を保持してください。

## 測定と配布の要件

テストを通すために科学的制限、ゴールデンデータ、出力停止要件を緩めてはいけません。ライブ取得は
スイープと同じデバイス所有ワーカーを共有します。GUI 描画に計測器 I/O を入れず、表示履歴を上限付きに
保ちます。

計測器データは一次資料とレビュー状態を含めて `specs/` に置きます。直列化 float の長さから物理的に
意味ある桁数を推定してはいけません。ネイティブ C/C++ ヘルパーの変更では、ポータブルバイナリーに
ソースとライセンス通知を添付する必要があります。任意のパッケージングユーティリティはホストツールであり、
ランタイムではありません。
