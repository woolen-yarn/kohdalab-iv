# Code coverage / コードカバレッジ

`.github/workflows/coverage.yml` reruns the existing device-free
`cargo test --workspace --locked` suite under `cargo-llvm-cov` on Linux. The job is
informational (`continue-on-error`), never blocks a PR, connects no hardware,
enables no outputs and changes no drivers. The `coverage-report` workflow artifact
contains an HTML report, `lcov.info` and `summary.txt`; the job summary lists the
`engine/safety.rs`, `engine/stop.rs`, `engine/gate.rs` and `engine/workflow.rs`
rows. No third-party upload service or token is used.

## Run locally

Use the pinned toolchain and the installer version recorded in the workflow:

```sh
rustup component add llvm-tools-preview
cargo install cargo-llvm-cov --version 0.6.15 --locked
cargo llvm-cov --workspace --locked --html   # target/llvm-cov/html/index.html
cargo llvm-cov report --summary-only         # per-file table, after a run
```

Coverage numbers are a regression signal for device-free code paths only. They are
not physical-device qualification.

## 日本語

`.github/workflows/coverage.yml` は、既存の実機を使わない
`cargo test --workspace --locked` を Linux 上で `cargo-llvm-cov` により再実行します。
このジョブは参考情報（`continue-on-error`）で PR をブロックせず、実機接続・出力の
有効化・ドライバー変更を行いません。ワークフローの `coverage-report` 成果物には
HTML レポート、`lcov.info`、`summary.txt` が含まれ、ジョブ概要には
`engine/safety.rs`・`engine/stop.rs`・`engine/gate.rs`・`engine/workflow.rs` の行を
表示します。外部のアップロードサービスやトークンは使いません。

### ローカルでの実行

固定ツールチェーンと、ワークフローに記載したインストーラーのバージョンを使います。

```sh
rustup component add llvm-tools-preview
cargo install cargo-llvm-cov --version 0.6.15 --locked
cargo llvm-cov --workspace --locked --html   # target/llvm-cov/html/index.html
cargo llvm-cov report --summary-only         # 実行後のファイル別一覧
```

カバレッジの数値は、実機を使わないコード経路の退行検出の目安にすぎず、
実機適合確認ではありません。
