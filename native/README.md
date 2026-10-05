# USB-GPIB helper

## Windows native build

Windows x64 frozen apps can bundle the same USBTMC and NI/82357B protocol code.
See `docs/windows_portable.md` for the precompiled-helper build kit and source
rebuild instructions. Initial WinUSB device assignment is required; see
`portable/windows/DRIVERS.md`. The user reported USB and both GPIB adapters working on Windows and confirmed
the final portable. Mock tests validate protocol behavior, not measurement accuracy.
See docs/portable_release.md for the current distribution; the notes below record
the development investigation.

The Windows NI shim uses private thread names to coexist with MinGW's pthread
runtime, the Python bridge reads pipes through a dedicated reader thread, and
82357B firmware is checked using Windows BCrypt SHA256 and cached in LOCALAPPDATA.
Only OS-provided DLLs are imported by the supplied statically linked helpers.

## macOS build

The macOS standalone app can use a separate GPL-2.0-or-later process built from
KAME's userspace linux-gpib port, communicating over framed stdin/stdout. Python
instrument controllers and measurement cleanup remain shared with the VISA path.

Build on an Apple Silicon Mac with Xcode Command Line Tools:

```sh
bash scripts/build_native_gpib.sh
uv run --extra gui --with pyinstaller python scripts/build_standalone.py \
  --native-gpib-helper build/native-gpib/kohdalab-gpib-helper
```

The builder pins KAME commit `b413cc7102e984fa8e5e13bd4993549a6e4584cc` and
libusb 1.0.30. It adds public timeout and addressed Go-To-Local methods to the
driver API; protocol code in `kohdalab_gpib_helper.cpp` is GPL-2.0-or-later.
The repository's MIT license continues to cover the Python application.
Distributions containing the GPL helper must also supply its complete
corresponding source, driver modifications, build instructions, and licenses.
Supply libusb's source and LGPL license as well when distributing the static link.

Supported resource strings are `GPIB0::<address>::INSTR`, with instrument
addresses 1–30 and controller address 0. There is one supported USB adapter per process.
Opening the adapter asserts REN and IFC; stop other GPIB software first. This is
not a read-only USB operation. Refresh finds listeners at primary addresses
1–30 on GPIB0 using addressed NDAC detection, including unregistered and non-SCPI
devices. It sends GPIB addressing bytes, but no SCPI, clear, trigger, or output
commands. Address 0 belongs to the controller; secondary addressing and multiple
adapters are not supported by the native transport. An already-open adapter is
reused without IFC or reopening; a temporary scan session releases REN and closes
when finished. No adapter produces an empty GPIB list, while a bus error is
reported. The GUI scans in a worker thread, blocks measurement during scanning,
and retains manually entered resources. Other resources use VISA
unless the optional USBTMC helper described below is bundled.
The environment variable `KOHDALAB_IV_GPIB_HELPER` selects an explicit helper
for development/testing; otherwise macOS frozen apps use their bundled helper.

GS210 at address 5 and 34411A at address 9 returned valid identities through the
unmodified driver on the remote Apple Silicon Mac. The persistent GUI transport
and actual output/measurement operation require a separate hardware verification.
Do not treat unit tests or IDN replies as verification of real measurement values,
compliance behavior, Stop cleanup, or USB-disconnection behavior. Hardware loss or
a hung helper can prevent software output-off; retain instrument hardware limits.

## Agilent 82357B on Apple Silicon

The helper automatically selects NI USB-GPIB or Agilent 82357B. Connecting multiple
supported adapters is rejected so a resource cannot silently refer to another bus.
The 82357B subset is GPL-2.0-only and adapts the protocol/initialization in
Linux v6.17 `drivers/staging/gpib/agilent_82357a/agilent_82357a.c/.h` and
`drivers/staging/gpib/include/tms9914.h` (Frank Mori Hess, 2002/2004).
Its operations include primary listener discovery, ASCII send/read/query,
selected device clear, addressed local control, REN, and controller initialization.
Read responses require EOI and are limited to 4096 bytes. A transfer failure
blocks further SCPI until reconnection; cleanup still attempts Local and REN release.
Tests use simulated USB endpoints and do not verify a physical 82357B.

The first remote probe reached adapter initialization but stopped at
`could not assert ATN` before identifying an instrument. The revised control
check uses physical `BSR_ATN_BIT` rather than the possibly corrupted ADSR status;
it still rejects commands when ATN is absent. It waits up to 100 ms for the
control transition, and reports BSR, ADSR and HW_CONTROL if it fails.
Native regression tests deliberately supply an incorrect ADSR while BSR is
asserted, and also verify that absent physical ATN blocks command transmission.
This workaround still needs confirmation on the remote physical adapter.

