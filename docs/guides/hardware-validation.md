# Hardware validation guide

Use this checklist after changing the application, VISA runtime, cable, adapter,
or resource string. Read the instrument manuals and [safety notes](../../SAFETY.md).
New preview instrument models remain physically unverified until qualified on
that exact setup.

## Verify the software path

Install or extract the complete package, including its helper programs. From
its executable directory, test without hardware:

```powershell
.\kohdalab-iv.exe --measure --simulate --output "$HOME\iv-simulation.csv"
```

Confirm that the run completes and writes CSV. Source builds can use
`cargo run -p kohdalab-iv --bin kohdalab-iv -- --measure --simulate --output /tmp/iv-simulation.csv`.
This simulation does not qualify physical communication or measurement accuracy.

## Prepare the physical setup

1. Confirm wiring, polarity, grounding, sample limits, and source mode.
2. Provide a manual way to disable output. Protect or disconnect sensitive samples
   for the first communication check.
3. If using VISA, verify discovery in the vendor connection utility first.
4. Launch the GUI and use Refresh in each device card. Check source and meter
   model and resource independently; see [supported hardware](../reference/hardware.md).
5. Connect each device. Verify identity, zero source level, and output off before
   attaching the sample.

## Review and run a small sweep

In the GUI, select the source mode and a conservative range, compliance, ramp,
settling time, NPLC, and output directory. Review Settings… before
Start. Keep the first sweep as small as the sample and instrument allow.

1. Watch the source front panel and check measured sign and approximate magnitude.
2. Press Stop and verify the controlled return to zero and output off.
3. Check that the partial CSV is preserved.
4. Run the short sweep to completion, then All Disconnect; confirm local control.
5. Save the reviewed GUI configuration to a local file outside Git.

## Qualify the Rust build with GS210 and 34411A

Repeat the short-sweep checklist separately for GPIB and direct USB. Use the
GS210 in USB communication mode. Keep the source terminals open where the
measurement permits, or use a protected test fixture. Use the smallest suitable
range and compliance for the setup. Record the identity and selected resource
of each instrument, the completed and stopped CSVs, and local control after
All Disconnect for each connection method.

### Balanced zero return (requires a second GS210)

Check the wiring and source assignments against the
[dual-source guide](../reference/dual-source-and-gui-control.md). Run one short
balanced sweep to completion and a second one that you stop with both source
levels away from zero. For each run, observe both GS210 front panels. Record
the last visible levels, how both levels approach zero, whether both reached
zero before OFF, and the OFF indication for each source. Record the configured
ratio and ramp settings. If display refresh hides individual steps, record that
limit on the observation. If a 10-second cutoff occurs, capture the warning
and the `zero_return_timeout` entry from the run JSON and lifecycle log, and
check both OFF indications. If either OFF is uncertain, use the front panel
to disable output and stop validation until the cause is resolved.

## Record the result

Use the [acceptance record template](../validation/gs210-34411a-acceptance.md).
Record application version, date, OS, VISA runtime, adapter, resources, instrument
firmware, range, compliance, and observed cleanup. Do not publish private lab
settings in packaged defaults. If cleanup fails, disable output manually,
preserve the log, and resolve the fault before increasing the range.

---

# ハードウェア検証ガイド

アプリケーション、VISA ランタイム、ケーブル、アダプター、リソース文字列を変更した後に、
このチェックリストを使用します。計測器マニュアルと[安全に関する注意](../../SAFETY.md)を
読んでください。新しいプレビュー計測器モデルは、その正確な構成で認定されるまで実機では
未検証です。

## ソフトウェア経路を確認する

ヘルパープログラムを含む完全なパッケージをインストールまたは展開します。実行ファイルの
ディレクトリから、ハードウェアを接続せずにテストします。

```powershell
.\kohdalab-iv.exe --measure --simulate --output "$HOME\iv-simulation.csv"
```

実行が完了し CSV が書き出されることを確認します。ソースビルドでは
`cargo run -p kohdalab-iv --bin kohdalab-iv -- --measure --simulate --output /tmp/iv-simulation.csv`
を使用できます。このシミュレーションは、実機通信や測定精度を認定するものではありません。

## 実機セットアップを準備する

1. 配線、極性、接地、試料の制限、ソースモードを確認します。
2. 出力を手動で無効化する手段を確保します。最初の通信確認では、感度の高い試料を保護または切断します。
3. VISA を使用する場合は、まずベンダーの接続ユーティリティで検出を確認します。
4. GUI を起動し、各デバイスカードで Refresh を使用します。ソースとメーターのモデル・リソースを個別に確認します。[対応ハードウェア](../reference/hardware.md)も参照してください。
5. 各デバイスを接続します。試料を接続する前に、識別、ゼロのソースレベル、出力オフを確認します。

## 小さなスイープを確認・実行する

GUI でソースモード、保守的な範囲、コンプライアンス、ランプ、待機時間、NPLC、出力ディレクトリを
選びます。Start の前に Settings… を確認してください。最初のスイープは、試料と計測器が
許容する範囲でできるだけ小さく保ちます。

1. ソース前面パネルを監視し、測定値の符号とおおよその大きさを確認します。
2. Stop を押し、制御されたゼロ復帰と出力オフを確認します。
3. 部分 CSV が保持されていることを確認します。
4. 短いスイープを完了まで実行してから All Disconnect を行い、ローカル制御を確認します。
5. レビュー済みの GUI 設定を Git 外のローカルファイルへ保存します。

## GS210 と 34411A で Rust 版を受け入れ検証する

短いスイープのチェックリストを GPIB と直接 USB でそれぞれ実施します。GS210 の USB は
通信モードを使います。測定が許せばソース出力端を開放し、試料が必要なら保護した
検証用治具を使います。構成に適した最小のレンジとコンプライアンスを選びます。
接続方式ごとに各機器の識別と選択したリソース、完走時と Stop 時の CSV、
All Disconnect 後のローカル操作復帰を記録します。

### Balanced 測定のゼロ復帰（GS210 がもう 1 台必要）

[2 ソースのガイド](../reference/dual-source-and-gui-control.md)に従って配線とソースの
割り当てを確認します。短い balanced スイープを 1 回完走させ、別のスイープでは
両ソースのレベルがゼロから離れた時点で Stop を押します。各回で 2 台の前面パネルを
観察し、最後に読み取れたレベル、ゼロへの近づき方、OFF 前に両方がゼロに達したか、
各ソースの OFF 表示を記録します。設定した比率とランプ設定も記録します。
表示の更新間隔で個々のステップを追えなかった場合は、その旨を記録します。
10 秒で打ち切られた場合は画面の警告、run JSON とライフサイクルログの
`zero_return_timeout` を保存し、両方の OFF 表示を確認します。
どちらかの OFF が不確かな場合は前面パネルで出力を無効にし、原因が解消するまで検証を中断します。

## 結果を記録する

[受入記録の雛形](../validation/gs210-34411a-acceptance.md)を使い、アプリケーションバージョン、
日付、OS、VISA ランタイム、アダプター、リソース、計測器ファームウェア、範囲、
コンプライアンス、観測したクリーンアップを記録します。パッケージの
既定値へ私的なラボ設定を公開しないでください。クリーンアップに失敗した場合は、手動で出力を
無効化し、ログを保存し、範囲を増やす前に障害を解決します。
