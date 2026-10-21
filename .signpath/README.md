# SignPath application preparation

Status: application preparation only; no account, organization ID, API token,
certificate or accepted Foundation subscription is configured here.

Use [CODE_SIGNING_POLICY.md](../CODE_SIGNING_POLICY.md) and
[PRIVACY.md](../PRIVACY.md) as public policy links in the application.
The application-review repository is the public `woolen-yarn/kohdalab-iv`;
use its public README, policy, build logs and release links for the application.
The laboratory mirror may require access and is not the public evidence source.
The public application is described in the root README, with existing installer
and portable releases. `release-build.yml` builds Windows helpers and the Rust
application from source on GitHub-hosted runners, records metadata/signatures,
and publishes unsigned workflow artifacts for review. A manual workflow run
requires no signing credentials and does not publish a GitHub release.

The separate `signing-input-<commit>` workflow artifact contains the five
allowlisted PE files at the ZIP paths used below (GUI/CLI at root, helpers under
`support/`). The version-rooted portable ZIP is a different review artifact; do
not submit it using the flat signing configuration.

`application-pe.xml` is a proposed exact allowlist for first-party PE files in
a GitHub workflow ZIP, with product/version constraints. It is not an activated
configuration or a full installer-signing pipeline. Do not add `**/*.exe` or the
upstream libwdi DLL/GPIB adaptation to it without Foundation review.

## Maintainer steps after acceptance

1. Confirm the responsible owner, reviewers and signing approvers, MFA, and
   repository access with SignPath. Confirm the public build repository above.
2. Install the SignPath GitHub App, link the trusted GitHub build system and
   configure required manual approval on the production signing policy.
3. Resolve upstream GPIB/libwdi and NSIS installer/uninstaller signing boundaries
   with SignPath before claiming a fully signed distribution.
4. Configure organization/project/policy IDs and a restricted API token in a
   protected GitHub environment. Use the official SignPath submit action only
   for artifacts uploaded by the same trusted workflow. Never sign artifacts
   uploaded manually from a maintainer PC under a verified-build policy.
5. Sign the allowlisted payload, validate product/version and signer identities,
   then regenerate FILES.json and portable ZIP. Sign the generated uninstaller
   through an approved NSIS hook before compiling/signing Setup.exe. The current
   unsigned NSIS builder has no such hook; this integration remains required.
6. Verify signatures and timestamps, regenerate final checksums, test installation,
   update and removal, and mirror the identical final files to both repositories.
7. Publish the Foundation credit only after acceptance. Do not present the
   current preview or workflow artifacts as signed.

The Foundation's acceptance decision, account access and MFA confirmation cannot
be supplied by repository code. See [official terms](https://signpath.org/terms)
and [GitHub integration](https://docs.signpath.io/trusted-build-systems/github).

---

# SignPath 申請準備（日本語）

状態: ここでは application preparation のみです。account、organization ID、API token、certificate、accepted Foundation subscription は設定していません。

application の public policy link には [CODE_SIGNING_POLICY.md](../CODE_SIGNING_POLICY.md) と [PRIVACY.md](../PRIVACY.md) を使用します。application-review repository は public `woolen-yarn/kohdalab-iv` であり、application にはその public README、policy、build log、release link を使用します。laboratory mirror は access を要する場合があり public evidence source ではありません。public application は root README に既存 installer/portable release とともに記述されています。`release-build.yml` は GitHub-hosted runner 上で Windows helper/Rust application を source から build し、metadata/signature を記録して review 用 unsigned workflow artifact を公開します。manual workflow run は signing credential を必要とせず GitHub release を publish しません。

別の `signing-input-<commit>` workflow artifact は、下記 ZIP path（GUI/CLI は root、helper は `support/`）にある five allowlisted PE file を含みます。version-rooted portable ZIP は別の review artifact であり flat signing configuration で submit してはいけません。

`application-pe.xml` は GitHub workflow ZIP 内 first-party PE file の product/version constraint 付き proposed exact allowlist です。activated configuration/full installer-signing pipeline ではありません。Foundation review なしに `**/*.exe` や upstream libwdi DLL/GPIB adaptation を追加してはいけません。

## acceptance 後の maintainer 手順 / Maintainer steps after acceptance

1. responsible owner、reviewer、signing approver、MFA、repository access を SignPath と確認し、上記 public build repository を確認します。
2. SignPath GitHub App を install し trusted GitHub build system を link、production signing policy に必要な manual approval を設定します。
3. fully signed distribution と主張する前に upstream GPIB/libwdi、NSIS installer/uninstaller signing boundary を SignPath と解決します。
4. protected GitHub environment に organization/project/policy ID と restricted API token を設定します。同じ trusted workflow が upload した artifact にのみ official SignPath submit action を使います。verified-build policy の下で maintainer PC から手動 upload した artifact を署名してはいけません。
5. allowlisted payload を署名し product/version/signer identity を validate、次に FILES.json/portable ZIP を regenerate します。Setup.exe の compile/sign 前に approved NSIS hook で generated uninstaller を署名します。current unsigned NSIS builder にはこの hook がなく、この integration は必要なままです。
6. signature/timestamp を verify、final checksum を regenerate、installation/update/removal を test し、identical final file を両 repository に mirror します。
7. Foundation credit は acceptance 後にのみ publish します。current preview/workflow artifact を signed と表現してはいけません。

Foundation の acceptance decision、account access、MFA confirmation は repository code では提供できません。[official terms](https://signpath.org/terms) と [GitHub integration](https://docs.signpath.io/trusted-build-systems/github) を参照してください。
