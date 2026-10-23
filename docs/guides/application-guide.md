# KohdaLab IV Native Desktop / KohdaLab IV ネイティブデスクトップ

> **日本語の案内:** 操作上重要な安全条件、コマンド名、設定キー、CSV 列名は英語原文の表記を
> そのまま使います。対応する短い操作手順は[クイックスタート](quick-start.md)と
> [ハードウェア検証](hardware-validation.md)で日本語でも確認できます。

Development **1.3.0-alpha.7** ports Yokogawa 7651, Agilent 34401A, Keysight 34465A and ADCMT 7461A from the former Python drivers. Protocol mocks pass; physical validation is pending. GS200 and native ADCMT USB are not added. See the instrument matrix in the source README.

Version **1.2.0** adds a Windows Setup.exe installer and per-device official VISA preference
on Windows.

Windows discovery first asks the installed 64-bit system VISA runtime for USB0
and GPIB0 instruments. These entries appear as `VISA::<resource>` in the app;
`VISA::` is an app routing prefix, not part of the vendor VISA resource string.
Old saved resource strings automatically choose VISA when it exposes the same
device. Other devices use the existing native WinUSB route. No runtime is
downloaded or bundled, and the app never replaces a driver during Refresh.
A selected VISA connection does not silently fall back after a lock/timeout/error.
Manufacturer drivers without a usable 64-bit VISA runtime are retained and need
their vendor runtime configured. The updated USB setup preserves all existing
non-WinUSB assignments, including third-party drivers.
USB0 and GPIB0 primary-address resources are supported. GPIB0 uses one backend
at a time: when VISA owns it, native controller probing is skipped and saved
GPIB0 addresses also use VISA. Multiple simultaneous GPIB controllers are not
supported. Official-VISA and native direct-USB instruments may be mixed.


Version 1.1.0 refreshes the existing three-column interface with quiet charcoal
cards, distinct input fields and action buttons, aligned sweep controls, readable
path/resource summaries, and explicit connection/output states. Paths and resource
strings remain editable: click a summary to edit the full value, or hover to view it.
Windows Program Files installation and safe updates remain available.
It keeps Run focused on Start / Stop / Save, with internal output-off
control and automatic cleanup. It includes complete-data Save, automatic source
setup and per-point Repeat averaging. It keeps unverified 82357B firmware acquisition/upload excluded and supplies
explicit redistribution terms, native licensing evidence and relinking steps.
Blue/purple forward/backward styles and the CSV-matched inspector are retained.

