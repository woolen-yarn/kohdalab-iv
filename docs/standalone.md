# Standalone desktop builds

Build on each target OS. The resulting app includes Python and its packages;
users do not need Python, uv, or a repository checkout. Windows builds produce
an executable folder; macOS builds produce an app bundle for the build machine's
architecture (Apple Silicon or Intel).

From the repository root, on a build machine with uv:

```sh
uv run --extra gui --with pyinstaller python scripts/build_standalone.py
```

Use `--target gui` or `--target cli` to build just one entry point.

## Run and distribute

- macOS: open `dist/KohdaLab IV.app`. Distribute the entire `.app` bundle.
- Windows: open `dist/KohdaLab IV/KohdaLab IV.exe`. Distribute the entire
  `dist/KohdaLab IV` folder, including `_internal`, as a ZIP.
- The standalone CLI is in `dist/kohdalab-iv`; distribute that whole folder.

The GUI uses `~/.kohdalab` as its working directory by default, so relative CSV
output such as `results` goes to `~/.kohdalab/results`. An explicit
`KOHDALAB_IV_STATE_DIR` or `KOHDALAB_STATE_DIR` also controls that working directory.
Select an absolute output directory in the GUI to save elsewhere. Editable
settings use the existing per-user configuration workflow.

The bundled `resources/simulated.json` profile can be selected through the GUI's
config picker. For an easier-to-find copy, include
`src/kohdalab_iv/resources/simulated.json` beside the distributed app.

Example hardware-free CLI check on macOS (use `.exe` on Windows):

```sh
dist/kohdalab-iv/kohdalab-iv --version
dist/kohdalab-iv/kohdalab-iv --config src/kohdalab_iv/resources/simulated.json measure
```

For the supported native portable connections, use the complete release builder
and [portable guide](portable_release.md). Other transports require a compatible
VISA backend and any manufacturer adapter drivers.
Confirm OS and CPU compatibility with the adapter vendor, especially for GPIB
on macOS. Packaging does not establish hardware support.

For NI GPIB-USB-HS+ on Apple Silicon, an optional userspace helper can replace the
NI driver for GPIB0 resources. See [native helper](../native/README.md) for the
build, supported resources, source-distribution requirements, and verification
limits. A build containing that helper needs neither Python nor NI drivers on
the target Mac. Refresh scans primary GPIB addresses 1–30 for listeners, including
unregistered instruments, and retains manual addresses. Discovery runs separately
from the GUI thread and is disabled during measurement. It does not send SCPI
commands or change instrument output settings.

The same native helper also supports an Agilent 82357B through a separate
userspace protocol implementation. Run the supplied
`setup-82357b-firmware.command` once on the target Mac to download and verify the
firmware; the helper loads it automatically after each adapter power-on.
Use only one supported GPIB adapter at a time. Physical 82357B identification was verified on the remote Apple Silicon Mac; native mock tests cover protocol
framing, two-stage firmware boot, discovery, failures and cleanup.

The optional direct USB helper supports the Agilent 34411A (0957:0A07) and
YOKOGAWA GS210/GS200 (0B21:0039). Set the GS210 USB mode to TMC. Bundle it with
`--native-usb-helper build/usbtmc-probe/kohdalab-usbtmc-helper` after running
`bash scripts/build_usbtmc_probe.sh`. It can coexist with the GPIB helper, allowing
GS210 at GPIB 5 and 34411A at
`USB0::0x0957::0x0A07::MY53000981::INSTR`. Serial matching prevents opening another
instrument. Refresh lists directly connected, supported USB instruments using
USB descriptors, and merges resources from an available VISA backend. It retains
manually entered addresses and combines these results with native GPIB discovery.
Include both helpers' source and licenses as described in the native helper documentation.

## Prepared portable releases

Use scripts/build_macos_portable.command or scripts/build_windows_portable.ps1
for a complete distributable ZIP. Both releases have English README files,
diagnostics, example profiles, licenses, corresponding source and checksums.
See [the portable release guide](portable_release.md) for downloads, platform
requirements and build kits. The Mac build is ad-hoc signed, not notarized;
Intel Mac builds are not included. Windows builds require a Windows x64 machine.

Direct USB operation was confirmed by the user for both instruments. A halted
34411A USB endpoint (`LIBUSB_ERROR_PIPE`) was cleared by closing the app and
unplugging/replugging the USB cable. The app does not automatically retry a
failed command. All dropdowns expand only when opened.
