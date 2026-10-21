# Windows fast driver setup revision - 2026-10-07

Extract `kohdalab-iv-0.2.3-windows-x64-portable-fast-setup.zip` into a new
folder and launch `KohdaLab IV.exe`. Keep `_internal` and `support` beside it.
The package has four top-level entries and includes English instructions,
licenses, diagnostics, and corresponding source in `support/SOURCE.zip`.

The libwdi diagnostic reader now opens the Windows SetupAPI log with
`FILE_SHARE_DELETE`. Its former handle blocked log replacement and caused
approximately 34 seconds of waiting in each installation pass. The original
signed-package staging and exact-instance `DiInstallDevice` assignment are
retained. The fixed device allowlist, signature checking, device activation,
and interface GUID verification are retained. Experimental alternate APIs
and unchecked package caching were not adopted.

Acceptance tests on the Windows instrument PC:

| Test | Result | Time |
| --- | --- | --- |
| Previous full NI package deletion/reinstallation | Passed | About 70 s |
| Fixed installer, full NI package deletion/reinstallation | Passed | 3.18 s |
| Repeat of NI package deletion/reinstallation | Passed | 2.89 s |
| Direct USB 34411A package deletion/reinstallation | Passed | 3.16 s |
| Final portable support/USB-Setup.exe, NI recovery | Passed | 2.93 s |

Both NI GPIB and direct USB three-point open-output sweeps passed again using
the frozen application measurement engine and the final portable helpers:
-1, 0, +1 mV; 1 mA hardware current limit; three saved CSV rows; no SCPI errors;
zero source level and output OFF afterward. No manufacturer VISA was added.
GUI startup smoke, native startup/self-tests, and the simulated sweep passed.
All 351 Python tests, 100% statement/branch coverage, Ruff and mypy passed.
Physical measurement used the frozen CLI; GUI buttons were not operated in the
SSH session. Open-output tests do not establish loaded measurement accuracy.

Times describe this test PC, not a universal installation deadline. Windows
approval, policies, reboots, and other hardware can change installation time.
Normal startup skips installation when supported devices are already ready.

The matching Windows and macOS source build kits are in `portable/build-kits/`.
The portable-v0.2.3 Windows asset and both source build kits are refreshed on
GitHub for Kohdalab and the personal mirror. The macOS application ZIP is
unchanged; earlier local revisions are retained for recovery.
Detailed timing logs and test evidence remain in the ignored
`validation/windows-20261007/` folder. Verify the ZIP with `SHA256SUMS.txt`.
