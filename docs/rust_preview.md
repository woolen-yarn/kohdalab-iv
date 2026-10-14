# Rust desktop release and validation

Version: **0.3.0**. This is the native desktop release,
separate from the stable Python portable. The full migration plan is in
[next_version_plan.md](next_version_plan.md).

Alpha.3 adds the blue/orange I-V application icon to both Windows executables,
the Mac app bundle and the GUI window. Editable SVG and platform icon assets
are included in the source; `scripts/build_iv_icons.py` rebuilds them.

Alpha.2 optimizes CSV allocation, repeated sweep planning, engine IPC and graph
updates while preserving measurement and persistence settings. The
[measured optimization report](rust_optimization.md) compares it with alpha.1;
the historical acceptance observations below retain their original context.

## Implemented

- A Rust workspace with separate planning/persistence, instrument transport,
  and desktop crates. Application crates forbid unsafe code.
- An egui/eframe desktop using the same GUI foundation as KohdaLab Camera
  Native, following the established three-column IV layout: instrument and
  profile settings on the left, sweep/output/run controls and graph in the
  center, and a log and Field / Value snapshot on the right. The original black theme,
  group boxes, compact controls and tall side-panel toggles are reproduced.
  Source and meter connect independently. Timing and limits open from the
  center; Config and output Browse use native file dialogs.
- A separate Rust measurement process. Device transfers run outside the GUI.
  The native C/C++ helpers remain isolated subprocesses; Rust does not make
  those external libraries memory safe.
- Explicit simulation, actual instrument identity checks, selected-resource
  locking, bounded communication frames/queues, helper transfer deadlines,
  and a two-second GUI heartbeat lease.
- IV and VI planning, four sweep patterns, repeats, source range/resolution
  validation, hardware compliance, software threshold, settling, NPLC,
  averaging, interruptible ramps, and a streamed resistance fit.
- The existing 46-column CSV layout, exclusive file creation, per-point flush,
  and a run JSON file recording configuration, identities, physical resource
  addresses, outcome, and warnings. Flush does not guarantee survival of a
  power failure.
- Separate settings at `~/.kohdalab/config/iv-next.json`. Existing Python GUI
  configurations are not modified or imported in this preview.
- Emergency shutdown sends Output Off before resetting the level. A poisoned
  helper is replaced under a bounded recovery deadline while retaining the
  device lock. The exact previously identified source must match before an
  OFF retry. A failed zero-level command does not discard a verified OFF state.
- Portable packaging, dependency licenses, native helper licenses, complete
  source and build scripts, file hashes, and an ad hoc signed Mac bundle.

## Validation recorded on 2026-10-13

The Windows test host was NEEDLEFISH. The NI GPIB-USB-HS+ instrument interface
used the **WINUSB** service with the **KohdaLab** driver provider. The test
therefore used the established portable driver and helpers, without vendor
VISA in the Rust path.

| Check | Observed result |
| --- | --- |
| Rust unit and fake-helper tests | 17 passed on Mac; strict Clippy passed |
| Full simulated engine process | Three points completed, 46 existing CSV columns, 1 kOhm fit, output OFF |
| Stop during a simulated long settle | Output OFF in about 0.10 s |
| GUI heartbeat loss during simulation | Output OFF and disconnected in about 2.00 s |
| Real Mac GUI | 41-point simulation, CSV save, plotted result, output OFF |
| Mac first GUI update | About 0.14 s on the development Mac; this is a startup proxy, not a cold-start benchmark |
| Windows NI GPIB identification | GS210 at address 5 and 34411A at address 9 identified; source output OFF |
| Windows NI GPIB measurement | Three points at -1 mV, 0, +1 mV; 1 mA compliance; readback matched; CSV saved; output OFF verified |
| Windows GUI and its Rust engine | Real NI GPIB three-point sweep passed; first GUI update about 0.12 s in the existing logged-in session |
| Repeated Windows resource discovery | Three invocations returned both resources; about 3.27 s, 1.00 s, 1.01 s |
| Incorrect source/meter assignment | Reversing addresses 5 and 9 was rejected by actual identity checks |
| Packaged Mac app | Signed bundle launched, 41 simulated points saved, output OFF; first update about 0.15 s |
| Packaged Windows app | ZIP and all file hashes matched; real three-point NI sweep completed with output OFF; first update about 0.13 s |
| Windows measurement memory sample | Process-tree peak working set about 156 MiB; private memory about 125 MiB |

