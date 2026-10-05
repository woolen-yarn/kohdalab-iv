# Windows native portable build

Build the GUI/CLI on a Windows 10/11 x64 PC. Python, libusb and native USB/GPIB
helpers are bundled in the result. Portable users do not need Python, uv or
VISA for the supported native connections. Initial WinUSB driver assignment
is handled by the bundled USB setup on first normal GUI launch; see
`portable/windows/DRIVERS.md`. Windows administrator/driver prompts remain.

## Build

Extract the native build kit into a writable folder. It contains precompiled
Windows x64 helpers, checksums and their complete corresponding source.
Only the Python application is built on your Windows PC. If needed, install uv:

```powershell
winget install --id astral-sh.uv -e
```

Open a new PowerShell window in the extracted folder and run:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\build_windows_portable.ps1
```

The script installs locked application dependencies, verifies native-helper
checksums and PE x64 headers, runs the USB helpers' hardware-free self-tests,
builds GUI/CLI with PyInstaller 6.22.3, validates the frozen executables, runs a
three-point simulated sweep and Qt startup test, and packages after success.
It does not install or change instrument drivers, nor query physical instruments.

Output for version 0.2.3:

```text
dist/releases/v0.2.3/windows-x64/
  kohdalab-iv-0.2.3-windows-x64-portable.zip
  SHA256SUMS.txt
  RELEASE_NOTES.md
  kohdalab-iv-0.2.3-windows-x64-portable/
```

Log: `build/windows-portable-build.log`. Re-running rebuilds the EXEs and replaces
only generated portable staging/release files. The previous VISA-only EXEs do
not acquire native support by copying the setup guide; rebuild with this kit.
Publish the release ZIP after real Windows instrument testing.

## Run and configure

Extract the entire portable ZIP and launch `KohdaLab IV.exe`.
Keep `_internal` and `support` alongside it. USB-Setup.exe/libwdi.dll are in
`support`. Initial setup runs automatically only for supported devices needing it.
Approve Windows prompts; no separate Zadig download or manual device selection
is needed. Setup and Windows hardware operation are test candidates pending
actual Windows verification. See DRIVERS.md. Direct USB supports
GS210/GS200 ID 0B21:0039 in TMC mode and 34411A ID 0957:0A07. GPIB supports NI
HS+ and Agilent 82357B, one adapter at a time. The latter additionally needs
checksum-verified firmware, downloaded automatically during initial USB setup.

Load a profile from `support/configs`, click Refresh, choose the actual Source/Meter
resources and matching device models, then Connect. Profiles contain the Mac
test setup's serials/addresses and must be adapted. Use `support/diagnostics` probes to
verify complete identities first. Windows native instrument identification,
actual measurement, Stop and output-off behavior still need hardware tests.

State/configuration/results use the per-user `.kohdalab` directory by default.
Choose an absolute output directory in the GUI if desired. Dropdowns expand
while open; the original closed-field/panel widths remain unchanged.

## Rebuild native helpers from source

The supplied helpers statically link libusb and MinGW runtime libraries;
separate libusb/compiler runtime DLL installation is unnecessary. Source and
licenses are supplied in `native/vendor` and `native/licenses`. The original
KAME driver commit and libusb source checksum are recorded there. The NI shim
has Windows name-isolation fixes; Agilent firmware uses Windows SHA256 APIs.

Install MSYS2 from https://www.msys2.org/ on a development PC. In its UCRT64
terminal install the compiler/build tools and build from the source root:

```sh
pacman -S --needed mingw-w64-ucrt-x86_64-gcc make
bash scripts/build_native_windows.sh
bash scripts/build_usb_setup.sh
```

In PowerShell refresh the precompiled-helper set/checksums, then rebuild the app:

```powershell
uv run --no-sync python scripts/windows_native.py --prepare
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\build_windows_portable.ps1
```

For cross-compilation, set `KOHDALAB_WINDOWS_CC=x86_64-w64-mingw32-gcc` and
`KOHDALAB_WINDOWS_CXX=x86_64-w64-mingw32-g++` when running the Bash builder.
For USB setup cross-builds also set `KOHDALAB_WINDOWS_RC` to the target windres
and `KOHDALAB_HOST_CC` to a compiler executable on the build host. libwdi is
dynamically linked as a replaceable LGPL-3.0-or-later DLL. Complete modifications
and library source are under `native/vendor/libwdi`.
Cross-compilation validates compilation, not execution or installation on Windows.

## Visible USB startup check

The application displays its main window before starting the USB setup check. The Status field shows the current phase and elapsed seconds; the log records phase changes with elapsed time. Connection and measurement controls are temporarily disabled until setup finishes. Already configured devices do not reinstall their drivers. If setup fails or Windows approval is cancelled, the GUI stays open with an error in the log. While setup is active, closing the app is deferred to avoid interrupting driver installation.

If checking devices takes several minutes, report the phase and elapsed time shown in the log. This build removes the invisible GUI wait; it does not claim that Windows USB enumeration or installation will complete within a fixed time.

## Refresh discovery status

The native portable uses its bundled USB/GPIB discovery only; it does not additionally load a locally installed vendor VISA runtime. This avoids an unbounded second discovery through an unrelated runtime. The source Python application retains its VISA discovery path.

During Refresh, Status displays the current discovery phase and elapsed seconds, and the log records phase changes. Direct USB helper discovery has a 10-second process timeout. A discovery failure restores Refresh and connection controls so the next scan can be retried. Windows hardware confirmation is required if repeated scans still fail; report the final phase, elapsed seconds and error text.

## Verify selected instrument

On Connect, built-in hardware controllers verify the actual instrument identity before the connection is registered and before model-specific configuration or output commands are sent. USB and GPIB use the same check. A mismatched manufacturer/model, empty or malformed identity, or failed identity query rejects the connection and closes its handle. The error includes the selected model, resource and received identity when available. Reusing an existing connection through Connect also rechecks identity.

Agilent and Keysight 34411A selections accept either manufacturer's 34411A identity, while 34401A and 34465A remain distinct models. The legacy 7651 uses its actual OS response instead of the display-only identify fallback. This checks the instrument model; it does not distinguish the intended role of two physical instruments of the same model.

## Distribution layout

The user-facing ZIP contains only four top-level entries:

```text
KohdaLab IV.exe
README.txt
_internal/
support/
```

The EXE is the only normal entry point. Support contains driver setup, diagnostic probes, example profiles, licenses, build details and SOURCE.zip (complete corresponding source). The CLI is built to validate the frozen simulated measurement but is not duplicated into the user distribution. Publish the portable ZIP and SHA256SUMS.txt from dist/releases; do not publish this development build kit as the portable app. The newly relocated EXE is smoke-tested before release packaging.
