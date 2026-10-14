# Rust performance optimization, alpha.2

Measured on 13 October 2026 against the preserved 0.3.0-alpha.1 release.
The 0.3.0-alpha.2 update preserves the original three-column UI, measurement
conditions and every-point CSV flush. Application crates still forbid unsafe
code; native USB/GPIB helpers and their driver assignments are unchanged.

## Same-Mac measurement processing

An Apple M4 Mac mini with 24 GB RAM ran both release binaries. Each workload
simulated a 1 kOhm circuit over -1 mV to +1 mV with zero waits, NPLC 1 and
averaging 1. One warm-up preceded seven trials per workload, alternating order.
Every CSV was validated with the original 46-column schema, exact row count,
ordered target/readback/current values and the original numerical tolerances.
All successful runs completed with simulated output OFF.

| Metric, median | Alpha.1 | Alpha.2 | Before / after |
| --- | ---: | ---: | ---: |
| 1001-point measurement and CSV loop | 6.476 ms | 3.074 ms | 2.11 |
| 10001-point measurement and CSV loop | 64.895 ms | 29.440 ms | 2.20 |
| 10001-point Run command to completed state | 115.116 ms | 31.785 ms | 3.62 |
| 1001-point fresh engine through disconnect/exit | 369.745 ms | 11.313 ms | 32.68 |
| 10001-point fresh engine through disconnect/exit | 438.710 ms | 38.790 ms | 11.31 |

The loop timer starts after planning and CSV header creation and ends after
cleanup, identically in both versions. Fresh-process timing includes startup,
connections, IPC, disconnect and exit. Its large improvement primarily removes
fixed polling and shutdown waits; it is not a GUI startup measurement.
Physical throughput still includes instrument conversion, communication,
configured NPLC, settling and ramp waits.

## Large sweep planning

A standalone release benchmark planned 990099 points: a valid 10001-point
linear sweep at 200 nV increments repeated 99 times. Both builds used the same
example and release profile; the old core source hash matches the original
benchmark and distributed alpha.1 source. One warm-up preceded five alternating
trials. Point count, endpoint metadata, direction counts, repeat/point indices,
range, compliance and configuration agreed. Small directional edge cases are
also tested against the original decimal algorithm.

| Metric, median | Alpha.1 | Alpha.2 |
| --- | ---: | ---: |
| Planning time | 80.297 ms | 17.543 ms |
| Standalone planner peak RSS | 123.641 MiB | 70.688 MiB |

Planning is **4.58 times faster**; peak RSS is **42.8% lower** in this isolated
workload. macOS `/usr/bin/time -l` supplied peak resident memory. These numbers
exclude the GUI, native helpers, measurement records and CSV writing and must
not be presented as total application memory.

## Windows GUI

Both packaged x64 release binaries ran on the same Ryzen 5 5500GT / 16 GB
Windows host. Isolated simulated profiles made no instrument connections.
One warm-up preceded five launches per version in alternating order, each
using the same 1240 by 760 window, one-second settle and two-second idle sample.

| Metric, median | Alpha.1 | Alpha.2 |
| --- | ---: | ---: |
| Visible main-window proxy | 123.202 ms | 137.907 ms |
| Early idle process-tree working set | 112.902 MiB | 113.355 MiB |
| Early idle process-tree Private Bytes | 103.398 MiB | 103.684 MiB |

This test does **not** demonstrate improved GUI launch time or idle resident
memory. Window appearance is polled every 25 ms and is not first paint or
interaction latency. Working-set sums may count shared pages more than once;
GPU memory is excluded. The original idle-memory objective remains outstanding.
The cached plot update removes full-history scans and temporary plot copies
from ordinary redraws; large-graph frame rate is not quantified here.

## Changes and retained safeguards

- CSV persistence reuses field and number buffers, caches invariant timing
  fields and reuses identical numerical representations. The original owned
  serializer remains the compatibility oracle, with literal negative-zero and
  escaped-field fixtures. CSV quoting, exponent padding, optional blanks and
  exclusive file creation remain compatible; header and every row still flush.
