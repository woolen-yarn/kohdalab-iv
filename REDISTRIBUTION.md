# Redistributing KohdaLab IV

This distribution contains separately licensed programs. The repository's MIT
license applies to KohdaLab's original application code; it does not replace
the licenses of the native helpers, libraries, fonts, or other dependencies.

## What to distribute

Distribute the complete portable ZIP, including `support/SOURCE.zip`,
`support/licenses/`, `support/THIRD_PARTY_NOTICES.md`, this file, and the file
manifest. On macOS these documents are beside the app in `support/`, rather
than inside the signed app bundle. Do not deliver only the GUI executable.
If you change a GPL/LGPL component, replace the corresponding source with the
source of your actual modified binary and record your changes and their date.
Do not restrict copying/modification of these components or reverse engineering
for debugging modifications to LGPL libraries.

| Component | Distribution license | Corresponding source / notices |
| --- | --- | --- |
| Rust GUI, engine, CLI, thin Python client | MIT, plus dependency licenses | `SOURCE.zip`: `next/`, `bindings/`; `licenses/rust/DEPENDENCIES.json` and individual notices |
| Combined NI/Agilent GPIB helper | GPL-2.0-only for the combined executable | `SOURCE.zip`: `native/kohdalab_gpib_helper.cpp`, `native/agilent_82357b.*`, `native/vendor/ni-gpib/`, build scripts; `licenses/GPL-2.0.txt` |
| USBTMC helper | MIT application code, statically linked LGPL-2.1-or-later libusb | `SOURCE.zip`: `native/usbtmc_helper.cpp`, headers, scripts, libusb source archive; `licenses/LGPL-2.1.txt` |
| Windows USB-Setup | MIT application, dynamically linked LGPL-3.0-or-later libwdi | `SOURCE.zip`: `native/windows_usb_setup.cpp`, headers, `native/vendor/libwdi/`, build script; `licenses/LGPL-3.0.txt` and `GPL-3.0.txt` |
| libusb 1.0.30 | LGPL-2.1-or-later | `SOURCE.zip`: `native/vendor/libusb-1.0.30.tar.bz2` (unmodified upstream source, including COPYING) |
| Windows MinGW runtimes | Respective permissive notices / GCC runtime exception | `licenses/mingw-w64-COPYING.txt`, `winpthreads-COPYING.txt`, `GCC-RUNTIME-EXCEPTION.txt` |
| GUI dependencies and fonts | Per-dependency licenses | `licenses/rust/` (including font license texts) |

The GPIB executable combines GPL-2.0-or-later NI code and GPL-2.0-only Agilent
code. Version 2 is the common permitted GPL version for that executable; the
Agilent part cannot simply be upgraded to GPLv3. Preserve the original notices.

## Independent native programs

The Rust executable does not link the GPL driver or include its source/header
definitions. It runs a replaceable GPIB program through standard input/output.
That program also runs independently (`--available`, `--idn`, framed byte-I/O).
The interface exchanges instrument addresses, timeouts, raw command bytes and
raw response bytes, not Rust application structures, plans, plots, or records.
Its interface is documented in `docs/native_helper_protocol.md` in SOURCE.zip.
`KOHDALAB_IV_HELPER_DIR` selects an alternative helper directory. Direct USB
transfers use the USBTMC program rather than the GPL driver; simulation opens
no native helper. The portable lookup expects both named helper files in its
helper directory, even when a measurement uses only direct USB.

This is a distribution of independent programs with their own license terms,
not a claim that process separation automatically exempts every combination
from the GPL. See the recorded evidence in `docs/redistribution_audit.md`.

## Rebuilding and replacing LGPL components

Extract `support/SOURCE.zip` into a writable directory. Complete helper and
library source plus the actual build scripts are provided, rather than only a
link or a time-limited written offer. See `native/REBUILDING.md` for commands to
rebuild/relink with a modified libusb or replace `libwdi.dll`. Code signing
uses no private distributor key: the Mac helper can be ad hoc signed locally.
Do not remove these materials when you pass on the binaries.

## 82357B firmware is excluded

No third-party 82357B firmware, automatic firmware downloader, firmware cache,
or firmware upload function is provided by this release. A public GitHub URL
and a checksum do not grant redistribution permission. We have not established
a license authorizing redistribution of `measat_releaseX1.8.hex`.

Only an already initialized adapter (`0957:0718`) can be used. An uninitialized
adapter (`0957:0518`) produces a clear error without uploading firmware or
replacing its boot driver. Firmware initialization must be obtained separately
from a source that grants the necessary rights. KohdaLab does not distribute
manufacturer software or promise that any external initialization workflow is
available on every OS. Do not add a firmware file to a portable ZIP or build kit.

These notes document the implemented licensing precautions and obligations;
they are not a warranty covering every jurisdiction, patent, trademark, or
third-party device/software agreement. No vendor endorsement is implied.
