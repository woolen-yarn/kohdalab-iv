# KohdaLab IV 1.0.2

- Preserve individual DC OFF requests when they overlap with Stop. / Stopと重なっても装置ごとの個別DC OFF要求を保持します。
- Verify Save Copy against the original measurement hashes and remove incomplete copies on mismatch. / 保存コピーを測定時のハッシュと照合し、不一致時は作成途中のコピーを削除します。
- Verify job-scoped Python Stop using the sweep's terminal outputs and include independent DC outputs in app-wide confirmation. / PythonのジョブID付きStopは掃引終了時の出力を確認し、アプリ全体の確認では独立DC出力も確認します。

---

# KohdaLab IV 1.0.1

Align the documented instrument catalog and common Python SDK. / 機種一覧の検査と共通Python SDKを更新しました。

---

# KohdaLab IV 1.4.8

Windows x64 and Apple Silicon Mac packages. Device cards support inline label editing and an expandable two-column layout. All Connect retains successful connections if one device fails. Long sweeps draw fewer markers.

Windows x64とApple Silicon Mac向けの配布版です。装置カードのラベルを直接編集でき、2列表示でも設定を展開できます。一括接続では、一部の装置が失敗しても接続できた装置を維持します。長い測定系列では点マーカーの描画を減らします。

Dual V-I uses the measured voltage when available and falls back to the source difference. The `analysis_dual_v1` CSV adds four per-source columns to the existing 26. Single-source CSV keeps its existing schema.

Dual V-Iの横軸は電圧計の実測値を優先し、実測値がなければソース間の差を使います。Dual用の `analysis_dual_v1` CSVは既存26列に各ソースの設定値と読返し値を計4列追加します。単一ソースのCSV形式は従来どおりです。
