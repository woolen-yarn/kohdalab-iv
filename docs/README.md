# KohdaLab IV documentation / ドキュメント

[Product overview / 製品概要](../README.md) · [Downloads / ダウンロード](https://github.com/woolen-yarn/kohdalab-iv/releases) · [Contributing / 開発への参加](../CONTRIBUTING.md)

## User guides / 操作ガイド

- [Application guide / アプリ操作ガイド](guides/application-guide.md)
- [Hardware validation / 実機検証](guides/hardware-validation.md)
- [Portable guide / portable版ガイド](guides/portable-guide.md)
- [Product details / 製品詳細](guides/product-details.md)
- [Quick start / はじめに](guides/quick-start.md)
- [Shared instruments (development) / 複数アプリのGPIB共有・開発版](guides/shared-instruments-development.md)

## Development / 開発

- [Next version plan / 次版の計画](development/next-version-plan.md)
- [Repository layout / 構成](development/repository-layout.md)
- [Rust optimization / Rust最適化](development/rust-optimization.md)
- [Windows signing / Windows署名](development/windows-signing.md)
- [Shared instrument service proposal / アプリ間の機器共有設計](development/shared-instrument-service.md)

## Reference / 仕様・対応情報

- [CSV format / CSV形式](reference/csv-format.md)
- [Hardware / 対応機器](reference/hardware.md)
- [Helper protocol / helper通信仕様](reference/helper-protocol.md)
- [Instrument precision / 機器精度と表示](reference/instrument-precision.md)
- [Legacy instruments / 旧機器の移植](reference/legacy-instruments.md)
- [Measurement spec / 測定仕様](reference/measurement-spec.md)
- [Meter ranging / メーターのAuto・固定レンジ](reference/meter-ranging.md)
- [Redistribution audit / 再配布監査](reference/redistribution-audit.md)

## Validation and evidence / 検証記録

- [Performance comparison / 性能比較](validation/performance-comparison.md)
- [Sweep latency / スイープ遅延](validation/sweep-latency.md)
- [Shared instrument service (development) / 機器共有の模擬検証](validation/shared-instruments.md)

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
- [Common API validation / 共通APIの検証](validation/common-control-api.md)
