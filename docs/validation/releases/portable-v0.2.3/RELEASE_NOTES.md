KohdaLab IV v0.2.3 portable desktop release for macOS Apple Silicon and Windows x64.

## Downloads

- **macos-arm64-portable.zip**: extract the entire archive and open KohdaLab IV.app. Requires Apple Silicon and macOS 13 or later.
- **windows-x64-portable.zip**: extract the entire archive and open KohdaLab IV.exe. Keep _internal and support beside the executable.
- **macos-release-build-kit.zip / windows-release-build-kit.zip**: source snapshots and platform launchers for rebuilding; these are not the end-user applications.
- **SHA256SUMS.txt**: checksums of all four ZIP assets.

Python and vendor VISA are not required for supported native connections: GS210 USB in TMC mode, Agilent/Keysight 34411A USB, and GPIB through NI GPIB-USB-HS+ or Agilent 82357B. Connect one GPIB adapter at a time. Windows can request administrator approval for WinUSB setup. On macOS, 82357B needs the included initial firmware-download script. The Mac app is ad-hoc signed and not notarized; see the English README for Open Anyway instructions.

Refresh displays progress and elapsed time, discovers unregistered GPIB listeners and supported USB instruments, and checks actual instrument identity before accepting a Device/Resource pairing. Dropdowns expand when opened without widening the original fields. Windows opens the GUI before driver setup begins.

The user reported native USB and both GPIB adapters working and confirmed the final Windows portable. Local automated validation passed 349 tests with 100% statement and branch coverage, source-package checks, native framing self-tests and Mac signature/GUI startup checks. This is not a certification of measurement accuracy or every possible hardware configuration.

English documentation, examples, diagnostic tools, licenses and corresponding native/application source are included. Rebuild kits are also stored in the project under portable/build-kits. The original Python v0.2.3 wheel/source release remains available separately.
