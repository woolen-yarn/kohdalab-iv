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

## Record the result

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

## 結果を記録する

アプリケーションバージョン、日付、OS、VISA ランタイム、アダプター、リソース、計測器
ファームウェア、範囲、コンプライアンス、観測したクリーンアップを記録します。パッケージの
既定値へ私的なラボ設定を公開しないでください。クリーンアップに失敗した場合は、手動で出力を
無効化し、ログを保存し、範囲を増やす前に障害を解決します。