The remote ATN revision passed control acquisition and write-complete
notification, then failed `invalid write status`. The initial transport had
incorrectly required an undocumented `0x4f` prefix on XFER_STATUS. The status
revision follows Linux-GPIB's count extraction at bytes 2–5 instead, retaining
the completion notification, full response length and exact write-count checks.
Errors include response length, raw bytes and counts. Set
`KOHDALAB_IV_82357B_TRACE=1` to print raw write statuses without printing SCPI
payloads. Native tests cover varying prefix bytes, short status packets, USB
errors and incomplete counts; instrument identities still need remote verification.

The next remote attempt stopped again before command transmission with
`BSR=0x01, ADSR=0x00, HW_CONTROL=0x3f`. No working 82357B instrument ID has
been verified. `gpib-probe --diagnose` enables phase tracing before/after hardware
reset, register setup, controller request, IFC assertion/release and TCA.
It emits no GPIB command/data bytes or SCPI; it does initialize the adapter and
change IFC/REN/ATN, then attempts REN release on exit. Use it with all other GPIB
software closed. Failure still stops rather than bypassing physical ATN checks.
The purpose is to determine where the controller state differs on the target
adapter, not to claim a hardware fix. Phase tracing is otherwise enabled only
when `KOHDALAB_IV_82357B_TRACE` is set.

Remote phase logs then showed a successful controller-only test starting with
`HW_CONTROL=0x3d`, followed by failed instrument probing starting with `0x3e`.
`RESET_TO_POWERUP` preserved `0x3e` through the AUX initialization. The low
`NOT_TI_RESET` bit was still clear: close had held the TI chip in external reset,
and open only released it after configuring the chip. Open now writes the
preserved power bits plus `NOT_TI_RESET | NOT_PARALLEL_POLL` before AUX setup,
then explicitly configures under software chip reset and releases software
reset before requesting system control. Phase tracing includes
`after hardware reset release`. The simulator now preserves this hardware
control state across powerup-reset and repeated opens, and ignores AUX writes
under external reset. The previous initialization fails the new regression;
the corrected initialization passes four consecutive close/open/query cycles.
Physical instrument communication still requires target-Mac confirmation.
The reset release is marked for cleanup before its USB acknowledgement is read,
so a lost acknowledgement still reasserts external reset on close. Tests cover
that applied-write/lost-ACK window and assert the software-reset setup order.

The physical reset-fix probe now acquired ATN and completed its first three-byte
GPIB addressing command. Its XFER_STATUS was exactly six bytes:
`01 20 03 00 00 00`. The eight-byte Linux buffer is a maximum request size;
only bytes 2–5 are needed for the 32-bit count. The helper accepts responses of
6–8 bytes, still requires the write-complete interrupt and matching count, and
rejects incomplete counts (0–5 bytes), USB errors and count mismatches.
Tests replay this six-byte response and exercise six-byte send/query, repeated
reopens, extended statuses, malformed statuses and cleanup. Instrument IDN
communication and real measurements are not yet physically verified.

The six-byte physical probe then completed its addressing command but timed out
on the data-write completion. The port had omitted the Linux common layer's
`ibgts()` call (`drivers/staging/gpib/common/iblib.c`), which releases ATN before
`interface->write` and `interface->read`. The 82357B NO_ADDRESS transfers do
not replace that common-layer transition. Send/read now call AUX_GTS and verify
physical ATN is clear before transferring data. Listener discovery and the
controller diagnostic use the same checked standby transition. Failure to
release ATN blocks data I/O and poisons further SCPI until reconnect.
The USB simulator now withholds data-write completion and read data when ATN is
still asserted; the previous code reproduces the target data-write timeout,
and the corrected transport passes send/query/read and reopen tests.

82357B starts as `0957:0518` and needs firmware after each power-on. The helper
uploads firmware using Cypress FX2 RAM requests, then waits for `0957:0718`.
Two uploads may be needed, as documented by the Linux-GPIB maintainer.
The firmware repository has no explicit redistribution license, so the application
does not bundle its binary. On the target Mac run once:

```sh
bash setup-82357b-firmware.command
```

The script downloads `measat_releaseX1.8.hex` directly from
`fmhess/linux_gpib_firmware`, commit `8a5c6c2c1a2adac4c770763a7236452a871a0113`,
and verifies SHA-256 `2c40213a3d0d8b7d7cc083b155a5ed094e29767214a9b8aa745486f35ef58664`.
It stores the file in `~/Library/Application Support/KohdaLab IV/`.
The loader checks that exact checksum and all Intel HEX records before modifying
the adapter; no disk firmware is permanently flashed. An override path can be
provided with `KOHDALAB_IV_82357B_FIRMWARE`. If uploading fails, unplug/replug the
adapter before retrying. No Python, NI software, or kernel driver is required.

Close the GUI/other GPIB tools, attach only 82357B, and verify identities:

```sh
./gpib-probe --idn 5 9
```

This mode sends only `*IDN?` plus GPIB controller/address/local operations.
It does not change instrument measurement or output settings. App Connect and
Start have their existing measurement semantics; they are not identity-only tests.
After this succeeds, use Refresh in the GUI to verify automatic resource discovery.
82357A, secondary addressing, multiple adapters and Windows native transport are
outside this implementation.

