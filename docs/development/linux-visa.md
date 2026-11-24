# Linux beyond simulation-only: design note / Linux 実機対応の設計メモ

Status: investigation only (issue #27, improvement Task 6). Nothing in this note
changes runtime behavior, packaging, or validation status. The Linux release is
still **simulation only** ([`packaging/linux/SIMULATION-ONLY.md`](../../packaging/linux/SIMULATION-ONLY.md)),
and no Linux hardware was connected for this note.

状態：調査のみ（issue #27、改善タスク 6）。本メモは実行時の動作、パッケージ、検証状況を
一切変更しません。Linux 版は引き続き**シミュレーション専用**であり、本メモの作成で
Linux 上の実機は接続していません。

## 1. What is platform-gated today

Source inspection on 2026-11-24 (`origin/main` at `536697a`):

| Gate | File | Effect on Linux |
| --- | --- | --- |
| `simulation-only` Cargo feature | `crates/iv-transport/Cargo.toml`, `crates/iv-desktop/Cargo.toml`, `.github/workflows/linux-package.yml` | Linux packages are built with it. `refuse_hardware()` in `crates/iv-transport/src/lib.rs` makes `HelperPaths::discover`, discovery, connect, and the shared-hub client fail closed. Helpers dropped next to the binary or named by `KOHDALAB_IV_HELPER_DIR` / `KOHDALAB_INSTRUMENT_HUB_CONFIG` are ignored. |
| `visa_helper()` | `crates/iv-transport/src/lib.rs` (`if !cfg!(any(windows, test))`) | Returns `None` on non-Windows production builds, so `discover_visa()` yields nothing and `spawn_for_resource()` cannot route a `ResourceKind::Visa`. |
| `HelperPaths::discover` roots | `crates/iv-transport/src/lib.rs` | Non-Windows builds look in `build/native-gpib` and `build/usbtmc-probe`, which are macOS development layouts. Only Windows looks for `USB-Setup`. |
| `connection_kind()` ADCMT message | `crates/iv-transport/src/lib.rs` | ADCMT 7461A over USB requires official VISA. On Linux that path is unreachable because of the `visa_helper()` gate. |
| Official VISA helper | `native/visa_helper.cpp` | With `#ifndef _WIN32`, `main()` prints "only available on Windows" and exits. On Windows, `visa64.dll`/`visa32.dll` are loaded only from System32 by absolute path. A test-only `KOHDALAB_VISA_TEST_DLL` compile-time override exists. |
| Fake VISA ABI | `native/tests/windows_visa_mock.cpp`, `native/tests/build_windows_visa_tests.ps1`, `native/tests/windows_visa_helper.ps1` | Windows-only (`__declspec(dllexport)`, `__stdcall`, MSVC `cl.exe`). Run by the Windows job in `.github/workflows/test.yml`. |
| Native USB-GPIB / USBTMC helpers | `native/kohdalab_gpib_helper.cpp`, `native/agilent_82357b.cpp`, `native/usbtmc_helper.cpp`, `native/usbtmc_transport.hpp` | Use libusb and are portable in principle, but are built and validated only for Windows (WinUSB) and macOS. No udev rule, kernel-driver detach policy, or Linux build script exists. |
| Process creation flags | `configure_process()` in `crates/iv-transport/src/lib.rs` | The Windows branch only hides the console window. Non-Windows is a no-op and already works under `cfg(unix)` tests. |
| Device lease | `DeviceLease` in `crates/iv-transport/src/lib.rs` | Uses `fs2` advisory locks under `$HOME/.kohdalab/iv-next-locks/`. Not platform-gated. On Linux this is `flock(2)`, which is per open file description and advisory. |

Not gated: all Rust driver logic (`drivers.rs`, `instruments.rs`, `inventory.rs`)
and the `kiv` line protocol. The `cfg(all(test, unix))` tests in
`crates/iv-transport/src/lib.rs` already exercise helper processes, leases, VISA
routing (with a shell-script fake `kohdalab-visa-helper`), and output-off
precedence on Linux CI. These are device-free.

## 2. Official Linux VISA story for supported instruments

The official references below were checked separately (Sol, 2026-11-24) and are
recorded in the triage research notes. The links still need to be copied into
this note from those notes before review. This file does not re-quote them, so
it adds no claim beyond what those sources say.

- **Keysight IO Libraries Suite for Linux.** The revision matrix lists supported
  distributions and kernels for each release. A Linux backend must name an exact
  IOLS revision and distribution from that matrix. "Linux" alone is not enough.
- **NI GPIB-USB-HS+.** The research notes record NI's policy that this adapter
  is unsupported on Linux. A Linux GPIB path must therefore not depend on NI
  VISA with an NI USB-GPIB adapter. The native `kohdalab-gpib-helper` NI branch
  (vendor `0x3923`) is in the same position until it is separately reviewed.
- **Yokogawa GS210.** Its USB interface must be distinguished from a generic
  USB-TMC class device. The official documentation decides whether the GS210 is
  reachable through a VISA `USB…::INSTR` resource on Linux. That cannot be
  inferred from the native libusb USBTMC helper working on macOS.
- **Keysight 34411A.** A USB-TMC/LAN/GPIB meter covered by Keysight VISA. The
  Windows fake ABI already models it (`MOCK34411`).

| Reference | Link |
| --- | --- |
| Keysight IO Libraries Suite for Linux, revision matrix | _TODO: copy from research notes_ |
| NI GPIB-USB-HS+ Linux support policy | _TODO: copy from research notes_ |
| Yokogawa GS210 USB / USB-TMC documentation | _TODO: copy from research notes_ |
| Keysight 34411A connectivity / IOLS support | _TODO: copy from research notes_ |

## 3. What single-owner lease and output shutdown imply for Linux

1. **Lease semantics stay the same.** `DeviceLease::acquire_adapter(true)` (shared
   VISA adapter lock) and the exclusive `gpib-adapter` lock for the native backend
   must keep their current meaning. On Linux, `flock` locks are advisory and do
   not stop another VISA client (for example, Keysight Connection Expert or a
   `pyvisa` script) from opening the same resource. This matches Windows today,
   and the note must not describe it as stronger isolation.
2. **There is one GPIB backend per controller.** The "GPIB0 cannot mix official
   VISA and native WinUSB sessions" refusal in `connect_endpoint()` and the
   discovery skip ("GPIB0 is owned by official VISA") are platform-independent
   and must stay in force on Linux. The wording in those messages says "WinUSB".
   A future Linux change may adjust that text, but not the rule.
3. **The kernel driver must not be detached automatically.** On Linux the kernel
   `usbtmc` module may bind USBTMC interfaces. A Linux native helper must fail
   closed with a clear message rather than call `libusb_detach_kernel_driver`
   silently. This is the Linux equivalent of "existing manufacturer drivers are
   preserved" on Windows.
4. **Output-off precedence does not depend on the platform.** `OutputOffReport`,
   `output_off_dispatch()`, and the `error_kind` classification are pure Rust
   above the helper process. A Linux backend only needs the helper to report
   write/read failures through the existing `kiv` error lines. No Linux-specific
   shutdown path is required, and none may be added that bypasses the existing
   one.
5. **Runtime loading stays isolated.** As on Windows, the Linux VISA runtime must
   be loaded only in the helper process (`#![forbid(unsafe_code)]` stays in
   `iv-transport`), from a fixed absolute path list. It must never use
   `LD_LIBRARY_PATH`, the working directory, or the application directory.

## 4. Proposed file-level transport changes (future PRs, not this one)

| Step | Files | Change | Gate |
| --- | --- | --- | --- |
| A | `native/visa_helper.cpp` | Add a `__linux__` branch: `dlopen` a fixed list of absolute paths (the exact paths come from the IOLS revision chosen in §2), `dlsym` the same ten `vi*` symbols, and keep the Windows branch byte-for-byte. Add a test-only `KOHDALAB_VISA_TEST_SO` compile-time override that mirrors `KOHDALAB_VISA_TEST_DLL`. | Compiles on Linux CI. Not packaged. |
| B | `native/tests/linux_visa_mock.cpp` (new), `native/tests/build_linux_visa_tests.sh` (new) | Port of `windows_visa_mock.cpp` as a shared object (see §5). Builds the mock and a mock-linked helper, then runs the same `--list` and session protocol checks as `windows_visa_helper.ps1`. | Linux CI job, device-free. |
| C | `crates/iv-transport/src/lib.rs` | Replace `cfg!(any(windows, test))` in `visa_helper()` with a gate that also admits Linux **only when** not `simulation-only` and behind a new opt-in Cargo feature (for example, `linux-visa-experimental`). Add Linux roots in `HelperPaths::discover`. Leave `refuse_hardware()` untouched. | Existing `cfg(unix)` tests plus new routing tests with the step B helper. |
| D | `scripts/package_linux.py`, `.github/workflows/linux-package.yml` | Unchanged until a physical validation plan exists. The released Linux artifact stays simulation-only. | — |

## 5. Device-free fake VISA ABI for Linux (step B)

The fake ABI should mirror `native/tests/windows_visa_mock.cpp` so the same
protocol assertions run on both platforms:

- **Export surface:** exactly the ten symbols `visa_helper.cpp` resolves:
  `viOpenDefaultRM`, `viFindRsrc`, `viFindNext`, `viOpen`, `viClose`,
  `viSetAttribute`, `viWrite`, `viRead`, `viClear`, and `viGpibControlREN`.
  Export them with `extern "C" __attribute__((visibility("default")))` and the
  default System V calling convention. Linux has no `__stdcall`.
- **Types:** keep the Windows mock's typedefs (`ViStatus = int32_t`,
  `ViSession`/`ViUInt32`/`ViAttr = uint32_t`, `ViAttrState = uintptr_t`). Add a
  `static_assert(sizeof(ViAttrState) == 8)` for x86-64 so an LP64 mismatch fails
  the build instead of corrupting attribute values.
