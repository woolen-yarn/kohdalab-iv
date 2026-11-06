# KohdaLab IV 1.4.2

## Recording completion / 保存完了処理

Ordinary and balanced-source runs now flush and request OS synchronization of the analysis CSV before computing file hashes and final metadata. Output cleanup retains priority. A synchronization failure marks the recording failed, retains partial files and requests gate shutdown through the existing failure path.

通常測定・balanced-source測定の解析CSVをflushし、OSへの同期を要求してからハッシュと最終metadataを作成します。出力の後処理を先に行う順序は維持します。同期失敗は記録失敗として扱い、部分ファイルを保持し、既存の異常処理でgate停止を要求します。

Startup errors preserve both existing files and partial evidence. No pathname-based rollback deletes a file that another process may have replaced. The error explicitly requires a new output name. Output is never enabled before the initial recording files are ready.

開始失敗時は既存ファイルと部分的な保存結果を保持します。別プロセスが置き換えた可能性があるファイルを、パス名だけで削除しません。エラーには新しい出力名が必要と表示し、初期保存ファイルの準備前に出力をONにしません。

## Compatibility and verification / 互換性と検証

CSV columns, numeric values, settings/API schemas and measurement behavior are unchanged. Keep analysis CSV, raw CSV and run JSON together. OS synchronization is not a guarantee against storage-device failure or sudden power loss. Tests cover output cleanup, metadata lifecycle, collisions and serialization failure; release verification covers Windows and macOS packages. No new physical-device qualification is claimed.

CSV列・数値・設定／API schema・測定動作は変更しません。解析CSV・raw CSV・run JSONを一緒に保管してください。OS同期は記憶装置の故障や突然の電源断まで保証するものではありません。出力後処理・metadataの更新・既存ファイルとの衝突・直列化失敗をテストし、Windows／Mac配布物も検証します。新たな実機検証ではありません。