## Direct 34411A USB

The MIT-licensed `usbtmc_probe.cpp` identifies a directly connected Agilent
34411A without changing measurement settings. This succeeded on the remote Mac
with VID `0957`, PID `0A07`, serial `MY53000981`. The persistent
`usbtmc_helper.cpp` uses the same USBTMC framing and selects the serial specified
by `USB0::0x0957::0x0A07::<serial>::INSTR`. Only this model/interface is supported.
The helper claims the USB interface and enables remote control if USB488
capabilities permit it. Closing returns it to local and deasserts REN.
The `--list` mode returns supported USB resource strings from USB descriptors,
without claiming the instrument interface, enabling remote mode, or sending SCPI.
Refresh uses this mode, combines it with any available VISA resource list, and
keeps existing manual addresses in the selectors. Native GPIB listener discovery
is combined with this list. Discovery failure is reported to the GUI rather than treated as an
empty result.
Device clear is explicitly unsupported; normal instrument status clearing uses
the existing `*CLS` command. A failed bulk transfer blocks further SCPI operations
until reconnect, so a stale response cannot become a subsequent measurement.

```sh
bash scripts/build_usbtmc_probe.sh
uv run --extra gui --with pyinstaller python scripts/build_standalone.py \
  --native-gpib-helper build/native-gpib/kohdalab-gpib-helper \
  --native-usb-helper build/usbtmc-probe/kohdalab-usbtmc-helper
```

`KOHDALAB_IV_USB_HELPER` selects the USB helper during development/testing.
Both helpers can run together: GS210 at GPIB 5 and 34411A over USB. Include the
MIT transport/probe/helper source and corresponding statically linked libusb
source/license/rebuild script when distributing. Running the app requires no
Python or NI driver installation. The USB probe's IDN success establishes
communication; real measurement and cleanup still need hardware verification.

Native framing/transport checks can be run without hardware:

```sh
build/usbtmc-probe/usbtmc-probe --self-test
build/usbtmc-probe/kohdalab-usbtmc-helper --self-test
clang++ -std=c++17 -fsanitize=address,undefined \
  -Ibuild/usbtmc-probe/libusb-1.0.30/libusb \
  native/usbtmc_helper.cpp tests/native_usbtmc_mock.cpp -o build/usbtmc-mock/helper
```

The last binary accepts serial `MY53000981` and implements a simulated USB
backend for exercising the real helper protocol. It must not be distributed as
the production helper. `KOHDALAB_TEST_USB_FAIL_READ=1` makes its `READ?` bulk read
time out for testing failure isolation.

The driver's Find Listeners extension is in `gpib_listener.inc`, appended by the
builder. It follows the [Linux-GPIB listener detection protocol](https://linux-gpib.sourceforge.io/doc_html/reference-function-ibln.html),
addressing the controller as talker before reading NDAC. `tests/native_gpib_listener.cpp`
exercises the real driver against a mock interface: all addresses, devices at
5/9/17/30, an empty bus, transient busy status, and cleanup after each failure.

### 82357B diagnostic read safety (2026-10-01)

The remote Mac received GS210 at address 3 and 34411A at address 7 through
82357B. The former returned `OKOGAWA,GS210,91N928756,2.01` with trace enabled;
the latter returned `Agilent Technologies,34411A,MY48001051,2.21-2.21-0.09-46-9`.
The previous 5/9 addresses were for different instruments.

Trace previously read ADSR after GTS, while the controller was listener-active.
Linux's driver explicitly works around TI9914 ADSR reads in this state by
asserting ATN first. Trace now skips ADSR unless BSR confirms ATN asserted,
including failure diagnostics. This prevents diagnostics from accessing the
data register while reception is active. The mock reproduces first-byte loss
with the old trace and verifies complete responses with the fix. Hardware
verification of the fix and actual measurement remain pending.

### Receive request ordering (2026-10-01)

The user verified the SafeTrace binary hash and disabled trace: GS210 still
returned `OKOGAWA`, so skipping ADSR alone did not resolve the physical issue.
The next candidate follows Linux-GPIB's receive ordering exactly: GTS register
write with checked USB acknowledgement, then USB READ, without the extra BSR
poll or trace register requests in between. Send and listener-scan ATN polling
remain intact. Mock tests enforce this ordering with trace both on and off;
this is a protocol regression check, not proof of the physical loss mechanism.
Hardware verification remains necessary.

The remote Mac verified the ReadOrder candidate with trace disabled:
`YOKOGAWA,GS210,91N928756,2.01` at GPIB 3 and
`Agilent Technologies,34411A,MY48001051,2.21-2.21-0.09-46-9` at GPIB 7.
Removing the interleaved status requests resolved the observed first-byte loss
in this run. Repeated reads, trace-enabled reads, GUI discovery and actual
measurement still need hardware verification.
