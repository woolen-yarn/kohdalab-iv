# Rust and Python performance comparison

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
