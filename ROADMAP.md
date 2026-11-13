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
