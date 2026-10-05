# Portable release and rebuild guide

The desktop portable includes Python and its dependencies. Extract the whole
ZIP and open KohdaLab IV.app (macOS) or KohdaLab IV.exe (Windows). Keep the
support folder and, on Windows, the _internal folder beside the application.
All distribution filenames and documentation are in English.

## Downloads

- [Kohdalab release](https://github.com/Kohdalab/kohdalab-iv/releases/tag/portable-v0.2.3)
- [Personal mirror](https://github.com/woolen-yarn/kohdalab-iv/releases/tag/portable-v0.2.3)

Choose macos-arm64 for Apple Silicon or windows-x64 for 64-bit Windows.
The older v0.2.3 release contains the Python package; portable-v0.2.3 is the
separate desktop distribution. SHA256SUMS.txt identifies the exact archives.

## Connections

Native direct USB supports Yokogawa GS210 (0B21:0039, TMC mode) and
Agilent/Keysight 34411A (0957:0A07). USB storage mode does not provide this
measurement transport. Native GPIB supports NI GPIB-USB-HS+ and Agilent 82357B.
Connect one GPIB adapter at a time. Select the matching Device and Resource for
each instrument. Refresh discovers connected USB instruments and GPIB listeners;
configured addresses alone are not evidence of a connected device.
The actual instrument identity is checked before accepting the connection.

Python and a vendor VISA installation are not required for these native
connections. This does not imply support for every USB instrument or adapter.
Other instruments and transports in the source toolkit can still use VISA.

Windows may request administrator approval for initial WinUSB setup or newly
connected supported USB interfaces. Keep support/USB-Setup.exe and libwdi.dll.
To restore a manufacturer driver, disconnect the application and remove the
instrument's WinUSB driver assignment in Device Manager, then install the
manufacturer driver if required. Do not delete Windows system driver files.
Starting this portable again can restore its WinUSB assignment after approval.

On macOS, 82357B requires a separate initial firmware download with
support/setup-82357b-firmware.command. Firmware is not redistributed.
The Mac app requires Apple Silicon and macOS 13 or later. It is ad-hoc signed,
not notarized. If blocked, attempt launch and use Open Anyway in System
Settings > Privacy & Security. No system security setting needs to be disabled.

Refresh shows scan phases and elapsed time. If USB stalls after reconnecting,
close the app, unplug and reconnect the affected USB instrument, then retry.
Diagnostics and example configurations are in support. User settings and
results are kept under the user's .kohdalab directory unless another output
location is selected; updating the portable does not erase those results.

## Rebuild kits

The project includes source build kits in portable/build-kits. The same kits
are release assets. Extract a kit into a new folder; it includes a project
snapshot, dependency sources, English README and a platform launcher.
Build machines need uv and network access for Python dependencies. Instrument
PCs running the finished portable do not need these build tools.

- Windows: double-click Build-Windows.cmd. It runs scripts/build_windows_portable.ps1
  and opens Release containing the portable ZIP and checksum. The supplied
  Windows native binaries are hash-checked before use. To rebuild those
  binaries, use scripts/build_native_windows.sh and scripts/build_usb_setup.sh
  in MSYS2 UCRT64; see docs/windows_portable.md.
- Mac: install Xcode Command Line Tools and uv, then run Build-macOS.command.
  It compiles native helpers from the supplied native/vendor sources, freezes
  the GUI and packages the app in project/dist/distribution. No historical
  dist folder or previous package is needed.

From a source checkout, use scripts/build_macos_portable.command or
scripts/build_windows_portable.ps1. Regenerate the kits with
scripts/create_macos_release_build_kit.py and
scripts/create_windows_release_build_kit.py. Build kits intentionally exclude
other build-kit archives to avoid recursive packaging.

## Validation and licenses

The user reported native USB and both GPIB adapters working in the development
series and confirmed the final Windows portable. Automated checks cover
identity mismatches, repeated discovery and timeout recovery, GUI startup,
packaging, and simulated sweeps. These checks do not certify measurement
accuracy or exhaustive long-running operation with every instrument.

The Python application is MIT licensed. Native helpers include GPL components
and statically linked LGPL libraries; corresponding source and build scripts
are included in support/SOURCE.zip. Retain licenses and source when distributing.
Windows build kits also supply libwdi source and its license notices.
