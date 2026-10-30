# KohdaLab IV 1.3.0-alpha.19 preview

## Changes

The first normal GUI launch automatically creates `~/.kohdalab/iv/config.json` (`%USERPROFILE%\.kohdalab\iv\config.json` on Windows) when it is missing. You do not need to prepare JSON manually.

## Compatibility and safety

Existing files are not rewritten on startup, including malformed, foreign and unsupported settings. Errors are displayed instead of silently replacing those files. Creation is atomic and cannot clobber a competing launch. Explicit configuration profiles and smoke tests do not initialize the production default. Loading/creating settings never connects instruments or enables source output. Existing acquisition, safety, CSV and external API contracts are unchanged.

## Downloads

Use the Windows Setup or portable ZIP, macOS Apple silicon preview ZIP, or corresponding source/build kit attached to this release. Windows executables are unsigned; the Mac application uses ad hoc signing and is not notarized.

## Validation

The first-launch regression tests cover missing defaults, byte preservation, invalid/future/foreign files and an unusable configuration directory. Final platform CI, compiled GUI startup checks and artifact provenance are recorded with the release. No new physical-device qualification is claimed.

## 日本語

初回の通常GUI起動時に、標準設定がなければ自動作成します。既存・不正・未対応の設定は上書きせず、エラーを表示します。別プロファイル指定とスモークテストは通常用設定を作成しません。装置への接続・出力ON、測定・CSV・APIの仕様は変更しません。Windows版は未署名、Mac版はアドホック署名で公証されていません。