- **Fixtures:** the same resources (`USB0::0x0957::0x0A07::MOCK34411::0::INSTR`,
  `GPIB0::7::INSTR`), the same `viOpen` checks (exclusive mode `1`, timeout
  `2000`), the same attribute whitelist, and the same `VI_ERROR_RSRC_NFOUND`
  paths. Shared fixtures keep Linux and Windows results comparable.
- **Isolation:** build into a temporary directory and link the helper against
  the mock only through `KOHDALAB_VISA_TEST_SO`. Never install it, package it,
  or place it on a library search path. The production helper build must not
  contain that macro. Copy the `windows_visa_helper.ps1` check that compares
  production against mock binaries.
- **Not proven:** passing these tests shows the helper's ABI use and `kiv`
  framing on Linux. It does **not** show that a real IOLS or NI-VISA runtime,
  adapter, or instrument works. No physical validation claim follows from it.

## 6. Smallest safe first step

Steps A and B only: build `visa_helper` and the Linux fake ABI in CI, and run
the ported protocol checks. This needs no physical device, no driver
installation, no udev rule, and no change to `iv-transport` or the shipped Linux
package. Steps C and D wait for maintainer review of the official references in
§2 and a written hardware-validation plan
([`docs/guides/hardware-validation.md`](../guides/hardware-validation.md)).

## 概要（日本語）

- 現状のゲート：Linux 版は `simulation-only` 機能でビルドされ、`refuse_hardware()` が全ての実機経路を拒否します。さらに `visa_helper()` は Windows 以外の本番ビルドで `None` を返し、`native/visa_helper.cpp` は Windows 以外では起動しません。
- 公式情報：Keysight IO Libraries Suite for Linux の対応表、NI GPIB-USB-HS+ が Linux 非対応である方針、GS210 の USB と汎用 USB-TMC の区別、Keysight 34411A の対応を別途確認済みです（リンクは調査メモから転記予定）。
- 保証の維持：アダプタロックの意味、GPIB バックエンドは一つだけという規則、出力停止の優先順位は変えません。Linux のカーネル `usbtmc` ドライバーは自動で切り離さず、明示的なエラーで止めます。
- 最初の一歩：`visa_helper` の Linux 版と、Windows 版モックと同じ fixture を持つ偽 VISA 共有ライブラリを CI でビルドしてテストするだけです。実機、ドライバー導入、配布物の変更は伴いません。物理的な検証を主張するものではありません。