- Planning converts each base target once and computes directions while
  emitting repeated points, eliminating duplicate repeated Decimal and
  direction arrays. Range/resolution limits and the one-million-point cap stay.
- Progress snapshots publish at most every 50 ms. Connection, completion and
  safety transitions publish immediately. A condition variable wakes the IPC
  writer; reusable buffers batch JSON without changing bounded line formats.
  Shutdown waits for actual final-state delivery, with a bounded fallback for
  a blocked consumer. Acquisition and output cleanup never wait on GUI drawing.
- The GUI reads a new state only when its revision changes. It appends newly
  received points to cached directional series and renders borrowed slices;
  the existing extrema reduction and direction breaks remain. Table values and
  reader buffers are reused. Live point queues can coalesce/drop display
  updates under load, as in alpha.1; CSV retains every acquired point.
- Stop, Output Off, heartbeat expiry, scientific limits, source identity checks,
  resource locks and verified output shutdown remain in place. No unsafe code,
  fast-math, shortened integration waits or reduced persistence was introduced.

## Verification and reproduction

Thirty Rust unit/fake-helper tests passed, as did strict Clippy, both platform
release builds and independent review. Engine integration checks passed:
compatible three-point CSV, independent connections, Stop with simulated output
OFF in 0.016 s and heartbeat loss cleanup in 2.008 s. These observations are
not guaranteed deadlines for failed hardware. Mac GUI simulation saved all
41 points and reported output OFF. The packaged Windows GUI likewise saved
41 simulated points, reported output OFF and exited with code zero.

The Windows packaged application also measured the authorized open-circuit
NI GPIB setup at -1 mV, 0 and +1 mV with 1 mA compliance. All three targets and
source readbacks matched, all 46 CSV fields were present, and a separate
post-run connection verified output OFF. The physical sweep took 2.142 s.
GS210 serial 91T729315 and 34411A serial MY53000981 matched the earlier test.
This is a communication and shutdown check, not known-resistor accuracy or
fresh acceptance of direct USB, 82357B, or driver reinstall behavior.

```sh
python3 scripts/benchmark_rust_optimization.py --before /path/to/alpha1/kohdalab-iv-next --after next/target/release/kohdalab-iv-next
cargo build --manifest-path next/Cargo.toml -p iv-core --example benchmark_plan --release --locked
python3 scripts/benchmark_rust_plan.py --before /path/to/old/benchmark_plan --after next/target/release/examples/benchmark_plan
```

```powershell
.\scripts\benchmark_rust_gui.ps1 -BeforePackage C:\alpha1 -AfterPackage C:\alpha2 -OutputDir C:\bench\rust -Repeats 5
```

Local raw evidence is excluded from distributions:

- `next/validation/optimization/run-20261013-234231/benchmark.json` and all CSVs/logs
- `next/validation/optimization/plan-20261013-234357/benchmark.json` and OS counter logs
- `next/validation/optimization/windows-gui-benchmark.json`
- `next/validation/optimization/engine-safety/` and packaged GUI/hardware evidence

The earlier exploratory timing run overlapped packaging and was excluded;
its validated CSVs and report remain preserved. A failed restricted OS-counter
read and the initial Windows validation-script enumeration error are also
preserved. The OS-counter test resumed with read access; the physical sweep
ran once and its saved CSV was revalidated without repeating measurement.

Before/after unsigned Mac engine SHA-256:

```text
alpha.1 bfa2eb96c1221ea3d44ce3de7518dd62b17765b1cad1c2097e9f2f2032180f47
alpha.2 23109e15230c44145cf561dd87fb68c7b208ccdc4498ccf84df1a4e3d26edf75
```

Windows executable SHA-256:

```text
alpha.1 ba79a93a5cf899a8b5c0d88d406ede3318e5faa58c163b2600dbe1999ebb316d
alpha.2 fc42a53506645f35bbc4b01126c806a636150b94ba0b3592f06cd90441955b8b
```

Preview packages are local under `next/dist/`; stable Python releases and
GitHub downloads were not replaced.