References: [GS210 manual, section 5.2](https://www.yokogawa.com/pdf/provide/E/GW/IM/0000023941/0/IMGS210-01EN.pdf),
[34411A SCPI comparison](https://www.keysight.com/us/en/assets/7018-02044/white-papers-archived/5990-3515.pdf).

Sweep targets can still be
negative; the measurement engine continues to validate all limits before a run.

The Rust desktop includes the KohdaLab IV app icon. It uses the same
egui and eframe GUI foundation as KohdaLab Camera Native, with a separate Rust
measurement engine and the established USB and GPIB helper processes.

The published v1.2.0 desktop supports Yokogawa GS210 and Agilent or Keysight 34411A. The development edition additionally supports the models listed above. It includes
IV and VI modes, linear, round trip, zero centered and custom sweeps, limits,
settling, averaging, NPLC, streamed CSV, run metadata, and an explicit simulator.
Hardware accuracy and all firmware variants are not certified by this release.

This update reuses CSV formatting buffers, builds sweep plans without
duplicate intermediate arrays, updates display state only when needed, and
appends to cached graph series. CSV still flushes every point; settling, NPLC,
limits and verified output shutdown are unchanged. See
`docs/development/rust-optimization.md` in the included source for the measured comparison.

The blue and orange I-V sweep icon appears in the Windows executables, Mac app
bundle and application window. Its editable source and platform formats are
included under `assets/`. To rebuild them, install Pillow and run
`python3 scripts/build_iv_icons.py` from the source root. Windows builds also
require MinGW `windres` or Windows SDK `rc.exe`; these are build tools only.

## Run

Extract the whole ZIP. On Windows run `Install-Application.cmd`, approve the
administrator prompt, and use the **KohdaLab IV** desktop shortcut. You can also
open `kohdalab-iv.exe` directly for portable use. On Apple Silicon open
`KohdaLab IV.app`. No Python, Rust, or vendor VISA installation is
required for supported native connections. Keep the support files with the app.
The Mac app is ad hoc signed and is not notarized.

1. The original IV layout is reproduced: Config / Source / Meter on the left,
   Measurement controls and graph in the center, and log / Field-Value table on
   the right. The tall `<` / `>` buttons collapse either side column.
2. Choose a Device in each instrument group. For a dry run, select
   SIMULATED_SOURCE and SIMULATED_METER. Legacy models are enabled in this development edition; physical validation is pending.
3. For real devices, click Refresh, choose or type each Resource, then use the
   independent Connect buttons or All Connect. Identity checks reject an
   incompatible instrument. Only one GPIB adapter may be attached; both roles
   share its session. Disconnecting the source verifies output OFF first.
4. Review Mode, Sweep and Start / End / Step in Measurement. Each quantity has
   its own unit. Timing / Compliance opens a dialog with OK / Cancel;
   extra timing settings and Readings per point (Repeat) are under Advanced.
   Repeat holds each source target, averages N meter readings and saves one
   CSV row; it does not repeat the sweep. NPLC stays under Meter.
   Source range and ramp step are automatic. Ramp step is the larger of the
   sweep step and 1% of the nominal source range, capped to the selected range.
   The requested Step magnitude rounds
   up to a whole source-resolution increment (1 uA for GS210 sweeps reaching
   100 mA); the effective step and ramp are shown below Start. Start / End must
   match that resolution. Invalid settings show a reason below Start. Limits
   are never increased automatically. Press Start to see an English dialog explaining
   any problem and how to fix it. Every real sweep verifies source output OFF
   then resets the source level to zero before configuration; failure prevents Start.
   The run JSON retains both requested and
   effective settings; CSV targets use the effective sweep.
5. Choose Directory with Browse, enter File, and choose Auto suffix. Run has
   Start / Stop / Save. Measurements are automatically saved as they are acquired.
   After completion or Stop cleanup, Save opens a file picker for a complete CSV
   copy and companion .run.json. It never exports a decimated graph or overwrites
   existing files. There is no separate Output Off button. Stop and shutdown
   retain the source cleanup and output-off verification. The output_off IPC
   command remains available to CLI and Python/Notebook clients. If cleanup
   reports Failed or output Unknown, check the instrument front panel and use
   Disconnect to retry cleanup before reconnecting; do not treat an unverified
   output state as safe.
6. The status strip shows progress, resistance and conductance. The right log
   remains visible during discovery and measurement. Field / Value toggles the
   exact 14 saved CSV fields; hover truncated entries for their full values.
7. Config Browse selects a JSON file. Load / Save use the displayed path. The
   Rust desktop config format is native Rust format.

Direct USB requires GS210 TMC mode (0B21:0039) or 34411A (0957:0A07).
Native USB/GPIB connections require the WinUSB assignments used by the existing portable; the
bundled `support/USB-Setup.exe` can install them. Setup is currently an explicit
support action in this release, rather than an automatic startup step. Administrator
approval may be required. The application never changes unrelated devices.
NI GPIB-USB-HS+ is supported. Agilent 82357B is supported only when already
initialized (0957:0718) through a separately authorized workflow. Boot ID
0957:0518 is refused without opening it or replacing its boot driver. This
release does not download, cache or upload firmware. Do not add a firmware file
to the portable or source build kit. See REDISTRIBUTION.md for licensing terms.

Simulation is a 1 kOhm circuit and never opens physical devices. CSV contains
every saved measurement point; the live graph can coalesce updates or reduce
its display cache during large runs. Existing result files are not overwritten.
Settings use `~/.kohdalab/config/iv-next.json`.

If communication fails, emergency cleanup attempts a fresh source helper and
verifies the source identity before retrying output off. If output cannot be
verified, the application reports Unknown: check the source front panel.
Hardware compliance and physical output controls remain essential.

## Windows installation and updates

1. Extract the complete Windows ZIP to a normal local folder, such as Downloads.
2. Close every running KohdaLab IV application. Double-click
   `Install-Application.cmd` and approve the Windows administrator prompt.
3. Start **KohdaLab IV** from its desktop shortcut. The installer does not start
   the application with administrator privileges or access any instrument.

The application is installed under `C:\Program Files\Kohdalab\KohdaLab IV`
(or the Windows Program Files location). The desktop contains a shortcut.
The shortcut uses `%USERPROFILE%\.kohdalab` as its working folder so relative
output paths are writable. User files remain separate:

| Content | Location |
| --- | --- |
| Settings | `%USERPROFILE%\.kohdalab\config\iv-next.json` |
| Default measurements | `%USERPROFILE%\.kohdalab\results` |
| Driver setup/cache | `%LOCALAPPDATA%\KohdaLab IV` |
| Previous installation backup | `KohdaLab IV\archive` inside your Windows Documents folder |

Existing absolute measurement paths remain unchanged. Installing the app does
not install/remove instrument drivers, edit the saved configuration or migrate
measurement files. Updates check the package files against the included `FILES.json`, stage the new files,
and preserve a verified backup of an existing installation before replacement.
Existing installation access controls are retained. If setup fails, keep the
extracted folder and the displayed backup path, and review the English error.
It does not change the PowerShell execution policy. Use the official GitHub
download links in the root README. The included hashes check internal file
consistency; they are not a publisher signature.

To update, download and extract the new ZIP, close IV and run its
`Install-Application.cmd` again. After success, the extracted folder can be
deleted; the installed copy owns all required support files. Direct portable
use remains available without this app installation step. Driver setup may
still require administrator approval separately.

## Developer build

Install rustup and the pinned Rust toolchain, then run from the source root:

```sh
cargo test --manifest-path Cargo.toml --workspace --locked
cargo clippy --manifest-path Cargo.toml --workspace --all-targets --locked -- -D warnings
cargo build --manifest-path Cargo.toml --release --locked
```

The native helpers can be rebuilt with `scripts/build_native_gpib.sh` and
`scripts/build_usbtmc_probe.sh` on Mac. Windows helper rebuilds use the existing
MSYS2 scripts. Set `KOHDALAB_IV_HELPER_DIR` to a directory containing both helpers,
or use the bundled support folder. New instrument PCs do not need these tools.

Windows cross builds use the `x86_64-pc-windows-gnu` target and MinGW, with a
static compiler runtime. The packager expects Windows binaries under
`target-windows/x86_64-pc-windows-gnu/release`. Mac binaries use
`target/release`. Package from the source root:

```sh
python3 scripts/package_rust_preview.py --platform macos-arm64
python3 scripts/package_rust_preview.py --platform windows-x64
python3 scripts/package_rust_build_kit.py
```

The source project also contains `scripts/build-macos.command` and
`scripts/build-windows.ps1`. The Windows script needs the Rust MSVC development
toolchain, Visual C++ Build Tools, and Python 3.11 or newer on the build PC.
It packages the native Cargo output with the hash checked existing helpers.
These development tools are not needed on instrument PCs. The build scripts
and packager are included in SOURCE.zip.
For a Windows build from SOURCE.zip alone, place the original bundled support
binaries in `packaging/windows/native` before packaging, or rebuild the native
helpers and setup tool using the existing C/C++ build instructions. A source
archive deliberately contains source rather than a second copy of the EXEs.

For command output on Windows, use `iv-cli.exe`. Developer checks include
`--list`, `--identify --source RESOURCE --meter RESOURCE`, and
`--measure --simulate --output NEW_CSV_PATH`. Hardware CLI smoke tests require
`--confirm-open-circuit` and are limited to a three point +/-1 mV sweep with
1 mA hardware compliance. `--gui-smoke --screenshot PNG_PATH` checks the real
window and the same engine, CSV, and simulator path. It does not test hardware.

`python3 scripts/verify_rust_preview.py BINARY --output-dir NEW_DIRECTORY`
checks the complete engine process, the existing CSV columns, Stop, and loss
of the GUI heartbeat using simulation. Hardware GUI smoke uses
`--gui-hardware-smoke --confirm-open-circuit --source RESOURCE --meter RESOURCE`
and the same three point limits as the hardware CLI smoke.

## Source and licenses

Rust application code is MIT licensed and forbids unsafe code. Dependencies
and native OS boundaries have their own memory safety and license properties.
The native helpers contain GPL components and LGPL libraries. Retain the
support licenses, dependency notices, `REDISTRIBUTION.md` and `SOURCE.zip` when
redistributing. The combined GPIB executable is GPLv2, not MIT or GPLv3. Complete
source and LGPL replacement/relinking instructions are in
`SOURCE.zip/native/REBUILDING.md`. Windows uses the OS inbox WinUSB rather than
a bundled Microsoft driver binary.
SOURCE.zip includes the Rust, helper, shared specification and build sources.

Official downloads are available at
https://github.com/Kohdalab/kohdalab-iv/releases/tag/portable-v1.2.0 and the
personal mirror at https://github.com/woolen-yarn/kohdalab-iv/releases/tag/portable-v1.2.0.
Earlier releases remain archived on GitHub. Historical preview validation is
recorded in `docs/guides/portable-guide.md`; current behavior and limitations are described here.

Per-model display policies, primary sources and audit limitations are documented
in `docs/reference/instrument-precision.md` and the shared source/meter `specs.toml` files.

Native CSV columns: `timestamp`, `elapsed_s`, `point_index`, `repeat_index`,
`sweep_index`, `direction`, `target_value`, `target_unit`, `voltage_V`,
`current_A`, `voltage_origin`, `current_origin`, `software_limit_hit`, `status`. Target units are V in IV mode
and A in VI mode. Voltage/current use source level readback (or the setpoint
when unavailable) and the meter reading according to mode. Source level
readback is not an independent voltage/current measurement.

The run JSON also records range/resolution changes with their point indices,
so autorange history is retained without duplicating metadata in every CSV row.

Run settings are saved before output is enabled. Final metadata is written to
a separate temporary file and renamed over the initial JSON after output
cleanup. If final metadata writing fails, initial settings remain available
and the run reports a warning. `in_progress: true` identifies an unfinished
metadata record; it does not establish the current physical output state.

## Live channels

Config stays at the top of the left panel. Source and Meter each embed compact
I/V values, zero-centered level bars and bounded filled time-history trends.
Each instrument displays its own channel according to the actual source function.
Live readings start after both instruments connect,
including before a sweep. The monitor uses the actual GS210 function and the
opposite 34411A DC measurement function, configures the meter at 1 NPLC, and
never enables the source output. During a sweep, settling waits also acquire live samples and each averaging
conversion updates the monitor. Live reads never reconfigure the sweep NPLC or
create CSV points. They can lengthen a wait by transport/conversion overhead;
no competing background transfers run. Monitor readings are not
written to measurement CSVs. Historical values are held after disconnect.
GS210 readings are programmed source level, not an independent /MON measurement.
A live-transfer failure stops the monitor and requests verified output shutdown.

This repository contains a Rust application and isolated native helper sources.
Legacy Python application code, notebook API, VISA bindings, PyInstaller builds and
Python CI have been removed. Optional packaging/validation/icon utilities use
Python's standard library (Pillow only for regenerating icons) on a build host;
they are not included as an application runtime or compatibility layer.

## Python and Notebook control

Version 1.3.0-alpha.6 includes automatic discovery of the Windows installation
and `KohdaLab IV.app` in `~/Applications` or `/Applications`. Install the bundled
client with `python -m pip install /path/to/python-control`, then use `NativeIV()`
without a path. For a portable package elsewhere, pass its directory or the Mac
app bundle. See `python-control/README.md` and
`python-control/examples/migrate-to-native.ipynb` in the distribution.


Use `python-control/kohdalab_iv_native.py` from the portable package, or
[the source client](../../bindings/python/README.md). It uses only the Python standard
library and invokes `iv-cli --engine`. The engine owns devices and safety cleanup;
its defaults are available through `iv-cli --default-config`.

Live acquisition targets a 50 ms start-to-start interval (up to 20 samples/s),
with a 33 ms display refresh interval. Actual acquisition rate depends on the
instrument, NPLC and USB/GPIB latency; no synthetic measured samples are added.
All floating CSV columns (`elapsed_s`, `target_value`, `voltage_V`, `current_A`)
use shortest round-trip scientific notation, including zero (`0e0`).

The central I-V plot ignores ordinary wheel scrolling. Drag, boxed zoom,
modifier-wheel zoom and double-click reset remain available. Direction colors
share measured turning points, so forward/backward lines meet at the endpoints.
Excel may reformat numeric CSV cells on import; choose Scientific number format
in Excel to display all electrical values as exponents. CSV text remains scientific.

## Live mode and palette

Mode changes switch GS210 voltage/current sourcing and the opposite 34411A
measurement function. Output OFF and zero are verified before the switch; zero
is verified again for the new function. Live history clears on a GUI Mode change.
Voltage is amber, current is teal, and forward/backward curves are blue/purple
with a legend. Mode changes never enable output.

### Sweep speed

Start with Repeat = 1. Reduce Wait time only as far as your circuit can settle;
First wait and Ramp wait are independent and still apply. Lower NPLC gives
faster conversions at the cost of resolution and mains-noise rejection (see
[34410A/34411A User's Guide](https://www.keysight.com/us/en/assets/9018-05586/user-manuals/9018-05586.pdf), Integration Time and Resolution).
At 50 Hz, 1 PLC is 20 ms and 0.2 PLC is 4 ms of integration, before autozero,
autoranging and communication overhead. The default 0.2 s per-point wait alone
limits the sweep to fewer than 5 points/s. Defaults are not reduced silently.
Per-point repeats share one source-setting operation and one settling wait.
Each individual conversion is checked against the software limit, so averaging
cannot hide an over-limit reading; partial accumulation is recorded on cutoff.
Legacy `average_count` is accepted when `repeat` is 1; otherwise `repeat` wins.
`repeat_index` remains 1 in the CSV; requested/effective counts are in run JSON.

Planning benchmark note: `repeat` now controls per-point accumulation. The
`benchmark_plan` example therefore builds 10,001 source targets with 99 readings
per target (990,099 meter conversions), not 990,099 source targets. Historical
planning timings in the optimization report used the earlier sweep-repeat
meaning and are not directly comparable to this corrected benchmark.

## Windows Setup.exe distribution

The installer edition and portable ZIP contain the same Rust application.
Run `kohdalab-iv-<version>-windows-x64-Setup.exe`, approve Windows administrator
permission, and follow the English setup wizard. It installs into
`C:\Program Files\Kohdalab\KohdaLab IV`, creates all-user desktop/Start-menu
shortcuts, and registers Windows Settings > Apps > Installed apps.
Run a newer Setup.exe to update. Close the GUI and any Python/Notebook engine
before updating or removing the app. Setup never changes instrument drivers.

Uninstall through Windows Installed apps or `Uninstall.exe`. Settings, results,
USB driver assignments and driver cache remain intact. A verified backup of the
application tree, including any user-added files, is retained under
`Documents\KohdaLab IV\archive`. Failed activation/registration rolls back.
The GUI selects the launching user's `.kohdalab` directory even from a shared
shortcut; it does not write measurement results under Program Files or Windows.

The portable ZIP remains available for direct use without app installation.
Do not use its legacy Install-Application.cmd to update a Setup-installed app;
use Setup.exe for that update, or run the portable executable separately.

Build an installer after producing the Windows portable directory:

```sh
python scripts/build_windows_installer.py --payload-dir dist/<windows-directory> --version <version> --output dist/KohdaLab-IV-Setup.exe
```

The pinned NSIS compiler is a build tool only. Its zlib notices and the complete
installer scripts are included in the build kit. Downloaded tool archives are
checked against the source-pinned SHA-256 before use. Python is only required
on this build machine, not on an end user's Windows PC.

See `docs/development/windows-signing.md` in the included source for publisher signing and execution warnings. No security controls are disabled by the app or installer.


## Code signing policy

This build is unsigned on Windows; SignPath acceptance and production signing
are pending. Public policies and the unsigned CI build procedure are included
in `support/SOURCE.zip`: `CODE_SIGNING_POLICY.md`, `PRIVACY.md` and
`.signpath/README.md`. No SignPath credentials or certificates are bundled.
See the source README for development and stable download links.


Source/Meter Device sections fold model, resource and connection controls together;
Meter NPLC is inside Device. Live values and connection status remain visible.
Output and Run have separate rounded outlines.

---

# KohdaLab IV ネイティブデスクトップ

開発版 **1.3.0-alpha.7** は旧 Python ドライバーから Yokogawa 7651、Agilent 34401A、Keysight 34465A、ADCMT 7461A を移植します。プロトコルモックは成功していますが実機検証は保留中です。GS200 とネイティブ ADCMT USB は追加していません。公開版 **1.2.0** は Windows Setup.exe と Windows での機器別公式 VISA 優先を追加します。

Windows の検出は最初にインストール済み 64-bit システム VISA ランタイムへ USB0/GPIB0 機器を問い合わせ、アプリでは `VISA::<resource>` と表示します。`VISA::` はアプリの経路接頭辞でベンダー VISA のリソース文字列の一部ではありません。同一デバイスを VISA が公開する古い保存リソースも自動で VISA を選びます。他の機器は既存のネイティブ WinUSB 経路を使います。ランタイムのダウンロード・同梱、Refresh 時のドライバー置換は行いません。選択済み VISA 接続はロック／タイムアウト／エラー後にネイティブ I/O へ黙ってフォールバックしません。使用可能な 64-bit VISA のないメーカー製ドライバーは保持し、ベンダーランタイムを設定する必要があります。更新済み USB セットアップは第三者ドライバーを含む非 WinUSB 割当を保持します。USB0 と GPIB0 一次アドレスを扱えますが、GPIB0 は同時に一つのバックエンドだけです。VISA が所有すればネイティブ制御器プローブを省略し、保存 GPIB0 アドレスも VISA を使います。複数 GPIB 制御器は未対応で、公式 VISA とネイティブ直接 USB は混在できます。

v1.1.0 は既存 3 列 UI を控えめな charcoal カード、区別された入力欄・操作ボタン、整列したスイープ操作、読みやすいパス／リソース概要、明示的な接続／出力状態で更新しました。パスとリソースは概要のクリックで全値編集、ホバーで全値表示ができます。Windows Program Files インストールと安全な更新を維持します。Run は Start / Stop / Save に集中し、内部の output-off 制御と自動クリーンアップを使います。完全データ Save、自動ソース設定、点ごとの Repeat 平均を含みます。未検証 82357B ファームウェア取得／アップロードは除外し、再配布条件、ネイティブライセンス証跡、再リンク手順を明示します。青／紫の順逆方向スタイルと CSV 対応インスペクターを維持します。スイープターゲットは負でもよく、実行前に全制限を検証します。

Rust デスクトップは KohdaLab IV アイコン、KohdaLab Camera Native と同じ egui/eframe GUI 基盤、分離 Rust 測定エンジン、既存 USB/GPIB ヘルパープロセスを使用します。公開 v1.2.0 は GS210 と Agilent/Keysight 34411A を、開発版は上記追加モデルも扱います。IV/VI、linear/round trip/zero centered/custom スイープ、制限、待機、平均、NPLC、ストリーム CSV、run メタデータ、明示的シミュレーターを含みますが、ハードウェア精度と全ファームウェア変種を認定するものではありません。CSV 書式バッファー、スイープ計画、表示状態、キャッシュ済みグラフ系列を最適化していますが、点ごとの CSV フラッシュ、待機、NPLC、制限、検証済み出力停止は不変です。アイコン資産は `assets/` にあり、`python3 scripts/build_iv_icons.py` で再生成します。Windows のビルドには MinGW `windres` または Windows SDK `rc.exe` が必要ですが、いずれもビルドツールだけです。

## 実行

ZIP 全体を展開します。Windows は `Install-Application.cmd` を実行し管理者確認を承認して **KohdaLab IV** デスクトップショートカットを使うか、ポータブルの `kohdalab-iv.exe` を直接開きます。Apple Silicon は `KohdaLab IV.app` を開きます。対応ネイティブ接続には Python、Rust、ベンダー VISA インストールは不要ですが、support ファイルはアプリと一緒に保ちます。Mac アプリは ad hoc 署名で公証されていません。

1. 左は Config / Source / Meter、中央は Measurement 操作とグラフ、右はログ／Field-Value 表です。背の高い `<` / `>` で左右列を折り畳みます。
2. 各計測器グループで Device を選びます。試運転には `SIMULATED_SOURCE` と `SIMULATED_METER` を選びます。レガシーモデルは開発版で有効ですが実機検証は保留中です。
3. 実機では Refresh を押し各 Resource を選択または入力して、独立した Connect または All Connect を使います。識別確認は不適合機器を拒否します。GPIB アダプターは一台だけで両役割がセッションを共有します。source 切断は先に output OFF を検証します。
4. Measurement で Mode、Sweep、Start / End / Step を確認し、量ごとの単位を選びます。Timing / Compliance は OK / Cancel のダイアログで開き、追加時間設定と Readings per point (Repeat) は Advanced にあります。Repeat は各 source target を保持して N 回の meter 読みを平均し、CSV 一行を保存します。スイープ全体を反復するものではありません。NPLC は Meter にあります。source range/ramp step は自動で、ramp step はスイープ step と公称 source range の 1% の大きい方を選択 range で上限にします。Step は source 分解能の整数倍へ切り上げ（100 mA に達する GS210 スイープでは 1 uA）られ、有効 step/ramp を Start 下に表示します。Start/End も分解能に一致する必要があります。無効設定は理由を Start 下に表示し、制限を自動で増やしません。Start は問題と修正法を英語ダイアログで示します。実スイープは設定前に output OFF を検証して source level をゼロへ戻し、失敗時は開始しません。run JSON は要求値と有効値を保持し、CSV target は有効スイープを使います。
5. Directory は Browse で選び、File と Auto suffix を指定します。Run は Start / Stop / Save です。測定は取得時に自動保存され、完了または Stop クリーンアップ後の Save は完全 CSV と `.run.json` のコピーを選ばせます。間引いたグラフは出力せず、既存ファイルを上書きしません。別の Output Off ボタンはありません。Stop と終了は source クリーンアップと output-off 検証を維持し、`output_off` IPC は CLI/Python/Notebook から利用できます。cleanup が Failed または output Unknown のときは前面パネルを確認し、再接続前に Disconnect で cleanup を再試行します。未検証出力状態を安全と扱いません。
6. status strip は進捗、抵抗、コンダクタンスを示します。右ログは検出・測定中も表示され、Field / Value は保存済み CSV 14 フィールドを切替え、切詰めた値はホバーで確認します。
7. Config Browse は JSON を選び、Load / Save は表示パスを使います。Rust デスクトップ設定形式はネイティブ Rust 形式です。

直接 USB は GS210 TMC mode (0B21:0039) または 34411A (0957:0A07) が必要です。ネイティブ USB/GPIB 接続には既存ポータブルの WinUSB 割当が必要で、同梱 `support/USB-Setup.exe` がインストールできます。これは自動起動ではなく明示的な支援操作で、管理者承認が必要な場合があります。アプリは無関係なデバイスを変更しません。NI GPIB-USB-HS+ をサポートします。Agilent 82357B は別途許可された手順で初期化済みの 0957:0718 に限り、boot ID 0957:0518 は開く／boot driver 置換なしで拒否します。本リリースは firmware をダウンロード・キャッシュ・アップロードせず、ポータブルやソースキットに firmware を追加してはいけません。

シミュレーションは 1 kOhm 回路で実デバイスを開きません。CSV は全保存測定点を含み、ライブグラフは大規模実行中に更新をまとめるか表示キャッシュを縮小します。既存結果ファイルは上書きせず、設定は `~/.kohdalab/config/iv-next.json` を使います。通信失敗時、緊急 cleanup は新規 source helper を試し、output off 再試行前に source identity を確認します。出力を検証できなければ Unknown を報告するので、source 前面パネルを確認してください。hardware compliance と物理出力制御は不可欠です。

## Windows のインストールと更新

完全な Windows ZIP を Downloads など通常のローカルフォルダーに展開し、稼働中の KohdaLab IV を全て閉じ、`Install-Application.cmd` をダブルクリックして管理者確認を承認します。デスクトップショートカットから **KohdaLab IV** を開始します。インストーラーは管理者としてアプリを起動せず、計測器にもアクセスしません。アプリは `C:\Program Files\Kohdalab\KohdaLab IV`（または Windows Program Files の場所）にインストールされ、デスクトップにショートカットを置きます。ショートカットの作業フォルダーは `%USERPROFILE%\.kohdalab` で、相対出力先が書込み可能です。

| 内容 | 場所 |
| --- | --- |
| Settings | `%USERPROFILE%\.kohdalab\config\iv-next.json` |
| Default measurements | `%USERPROFILE%\.kohdalab\results` |
| Driver setup/cache | `%LOCALAPPDATA%\KohdaLab IV` |
| Previous installation backup | Windows Documents 内 `KohdaLab IV\archive` |

既存の絶対測定パスは変わりません。インストールは計測器ドライバー、保存設定、測定ファイルを追加・削除・移行しません。更新は `FILES.json` でパッケージファイルを検査し、新ファイルを stage して既存インストールを置換する前に検証済みバックアップを保持します。既存アクセス制御は維持します。失敗時は展開フォルダーと表示されたバックアップパスを保持し、英語エラーを確認します。PowerShell 実行ポリシーは変更しません。含まれるハッシュは内部ファイル一貫性の確認であり publisher signature ではありません。更新には新 ZIP を展開し IV を閉じて `Install-Application.cmd` を再実行します。成功後は展開フォルダーを削除でき、インストール済みコピーが support ファイルを所有します。ポータブル直接利用も可能で、ドライバー設定は別途管理者承認を要し得ます。

## 開発ビルド

rustup と固定 Rust toolchain を導入し、source root から実行します。

```sh
cargo test --manifest-path Cargo.toml --workspace --locked
cargo clippy --manifest-path Cargo.toml --workspace --all-targets --locked -- -D warnings
cargo build --manifest-path Cargo.toml --release --locked
```

Mac では native helpers を `scripts/build_native_gpib.sh` と `scripts/build_usbtmc_probe.sh` で再ビルドします。Windows helper の再ビルドには既存 MSYS2 scripts を使います。`KOHDALAB_IV_HELPER_DIR` に両 helper を含む directory を設定するか、bundled support folder を使います。新しい instrument PC にこれらの build tools は不要です。Windows cross build は static compiler runtime 付き `x86_64-pc-windows-gnu` target を使います。packager は Windows binaries を `target-windows/x86_64-pc-windows-gnu/release`、Mac binaries を `target/release` に期待します。source root から package を作成します。

```sh
python3 scripts/package_rust_preview.py --platform macos-arm64
python3 scripts/package_rust_preview.py --platform windows-x64
python3 scripts/package_rust_build_kit.py
```

`scripts/build-macos.command` と `scripts/build-windows.ps1` も source project にあります。Windows script は build PC 上で Rust MSVC development toolchain、Visual C++ Build Tools、Python 3.11 以降を必要とし、hash-checked existing helpers と native Cargo output を package します。これらの development tools は instrument PCs に不要で、build scripts/packager は SOURCE.zip に入っています。SOURCE.zip だけから Windows build を行う場合は packaging 前に元の bundled support binaries を `packaging/windows/native` に置くか、既存 C/C++ build instructions で native helpers/setup tool を再ビルドします。source archive は EXE の第二コピーでなく source を意図的に含みます。

Windows の command output には `iv-cli.exe` を使います。developer checks は `--list`、`--identify --source RESOURCE --meter RESOURCE`、`--measure --simulate --output NEW_CSV_PATH` を含みます。hardware CLI smoke test は `--confirm-open-circuit` が必要で、three point の +/-1 mV sweep と 1 mA hardware compliance に限定します。`--gui-smoke --screenshot PNG_PATH` は実 window、同じ engine、CSV、simulator path を確認し、hardware は test しません。`python3 scripts/verify_rust_preview.py BINARY --output-dir NEW_DIRECTORY` は simulation で complete engine process、existing CSV columns、Stop、GUI heartbeat loss を確認します。hardware GUI smoke は `--gui-hardware-smoke --confirm-open-circuit --source RESOURCE --meter RESOURCE` を使い、hardware CLI smoke と同じ three point limits を使います。

## ソースとライセンス

Rust application code は MIT license で unsafe code を禁止します。dependencies と native OS boundaries には独自の memory-safety/license properties があります。native helpers は GPL components と LGPL libraries を含むため、再配布時には support licenses、dependency notices、`REDISTRIBUTION.md`、`SOURCE.zip` を保持してください。combined GPIB executable は GPLv2 であり、MIT/GPLv3 ではありません。complete source と LGPL replacement/relinking instructions は `SOURCE.zip/native/REBUILDING.md` にあります。Windows は bundled Microsoft driver binary ではなく OS inbox WinUSB を使います。SOURCE.zip には Rust/helper/shared specification/build sources が入ります。

official downloads は https://github.com/Kohdalab/kohdalab-iv/releases/tag/portable-v1.2.0、および personal mirror の https://github.com/woolen-yarn/kohdalab-iv/releases/tag/portable-v1.2.0 にあります。earlier releases は GitHub に archive されます。historical preview validation は `docs/guides/portable-guide.md`、current behavior/limitations はこのガイドに記録します。model ごとの display policies、primary sources、audit limitations は `docs/reference/instrument-precision.md` と shared source/meter `specs.toml` files にあります。

native CSV columns は `timestamp`、`elapsed_s`、`point_index`、`repeat_index`、`sweep_index`、`direction`、`target_value`、`target_unit`、`voltage_V`、`current_A`、`voltage_origin`、`current_origin`、`software_limit_hit`、`status` です。target units は IV mode で V、VI mode で A です。voltage/current は mode に従って source level readback（利用不能なら setpoint）と meter reading を用います。source level readback は independent voltage/current measurement ではありません。run JSON は point indices とともに range/resolution changes を記録し、各 CSV row に metadata を重複せず autorange history を残します。run settings は output enable 前に保存します。final metadata は output cleanup 後に別 temporary file へ書き初期 JSON を rename 置換します。final metadata writing が失敗しても initial settings は使え、run は warning を報告します。`in_progress: true` は unfinished metadata record を示し、current physical output state を示しません。

## ライブチャンネル

Config は left panel 上部に残ります。Source/Meter は実際の source function に従う各 channel の compact I/V values、zero-centered level bars、bounded filled time-history trends を埋め込みます。両 instruments 接続後、sweep 前から live readings を開始します。monitor は actual GS210 function と反対の 34411A DC measurement function を用い、meter を 1 NPLC に設定し source output を決して enable しません。sweep 中の settling waits も各 averaging conversion も live samples を取得して monitor を更新します。live reads は sweep NPLC を再設定せず CSV points を作りません。transport/conversion overhead により wait は長くなり得ますが、competing background transfers は動作しません。monitor readings は measurement CSVs に書かず、historical values は disconnect 後も保持します。GS210 readings は programmed source level で independent `/MON` measurement ではありません。live-transfer failure は monitor を止め verified output shutdown を要求します。

この repository は Rust application と isolated native helper sources を含みます。legacy Python application code、notebook API、VISA bindings、PyInstaller builds、Python CI は削除済みです。optional packaging/validation/icon utilities は build host 上で Python standard library（icon regeneration のみ Pillow）を使いますが、application runtime/compatibility layer には含みません。

## Python と Notebook 制御

v1.3.0-alpha.6 は Windows installation と `~/Applications` または `/Applications` の `KohdaLab IV.app` を自動発見します。同梱 client は `python -m pip install /path/to/python-control` で install し、path なしの `NativeIV()` を使えます。別の場所の portable package にはその directory または Mac app bundle を渡します。distribution 内の `python-control/README.md` と `python-control/examples/migrate-to-native.ipynb` を参照してください。

portable package の `python-control/kohdalab_iv_native.py` または[ソース client](../../bindings/python/README.md)を使います。これは Python standard library だけを使い `iv-cli --engine` を起動します。engine が devices/safety cleanup を所有し、その defaults は `iv-cli --default-config` で得られます。live acquisition は 50 ms start-to-start interval（最大 20 samples/s）、33 ms display refresh interval を目標とします。actual acquisition rate は instrument、NPLC、USB/GPIB latency に依存し、synthetic measured samples は追加しません。floating CSV columns（`elapsed_s`、`target_value`、`voltage_V`、`current_A`）は zero を含む shortest round-trip scientific notation を使います。

central I-V plot は通常 wheel scrolling を無視します。drag、boxed zoom、modifier-wheel zoom、double-click reset を使えます。direction colors は measured turning points を共有し、forward/backward lines は endpoints で接続します。Excel は import 時に numeric CSV cells を再書式化するため、すべての electrical values を exponent 表示するには Excel で Scientific number format を選びます。CSV text は scientific のままです。

## ライブモードとパレット

Mode changes は GS210 voltage/current sourcing と反対の 34411A measurement function を切替えます。切替前に output OFF/zero を検証し、新 function でも zero を再検証します。GUI Mode change で live history は clear されます。voltage は amber、current は teal、forward/backward curves は legend 付き blue/purple です。Mode changes が output を enable することはありません。

### スイープ速度

Repeat = 1 から開始します。Wait time は circuit が settle できる範囲までだけ下げます。First wait/Ramp wait は独立して適用されます。NPLC を下げれば conversion は速くなりますが resolution/mains-noise rejection を失います（[34410A/34411A User's Guide](https://www.keysight.com/us/en/assets/9018-05586/user-manuals/9018-05586.pdf) の Integration Time and Resolution を参照）。50 Hz では 1 PLC は 20 ms、0.2 PLC は autozero/autorange/communication overhead 前に 4 ms integration です。default 0.2 s per-point wait だけで sweep は 5 points/s 未満となり、defaults を黙って下げません。per-point repeats は一 source-setting operation と一 settling wait を共有します。individual conversion を software limit に確認するため averaging は over-limit reading を隠せず、cutoff 時の partial accumulation を記録します。legacy `average_count` は `repeat` が 1 のとき受け入れ、それ以外は `repeat` が優先します。CSV の `repeat_index` は 1 のままで requested/effective counts は run JSON にあります。

planning benchmark では `repeat` が per-point accumulation を制御します。従って `benchmark_plan` example は 10,001 source targets と各 target 99 readings（990,099 meter conversions）を構築し、990,099 source targets ではありません。optimization report の historical planning timings は earlier sweep-repeat meaning を使ったため、この修正済み benchmark と直接比較できません。

## Windows Setup.exe 配布

installer edition と portable ZIP は同じ Rust application を含みます。`kohdalab-iv-<version>-windows-x64-Setup.exe` を実行して Windows administrator permission を承認し、English setup wizard に従います。`C:\Program Files\Kohdalab\KohdaLab IV` に install し、all-user desktop/Start-menu shortcuts を作成、Windows Settings > Apps > Installed apps へ登録します。update/removal 前に GUI と Python/Notebook engine を閉じます。Setup は instrument drivers を変更しません。

Windows Installed apps または `Uninstall.exe` から uninstall しても settings、results、USB driver assignments、driver cache は残ります。user-added files を含む application tree の verified backup は `Documents\KohdaLab IV\archive` に残ります。failed activation/registration は rollback します。GUI は shared shortcut でも launching user の `.kohdalab` directory を選び、Program Files/Windows に measurement results を書きません。portable ZIP は app installation なしに直接使用できます。Setup-installed app の update に legacy `Install-Application.cmd` を使わず Setup.exe を使うか、portable executable を別に実行します。

Windows portable directory を作成後、installer は次で build します。

```sh
python scripts/build_windows_installer.py --payload-dir dist/<windows-directory> --version <version> --output dist/KohdaLab-IV-Setup.exe
```

pinned NSIS compiler は build tool だけです。zlib notices と complete installer scripts は build kit に含まれます。downloaded tool archives は使用前に source-pinned SHA-256 と照合します。Python はこの build machine だけに必要で、end user の Windows PC には不要です。publisher signing と execution warnings は included source の `docs/development/windows-signing.md` を参照します。app/installer は security controls を無効化しません。

## コード署名ポリシー

この build は Windows で unsigned です。SignPath acceptance と production signing は保留中です。public policies と unsigned CI build procedure は `support/SOURCE.zip` の `CODE_SIGNING_POLICY.md`、`PRIVACY.md`、`.signpath/README.md` にあります。SignPath credentials/certificates は同梱しません。development/stable download links は source README を参照してください。

Source/Meter Device sections は model/resource/connection controls を一緒に折り畳み、Meter NPLC は Device 内にあります。live values と connection status は表示されたままです。Output と Run は別々の rounded outlines を持ちます。
