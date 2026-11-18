# KohdaLab IV 1.0.5

DC output keeps live acquisition active during normal operation. GUI simulation checks now require multiple acquisitions while output is ON. Source cards distinguish WAITING, DC LIVE and STALE.

If an operation error stops monitoring, ordinary sources are verified OFF first, including a lone Source 1. This also applies to refused range changes or Start during held DC. Failed OFF confirmation remains Unknown; the original error and cleanup error both remain visible. Independent inventory outputs retain their existing ownership and cleanup policies.

Numeric readouts choose units from the current value and meter resolution, while plot axes retain the history scale. A new 1 nA value can read as nA despite earlier mA samples. Scientific values and CSV precision are unchanged. Range is full-scale, not a minimum measurable signal. Meter Range follows the active DC function even when a different sweep mode is planned; Sweep limit remains the threshold for that planned run.

DC出力は正常時にライブ取得を継続し、GUI SimulationでもON中に複数回更新されることを確認します。Sourceカードはデータ待ち・DC更新中・更新停止を区別します。

エラーで監視を止める際は、Source 1単独を含め通常のSourceのOFFを先に確認します。DC中のレンジ変更やStartの拒否も対象です。確認できない場合はUnknownを維持し、元のエラーとOFF確認エラーを表示します。独立inventory出力の所有権・終了方針は従来通りです。

数値表示は現在値とメーターの分解能に合う単位を使い、グラフは履歴の軸範囲を維持します。以前にmA級の値があっても現在のnA値を読みやすくし、元データ・CSV精度は変更しません。レンジは測定上限で、最小測定値ではありません。予定した掃引モードが異なっていてもRangeは現在のDC動作に合わせ、Sweep limitは予定した掃引の停止しきい値として区別します。

Hardware was not tested for this change. / 今回の修正の実機検証は未実施です。
