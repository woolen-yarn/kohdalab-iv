# Code signing policy

## Current status

KohdaLab IV has not yet been accepted by SignPath Foundation. Current Windows
release files are unsigned. This policy prepares the application; it is not a
claim of sponsorship, certification or an existing signing service. macOS ad hoc
signatures are separate and do not provide Apple Developer ID notarization.

## Responsibility and approval

The public source repositories are [Kohdalab/kohdalab-iv](https://github.com/Kohdalab/kohdalab-iv)
and [woolen-yarn/kohdalab-iv](https://github.com/woolen-yarn/kohdalab-iv).
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

The intended first-party PE allowlist is `kohdalab-iv-next.exe`, `iv-cli.exe`,
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
