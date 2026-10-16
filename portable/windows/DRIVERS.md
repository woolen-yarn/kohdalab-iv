# Windows USB setup

The development preview first uses an installed 64-bit official VISA runtime
for instruments it enumerates. Manufacturer driver packages alone are not a
VISA runtime. Existing WinUSB devices use the native helpers as before. A busy
or failed VISA connection is reported; it is never retried through native I/O.

Connect supported instruments. If a device has no assigned driver, run
`support/USB-Setup.exe` and approve any Windows administrator/driver prompts.
The Rust GUI does not run the setup executable automatically. Afterwards,
launch `kohdalab-iv-next.exe`, click Refresh, select the
actual Source and Meter resources and connect them. No separate Python, VISA,
manufacturer installer or Zadig download is needed for these native connections.

Direct USB communication and GPIB communication with both adapters have been
reported working on the user's Windows instrument PC. The NI HS+ Windows fix was
verified with physical open-output GS210/34411A sweeps through GPIB and direct
USB on 2026-10-07 (three points, -1 to +1 mV, 1 mA hardware current limit).
Windows may request publisher/driver confirmation, a reconnect, or a restart.
A system administrator's policy may prohibit the generated driver package.

Run `support/USB-Setup.exe` again if a device driver was removed in Device
Manager. The setup tool can recover a missing NI communication child when its
USB parent is present. It verifies device startup/interface GUIDs and waits up
to 15 seconds for activation. Follow its reconnect/restart message if setup
fails, then retry Refresh in the GUI. Initial driver installation may take
longer than an already-configured check; installation requires Windows approval.

## Supported devices

| Device | USB vendor:product ID | Required mode |
| --- | --- | --- |
| YOKOGAWA GS210 (may appear as GS200) | 0B21:0039 | TMC |
| Agilent/Keysight 34411A | 0957:0A07 | USBTMC |
| NI GPIB-USB-HS+ | 3923:7618 | GPIB adapter |
| Agilent 82357B | 0957:0718 only | Already initialized under separately obtained firmware rights |

Set GS210 to TMC mode before connecting. Storage-mode devices, nonzero composite
interfaces and other USB IDs are excluded from this setup's selection. Do not
change to Storage mode while using the native measurement connection.
The NI HS+ Analyzer interface can remain without a driver; it is not used by
this application. The communication interface (MI_00) must use WinUSB.
Only one supported GPIB adapter may be connected at a time. GPIB0 uses controller
address 0 and scans listeners at primary addresses 1-30, including unregistered
models. Secondary addresses and multiple adapters are not supported.

## What initial setup does

- Checks the fixed supported-device allowlist and existing WinUSB interface GUID.
- Preserves every existing non-WinUSB driver assignment. It does not decide that
  a vendor driver is disposable merely because VISA cannot currently see it.
- Requests administrator privileges only when configuration is required.
- Uses a bundled, replaceable libwdi DLL to generate, sign and install a small
  device-specific package pointing to Windows' own WinUSB driver. It installs
  the generated publisher certificate in the machine certificate stores.
- Assigns the driver to the selected PnP instance, not every matching device.
- Configures only an already initialized 82357B (0957:0718). It never downloads
  or uploads firmware, configures boot 0957:0518, or stages a driver for firmware
  re-enumeration. Other supported devices are still configured if a boot
  82357B is connected. No Internet connection is needed for these setup steps.

The app and installer do not disable Windows driver-signature enforcement or
boot security settings. Setup does not replace an existing manufacturer driver. Older native-only
releases could assign WinUSB in its place; restore the manufacturer driver
manually if you want those devices to use official VISA. The original system
restore settings are preserved. No SCPI commands or measurement sweeps are
sent by USB setup.

Existing WinUSB configurations with an interface GUID are reused. To check or
retry manually, run `USB-Setup.exe` inside `support`. Keep `libwdi.dll` beside USB-Setup.exe. This is a one-time
USB configuration, not an installation of the KohdaLab application.

## Logs and excluded firmware

Setup logs are in `%LOCALAPPDATA%\KohdaLab IV`:

```text
usb-setup.log
usb-drivers/
```

The setup log records target device IDs and previous driver services. Windows
also records driver installation in its SetupAPI log. Keep exact error messages
if setup fails. Do not repeatedly start setup while a driver installation is
still running. Resolve pending Windows installation/restart requests first.

Firmware rights have not been established. Firmware files, automatic acquisition,
cache loading and upload are excluded, even if an earlier release left a file
on the user's PC. A boot adapter is refused with an explanatory message. Obtain
initialization separately through an appropriately licensed workflow; do not
add its files to this distribution. See REDISTRIBUTION.md and
SOURCE.zip/docs/redistribution_audit.md. No source output is enabled during
setup. GPIB identification later initializes IFC/REN.

## Instrument checks

With the GUI closed, verify complete identities before a measurement. Replace
GPIB addresses 3 and 7 with the instruments' actual addresses:

```powershell
& '.\support\diagnostics\usbtmc-probe.exe' --idn
& '.\support\diagnostics\gpib-probe.exe' --idn 3 7
```

Diagnostic probes are rebuilt from the same source, but may be available only
in the build kit rather than the minimal portable. For an 82357B, an initialized
0957:0718 device must already be present; this app cannot initialize boot
0957:0518. Use the app's Refresh
and select the actual resources rather than retaining example serials/addresses.
Hardware identification, measurement, Stop and output-off need Windows tests.

## Source and alternatives

Complete setup/helper/dependency source, licenses and rebuild scripts are in
`native` and `scripts` after extracting `support/SOURCE.zip`. The libwdi
modifications are documented in `native/vendor/libwdi/KOHDALAB-MODIFICATIONS.md`.
The LGPL DLL can be replaced with a compatible modified build.

Zadig is an optional manual fallback if the dedicated installer fails; it is not
required by the normal flow. Use only the listed USB IDs and TMC interfaces.
- libwdi: https://github.com/pbatard/libwdi
- WinUSB: https://learn.microsoft.com/en-us/windows-hardware/drivers/usbcon/winusb-installation
- Optional Zadig fallback: https://zadig.akeo.ie/

The 2026-10-07 installer shares deletion access when reading Windows SetupAPI
logs so log rotation does not block installation. On the instrument test PC,
full NI driver-package removal/reinstallation dropped from about 70 seconds to
about 3 seconds. Timing varies with Windows policy, approval, and hardware.
Normal startup reuses active drivers. Installer phases remain visible in the GUI.
