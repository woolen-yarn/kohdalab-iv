# KohdaLab IV documentation / ドキュメント


- [Download for your OS / OS別ダウンロード](https://github.com/woolen-yarn/kohdalab-iv/releases/latest)
- [Install and try Simulation / インストールと模擬操作](guides/quick-start.md)
- [Hardware and connections / 対応機器と接続](reference/hardware.md)
- [Python and external API / Pythonと外部API](reference/common-control-api.md)

[Product overview / 製品概要](../README.md) · [Downloads / ダウンロード](https://github.com/woolen-yarn/kohdalab-iv/releases) · [Contributing / 開発への参加](../CONTRIBUTING.md)

## User guides / 操作ガイド

- [Application guide / アプリ操作ガイド](guides/application-guide.md)
- [Hardware validation / 実機検証](guides/hardware-validation.md)
- [GS210 + 34411A acceptance record template / GS210 + 34411A 実機受入記録の雛形](validation/gs210-34411a-acceptance.md)
- [Portable guide / portable版ガイド](guides/portable-guide.md)
- [Product details / 製品詳細](guides/product-details.md)
- [Source and meter values / ソース設定値とメーター実測値](guides/source-and-meter-values.md)
- [Quick start / はじめに](guides/quick-start.md)
- [Shared instruments (development) / 複数アプリのGPIB共有・開発版](guides/shared-instruments-development.md)

## Development / 開発

- [Repository layout / 構成](development/repository-layout.md)
- [Rust optimization / Rust最適化](development/rust-optimization.md)
- [Windows signing / Windows署名](development/windows-signing.md)
- [Shared instrument service proposal / アプリ間の機器共有設計](development/shared-instrument-service.md)

## Reference / 仕様・対応情報

- [Dual source and GUI control / 2ソースとGUI制御](reference/dual-source-and-gui-control.md)
- [CSV format / CSV形式](reference/csv-format.md)
- [Hardware / 対応機器](reference/hardware.md)
- [Helper protocol / helper通信仕様](reference/helper-protocol.md)
- [Instrument precision / 機器精度と表示](reference/instrument-precision.md)
- [Legacy instruments / 旧機器の移植](reference/legacy-instruments.md)
- [Manual DC output / 手動DC出力](reference/manual-dc-output.md)
- [Measurement output bundle / 測定データの保存セット](reference/measurement-output.md)
- [Measurement spec / 測定仕様](reference/measurement-spec.md)
- [Meter ranging / メーターのAuto・固定レンジ](reference/meter-ranging.md)
- [Settings storage / 設定の保存先](reference/settings-storage.md)
- [Redistribution audit / 再配布監査](reference/redistribution-audit.md)

## Validation and evidence / 検証記録

- [Performance comparison / 性能比較](validation/performance-comparison.md)
- [Sweep latency / スイープ遅延](validation/sweep-latency.md)
- [Shared instrument service (development) / 機器共有の模擬検証](validation/shared-instruments.md)

- [GUI stack verification / GUI基盤の検証](validation/maintained-gui-stack.md)
- [Historical release records / 過去のリリース記録](validation/releases/): preserved snapshots and checksums. / 当時の記録とチェックサムを保持。

## Project policies / プロジェクト方針

- [CONTRIBUTING.md / 開発への参加](../CONTRIBUTING.md)
- [SECURITY.md / セキュリティ方針](../SECURITY.md)
- [SAFETY.md / 安全上の注意](../SAFETY.md)
- [CODE_SIGNING_POLICY.md / コード署名方針](../CODE_SIGNING_POLICY.md)
- [PRIVACY.md / プライバシー](../PRIVACY.md)
- [REDISTRIBUTION.md / 再配布条件](../REDISTRIBUTION.md)
- [CHANGELOG.md / 変更履歴](../CHANGELOG.md)
- [ROADMAP.md / 今後の計画](../ROADMAP.md)

English and Japanese share the links above. Machine-readable JSON evidence, source identifiers, and exact upstream license texts remain original. / 英日とも上の同じリンクをご利用ください。機械可読のJSON証跡、ソース識別子、上流ライセンス本文は原本を保持しています。

## Suite integration / アプリ連携

- [Common GUI control / 共通GUI API](reference/common-control-api.md)
- [GUI API and ownership / GUI APIと所有権](reference/gui-api.md)
- [GUI API v2 verification / GUI API v2の検証](validation/gui-api-v2.md)
- [Common API validation / 共通APIの検証](validation/common-control-api.md)

- [Suite quality audit / 5アプリ品質点検](validation/suite-quality-audit.md)

## Historical notes and media / 過去の記録と画像

- [Native migration plan (archived) / ネイティブ移行時の計画（履歴）](development/next-version-plan.md)
- [Release notes / リリースノート](release-notes.md)
- [Application settings in alpha.17 / alpha.17の設定検証](validation/application-settings-alpha17.md)
- [Manual DC in alpha.16 / alpha.16の手動DC検証](validation/manual-dc-alpha16.md)
- [Screenshot notes / スクリーンショットの記録](images/README.md)
