# GS210 + 34411A hardware acceptance record / 実機受入記録

Blank template. Copy it to the lab's record location and follow the
[hardware validation guide](../guides/hardware-validation.md). Fill one record
per connection method and configuration. For each check, mark Pass, Fail or
Not run and record the observation or reason. Keep raw data and private lab
settings in the lab record.

未記入の雛形です。研究室の記録先へコピーし、
[実機検証ガイド](../guides/hardware-validation.md)に従って接続方式・構成ごとに記入します。
各項目に合格・不合格・未実施のいずれかと、観察結果または理由を記録します。
測定原本と非公開のラボ設定は研究室の記録先に保管します。

| Setup / 構成 | Record / 記入欄 |
| --- | --- |
| Date and operator / 日付・測定者 | |
| KohdaLab IV version, build or source commit / 版・ビルドまたはソースコミット | |
| OS and architecture / OS・アーキテクチャ | |
| Transport for each instrument (GPIB or direct USB), VISA runtime version or native helper, adapter / 各機器の接続方式（GPIB または直接 USB）、VISA ランタイム版またはネイティブヘルパー、アダプター | |
| GS210 identity, firmware, resource / GS210 の識別・ファームウェア・リソース | |
| 34411A identity, firmware, resource / 34411A の識別・ファームウェア・リソース | |
| Second GS210 identity, firmware, resource (balanced only) / 2 台目の GS210 の識別・ファームウェア・リソース（balanced のみ） | |
| Wiring, open terminals or protected test fixture / 配線・出力端開放または保護した検証用治具 | |
| Sweep settings with units, source and meter ranges, source compliance, ramp step/wait, settling, NPLC and balanced ratio where applicable / 単位付きスイープ設定、ソース・メーターのレンジ、ソースのコンプライアンス、ランプの刻み・待機、整定、NPLC、該当時は balanced の比率 | |

| Check / 確認項目 | Result / 判定 | Observation and evidence path / 観察結果・証跡パス |
| --- | --- | --- |
| Instrument identities, initial source level zero and output OFF / 機器の識別、初期ソースレベルのゼロと出力 OFF | | |
| Short sweep completed, expected sign and approximate magnitude, completed CSV saved / 短いスイープの完走、想定した符号・おおよその大きさ、完走時の CSV 保存 | | |
| Stop during sweep, front-panel zero return and OFF, partial CSV preserved / スイープ途中で Stop、前面パネルのゼロ復帰・OFF、途中までの CSV 保存 | | |
| All Disconnect followed by local control / All Disconnect 後のローカル操作復帰 | | |
| Balanced completion, observed levels and zero-before-OFF for both sources / balanced 完走時の観察レベルと各ソースのゼロ到達・その後の OFF | | |
| Balanced Stop, observed levels and zero-before-OFF for both sources / balanced Stop 時の観察レベルと各ソースのゼロ到達・その後の OFF | | |
| If a zero-return cutoff occurred, warning, last programmed levels in run JSON and lifecycle log, and both OFF indications / ゼロ復帰打ち切り時の警告、run JSON とライフサイクルログの最後の指令レベル、両方の OFF 表示 | | |
| Saved configuration, unresolved issues, and any manual shutdown / 保存した設定、未解決事項、手動で出力を無効化した場合の記録 | | |
