<img src="assets/kohdalab-iv.png" width="64" height="64" align="right" alt="KohdaLab IV icon">

# KohdaLab IV

[![CI](https://github.com/woolen-yarn/kohdalab-iv/actions/workflows/test.yml/badge.svg)](https://github.com/woolen-yarn/kohdalab-iv/actions/workflows/test.yml)
[![Release](https://img.shields.io/github/v/release/woolen-yarn/kohdalab-iv)](https://github.com/woolen-yarn/kohdalab-iv/releases/latest)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

Rust desktop application for DC I-V and V-I measurements, live source/meter monitoring, and USB/GPIB instrument control.
DC I-V・V-I 測定、ソース／メーターのライブ監視、USB/GPIB 計測器制御のための Rust デスクトップアプリケーションです。

[Download / ダウンロード](https://github.com/woolen-yarn/kohdalab-iv/releases) · [Documentation / ドキュメント](docs/README.md) · [Report an issue / 問題を報告](https://github.com/woolen-yarn/kohdalab-iv/issues)

- Voltage-source/current-measure and current-source/voltage-measure sweeps.
- Live monitoring, source protection, CSV recording, and run metadata.
- One Rust engine for the desktop GUI and optional Python/Jupyter client.

- 電圧源・電流測定、および電流源・電圧測定のスイープ。
- ライブ監視、ソース保護、CSV 記録、実行メタデータ。
- デスクトップ GUI と任意の Python/Jupyter クライアントで共通の Rust エンジン。

## Interface / 画面

![KohdaLab IV interface](docs/images/kohdalab-iv.png)

Simulation example: instrument setup and live signals on the left, sweep and plot in the centre, and saved values/logs on the right.

シミュレーション例です。左に計測器の設定とライブ信号、中央にスイープとプロット、右に保存値とログを表示します。

## Downloads / ダウンロード

| Channel / 配布チャネル | Version / バージョン | Platforms / 対応プラットフォーム |
| --- | --- | --- |
| Stable / 安定版 | [1.2.0](https://github.com/woolen-yarn/kohdalab-iv/releases/tag/portable-v1.2.0) | Windows x64 Setup / portable ZIP; macOS Apple silicon portable ZIP / Windows x64 セットアップ・ポータブル ZIP、macOS Apple silicon ポータブル ZIP |
| Preview / プレビュー | [1.3.0-alpha.7](https://github.com/woolen-yarn/kohdalab-iv/releases/tag/preview-v1.3.0-alpha.7) | Windows x64 Setup / preview ZIP; macOS Apple silicon preview ZIP / Windows x64 セットアップ・プレビュー ZIP、macOS Apple silicon プレビュー ZIP |

Rust and Python are not required to run the packaged application. Current Windows
artifacts are unsigned; download packages and checksums together from Releases.
macOS bundles use ad hoc signing and are not Apple-notarized.

配布アプリケーションの実行に Rust や Python は不要です。現在の Windows 配布物は未署名です。Releases からパッケージとチェックサムを必ず一緒に取得してください。macOS バンドルは ad hoc 署名であり、Apple の公証は受けていません。

## Supported hardware / 対応機器

| Role / 役割 | Model / API identifier / モデル・API 識別子 | Availability / 対応状況 |
| --- | --- | --- |
| Source / ソース | Yokogawa GS210 · `YOKOGAWA_GS210` | Stable + preview / 安定版・プレビュー |
| Source / ソース | Yokogawa 7651 · `YOKOGAWA_7651` | Preview; physical validation pending / プレビュー。実機検証待ち |
| Meter / メーター | Agilent / Keysight 34411A · `AGILENT_34411A`, `KEYSIGHT_34411A` | Stable + preview / 安定版・プレビュー |
| Meter / メーター | Agilent 34401A · `AGILENT_34401A` | Preview; physical validation pending / プレビュー。実機検証待ち |
| Meter / メーター | Keysight 34465A · `KEYSIGHT_34465A` | Preview; physical validation pending / プレビュー。実機検証待ち |
| Meter / メーター | ADCMT 7461A · `ADCMT_7461A` | Preview; physical validation pending / プレビュー。実機検証待ち |
| Simulation / シミュレーション | `SIMULATED_SOURCE`, `SIMULATED_METER` | Built in / 内蔵 |

See [hardware and transport](docs/reference/hardware.md) for USB/USBTMC/GPIB, official VISA, adapter requirements, and model limitations. Stable downloads do not include the additional preview drivers.

USB/USBTMC/GPIB、公式 VISA、アダプター要件、モデルごとの制約は[ハードウェアと転送](docs/reference/hardware.md)を参照してください。安定版のダウンロードには、プレビュー版用の追加ドライバーは含まれません。

## Quick start / はじめに

1. Choose the stable release or explicitly opt into the preview. Install Setup or extract the complete portable package.
2. Start with the simulated source and meter to explore the interface.
3. Review source/meter identities, connection routes, ranges, and protection settings before connecting a load.
4. Choose a sweep and output location, then Start. Monitoring and mode selection do not enable source output.

1. 安定版を選ぶか、明示的にプレビュー版を選択します。セットアップを実行するか、ポータブルパッケージ一式を展開します。
2. まずシミュレーションのソースとメーターで、インターフェースを確認します。
3. 負荷を接続する前に、ソース／メーターの識別、接続経路、レンジ、保護設定を確認します。
4. スイープと出力先を選んで Start を押します。監視とモード選択だけではソース出力は有効になりません。

Python/Jupyter access: [client guide](bindings/python/README.md). Detailed measurement semantics: [application guide](docs/guides/application-guide.md).

Python・Jupyter からの利用は[クライアントガイド](bindings/python/README.md)、詳細な測定仕様は[アプリケーションガイド](docs/guides/application-guide.md)を参照してください。

## Development / 開発

Run commands from the repository root. The root `Cargo.toml` and `Cargo.lock`
are the workspace entry point; product packages live under `crates/`.

リポジトリのルートからコマンドを実行します。ルートの `Cargo.toml` と `Cargo.lock` がワークスペースの入口で、製品パッケージは `crates/` にあります。

```sh
cargo fmt --all --check
cargo test --workspace --locked
cargo build --release --locked
cargo run --locked --bin kohdalab-iv
```

```text
crates/       Rust desktop, measurement core, and transport
bindings/     Thin Python/Jupyter client
native/       Isolated native helpers and upstream notices
packaging/    Windows installer and driver-setup assets
scripts/      Build, package, and validation entry points
specs/        Reviewed instrument profiles and measurement specifications
tests/        Packaging and distribution-policy tests
docs/         Guides, development notes, references, validation, images
```

ディレクトリの役割は、`crates/`（Rust デスクトップ、測定コア、転送）、`bindings/`（薄い Python/Jupyter クライアント）、`native/`（分離されたネイティブヘルパーと上流通知）、`packaging/`（Windows インストーラーとドライバー設定資産）、`scripts/`（ビルド・パッケージ・検証の入口）、`specs/`（レビュー済み計測器プロファイルと測定仕様）、`tests/`（パッケージングと配布ポリシーのテスト）、`docs/`（ガイド、開発ノート、参照資料、検証、画像）です。

See [repository layout](docs/development/repository-layout.md) and
[CONTRIBUTING.md](CONTRIBUTING.md) before changing device or build boundaries.
Tests must not connect hardware, move stages, enable outputs, or alter drivers.

デバイス境界やビルド境界を変更する前に、[リポジトリ構成](docs/development/repository-layout.md)と [CONTRIBUTING.md](CONTRIBUTING.md) を確認してください。テストでハードウェア接続、出力有効化、ドライバー変更を行ってはいけません。

## Documentation / ドキュメント

- [Documentation index](docs/README.md): the entry point for all guides and references.
- `docs/guides/`: installation and everyday operation.
- `docs/development/`: architecture, extension points, and build/release maintenance.
- `docs/reference/`: hardware, protocols, formats, and compatibility contracts.
- `docs/validation/`: versioned evidence and measured limitations.
- `docs/images/`: repository screenshots and figures.

- [ドキュメント一覧](docs/README.md)：すべてのガイドと参照資料の入口。
- `docs/guides/`：インストールと日常の操作。
- `docs/development/`：アーキテクチャ、拡張点、ビルド／リリース保守。
- `docs/reference/`：ハードウェア、プロトコル、形式、互換性契約。
- `docs/validation/`：バージョン管理された証跡と計測上の制約。
- `docs/images/`：リポジトリのスクリーンショットと図。

## KohdaLab applications / KohdaLabのアプリケーション

[IV](https://github.com/woolen-yarn/kohdalab-iv) · [TRKR](https://github.com/woolen-yarn/kohdalab-trkr) · [Camera](https://github.com/woolen-yarn/kohdalab-camera)

The applications share a root Cargo workspace, `crates/` source packages, focused
build scripts, and the same documentation categories. Hardware backends and
qualification remain product-specific.

これらのアプリケーションは、ルート Cargo ワークスペース、`crates/` のソースパッケージ、目的別ビルドスクリプト、共通のドキュメント分類を共有しています。ハードウェアバックエンドと認定は製品ごとに異なります。

## License / ライセンス

Project code is [MIT licensed](LICENSE). Native helpers and redistributed
dependencies retain their own licenses; keep the accompanying notices and source.
See [redistribution terms](REDISTRIBUTION.md), [code signing](CODE_SIGNING_POLICY.md), and [privacy](PRIVACY.md).

プロジェクトコードは [MIT ライセンス](LICENSE)です。ネイティブヘルパーと再配布する依存関係にはそれぞれのライセンスが適用されるため、付属の通知とソースを保持してください。[再配布条件](REDISTRIBUTION.md)、[コード署名](CODE_SIGNING_POLICY.md)、[プライバシー](PRIVACY.md)も参照してください。
