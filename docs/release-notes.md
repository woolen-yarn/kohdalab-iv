# KohdaLab IV 1.3.0-alpha.18 preview

## Changes / 変更内容

Add the authenticated Control Protocol 1 adapter and separate common Python SDK. Refresh the current simulation screenshot and standardize PR/release maintenance.

## Compatibility and migration / 互換性と移行

The native protocol and 18-column CSV remain supported. Common operations use IV's existing exclusive ownership and fresh Output OFF release verification. Config remains `~/.kohdalab/iv/config.json`.

## Downloads / ダウンロード

Windows x64 Setup and portable preview ZIP; macOS Apple-silicon preview ZIP; source/build kit and checksums. Windows is unsigned; macOS is ad hoc signed, not notarized.

[Release packages](https://github.com/woolen-yarn/kohdalab-iv/releases). Existing older releases remain available.

## Validation and provenance / 検証と出所

Local Rust workspace: 280 passed; strict clippy passed. Real GUI common/native simulation: three points, 18 columns, duplicate receipts, exclusive ownership, verified stop and release/reacquire.

The common Python SDK has seven device-free tests. Source/build identifiers, package manifests and SHA-256 evidence accompany the release. These checks do not newly qualify physical instruments, optical closure, camera calibration or scientific accuracy.

## 日本語

共通GUI APIとPython SDKを追加しました。既存の測定・CSV・停止方針は維持します。Windowsは未署名、Macはad hoc署名です。実機の新規適合確認は行っていません。
