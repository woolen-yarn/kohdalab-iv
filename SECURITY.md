# Security

Report suspected vulnerabilities privately to the repository maintainers through
GitHub security reporting where available. Do not publish credentials or personal
instrument-network details in an issue.

Application crates forbid unsafe Rust. Native C/C++ helpers and OS libraries have
separate memory-safety boundaries; they remain isolated processes. Instrument
access uses exclusive device leases and bounded IPC frames.

CI checks Rust dependencies using cargo-audit against RustSec. Keep Cargo.lock,
compiler versions, native helper hashes and license notices reviewable. Do not
replace upstream firmware or driver binaries with unverified downloads.

---

# セキュリティ

脆弱性の疑いは、利用可能な場合 GitHub のセキュリティ報告を通じて、リポジトリ保守者へ
非公開で報告してください。資格情報や個人の計測器ネットワーク詳細を Issue に公開しないでください。

アプリケーションクレートでは unsafe Rust を禁止しています。ネイティブ C/C++ ヘルパーと
OS ライブラリには別個のメモリー安全性境界があり、分離プロセスとして動作します。計測器アクセスは
排他的なデバイスリースと上限付き IPC フレームを使用します。

CI は RustSec を用いる cargo-audit で Rust 依存関係を確認します。`Cargo.lock`、コンパイラー
バージョン、ネイティブヘルパーのハッシュ、ライセンス通知をレビュー可能な状態に保ってください。
検証されていないダウンロードで上流のファームウェアやドライバーバイナリーを置き換えないでください。
