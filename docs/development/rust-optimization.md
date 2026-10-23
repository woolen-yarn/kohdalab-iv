# Rust performance optimization, alpha.2 / Rust 性能最適化、alpha.2

> English source is followed by a complete Japanese counterpart. Hashes, commands,
> benchmark conditions, API identifiers, and measured values are identical in both sections.

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
python3 scripts/benchmark_rust_optimization.py --before /path/to/alpha1/kohdalab-iv-next --after target/release/kohdalab-iv
cargo build --manifest-path Cargo.toml -p iv-core --example benchmark_plan --release --locked
python3 scripts/benchmark_rust_plan.py --before /path/to/old/benchmark_plan --after target/release/examples/benchmark_plan
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

Preview packages are local under `dist/`; stable Python releases and
GitHub downloads were not replaced.

---

# Rust 性能最適化、alpha.2

保存した 0.3.0-alpha.1 release に対して 2026 年 10 月 13 日に計測しました。0.3.0-alpha.2 は元の三列 UI、測定条件、全点 CSV flush を保持します。application crates は引き続き unsafe code を禁止し、native USB/GPIB helpers と driver assignments は不変です。

## 同一 Mac の測定処理

24 GB RAM の Apple M4 Mac mini で両 release binaries を実行しました。各 workload は -1 mV から +1 mV の 1 kOhm circuit、zero waits、NPLC 1、averaging 1 を simulation しました。各 workload は warm-up 一回の後に順番を交互にして七 trials を行いました。全 CSV は元の 46-column schema、exact row count、ordered target/readback/current values、元の numerical tolerances で検証しました。成功 runs はすべて simulated output OFF で完了しました。

| Metric、median | Alpha.1 | Alpha.2 | Before / after |
| --- | ---: | ---: | ---: |
| 1001-point measurement and CSV loop | 6.476 ms | 3.074 ms | 2.11 |
| 10001-point measurement and CSV loop | 64.895 ms | 29.440 ms | 2.20 |
| 10001-point Run command to completed state | 115.116 ms | 31.785 ms | 3.62 |
| 1001-point fresh engine through disconnect/exit | 369.745 ms | 11.313 ms | 32.68 |
| 10001-point fresh engine through disconnect/exit | 438.710 ms | 38.790 ms | 11.31 |

loop timer は planning と CSV header creation の後に開始し cleanup 後に終了し、両 version で同一です。fresh-process timing は startup、connections、IPC、disconnect、exit を含みます。大きな改善は主に fixed polling/shutdown waits の除去で、GUI startup measurement ではありません。physical throughput には依然 instrument conversion、communication、configured NPLC、settling、ramp waits が含まれます。

## 大規模スイープ計画

standalone release benchmark は 990099 points を計画しました。これは 200 nV increments の valid 10001-point linear sweep を 99 回繰返すものです。両 builds は同じ example/release profile を使い、old core source hash は元 benchmark/distributed alpha.1 source と一致します。warm-up 一回の後、交互に五 trials を行いました。point count、endpoint metadata、direction counts、repeat/point indices、range、compliance、configuration は一致し、小さな directional edge cases も元 Decimal algorithm に対して試験しました。

| Metric、median | Alpha.1 | Alpha.2 |
| --- | ---: | ---: |
| Planning time | 80.297 ms | 17.543 ms |
| Standalone planner peak RSS | 123.641 MiB | 70.688 MiB |

planning は **4.58 倍高速**で、isolated workload の peak RSS は **42.8% 低下**です。macOS `/usr/bin/time -l` が peak resident memory を供給しました。これらは GUI、native helpers、measurement records、CSV writing を除外し、total application memory として提示してはいけません。

## Windows GUI

同じ Ryzen 5 5500GT / 16 GB Windows host で packaged x64 release binaries を実行しました。isolated simulated profiles は instrument connections を作りません。各 version で warm-up 一回の後に order を交互にして五 launches、同じ 1240 by 760 window、one-second settle、two-second idle sample を使いました。

| Metric、median | Alpha.1 | Alpha.2 |
| --- | ---: | ---: |
| Visible main-window proxy | 123.202 ms | 137.907 ms |
| Early idle process-tree working set | 112.902 MiB | 113.355 MiB |
| Early idle process-tree Private Bytes | 103.398 MiB | 103.684 MiB |

この test は GUI launch time や idle resident memory の改善を**示しません**。window appearance は 25 ms ごとに poll しており first paint/interaction latency ではありません。working-set sums は shared pages を複数回数え得て GPU memory は除外します。元の idle-memory objective は未達です。cached plot update は ordinary redraws の full-history scans/temporary plot copies を除きますが、large-graph frame rate はここで定量化していません。

