KohdaLab IV v1.0.1

Windows portable packages now include an optional application installer.
Extract the entire ZIP, double-click Install-Application.cmd, approve the Windows
administrator prompt, and start KohdaLab IV from its desktop shortcut.

- Application: C:\Program Files\Kohdalab\KohdaLab IV.
- Settings and default measurements: %USERPROFILE%\.kohdalab.
- Driver setup/cache: %LOCALAPPDATA%\KohdaLab IV, unchanged by app installation.
- The shortcut starts in the user data folder, so relative result paths remain
  writable. The application is not launched with elevated privileges by setup.
- Close IV and run the installer from a new ZIP to update. The installer verifies
  package files against FILES.json and retains a verified backup before replacing an existing
  installation. Existing installation access controls are retained.
- Direct portable use remains available by opening kohdalab-iv-next.exe without
  running the installer. After a successful installation the extracted download
  folder is no longer needed.

Download from the official GitHub release links. The bundled file hashes check
internal consistency and are not a publisher signature.

The app installer is separate from support/USB-Setup.exe for instrument WinUSB
assignments. Setup does not alter PowerShell execution policy or instrument
settings, run measurements, or download firmware. Windows administrator approval
may still be needed for app placement and separately for instrument drivers.

Mac portable, Windows portable and the source build kit have matching v1.0.1
versions. README install/update instructions and both repositories' download
links are updated. Start / Stop / Save and v1.0.0 measurement behavior remain.
Mac bundles are ad hoc signed and not Apple-notarized. 82357B requires separate
authorized initialization; no firmware acquisition or upload is included.
Native corresponding source and GPL/LGPL licensing/rebuild material remain in
the packages.

Validation includes isolated Windows installation, reinstallation, update,
backup/data preservation, shortcut, malformed-package rejection and failed-update
rollback tests;
installation and simulation from Program Files on Windows; Mac portable
simulation; three-OS Rust/Python tests and dependency audit. No physical source
output was enabled for this packaging update.