The real test used an **open circuit**, explicitly authorized by the user.
The source was YOKOGAWA GS210, serial 91T729315, firmware 2.02; the meter was
Agilent 34411A, serial MY53000981, firmware 2.40-2.40-0.09-46-09.
The three-point engine run took about **2.17 s** and the entire CLI invocation
about **3.04 s**, including connection and cleanup.

The packaged engine simulation also passed Stop in about **0.06 s** and
heartbeat-loss cleanup in about **2.00 s**. These timing observations are
specific to short tests and are not guaranteed deadlines for a failed device.
The Windows memory sample included the GUI, Rust engine and native helper,
sampled every 100 ms during a three-point run. Working-set sums can count
shared pages more than once; GPU memory was not measured. The idle-memory
target has not been established by this peak measurement.

An open-circuit current fluctuates near zero. Its resistance fit is not a
known-resistor accuracy measurement. This test establishes communication,
setting/readback, data persistence and confirmed output shutdown on this
hardware configuration. It does not establish metrology accuracy.

Local evidence is kept under ignored `next/validation/` and `build/iv-next-*`
logs. Hardware evidence includes `ni-three-points-1.csv` and its `.run.json`.
It is deliberately excluded from distributed source and portable archives.

## Original UI reproduction

The earlier three-column interpretation was replaced with the original Python
IV interface as the visual reference. Its black theme, Config / Source / Meter
boxes, nested Measurement / Output / Run boxes, independent unit selectors,
compact Status / Point / Resistance / Conductance strip, blue/orange plot,
plain log, Field / Value toggle and tall `<` / `>` panel controls are reproduced.
The initial window is 1240 × 760. Resource fields stay compact; opened menus
expand to show full addresses. Browse / Load / Save, Auto suffix and individual
instrument connections are functional. Unsupported legacy models remain
visible but disabled. This reproduction does not imply configuration-format or
instrument-driver parity with all legacy models.

Independent connections preserve the other role and its GPIB session. Rejected
identities leave the valid role intact. Source disconnection performs verified
output shutdown; heartbeat loss and application shutdown also clean up a
source-only connection. A mixed simulated/real pair cannot start a measurement.

Validation after the reproduction: 25 unit/fake-helper tests and strict Clippy
passed. Both Mac and Windows GUIs completed 41 simulated points, saved CSV,
reported output OFF and exited successfully. First GUI updates were about
0.14 s on Mac and 0.095 s on Windows in these warm tests. Mac UI interaction
checks verified both side-panel toggles, the Field / Value toggle, native output
Browse and restoration of the prior wait time on Cancel.

The Windows NI GPIB devices also passed source-only connection, adding the
meter to the same helper, rejecting a duplicate resource without disturbing the
source, disconnecting the source with verified OFF while keeping the meter,
and reconnecting / disconnecting both. This connection check never enabled
output. It used the same GS210 at address 5 and 34411A at address 9 as above.

Local screenshots and test logs use the ignored `next/validation/gui-*-legacy.png`
and `build/iv-next-legacy-*` paths. Earlier measurement evidence above is retained
separately from these UI and independent-connection checks.

## Known limitations and future acceptance work

The [Python and Rust performance comparison](performance_comparison.md) records
warm Windows window appearance around 0.125 s versus 0.503 s, simulated Mac
processing-loop improvements, and the remaining resident-memory issue:
110.7 MiB versus Python's 104.4 MiB after sixty seconds. These are specific
benchmark conditions, rather than acceptance of every performance target.

- Real-device acceptance for direct USB, mixed USB/GPIB, and Agilent 82357B in
  the Rust edition. The existing native routes are reused, but their previous
  Python-edition results are not treated as Rust-edition acceptance.
- Automatic asynchronous Windows driver setup/recovery and clean driver
  uninstall/reinstall acceptance. The release includes the existing manual
USB-Setup support tool. The current run did not uninstall the driver.
- Import of legacy configurations and support for other instrument models.
- Cold startup, broader process memory profiling and large-graph benchmarks.
  The Windows idle process-tree target of at most 100 MiB is still unmet.
- Longer runs, all real-device sweep modes, unplug/replug recovery, source or
  meter power loss, and process failure acceptance with actual hardware.
- Installer/signing/notarization and general-release documentation.

The release ZIPs are built into `next/dist/` and published as `portable-v0.3.0`
in the Kohdalab repository and personal mirror. The earlier measurements above
retain their preview-version context; release promotion does not imply that
the remaining acceptance work has been completed.
