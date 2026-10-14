# Native instrument programs

The Rust application uses independent native executables for USBTMC and GPIB
byte I/O. No manufacturer VISA runtime or kernel driver binary is bundled.
Windows assigns its inbox WinUSB to the supported communication interfaces.

## License boundaries

- The combined NI/Agilent GPIB executable is distributed under GPLv2.
  The NI userspace port permits GPL-2.0-or-later; the Linux-derived Agilent
  adaptation permits GPL-2.0-only. This is not an MIT driver.
- USBTMC application source is MIT and statically links LGPL-2.1-or-later libusb.
  The GPIB executable also links that library. Full source and relinking
  instructions accompany the binaries.
- The independent Windows setup program is MIT and dynamically links a
  replaceable LGPLv3 libwdi DLL. This build embeds no WDK coinstaller or
  Microsoft kernel binary. It points to the installed Windows WinUSB driver.

See root [REDISTRIBUTION.md](../REDISTRIBUTION.md),
[licensing evidence](../docs/redistribution_audit.md),
[provenance](vendor/README.md), and [rebuild/relink instructions](REBUILDING.md).
The standalone framed byte protocol is documented in
[docs/native_helper_protocol.md](../docs/native_helper_protocol.md).

## Supported connections

- NI GPIB-USB-HS+ (3923:7618): native GPIB.
- Agilent 82357B: only an already initialized 0957:0718 adapter.
- GS210/GS200 USBTMC (0B21:0039), TMC mode; storage mode is excluded.
- Agilent/Keysight 34411A USBTMC (0957:0A07).

An uninitialized 82357B (0957:0518) is refused without opening it, replacing
its boot driver or uploading firmware. Firmware redistribution rights have
not been established; this release has no downloader or uploader. Obtain any
initialization separately under appropriate rights. External manufacturer
software and operating-system availability are outside this distribution.

Only one supported GPIB adapter may be connected at a time. Controller address
is 0; primary instrument addresses are 1-30. Listener discovery sends GPIB
addressing/control operations, but no SCPI commands, trigger or source-output
commands. USB descriptor discovery does not enable an instrument output.
Opening a GPIB adapter initializes IFC/REN, so it is not a passive bus operation.

## Building and replacing helpers

Use `scripts/build_native_gpib.sh` / `scripts/build_usbtmc_probe.sh` on macOS
or `scripts/build_native_windows.sh` / `scripts/build_usb_setup.sh` on Windows
(MSYS2 UCRT64). Build-host compiler tools are not required on a user's PC.
See REBUILDING.md for full commands and modified-library relinking.

`KOHDALAB_IV_HELPER_DIR` selects a replacement directory containing both named
helper executables. Otherwise the Rust portable uses executables in its support
folder. The measurement engine owns transfers and cleanup; the optional thin
Python client calls that Rust engine. Device/transport errors remain checked.
A software output-off request cannot guarantee shutdown after cable/hardware
loss; retain instrument hardware limits.

Development history is retained in Git and historical release notes. Previous
firmware downloader instructions do not describe the current distribution.
