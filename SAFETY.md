# Safety Notes

KohdaLab IV controls electrical source and measurement instruments. Software
checks are useful, but they are not a substitute for instrument limits,
interlocks, current compliance, correct wiring, and operator supervision.

## Before Measurement

- Confirm wiring and polarity.
- Confirm source mode: voltage source or current source.
- Set conservative source limits and compliance.
- Start with a small sweep range.
- Verify the output file path before long measurements.
- Keep a manual way to disable output.

## During Measurement

- Do not disconnect instruments during an active sweep.
- Use the GUI Stop action or interrupt the CLI so cleanup can run.
- Watch for compliance, overload, unexpected resistance, or unstable readings.
- Stop immediately if the sample, wiring, or instruments behave unexpectedly.

## After Measurement

- Confirm the source returned to zero or output off.
- Save the config used for the run.
- Record any manual hardware changes in lab notes.

The software is provided without warranty. Operators are responsible for safe
laboratory practice and for following instrument manuals.

---

# 安全に関する注意

KohdaLab IV は電気的なソースおよび測定計測器を制御します。ソフトウェアの
チェックは有用ですが、計測器の制限、インターロック、電流コンプライアンス、
正しい配線、操作者による監督の代わりにはなりません。

## 測定前

- 配線と極性を確認します。
- 電圧源か電流源か、ソースモードを確認します。
- 保守的なソース制限とコンプライアンスを設定します。
- 小さいスイープ範囲から開始します。
- 長時間測定の前に出力ファイルのパスを確認します。
- 出力を手動で無効化する手段を確保します。

## 測定中

- 実行中のスイープで計測器を切断しないでください。
- GUI の Stop 操作または CLI の割り込みを使用し、クリーンアップを実行させます。
- コンプライアンス、過負荷、予期しない抵抗値、不安定な読み値を監視します。
- 試料、配線、計測器に予期しない挙動があれば、直ちに停止します。

## 測定後

- ソースがゼロに戻ったか、または出力がオフになったことを確認します。
- 実行に使用した設定を保存します。
- 手作業で変更したハードウェアはラボノートに記録します。

本ソフトウェアは無保証で提供されます。安全な実験室運用および計測器マニュアルの
遵守は操作者の責任です。
