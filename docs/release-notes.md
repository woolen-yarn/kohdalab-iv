# KohdaLab IV 1.0.7

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
