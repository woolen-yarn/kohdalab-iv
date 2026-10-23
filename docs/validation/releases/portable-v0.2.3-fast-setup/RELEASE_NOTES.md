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

## Windows update - 2026-10-07

The Windows portable ZIP and both source build kits have been refreshed.
The macOS application ZIP is unchanged. Existing README download links fetch
the updated Windows package. Verify the downloads against SHA256SUMS.txt.

- Fix NI GPIB-USB-HS+ initialization with WinUSB assigned only to its GPIB
  communication interface; the unused analyzer interface needs no driver.
- Recheck driver readiness before Refresh, recover removed NI device nodes,
  and wait for active devices/interfaces before scanning. Failed setup does
  not start discovery.
- Fix the libwdi log reader's missing delete-sharing access, which blocked
  Windows SetupAPI log replacement. Full NI driver-package reinstallation
  dropped from about 70 seconds to 2.89-3.18 seconds on the test PC. Direct
  USB DMM recovery took 3.16 seconds; final portable recovery took 2.93 seconds.
  Windows policy, approval, and hardware can change these times.
- Preserve signed-package verification, exact-instance assignment, and the
  fixed USB allowlist. No manufacturer VISA installation was added.

NI GPIB and direct USB open-output sweeps passed with the frozen application
measurement engine and final portable helpers: -1, 0, +1 mV, 1 mA hardware
current limit, three saved CSV rows, no SCPI errors, zero source level and
output OFF afterward. The user also confirmed the revised portable working.
Physical tests used the frozen CLI; GUI startup and sequencing were checked
separately. These tests do not certify loaded measurement accuracy.

Matching source and rebuild scripts are included in support/SOURCE.zip and
the updated build kits.
