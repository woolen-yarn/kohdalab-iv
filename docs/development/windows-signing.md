# Windows Code Signing / Windows コード署名

> English source is followed by a complete Japanese counterpart. File names, commands,
> signature states, and release evidence are preserved identically in both sections.

The Windows release currently contains unsigned application binaries. This is
expected for the public v1.2.0 build: the project does not own a Windows code
signing certificate and no certificate with a private key was available in the
release machine's CurrentUser or LocalMachine certificate stores.

## What was checked on the Windows test machine

`Get-AuthenticodeSignature` reported `NotSigned` for the installed application
and helper binaries, including:

- `iv-cli.exe`
- `kohdalab-iv-next.exe`
- `support/kohdalab-gpib-helper.exe`
- `support/kohdalab-usbtmc-helper.exe`
- `support/kohdalab-visa-helper.exe`
- `support/USB-Setup.exe`
- `Uninstall.exe`
- the downloaded Setup executables

No matching Code Integrity, Defender detection, SmartScreen, or AppLocker block
event was found for these files in the inspected logs. The observed Windows
warning is therefore consistent with an unsigned, low-reputation download, but
the exact block cause cannot be proven from those logs alone. The inspected
files also had no `Zone.Identifier` stream at the time of inspection.

## Release signing procedure

Signing must happen after the Windows binaries and helper DLLs are built and
before the payload manifest, ZIP, and Setup executable are generated. Use a
certificate issued for the publisher identity and a timestamp authority. The
certificate private key must remain in the signing service or protected
certificate store; it must never be committed or placed in a portable package.

After agreeing the component boundaries with the signing provider, sign and verify:

1. `iv-cli.exe`
2. `kohdalab-iv.exe`
3. the approved first-party helper allowlist (not upstream components under the project identity)
4. the NSIS-generated `Uninstall.exe`
5. the final NSIS Setup executable

The NSIS uninstaller is generated during Setup compilation, so it must be
signed before the payload is finalized or signed through an NSIS signing hook.
Signing only the outer Setup file leaves the installed binaries unsigned.

After signing, verify every file with:

```powershell
Get-AuthenticodeSignature .\path\to\file.exe | Format-List Status,SignerCertificate
```

The expected result is `Status : Valid` and a signer certificate matching the
release publisher. Recompute `FILES.json` and `SHA256SUMS.txt` after signing;
signing changes the file bytes. Then build the ZIP and Setup from that exact
payload and verify the outer artifacts again.

The repository includes a read-only release gate for this check:

```powershell
.\scripts\check_windows_signatures.ps1 -Directory .\windows-payload `
    -ReportPath .\windows-signatures.json
```

The default mode records unsigned files and exits successfully, which is useful
while signing is not yet available. Add `-RequireSigned` in the release job to
fail when any `.exe` or `.dll` is not reported as `Valid`:

```powershell
.\scripts\check_windows_signatures.ps1 -Directory .\windows-payload `
    -ReportPath .\windows-signatures.json -RequireSigned
```

Signing improves publisher identity and SmartScreen reputation over time, but
it does not guarantee that every new release opens without a first-run warning.
SmartScreen reputation is also affected by download source, distribution
volume, and the age of the certificate. Do not disable Defender, SmartScreen,
driver-signature enforcement, or Windows security policy as a workaround.

The current unsigned release remains usable through the normal Windows
approval path. Users should obtain it from the official GitHub release and
verify the published SHA-256 checksum before running it.


## SignPath preparation (1.3.0-alpha.3)

Public policy: [Code signing policy](../../CODE_SIGNING_POLICY.md). Application
notes and the proposed first-party PE configuration live in [`.signpath`](../../.signpath/README.md).
The manual `release-build.yml` workflow rebuilds native helpers and Rust binaries
on GitHub-hosted Windows runners and uploads unsigned artifacts with build-origin
metadata, PE metadata and signature reports. It has no signing token, no signing
step and no automatic GitHub release publication.

Rust and project helper executables carry the same `KohdaLab IV` product name
and Cargo application version. `check_windows_metadata.ps1` rejects missing or
inconsistent metadata. `libwdi.dll` retains its upstream identity. The GPL GPIB
adaptation is separately disclosed for Foundation eligibility review. A valid
redistribution license does not establish eligibility to sign an upstream
derivative with a Foundation project certificate.

The existing `-RequireSigned` audit checks **all** PE files. Do not enable it
blindly when the approved distribution intentionally retains unsigned upstream
files. The eventual signing gate must use the provider-approved allowlist and
validate publisher identity/timestamps, with explicit upstream exceptions.
This integration, including the NSIS uninstaller signing hook, is pending.
Foundation acceptance/MFA/account setup are maintainer actions and cannot be
claimed by committing configuration files.


