# Native roadmap

Current work: continuous I/V monitor, pinned channel cards and bounded time
history; a Rust-only application and native CSV/schema.

Future work must preserve single-owner instrument access, output shutdown,
per-point CSV flushing and documented resolution semantics. Extend device
profiles only after their official protocol has been reviewed and tested.

Historical migration and performance evidence remains in the docs and Git
history; historical results must not be rewritten to match later software.

---

# ネイティブ実装ロードマップ

現在の作業は、連続 I/V モニター、固定チャンネルカード、上限付き時間履歴、Rust のみの
アプリケーション、ネイティブ CSV／スキーマです。

将来の作業でも、単一所有者による計測器アクセス、出力停止、点ごとの CSV フラッシュ、
文書化された分解能の意味を維持しなければなりません。デバイスプロファイルは、公式プロトコルを
レビューしてテストした後にのみ拡張します。

過去の移行および性能の証跡は docs と Git 履歴に残ります。過去の結果を後のソフトウェアに
合わせて書き換えてはいけません。
