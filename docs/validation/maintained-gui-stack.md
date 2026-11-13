# Maintained GUI stack / GUI依存の保守更新

Unreleased: migrate eframe/egui to 0.34.3 and egui_plot to 0.35. The old renderer's
unmaintained ttf-parser is removed. Minimum Rust is 1.92. Root-UI lifecycle and
existing test harnesses are migrated without changing measurement or layout assertions.

未公開の変更としてeframe/eguiを0.34.3、egui_plotを0.35へ更新し、描画経由の保守終了依存を除きました。
最低Rust版は1.92です。測定・配置の期待値を維持し、GUIとテストのライフサイクルを移行しています。

The Mac native Simulation API check passed: exclusive ownership, duplicate request
handling, three recorded points, all 26 CSV columns, verified stop and reacquisition.
The prior API smoke script still expected an obsolete 18-column schema; it now
validates explicit current column names and known simulation values. Both package
CI jobs run this check. Pinned GUI license notices are included with hash checks.

Mac実GUIのSimulationで制御権の排他、重複要求、3点の測定、CSV全26列、停止確認、再取得が成功しました。
API検証スクリプトに残った旧18列の期待値を現行列名と既知の模擬測定値の検証へ修正し、
Windows/Mac配布CIに組み込みました。上流固定のGUIライセンス文はハッシュ検証して同梱します。

Linux-only AccessKit still depends on unmaintained zbus5-family packages. These are
not Windows/Mac dependencies; no warning is suppressed and accessibility is retained.
New physical fault/stop tests were waived by the operator this round, not passed.

Linux専用AccessKitには保守終了したzbus5系依存が残ります。Windows/Macの依存には含まれず、
警告は隠さずアクセシビリティを維持します。今回の実機異常・停止試験は利用者の指示で省略し、合格とは扱いません。

## Common SDK lease race / 共通SDKの制御権競合

The Python SDK now serializes lease transitions and rejects retired heartbeat
workers by generation. A heartbeat queued before voluntary release cannot poison
the next acquisition. Real active-lease revocation remains an error. Deterministic
regression tests cover both cases; the common SDK suite passes 16 tests.

Python SDKでは制御権の遷移を排他化し、終了したheartbeatを世代で判別します。
正常な解放の前に待機していたheartbeatが次の制御権取得を誤って失敗させる競合を修正しました。
実際の制御権喪失は引き続きエラーとし、両ケースの再現試験を含む共通SDK 16件が成功しています。

## Hidden-window processing / ウィンドウ非表示時の処理

Controller/API polling and close protection run in `App::logic`, independently of
visible drawing. Tests drive logic without UI: background progress and pending-copy
close cancellation remain active. TRKR also tests focus-loss release of an alignment
hold; IV tests active-close Stop selection, with delivery covered by client/engine tests.

制御・APIの結果処理と閉じる際の保護を`App::logic`へ移し、描画と独立して動かします。
UIを呼ばない回帰試験で進行と保存中の終了取消を確認します。TRKRではフォーカス喪失時の
長押し移動解除、IVでは測定中の終了要求がStopを選ぶことを確認し、送信経路はClient/Engine試験で検証します。
