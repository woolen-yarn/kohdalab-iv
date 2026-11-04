# KohdaLab IV 1.4.1

## Responsive file operations and validated custom points

Config and Output pickers now use native asynchronous sheets, and Save copies a
finished run outside the GUI update loop. The GUI remains responsive while a
selection or copy is pending. A selected copy destination is protected from
ordinary window close until its export finishes; pending operations also reject
conflicting external-control changes.

Custom sweep points retain the last valid setting while text is incomplete or
non-finite. The dialog explains the invalid entry and cannot accept it until the
list is valid. CSV schemas, units, calibration, scan plans, source protection,
and instrument initialization are unchanged.

## Paths, compatibility and safety

An empty native home-directory variable now falls back to the valid platform
alternative. The common Python client makes the same choice and avoids repeated
full-buffer response scans. The shared control wire protocol remains version 1;
existing settings, result files, and external clients need no migration.

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
source build kit are distributed with this release. Windows binaries are
unsigned. macOS bundles are ad hoc signed and are not Apple-notarized. This
release has software, simulation, packaging, and documentation checks; it does
not claim a new physical-device qualification for the Source/Meter feature.

## 日本語

### 応答を保つファイル操作とカスタム測定点検証

Config・Outputの選択画面をネイティブ非同期化し、終了済み測定のSaveコピーもGUI更新処理とは別に実行します。選択・コピー中もGUIは応答を保ちます。コピー先を選んだ後は、export終了まで通常のウィンドウ終了を防ぎ、処理中の外部制御による競合も拒否します。

カスタム掃引点は入力途中や非有限値のときも直前の有効な設定を保持します。無効な入力はその場で説明し、有効なリストになるまで確定できません。CSVスキーマ・単位・校正・走査計画・Source保護・計測器初期化は変更しません。

### パス・互換性・安全性

OS固有のホーム環境変数が空の場合は有効な代替先を使います。共通Pythonクライアントも同じ優先順を使い、API応答の全バッファ再走査を避けます。共通制御プロトコルはversion 1のままで、既存設定・結果ファイル・外部クライアントの移行は不要です。

### 互換性と安全性

標準のアプリ専用保存先は`~/.kohdalab/iv/config/config.json`で、同階層に`run`、`results`、`logs`があります（Windowsでは`%USERPROFILE%\.kohdalab\iv\...`）。初回の通常GUI起動時に設定がなければ原子的に作成します。既存・不正・別アプリ・未対応の設定は置換せずエラー表示します。旧設定は自動移行しません。設定の作成・読込だけでは装置接続やSource出力ONは行いません。

Source保護、Meter limit、固定レンジ、GUIの制御権、終了処理の安全方針は変更しません。追加Meterの取得は順次であり、ハードウェア同期ではありません。割り当てたMeterが失敗した場合はSource値へ黙って置換せず、既存の安全停止処理に従います。

### 配布と検証範囲

このリリースにはWindows x64 Setup/portable、macOS Apple silicon portable、対応するsource build kitを用意します。Windowsバイナリは未署名です。macOSバンドルはad hoc署名で、Apple公証は受けていません。今回の確認はソフトウェア・simulation・パッケージ・ドキュメントです。Source/Meter機能について、新たな実機適合確認は主張しません。