Version 1.3.0-alpha.5 renames the desktop PE to `kohdalab-iv.exe`. The old
filename above records the historical v1.2.0 audit. Current allowlists and
metadata checks use only the new name; no legacy executable alias is shipped.

---

# Windows コード署名

Windows release は現在 unsigned application binaries を含みます。これは public v1.2.0 build で想定されます。project は Windows code signing certificate を所有せず、release machine の CurrentUser/LocalMachine certificate stores に private key を持つ certificate はありませんでした。

## Windows test machine で確認したこと

`Get-AuthenticodeSignature` は installed application/helper binaries で `NotSigned` を報告しました。対象は `iv-cli.exe`、`kohdalab-iv-next.exe`、`support/kohdalab-gpib-helper.exe`、`support/kohdalab-usbtmc-helper.exe`、`support/kohdalab-visa-helper.exe`、`support/USB-Setup.exe`、`Uninstall.exe`、downloaded Setup executables です。inspected logs にこれらの files の matching Code Integrity、Defender detection、SmartScreen、AppLocker block event はありませんでした。従って observed Windows warning は unsigned/low-reputation download と整合しますが、exact block cause は logs だけから証明できません。inspection 時に files は `Zone.Identifier` stream も持ちませんでした。

## Release signing procedure

Windows binaries/helper DLLs の build 後、payload manifest/ZIP/Setup executable の生成前に signing を行います。publisher identity 用 certificate と timestamp authority を使い、certificate private key は signing service/protected certificate store に残して commit/portable package へ置きません。signing provider と component boundaries を合意後、`iv-cli.exe`、`kohdalab-iv.exe`、approved first-party helper allowlist（project identity 下の upstream components を除く）、NSIS-generated `Uninstall.exe`、final NSIS Setup executable を sign/verify します。NSIS uninstaller は Setup compilation 中に生成されるため payload finalize 前に sign するか NSIS signing hook 経由で sign します。outer Setup file だけの signing では installed binaries は unsigned のままです。

signing 後は全 file を次で verify します。

```powershell
Get-AuthenticodeSignature .\path\to\file.exe | Format-List Status,SignerCertificate
```

expected result は `Status : Valid` と release publisher に一致する signer certificate です。signing は file bytes を変えるため、後に `FILES.json` と `SHA256SUMS.txt` を recompute し、exact payload から ZIP/Setup を build して outer artifacts を再 verify します。repository は read-only release gate を含みます。

```powershell
.\scripts\check_windows_signatures.ps1 -Directory .\windows-payload `
    -ReportPath .\windows-signatures.json
```

default mode は unsigned files を記録して successfully exit し、signing 未利用時に有用です。release job では any `.exe`/`.dll` が `Valid` でなければ fail する `-RequireSigned` を追加します。

```powershell
.\scripts\check_windows_signatures.ps1 -Directory .\windows-payload `
    -ReportPath .\windows-signatures.json -RequireSigned
```

signing は publisher identity と時間経過に伴う SmartScreen reputation を改善しますが、全 new release が first-run warning なしに開く保証ではありません。SmartScreen reputation は download source、distribution volume、certificate age にも影響されます。回避策として Defender、SmartScreen、driver-signature enforcement、Windows security policy を無効化してはいけません。current unsigned release は normal Windows approval path で利用でき、users は official GitHub release から取得して実行前に published SHA-256 checksum を確認します。

## SignPath 準備（1.3.0-alpha.3）

public policy は[Code signing policy](../../CODE_SIGNING_POLICY.md)です。application notes/proposed first-party PE configuration は[`.signpath`](../../.signpath/README.md)にあります。manual `release-build.yml` workflow は GitHub-hosted Windows runners で native helpers/Rust binaries を rebuild し、build-origin metadata、PE metadata、signature reports 付き unsigned artifacts を upload します。signing token/signing step/automatic GitHub release publication はありません。

Rust/project helper executables は同じ `KohdaLab IV` product name と Cargo application version を持ちます。`check_windows_metadata.ps1` は missing/inconsistent metadata を reject します。`libwdi.dll` は upstream identity を保持し、GPL GPIB adaptation は Foundation eligibility review 用に別途開示します。有効な redistribution license は Foundation project certificate で upstream derivative を sign する eligibility を確立しません。

existing `-RequireSigned` audit は**全** PE files を確認します。approved distribution が intentionally unsigned upstream files を残す場合に無条件で有効化してはいけません。eventual signing gate は provider-approved allowlist を使い、explicit upstream exceptions と共に publisher identity/timestamps を validate する必要があります。この integration（NSIS uninstaller signing hook を含む）は保留中です。Foundation acceptance/MFA/account setup は maintainer actions で、configuration files を commit しても主張できません。

Version 1.3.0-alpha.5 は desktop PE を `kohdalab-iv.exe` に rename します。上の old filename は historical v1.2.0 audit を記録します。current allowlists/metadata checks は new name だけを使い、legacy executable alias は出荷しません。
