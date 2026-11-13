# KohdaLab IV v1.0.0 verification / 検証状況

| Package / 配布物 | Verification / 検証範囲 |
| --- | --- |
| Windows x64 portable ZIP | SHA-256, portable GUI simulation on Sardine, operator-approved hardware qualification / SHA-256、SardineでGUI模擬試験、実機判定はユーザー承認済み |
| Windows x64 Setup EXE | Built from the portable payload. Installer lifecycle not independently exercised for this candidate / ポータブルと同じ内容から生成。候補版でのインストール・アンインストールは未検証 |
| macOS Apple silicon portable ZIP | App signature and GUI simulation / 署名とGUI模擬試験 |
| macOS Apple silicon DMG | Disk image checksum / イメージのチェックサムを確認 |
| Linux | Packaging and qualification pending / 配布・検証準備中 |

Windows and macOS portable simulation each completed 41 points and ended with Output OFF. Rust regression: 364 passed, 3 skipped. Physical qualification is recorded as operator approval. The final Sardine rerun was not independently logged.

Windows・macOSのポータブル版で模擬測定41点と終了後の出力OFFを確認しました。回帰試験は364件合格、3件スキップです。最終実機判定はユーザー承認として記録し、最終候補でのSardine実測ログはありません。

## Hardware and connection evidence / 機器・接続方法と検証

The rows below record earlier Windows physical tests.  A model's other interfaces are outside this evidence.
下表はWindowsでの過去の実機試験です。

| Model / モデル | Role / 役割 | Physical connection tested / 実機確認した接続 | Test scope / 試験範囲 |
| --- | --- | --- | --- |
| Yokogawa GS210 | Source / ソース | GPIB / VISA; USBTMC / VISA | 34401Aとの掃引 / sweep with 34401A |
| Yokogawa 7651 | Source / ソース | GPIB / VISA | 4端子 I source / V meter。候補 `38522c5` でOFF時ライブとTRKR同時接続 / 4-wire sweep on earlier candidates; output-off live with TRKR on `38522c5` |
| Agilent 34401A | Meter / メーター | GPIB / VISA | GS210との4端子電圧測定 / 4-wire voltage reading with GS210 |
| ADCMT 7461A | Meter / メーター | GPIB / VISA | 7651との固定・Autoレンジ試験 / fixed and auto range with 7651 |
| Keysight 34465A | Meter / メーター | USBTMC / VISA | 7651との固定1 V、7点掃引、終了後OFF / fixed 1 V, seven points and output OFF |
| Agilent 34411A | Meter / メーター | GPIB / VISA; USBTMC / VISA | `b68438d` で固定1 V、7点掃引、終了後OFF。最新試験ではUSB未検出 / passed previously; USB absent in the latest trial |

Select an official VISA resource returned by **Device → Refresh** on Windows.
For a shared GPIB adapter use official VISA in both apps and different
instrument addresses. Confirm the selected source shows Output OFF before
changing wiring. [Hardware and transport](reference/hardware.md)

Windowsでは **Device → Refresh** で検出した公式VISAリソースを選択します。
GPIBを共有する場合は両アプリで公式VISAを使い、異なる機器アドレスを選びます。
配線変更前にソースのOutput OFFを確認します。[ハードウェアと転送](reference/hardware.md)

Displayed live values use instrument range/resolution where available. CSV
values retain acquisition precision, with range and resolution in raw evidence.
ライブ表示は機器のレンジ・分解能が得られる場合に反映します。CSVは取得値を保存し、
レンジ・分解能はrawに記録します。[CSV仕様](reference/csv-format.md)


