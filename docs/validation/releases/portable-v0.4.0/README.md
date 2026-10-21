# KohdaLab IV portable v0.4.0

Rust desktop with integrated live I/V cards, reviewed instrument profiles, compact
14-column scientific CSV, isolated native USB/GPIB helpers and Python/Notebook control.

Downloads: [Kohdalab](https://github.com/Kohdalab/kohdalab-iv/releases/tag/portable-v0.4.0)
and [personal mirror](https://github.com/woolen-yarn/kohdalab-iv/releases/tag/portable-v0.4.0).
Both contain the macOS arm64 portable, Windows x64 portable, source build kit and checksums.

Validation: Rust workspace tests, Clippy, simulation CSV round-trip checks, output-OFF
recovery/lease tests, GUI screenshot, Windows GS210/34411A live switching and acquisition.
Windows driver setup uses the included WinUSB helper; macOS bundles are ad hoc signed.
Instrument function changes require verified output OFF and zero.

See root README.md, next/README.md and bindings/python/README.md for usage/build instructions.
