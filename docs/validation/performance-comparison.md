# Rust and Python performance comparison / Rust と Python の性能比較

> **Language navigation / 言語案内:** The English historical evidence appears first, followed by a complete Japanese counterpart. Benchmark conditions, values, source and hash references, and limitations are preserved in both languages.
> 英語の履歴上の根拠を先に置き、その後に完全な日本語対訳を掲載します。ベンチマーク条件、数値、source/hash 参照、制限事項は両言語で保持します。

Measured on 13 October 2026. The Rust 0.3.0-alpha.1 preview shows its main window
about 4.0 times sooner than Python 0.2.3 and processes the tested simulated
CSV workload about 16.4 times faster inside the measurement loop.
Windows archive size is 90.7% smaller. Resident working-set totals after
60 seconds are 6.1% higher in Rust, so lower resident memory is still
an outstanding objective.

The later [alpha.2 optimization report](../development/rust-optimization.md) compares the
Rust versions directly. The historical alpha.1 measurements below are retained.

## Windows portable GUI

Both unmodified Windows x64 GUI binaries ran on the same AMD Ryzen 5 5500GT
machine, with 16 GB installed RAM and Windows 10.0.26300.0. The app window size
was 1240 × 760. Python used the latest fast-setup portable; Rust used the original
UI reproduction preview. The executables and dependency hashes are recorded in
the raw report.

| Metric | Python 0.2.3 | Rust 0.3.0-alpha.1 |
| --- | ---: | ---: |
| Visible main-window median, ten warm launches | 0.503 s | 0.125 s |
| Visible main-window minimum to maximum | 0.496–0.522 s | 0.108–0.138 s |
| Early idle working-set median, ten launches | 104.5 MiB | 113.0 MiB |
| Working-set total after sixty seconds, one run each | 104.4 MiB | 110.7 MiB |
| Private Bytes after sixty seconds | 412.1 MiB | 100.7 MiB |
| Participating processes while unconnected | 1 | 2 |

Each startup trial waited one second after window appearance, then sampled
process counters for two seconds. The first launch of each app was a warm-up
and excluded; ten subsequent trials alternated app order. A separate run held
each app for sixty seconds before a two-second memory sample. Memory figures
sum the GUI and all its child processes. The longer idle Rust sample contained
about 101.1 MiB for the GUI and 9.6 MiB for the engine. No instrument helper was
active. Median idle CPU counters were zero at the short sample's resolution;
this does not establish zero CPU consumption.

The startup criterion was a visible main-window handle and expected app title,
polled every 25 ms. It is an OS window-creation proxy and does not prove first
paint, first usable control, cold start, or a high frame rate. No cache purge or
OS restart was performed.

The Python executable and its `_internal` dependencies were unchanged. An
isolated copy with a directory junction omitted `support/USB-Setup.exe`, so its
automatic driver check did not run. The Rust alpha also does not perform that
check automatically. This isolates GUI launch behavior; a first driver install
or regular automatic setup completion is a separate operation. Both apps used
isolated simulated configs and never connected to physical instruments.

