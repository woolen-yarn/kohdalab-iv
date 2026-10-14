# Third-party notices — native portable

KohdaLab IV is a distribution of separately licensed native programs and
libraries. The root MIT license does not cover third-party or GPL-only code.
See REDISTRIBUTION.md for obligations and SOURCE.zip/native/REBUILDING.md for
rebuilding/relinking and replacing libraries.

- Rust application / CLI / thin Python control client: MIT; all target-specific
  dependency and font license notices are supplied in licenses/rust/. No Qt,
  PySide, PyInstaller, Python interpreter, NumPy or PyVISA runtime is bundled.
- kohdalab-gpib-helper[.exe]: GPL-2.0-only for the combined executable.
  KAME's userspace NI driver and Linux-GPIB permit GPLv2 or later; the Agilent
  adaptation permits GPLv2 only. Original attribution is retained in source.
  Upstream Linux-GPIB portions include Frank Mori Hess's work. GPL-2.0.txt and
  complete corresponding source/build scripts are supplied.
- kohdalab-usbtmc-helper[.exe]: MIT application source, statically linked with
  libusb 1.0.30 (LGPL-2.1-or-later, Copyright libusb contributors).
- libusb 1.0.30 is also statically linked by the GPIB helper. Its unmodified
  source archive, COPYING, helper source and relinking instructions are in
  SOURCE.zip. LGPL-2.1.txt is also supplied separately.
- Windows USB-Setup.exe: MIT application, dynamically linked with the replaceable
  libwdi.dll (LGPL-3.0-or-later, Pete Batard and contributors). Full libwdi source,
  configuration, dated changes and rebuild instructions are in SOURCE.zip.
  GPL-3.0.txt and LGPL-3.0.txt are supplied. This uses Windows' inbox WinUSB,
  not a bundled Microsoft kernel driver or WDK coinstaller.
- Windows MinGW runtime notices and GCC runtime exception are supplied in
  licenses/. System OS APIs are obtained from the installed operating system.

82357B firmware is NOT licensed by the above open-source licenses merely
because a GPL driver can upload it. This distribution does not include, obtain
or upload that firmware. Only an already initialized adapter is supported.
Do not add manufacturer firmware or vendor VISA installers to this package.

Keep SOURCE.zip, licenses, notices and REDISTRIBUTION.md with redistributed
binaries. If you modify a GPL/LGPL binary, provide its actual modified source
and update the modification notices. No manufacturer endorsement is implied.
