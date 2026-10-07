# KohdaLab libwdi build

Upstream: libwdi v1.5.0, commit
`90278c538a8fb5fd82aab25ae7f5a9887ca468ce` from
https://github.com/pbatard/libwdi . Complete source and LGPL/GPL license texts
are supplied here. This build is for Windows 10/11 x64 only.

Changes dated 2026-10-01:

- Enable inbox WinUSB with no WDK coinstaller or third-party kernel binary.
- Use a WinUSB INF referencing the OS-provided winusb.inf service sections,
  with an interface GUID, and a generated catalog covering that INF alone.
- Supply WinUSB package version metadata without extracting a coinstaller.
- Change the embedded installer to use DiInstallDevice for the exact enumerated
  PnP device instance. The original all-matching-hardware-ID update is not used.
- Preserve the system restore configuration rather than temporarily disabling it.

libwdi generates and signs a device-specific driver package and installs its
publisher certificate in the Windows machine certificate stores, as its standard
installation code does. Windows driver-signature enforcement and boot security
settings are not disabled. Administrative privileges and Windows policy approval
are required; an organization may prohibit locally generated driver packages.

Rebuild the replaceable libwdi DLL and setup application with
`scripts/build_usb_setup.sh`. The independent setup source is
`native/windows_usb_setup.cpp` (MIT). The DLL remains replaceable by a modified
compatible libwdi build. The supplied binaries are cross-compiled candidates;
Windows installation and hardware operation must be verified before release.

Changes dated 2026-10-07:

- Allow Windows to replace/rotate the SetupAPI log while the diagnostic reader
  holds it open (`FILE_SHARE_DELETE`). Without this flag Windows retried log
  replacement for about 34 seconds in each installation pass.
- Record installer phase timing in `driver-install-timing.log` for diagnosis.
- Preserve the original signed-package staging and exact-instance installation.
  No signature checks, device allowlist rules, or restore settings were relaxed.