Working sets contain pages resident in physical memory, including shared pages.
Summing processes may count shared pages more than once. Private Bytes describes
private allocated memory and must not be substituted for resident RAM. GPU
memory was not measured. See Microsoft's [working-set definition](https://learn.microsoft.com/en-us/windows/win32/memory/working-set)
and [.NET private-memory counter](https://learn.microsoft.com/en-us/dotnet/api/system.diagnostics.process.privatememorysize64).

## Mac measurement engine and CSV processing

The same Apple M4 Mac mini with 24 GB RAM ran CPython 3.13.13 against the current
Python 0.2.3 source APIs and the optimized Rust release engine. This compares
measurement implementations on one machine; it is separate from the Windows
frozen GUI comparison.

Both engines simulated a 1 kOhm reference over −1 mV to +1 mV, with identical
linear targets, NPLC 1, averaging 1, range and compliance. All settling and ramp
waits were zero. A 1001-point warm-up per engine preceded five trials for each
count, alternating order. Every successful run exited with code zero and
validated all 46 CSV columns, exact row counts, ordered target/readback values,
current response, legitimate empty resistance at zero and simulated output OFF.

| Point count | Python loop median | Rust loop median | Python divided by Rust |
| --- | ---: | ---: | ---: |
| 1001 | 0.108029 s | 0.006591 s | 16.4 |
| 10001 | 1.076678 s | 0.065821 s | 16.4 |

The Python loop timer surrounds `Experiment.run_iv` after planning and simulated
connection. Rust's `Snapshot.elapsed` starts after planning and CSV header
creation and ends after output cleanup. Both include per-point CSV flushing.
The boundaries differ by initial setup/header work; they do not include imports,
process startup, plotting, device I/O or real meter integration time. These are
application implementations with different row materialization and streaming,
not a language-only microbenchmark. The Rust live point-event queue may coalesce
updates while the validated CSV retains every row.

Whole fresh-process wall time is a different metric:

| Point count | Python launch through exit | Rust launch through disconnect and exit |
| --- | ---: | ---: |
| 1001 | 0.162 s | 0.367 s |
| 10001 | 1.145 s | 0.435 s |

Rust's IPC, state publication and shutdown add fixed overhead to short standalone
jobs. The GUI normally keeps its engine alive, so each GUI sweep does not incur
a fresh engine launch. Real measurement speed also includes configured waits,
communication and instrument conversion; the 16.4 ratio applies to
the simulated processing loop rather than a physical measurement throughput.

A proposed 100001-point linear sweep over the same ±1 mV range was excluded:
20 nV steps violate the Rust source's 100 nV resolution. A custom list also
exceeded the bounded 64 KiB input frame. The constraints and tolerances were
preserved. Failed development attempts remain separate from the successful raw
report and do not enter these statistics.

## Distribution size

These figures describe complete current archives, including their supplied
support, source and license material. The Rust preview supports fewer models
and features than the Python edition; GUI/runtime and package contents also
contribute to the size difference.

| Platform | Python ZIP | Rust ZIP | Python extracted | Rust extracted |
| --- | ---: | ---: | ---: | ---: |
| Windows x64 | 76.40 MB | 7.07 MB | 192.52 MB | 15.23 MB |
| Mac Apple Silicon | 68.46 MB | 5.62 MB | 160.81 MB | 11.36 MB |

Archive sizes use decimal MB. Process memory uses MiB. File hashes and exact
bytes are retained in `next/validation/performance-summary.json`.

## Reproduction and evidence

The development benchmark scripts are [benchmark_portable_gui.ps1](../../scripts/benchmark_portable_gui.ps1)
and [benchmark_iv_engines.py](https://github.com/woolen-yarn/kohdalab-iv/blob/5f35cbaf88175bc9f5068a51bac639b1bdb0943d/scripts/benchmark_iv_engines.py). They do not
modify shipping app code or access physical instruments. Windows launches need
the existing logged-in interactive desktop; SSH session zero is unsuitable for
GUI timing. The temporary interactive tasks and observed apps were removed or
closed after measurement. Original user configs remain unchanged.

```powershell
.\scripts\benchmark_portable_gui.ps1 -PythonPackage C:\path\python-portable -RustPackage C:\path\rust-preview -SimulatedConfig .\portable\configs\simulated.json -OutputDir C:\bench\gui -Repeats 10
.\scripts\benchmark_portable_gui.ps1 -PythonPackage C:\path\python-portable -RustPackage C:\path\rust-preview -SimulatedConfig .\portable\configs\simulated.json -OutputDir C:\bench\idle -Repeats 1 -IdleSettleSeconds 60 -SkipWarmup
```

```sh
.venv/bin/python scripts/benchmark_iv_engines.py
```

Raw evidence is local and excluded from distributions:

- `next/validation/performance-windows/gui-benchmark.json`
- `next/validation/performance-windows/gui-long-idle.json`
- `next/validation/performance-mac/run-20261013-225249/benchmark.json`, configs,
  CSVs and subprocess logs
- `next/validation/performance-summary.json`

The resident-memory target of at most 100 MiB for the process tree remains unmet
in this Windows idle test. Maintaining process isolation while reducing GUI and
engine footprint is the next performance concern. Large live-graph frame rates
and physical sweep timings require their own acceptance conditions.

---

# Rust と Python の性能比較（日本語）

2026年10月13日に測定しました。Rust 0.3.0-alpha.1 preview は Python 0.2.3 より main window を約 **4.0 倍**早く表示し、測定 loop 内の tested simulated CSV workload を約 **16.4 倍**速く処理します。Windows archive size は **90.7%**小さくなりました。60 秒後の resident working-set total は Rust で **6.1%**高く、より低い resident memory は未達の目的です。

後の [alpha.2 optimization report](../development/rust-optimization.md) は Rust version 同士を直接比較します。以下の historical alpha.1 measurement は保持します。

## Windows portable GUI

変更していない Windows x64 GUI binary を、16 GB installed RAM、Windows 10.0.26300.0 の同じ AMD Ryzen 5 5500GT machine で実行しました。app window size は 1240 × 760 です。Python は latest fast-setup portable、Rust は original UI reproduction preview を使用しました。executable/dependency hash は raw report に記録されています。

| 指標 / Metric | Python 0.2.3 | Rust 0.3.0-alpha.1 |
| --- | ---: | ---: |
| Visible main-window median, ten warm launches / 可視 main window の中央値、warm launch 10 回 | 0.503 s | 0.125 s |
| Visible main-window minimum to maximum / 可視 main window の最小–最大 | 0.496–0.522 s | 0.108–0.138 s |
| Early idle working-set median, ten launches / early idle working-set 中央値、10 launch | 104.5 MiB | 113.0 MiB |
| Working-set total after sixty seconds, one run each / 60 秒後 working-set total、各 1 run | 104.4 MiB | 110.7 MiB |
| Private Bytes after sixty seconds / 60 秒後 Private Bytes | 412.1 MiB | 100.7 MiB |
| Participating processes while unconnected / 未接続時の参加 process | 1 | 2 |

各 startup trial は window appearance 後 1 秒待機し、続けて 2 秒間 process counter を sample しました。各 app の最初の launch は warm-up として除外し、その後の 10 trial は app order を交互にしました。別 run では各 app を 60 秒保持してから 2 秒の memory sample を取得しました。memory figure は GUI と全 child process の合計です。長い idle Rust sample は GUI 約 101.1 MiB、engine 約 9.6 MiB でした。instrument helper は active ではありません。short sample の resolution では median idle CPU counter は zero でしたが、これは CPU consumption が zero である証明ではありません。

startup criterion は、expected app title を持つ visible main-window handle を 25 ms ごとに poll することです。これは OS の window-creation proxy であり、first paint、first usable control、cold start、高 frame rate を証明しません。cache purge/OS restart は行っていません。

Python executable とその `_internal` dependency は不変です。directory junction を使う isolated copy では `support/USB-Setup.exe` を除き、automatic driver check を実行しませんでした。Rust alpha もこの check を自動実行しません。これは GUI launch behavior を分離するもので、first driver install/regular automatic setup completion は別 operation です。両 app は isolated simulated config を使用し、physical instrument に接続していません。

working set は shared page を含む physical memory resident page を含みます。process 合計では shared page を複数回数えることがあります。Private Bytes は private allocated memory を表し resident RAM の代用にしてはいけません。GPU memory は測定していません。Microsoft の [working-set definition](https://learn.microsoft.com/en-us/windows/win32/memory/working-set) と [.NET private-memory counter](https://learn.microsoft.com/en-us/dotnet/api/system.diagnostics.process.privatememorysize64) を参照してください。

## Mac 測定 engine と CSV 処理 / Mac measurement engine and CSV processing

同じ Apple M4 Mac mini（24 GB RAM）で、CPython 3.13.13 を current Python 0.2.3 source API、optimized Rust release engine と比較しました。これは一台の machine 上の measurement implementation 比較であり、Windows frozen GUI comparison とは別です。

両 engine は −1 mV から +1 mV の 1 kOhm reference を同じ linear target、NPLC 1、averaging 1、range/compliance で simulate しました。settling/ramp wait はすべて zero です。各 engine で 1001-point warm-up 後、各 count に対して order を交互にした 5 trial を行いました。successful run はすべて exit code zero で終了し、全 **46 CSV columns**、exact row count、ordered target/readback value、current response、zero の正当な empty resistance、simulated output OFF を validate しました。

| Point count | Python loop median | Rust loop median | Python divided by Rust |
| --- | ---: | ---: | ---: |
| 1001 | 0.108029 s | 0.006591 s | 16.4 |
| 10001 | 1.076678 s | 0.065821 s | 16.4 |

Python loop timer は planning/simulated connection 後の `Experiment.run_iv` を囲みます。Rust の `Snapshot.elapsed` は planning/CSV header creation 後に始まり output cleanup 後に終わります。両者は point ごとの CSV flush を含みます。boundary は initial setup/header work の差があり、import、process startup、plotting、device I/O、real meter integration time を含みません。これは row materialization/streaming が異なる application implementation の比較で、language-only microbenchmark ではありません。Rust live point-event queue は update を coalesce できますが、validated CSV は全 row を保持します。

fresh-process wall time は別の metric です。

| Point count | Python launch through exit | Rust launch through disconnect and exit |
| --- | ---: | ---: |
| 1001 | 0.162 s | 0.367 s |
| 10001 | 1.145 s | 0.435 s |

Rust の IPC、state publication、shutdown は short standalone job に fixed overhead を追加します。GUI は通常 engine を生存させるため、各 GUI sweep では新しい engine launch を負担しません。real measurement speed には configured wait、communication、instrument conversion も含まれます。**16.4** の比は physical measurement throughput ではなく simulated processing loop に適用されます。

同じ ±1 mV range の proposed 100001-point linear sweep は除外しました。20 nV step が Rust source の 100 nV resolution に違反するためです。custom list は bounded **64 KiB** input frame も超えました。constraint/tolerance は保持しました。failed development attempt は successful raw report とは別に保管し、この statistic に入れません。

## Distribution size

これらの figure は supplied support/source/license material を含む complete current archive を表します。Rust preview は Python edition より少ない model/feature に対応し、GUI/runtime/package content も size difference に寄与します。

| Platform | Python ZIP | Rust ZIP | Python extracted | Rust extracted |
| --- | ---: | ---: | ---: | ---: |
| Windows x64 | 76.40 MB | 7.07 MB | 192.52 MB | 15.23 MB |
| Mac Apple Silicon | 68.46 MB | 5.62 MB | 160.81 MB | 11.36 MB |

archive size は decimal MB、process memory は MiB を使います。file hash/exact byte は `next/validation/performance-summary.json` に保持します。

## 再現と根拠 / Reproduction and evidence

development benchmark script は [benchmark_portable_gui.ps1](../../scripts/benchmark_portable_gui.ps1) と [benchmark_iv_engines.py](https://github.com/woolen-yarn/kohdalab-iv/blob/5f35cbaf88175bc9f5068a51bac639b1bdb0943d/scripts/benchmark_iv_engines.py) です。shipping app code を変更せず physical instrument にも access しません。Windows launch は既存 logged-in interactive desktop を必要とし、SSH session zero は GUI timing に適しません。temporary interactive task/observed app は measurement 後に remove/close しました。original user config は不変です。

```powershell
.\scripts\benchmark_portable_gui.ps1 -PythonPackage C:\path\python-portable -RustPackage C:\path\rust-preview -SimulatedConfig .\portable\configs\simulated.json -OutputDir C:\bench\gui -Repeats 10
.\scripts\benchmark_portable_gui.ps1 -PythonPackage C:\path\python-portable -RustPackage C:\path\rust-preview -SimulatedConfig .\portable\configs\simulated.json -OutputDir C:\bench\idle -Repeats 1 -IdleSettleSeconds 60 -SkipWarmup
```

```sh
.venv/bin/python scripts/benchmark_iv_engines.py
```

raw evidence は local で distribution から除外します。

- `next/validation/performance-windows/gui-benchmark.json`
- `next/validation/performance-windows/gui-long-idle.json`
- `next/validation/performance-mac/run-20261013-225249/benchmark.json`、config、CSV、subprocess log
- `next/validation/performance-summary.json`

process tree の resident-memory target（at most **100 MiB**）はこの Windows idle test で未達です。process isolation を保ちながら GUI/engine footprint を減らすことが次の performance concern です。large live-graph frame rate と physical sweep timing には独自の acceptance condition が必要です。