## 変更と保持した safeguards

- CSV persistence は field/number buffers を再利用し invariant timing fields を cache、同一 numerical representations を再利用します。元の owned serializer は literal negative-zero/escaped-field fixtures を持つ compatibility oracle のままです。CSV quoting、exponent padding、optional blanks、exclusive file creation は互換で、header/every row は引き続き flush します。
- planning は各 base target を一回だけ convert し、repeated points を emit しながら directions を計算して duplicate repeated Decimal/direction arrays を除きます。range/resolution limits と one-million-point cap は残ります。
- progress snapshots は最大 50 ms ごとです。connection/completion/safety transitions は即時 publish します。condition variable が IPC writer を wake し、reusable buffers が bounded line formats を変えず JSON を batch します。shutdown は actual final-state delivery を待ち、blocked consumer 用 bounded fallback を持ちます。acquisition/output cleanup は GUI drawing を待ちません。
- GUI は revision が変わる場合のみ new state を読み、newly received points を cached directional series へ append し borrowed slices を render します。existing extrema reduction/direction breaks は残り、table values/reader buffers を再利用します。live point queues は alpha.1 と同様 load 時に display updates を coalesce/drop できますが、CSV は全 acquired point を保持します。
- Stop、Output Off、heartbeat expiry、scientific limits、source identity checks、resource locks、verified output shutdown は残ります。unsafe code、fast-math、shortened integration waits、reduced persistence は導入していません。

## 検証と再現

30 Rust unit/fake-helper tests、strict Clippy、両 platform release builds、independent review が成功しました。engine integration checks は compatible three-point CSV、independent connections、simulated output OFF を伴う 0.016 s の Stop、2.008 s の heartbeat loss cleanup を通過しました。これらは failed hardware の guaranteed deadlines ではありません。Mac GUI simulation は全 41 points を保存して output OFF を報告し、packaged Windows GUI も 41 simulated points 保存、output OFF 報告、code zero で exit しました。

Windows packaged application は authorized open-circuit NI GPIB setup を -1 mV、0、+1 mV、1 mA compliance で測定しました。全三 targets/source readbacks は一致し、全 46 CSV fields が存在し、separate post-run connection が output OFF を検証しました。physical sweep は 2.142 s で、GS210 serial 91T729315 と 34411A serial MY53000981 は earlier test と一致しました。これは communication/shutdown check であり、known-resistor accuracy、direct USB、82357B、driver reinstall behavior の fresh acceptance ではありません。

```sh
python3 scripts/benchmark_rust_optimization.py --before /path/to/alpha1/kohdalab-iv-next --after target/release/kohdalab-iv
cargo build --manifest-path Cargo.toml -p iv-core --example benchmark_plan --release --locked
python3 scripts/benchmark_rust_plan.py --before /path/to/old/benchmark_plan --after target/release/examples/benchmark_plan
```

```powershell
.\scripts\benchmark_rust_gui.ps1 -BeforePackage C:\alpha1 -AfterPackage C:\alpha2 -OutputDir C:\bench\rust -Repeats 5
```

local raw evidence は distributions から除外します: `next/validation/optimization/run-20261013-234231/benchmark.json` と全 CSVs/logs、`next/validation/optimization/plan-20261013-234357/benchmark.json` と OS counter logs、`next/validation/optimization/windows-gui-benchmark.json`、`next/validation/optimization/engine-safety/` と packaged GUI/hardware evidence。earlier exploratory timing run は packaging と重なったため除外し、validated CSVs/report は保存しています。failed restricted OS-counter read と initial Windows validation-script enumeration error も保存し、OS-counter test は read access で再開、physical sweep は一回実行して saved CSV を再測定なしに revalidate しました。

before/after unsigned Mac engine SHA-256:

```text
alpha.1 bfa2eb96c1221ea3d44ce3de7518dd62b17765b1cad1c2097e9f2f2032180f47
alpha.2 23109e15230c44145cf561dd87fb68c7b208ccdc4498ccf84df1a4e3d26edf75
```

Windows executable SHA-256:

```text
alpha.1 ba79a93a5cf899a8b5c0d88d406ede3318e5faa58c163b2600dbe1999ebb316d
alpha.2 fc42a53506645f35bbc4b01126c806a636150b94ba0b3592f06cd90441955b8b
```

preview packages は `dist/` 下の local にあり、stable Python releases/GitHub downloads は置換していません。
