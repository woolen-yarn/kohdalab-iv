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
