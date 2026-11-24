# Native roadmap

The native migration delivered the continuous I/V monitor, pinned channel cards,
bounded time history, Rust application and native CSV schema. See the
[change history](CHANGELOG.md) for shipped work. Track proposed work in
[GitHub Issues](https://github.com/woolen-yarn/kohdalab-iv/issues) and changes
under review in [pull requests](https://github.com/woolen-yarn/kohdalab-iv/pulls).

Future work must preserve single-owner instrument access, output shutdown,
per-point CSV flushing and documented resolution semantics. Extend device
profiles only after their official protocol has been reviewed and tested.

Historical migration and performance evidence remains in the docs and Git
history; historical results must not be rewritten to match later software.

## Tracked: Linux beyond simulation-only

The Linux release stays simulation-only. The first step is device-free: build
the official-VISA helper and a fake VISA ABI on Linux CI, and run the existing
helper protocol checks against them. Linux hardware routes and packages wait
until the official Linux VISA and adapter support has been reviewed and a
hardware-validation plan exists. No physical validation is claimed. See the
[Linux design note](docs/development/linux-visa.md).

---

# ネイティブ実装ロードマップ

ネイティブ移行で、連続 I/V モニター、固定チャンネルカード、上限付き時間履歴、Rust アプリ、
ネイティブ CSV スキーマを提供しました。出荷済みの内容は[変更履歴](CHANGELOG.md)、
今後の提案は [GitHub Issues](https://github.com/woolen-yarn/kohdalab-iv/issues)、
レビュー中の変更は [Pull Requests](https://github.com/woolen-yarn/kohdalab-iv/pulls) を参照してください。

将来の作業でも、単一所有者による計測器アクセス、出力停止、点ごとの CSV フラッシュ、
文書化された分解能の意味を維持しなければなりません。デバイスプロファイルは、公式プロトコルを
レビューしてテストした後にのみ拡張します。

過去の移行および性能の証跡は docs と Git 履歴に残ります。過去の結果を後のソフトウェアに
合わせて書き換えてはいけません。

## 追跡項目：シミュレーション専用を超える Linux 対応

Linux 版はシミュレーション専用のままです。最初の一歩は実機を使いません。Linux CI 上で公式 VISA
ヘルパーと偽 VISA ABI をビルドし、既存のヘルパープロトコル検査を実行します。Linux での実機接続と
配布物は、公式の Linux VISA とアダプタ対応のレビュー、およびハードウェア検証計画ができるまで
保留します。物理的な検証は主張しません。詳細は [Linux 設計メモ](docs/development/linux-visa.md)
を参照してください。
