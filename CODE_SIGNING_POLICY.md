# Code signing policy / コード署名ポリシー

> **日本語の案内:** 以下は署名サービス、責任分界、配布時の確認事項を示す運用ポリシーです。
> SignPath の契約条件やソフトウェアのライセンス本文ではありません。固有名詞、実行ファイル名、
> 将来の承認状況に関する英語本文を正として参照してください。

## Current status

KohdaLab IV has not yet been accepted by SignPath Foundation. Current Windows
release files are unsigned. This policy prepares the application; it is not a
claim of sponsorship, certification or an existing signing service. macOS ad hoc
signatures are separate and do not provide Apple Developer ID notarization.

## Responsibility and approval

The public source and application-review repository is
[woolen-yarn/kohdalab-iv](https://github.com/woolen-yarn/kohdalab-iv).
[Kohdalab/kohdalab-iv](https://github.com/Kohdalab/kohdalab-iv) is the laboratory
mirror and may require organization access; its visibility is not changed by a release.
The currently designated project maintainer, committer and reviewer is
[@woolen-yarn](https://github.com/woolen-yarn), also named in `.github/CODEOWNERS`.
The proposed signing approver is that maintainer. Before enabling production
signing, the repository owner must confirm these roles in SignPath and enable
multi-factor authentication for all repository/signing team accounts. This
repository does not claim that account settings have already been verified.

Every production signing request must be manually approved by the designated
approver after source review and successful CI. Signing credentials stay in the
signing service and protected GitHub secrets. Pull requests from untrusted
contributors must never receive signing secrets. The same signed artifacts and
checksums will be mirrored to both repositories; mirrors must not rebuild them.

## Signing boundaries

The intended first-party PE allowlist is `kohdalab-iv.exe`, `iv-cli.exe`,
`support/kohdalab-usbtmc-helper.exe`, `support/kohdalab-visa-helper.exe` and
`support/USB-Setup.exe`. Each carries the product name `KohdaLab IV` and the same
application product version. Source-built artifacts and build logs must be
available for review before signing is enabled.

`support/libwdi.dll` and its embedded installer are upstream OSS components.
The GPIB helper contains Linux-GPIB/KAME-derived code. These are disclosed in
[REDISTRIBUTION.md](REDISTRIBUTION.md) and must not be silently signed as solely
project-owned software. Their signing eligibility must be agreed with SignPath;
redistribution permission and Foundation signing eligibility are different.
The installer and generated uninstaller also require an approved signing path.
No blanket recursive signing of all EXEs/DLLs is permitted.

## Privacy and system changes

See [Privacy policy](PRIVACY.md). Measurement files and configuration stay local
unless the user explicitly exports or transfers them. Instrument communication
is directed to user-selected devices; there is no application telemetry.

Windows Setup announces installation and uses Windows administrator approval.
It preserves configuration, measurements and driver assignments on update and
uninstall. The separately invoked USB setup tool announces driver changes and
requires administrator approval. No security controls are disabled.

## After acceptance

Only after Foundation acceptance and activation, publish the required credit:
“Free code signing provided by [SignPath.io](https://about.signpath.io/),
certificate by [SignPath Foundation](https://signpath.org/).”
This sentence describes the future attribution requirement, not current support.

Actual signature status and publisher identity will be stated on each release.
A signature identifies a publisher and checks file integrity; it does not
certify instrument accuracy or guarantee the absence of Windows warnings.

References: [Foundation terms](https://signpath.org/terms),
[GitHub build origin verification](https://docs.signpath.io/trusted-build-systems/github).

---

# コード署名ポリシー

## 現在の状態

KohdaLab IV はまだ SignPath Foundation に受理されていません。現在の Windows リリースファイルは
未署名です。このポリシーはアプリケーションを準備するものであり、支援、認証、既存の署名サービスを
主張するものではありません。macOS の ad hoc 署名は別のもので、Apple Developer ID 公証を提供しません。

## 責任と承認

公開ソースおよびアプリケーション審査リポジトリは
[woolen-yarn/kohdalab-iv](https://github.com/woolen-yarn/kohdalab-iv) です。
[Kohdalab/kohdalab-iv](https://github.com/Kohdalab/kohdalab-iv) は研究室ミラーであり、組織アクセスが
必要な場合があります。リリースで可視性を変更しません。現在指定されたプロジェクト保守者、コミッター、
レビューアーは `.github/CODEOWNERS` にも記載される [@woolen-yarn](https://github.com/woolen-yarn) です。
提案する署名承認者もこの保守者です。本番署名を有効化する前に、リポジトリ所有者は SignPath でこれらの
役割を確認し、すべてのリポジトリ／署名チームアカウントに多要素認証を有効化しなければなりません。
本リポジトリは、アカウント設定がすでに検証済みとは主張しません。

本番署名要求はすべて、ソースレビューと CI 成功後に指定承認者が手動承認します。署名資格情報は署名サービスと
保護された GitHub secrets 内に留めます。信頼できない寄稿者からの pull request に署名 secrets を渡しては
いけません。同じ署名済み artifacts と checksums を両リポジトリへミラーします。ミラーは再ビルドしてはいけません。

## 署名境界

意図する第一者 PE allowlist は `kohdalab-iv.exe`、`iv-cli.exe`、
`support/kohdalab-usbtmc-helper.exe`、`support/kohdalab-visa-helper.exe`、
`support/USB-Setup.exe` です。各ファイルは製品名 `KohdaLab IV` と同じ application product version を
持ちます。ソースからのビルド artifacts と build logs は署名有効化前にレビュー可能でなければなりません。

`support/libwdi.dll` とその埋め込みインストーラーは上流 OSS コンポーネントです。GPIB helper には
Linux-GPIB/KAME 由来のコードが含まれます。これらは [REDISTRIBUTION.md](REDISTRIBUTION.md) で開示され、
プロジェクトのみが所有するソフトウェアとして黙って署名してはいけません。署名適格性は SignPath と合意する
必要があります。再配布許可と Foundation の署名適格性は異なります。インストーラーと生成 uninstaller にも
承認済み署名経路が必要です。すべての EXE/DLL を再帰的に包括署名することは許可しません。

## プライバシーとシステム変更

[プライバシーポリシー](PRIVACY.md)を参照してください。測定ファイルと設定は利用者が明示的に
エクスポートまたは転送しない限りローカルに留まります。計測器通信は利用者が選んだデバイスに向けられ、
アプリケーションのテレメトリーはありません。

Windows Setup はインストールを告知し、Windows 管理者承認を使います。更新とアンインストールで設定、
測定、ドライバー割当を保持します。別途起動する USB setup tool はドライバー変更を告知し管理者承認を
必要とします。セキュリティ制御を無効化しません。

## 受理後

Foundation の受理と有効化の後にのみ、次の必要なクレジットを公開します。
“Free code signing provided by [SignPath.io](https://about.signpath.io/), certificate by
[SignPath Foundation](https://signpath.org/).” この文は将来の帰属要件を示すもので、現在の支援ではありません。

実際の署名状態と publisher identity は各リリースに記載します。署名は publisher を識別しファイル整合性を
確認しますが、計測器精度を認証したり Windows warning がないことを保証したりしません。

参照: [Foundation terms](https://signpath.org/terms)、
[GitHub build origin verification](https://docs.signpath.io/trusted-build-systems/github)。
