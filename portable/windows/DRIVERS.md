# Windows USB setup

Connect supported instruments, then launch `KohdaLab IV.exe`.
The app automatically checks initial USB setup. If setup is required, approve
the Windows administrator/driver prompts. Afterwards, click Refresh, select the
actual Source and Meter resources and connect them. No separate Python, VISA,
manufacturer installer or Zadig download is needed for these native connections.

Direct USB communication and GPIB communication with both adapters have been
reported working on the user's Windows instrument PC. Recent Refresh, identity
validation and distribution-layout changes still require confirmation there.
Windows may request publisher/driver confirmation, a reconnect, or a restart.
A system administrator's policy may prohibit the generated driver package.

## Supported devices

| Device | USB vendor:product ID | Required mode |
| --- | --- | --- |
| YOKOGAWA GS210 (may appear as GS200) | 0B21:0039 | TMC |
| Agilent/Keysight 34411A | 0957:0A07 | USBTMC |
| NI GPIB-USB-HS+ | 3923:7618 | GPIB adapter |
| Agilent 82357B | 0957:0518 and 0957:0718 | Boot and operational USB IDs |

Set GS210 to TMC mode before connecting. Storage-mode devices, nonzero composite
interfaces and other USB IDs are excluded from this setup's selection. Do not
change to Storage mode while using the native measurement connection.
Only one supported GPIB adapter may be connected at a time. GPIB0 uses controller
address 0 and scans listeners at primary addresses 1-30, including unregistered
models. Secondary addresses and multiple adapters are not supported.

## What initial setup does

- Checks the fixed supported-device allowlist and existing WinUSB interface GUID.
- Requests administrator privileges only when configuration is required.
- Uses a bundled, replaceable libwdi DLL to generate, sign and install a small
  device-specific package pointing to Windows' own WinUSB driver. It installs
  the generated publisher certificate in the machine certificate stores.
- Assigns the driver to the selected PnP instance, not every matching device.
- For 82357B, downloads checksum-verified upstream firmware and stages the
  operational USB ID's package before firmware re-enumeration. First-time
  firmware setup requires Internet access; cached firmware works offline.

The app and installer do not disable Windows driver-signature enforcement or
boot security settings. Setup replaces the manufacturer's driver for selected
native devices; vendor VISA tools may no longer access those devices. To use
those tools later, restore the manufacturer's driver. The original system
restore settings are preserved. No SCPI commands or measurement sweeps are
sent by USB setup.

Existing WinUSB configurations with an interface GUID are reused. To check or
retry manually, run `USB-Setup.exe` or `Setup-Instrument-Drivers.cmd` inside
`support`. Keep `libwdi.dll` beside USB-Setup.exe. This is a one-time
USB configuration, not an installation of the KohdaLab application.

## Logs and firmware

Setup log and firmware are in `%LOCALAPPDATA%\KohdaLab IV`:

```text
usb-setup.log
measat_releaseX1.8.hex
usb-drivers/
```

The setup log records target device IDs and previous driver services. Windows
also records driver installation in its SetupAPI log. Keep exact error messages
if setup fails. Do not repeatedly start setup while a driver installation is
still running. Resolve pending Windows installation/restart requests first.

Firmware is not redistributed in the ZIP. The upstream download and expected
SHA256 are pinned in the included setup source and manual firmware script.
If first-time download fails, connect to the Internet and retry. No source
output is enabled during setup. GPIB identification later initializes IFC/REN.

## Instrument checks

With the GUI closed, verify complete identities before a measurement. Replace
GPIB addresses 3 and 7 with the instruments' actual addresses:

```powershell
& '.\support\diagnostics\usbtmc-probe.exe' --idn
& '.\support\diagnostics\gpib-probe.exe' --idn 3 7
```

Choose the probe for your connection. For an 82357B, the first native GPIB open
loads firmware into RAM and the adapter re-enumerates. If Windows needs more
installation time, wait, reconnect if requested, and retry. Use the app's Refresh
and select the actual resources rather than retaining example serials/addresses.
Hardware identification, measurement, Stop and output-off need Windows tests.

## Source and alternatives

Complete setup/helper/dependency source, licenses and rebuild scripts are in
`application/native` and `application/scripts` after extracting `support/SOURCE.zip`. The libwdi
modifications are documented in `native/vendor/libwdi/KOHDALAB-MODIFICATIONS.md`.
The LGPL DLL can be replaced with a compatible modified build.

Zadig is an optional manual fallback if the dedicated installer fails; it is not
required by the normal flow. Use only the listed USB IDs and TMC interfaces.
- libwdi: https://github.com/pbatard/libwdi
- WinUSB: https://learn.microsoft.com/en-us/windows-hardware/drivers/usbcon/winusb-installation
- Optional Zadig fallback: https://zadig.akeo.ie/
