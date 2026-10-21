# Windows Code Signing

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
