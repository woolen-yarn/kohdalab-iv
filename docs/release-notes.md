# KohdaLab IV 1.0.8

## Changes / 変更点

- Stop an active sweep and reverify independent source outputs if the instrument worker stops unexpectedly. Output remains `Unknown` when OFF cannot be confirmed. / 独立機器の処理が異常終了したときは掃引を停止し、出力OFFを再確認します。確認できない場合は `Unknown` と表示します。
- Preserve queued individual DC OFF requests and handle malformed replies without panicking. The shared instrument hub closes a session when it cannot safely track requests. / 個別DC OFF要求を保持し、不正な計器応答はエラーとして扱います。機器ハブは要求を安全に追跡できない接続を閉じます。
- Reorganize GUI modules and update dependencies, including eframe and egui_plot together. Existing settings and output formats are compatible. / GUIの内部構成を整理し、eframeとegui_plotなどを更新しました。既存の設定と出力形式は互換です。

## Downloads and validation / 配布・検証

Windows x64 Setup and portable ZIP, macOS Apple Silicon DMG and portable ZIP, and Ubuntu 24.04 x64 simulation packages are built from the same source. The Windows packages are unsigned. Linux packages are for simulation. See `SHA256SUMS.txt` and the build origin files on the release page. No new physical measurements were performed.

Windows x64（Setup・ZIP）、Apple Silicon Mac（DMG・ZIP）、Ubuntu 24.04 x64（DEB・tar.gz）を同一ソースからビルドします。Windows版は未署名、Linux版はシミュレーション専用です。配布ページの `SHA256SUMS.txt` とビルド情報を参照してください。今回の実機測定は未実施です。

## Previous release / 前の版

### KohdaLab IV 1.0.7

## Changes / 変更点

Align the external-control bar and distinguish control ownership from instrument state. Continue to require verified output OFF on release. Include the common Python SDK 1.0.4.

## Validation scope / 検証範囲

The previous hardware compatibility and safety guidance remains applicable. The release software checks include simulation. New physical-device operating scenarios are outside this qualification.

See SHA256SUMS.txt and the release page's platform packages. / 対応OSとSHA-256は配布ページをご覧ください。

## Previous release / 前の版

### KohdaLab IV 1.0.6

Reference display now offers **Overlay / Separate**, saved with the profile. Legacy profiles keep Overlay. Separate plots use one source coordinate per acquired record, with linked x zoom/pan and independent y ranges. Missing reference readings are gaps or waiting, never fabricated zeroes. Y axes, legends and tooltips follow device card labels: `Voltage Channel (mV)` and `Current Gate (µA)`, for example.

Channel R/G is a primary-meter free-intercept fit; separate reference leakage is never used as its denominator. Completed gate-summary CSVs can display resistance/conductance above source-zero mean leakage, with explicit gate values, completion and a positive valid-zero count. Partial/stopped sweeps stay visible but are not counted as completed gate fits. Acquisition CSV and output protection contracts are unchanged.

Both histories and bounds reset for a new accepted run ID from GUI or API. Rejected starts retain the previous result. Narrow center panes fit Output and Start/Stop/Save, and status/point/RG information uses two rows. The Windows GUI acceptance suite includes Separate, linked x bounds and a narrow window; an API Simulation check covers consecutive runs and partial protection-stop cleanup.

参照表示 **Overlay / Separate** をプロファイルに保存できます。旧設定はOverlayを維持します。Separateは同じ測定点のソース座標を共有して上下二段に分け、横軸のズーム・パンを同期し、縦軸範囲は独立にします。欠測・未取得を0で補いません。Y軸・凡例・ツールチップは量と機器カードのラベルに連動します。

主回路のR/Gは主計器の自由切片フィットで求め、別回路のリークを分母にしません。明示的なゲート電圧・完了状態・正のゼロ電流有効件数を持つ集計CSVを読み込み、上段のR/Gと下段のリーク平均を比較できます。停止ランの点は保持し、完了ランの集計から除外します。取得CSVと出力保護の契約は変更しません。

GUI・APIの新しい有効ランIDで両段の履歴と範囲をリセットし、拒否されたStartでは前の結果を残します。狭い中央ペインでも保存先とStart/Stop/Saveを収め、状態・点数・R/Gを二行にします。WindowsのSimulation検証は二段表示・横軸同期・狭い画面を含み、APIでは連続ランと保護停止時の終了確認を検証します。

Physical-device acceptance measurements were not performed for this change. Nautilus display verification uses Simulation or read-only CSV replay, without new instrument output. / 今回の変更の実機受入測定は未実施です。Nautilusの表示確認はSimulationまたはCSVの読み取り専用再生で行い、新たな印加はしません。
