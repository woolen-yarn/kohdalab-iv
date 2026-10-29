# Changelog / 変更履歴

> **Language navigation / 言語案内:** The English historical record appears first, followed by a complete Japanese counterpart. Versions, dates, API names, commands, compatibility wording, and historical claims are preserved in both languages.
> 英語の履歴記録を先に置き、その後に完全な日本語対訳を掲載します。バージョン、日付、API 名、コマンド、互換性の表現、履歴上の主張は両言語で保持します。

## Unreleased

- Separate IV settings into `~/.kohdalab/iv/config.json`. Use only the application-owned default; remove automatic legacy/preview profile discovery. Standardize Windows install working data under `.kohdalab/iv` and default measurements under `iv/results`. Reject foreign application identities, unidentified JSON and newer schemas before loading or replacing settings; save atomically and report autosave failures.
- IV設定をアプリ専用フォルダーへ分離し、旧Rust版設定は元ファイルを残して初回に引き継ぎます。別アプリ・未識別JSON・未対応の新版設定は読み込みと上書きを拒否し、安全な保存とエラー表示を行います。

## 1.3.0-alpha.16 — 2026-10-29

- Keep the central Measurement controls in two columns. Place Source compliance in the Source card, Meter limit/range in the Meter card, and Wait time in Settings → Timing; retain compact Run controls.
- Show verified current-input shunt/terminal resistance for fixed ranges. Preserve independent Meter limit and Source compliance, including legacy profiles.
- Guard Auto range changes across documented resistance groups, stopping conservatively on unknown loading changes. Verify the baseline before sweep/DC output ON and preserve it across held DC edits; request OFF before further acquisition/evidence and exclude the transition conversion from analysis CSV. ADCMT range monitoring remains unavailable pending verified commands.
- 中央は2列を維持し、コンプライアンス・Meter limit／rangeを左の各カードへ、Wait timeをSettings内へ整理。Autoの抵抗変化を検出した場合は出力OFF・停止し、固定レンジでの再測定を案内します。
- Replace automatic manual-DC range changes with an explicit documented native-range selector and left/right range arrows. A fixed DC request validates level and ramp step on that selected source grid; changing an explicit range while output is ON is rejected and handled fail-safe.
- Keep range-derived V/mV/mA units and fixed decimal places in the Source DC display. Make Output and Zero equal-width controls, while retaining the compact Device → live → settings workflow for Source and Meter cards.
- Preserve CSV formats, meter transition protection, source protection and existing GUI/API compatibility when no explicit DC range is supplied. Preview validation records automated mock/simulation coverage; publication proceeds without a new all-device physical retest at the user’s direction.
- 手動DCの自動レンジ変更を、対応するネイティブレンジの明示選択と左右矢印へ置き換えました。固定DC要求は選択レンジのグリッドで値とランプを検証し、出力ON中の明示レンジ変更は拒否して安全側で処理します。
- Source DC表示は選択レンジ由来のV/mV/mA単位と固定小数桁を維持します。OutputとZeroは同じ幅とし、Source／MeterカードのDevice → live → settingsのコンパクトな流れを維持します。
- 明示レンジを指定しない既存GUI/APIでは互換性を維持し、CSV形式、メーター遷移保護、ソース保護も保持します。プレビューでは自動mock／simulationを検証し、利用者の指示により全装置の実機再試験を待たず公開します。新たな実機測定の記録を主張しません。

## 1.3.0-alpha.15 — 2026-10-29

- Add independent Auto/fixed meter voltage and current ranges. Verify fixed settings before source ON and monitor actual range through return sweeps, repeated/settling conversions, idle live acquisition, and Dual; request OFF on mismatch. ADCMT 7461A remains Auto-only pending command verification.
- Add manual DC source cards, per-digit adjustments within a verified source range, and sweep ownership handoff without overlapping the Meter live monitor.
- Preserve analysis/raw CSV columns, retain requested ranges in run configuration, and reject fixed requests to incompatible older engines/GUI clients.
- Include the authenticated shared GPIB service source as an explicit development option; default direct transport is unchanged. No new physical hardware qualification is claimed.
- Update both repositories' download links, corresponding source/build kit, Windows Setup/portable and macOS ARM64 portable packages.
- 電圧・電流ごとのAuto／固定レンジ、出力前の設定確認と測定中のレンジ監視を追加。ADCMT 7461Aは通信仕様確認までAutoのみです。
- 手動DCカードと桁ごとの調整を追加し、測定との制御競合を防止。CSV列は維持し、旧エンジンへの固定指定は拒否します。
- GPIB共有サービスは開発用ソースとして同梱し、通常接続は変更しません。今回の実機適合確認を意味しません。

## 1.3.0-alpha.14 — 2026-10-28

- Align GUI automation stop options, typed errors and terminal receipt semantics with TRKR, retaining IV's fail-safe defaults and verified normal Output OFF cleanup.
- Independently select disconnect, Python exception and wait-timeout stops. Keep an active detached job locked against GUI edits when continuation is explicitly selected.
- Add `stop_and_wait()` with output verification; report failed safety cleanup as an error rather than a stopped success. Preserve CSV and profile schemas.
- Update English/Japanese usage and stop-policy documentation. Automated tests and simulations do not constitute new physical hardware qualification.

## 1.3.0-alpha.13 — 2026-10-28

- Add Dual V-I sweeps for two voltage sources with exact integer ratios, independent source safety paths, and an explicit differential `V1 - V2` analysis value.
- Add a left-panel inventory for multiple saved Sources and Meters. Inactive devices remain visible but dimmed; active roles are assigned in Settings.
- Preserve the existing 18-column analysis CSV and raw CSV contracts. Store dual ratios and per-source setpoints/readbacks in run JSON without claiming unavailable terminal-voltage measurements.
- Align GUI API protocol 2 with TRKR-style ownership, UUID job receipts, asynchronous job results, configuration/connection operations and machine-readable error codes while preserving IV Stop/Output OFF behavior.
- Do not skip a due Dual live protection conversion when slow source-status queries consume a positive settle interval. Keep the zero-wait path unchanged and stop before saving a limited point.
- Associate finite Gate/connection job outcomes with that operation rather than a previous measurement's Stopped phase. Actual safety cancellation and measurement protection stops remain unsuccessful.
- This preview has automated and simulation coverage; no new physical hardware qualification is claimed.

## 1.3.0-alpha.12 — 2026-10-24

- Show latched, pulsing Source/Meter live-monitor warnings and limit-stop popups with frozen run thresholds, trigger/status evidence, verified OFF status and safe recovery guidance. Preserve alerts on OFF failures and clear them on the next run attempt, including invalid attempts.

- Separate Source hardware protection from the external Meter software stop in **Settings… / Measurement Settings**, including four-terminal wiring guidance and effective range/protection values.
- Detect GS210 H/L and EMR with the non-clearing condition register without /MON, including zero-wait ramp steps. Request OFF before persisting a trigger; exclude the limited target from analysis CSV/fitting while retaining raw replies and structured stop metadata.
- Verify programmed GS210 protection before enabling output. Account for fixed 200 mA protection in 10/100 mV ranges; an explicitly smaller current protection selects a compatible range. Preserve legacy range/resolution until protection fields are edited.
- Use settings schema 2 and run metadata schema 7; keep the 18-column analysis CSV. Update Python terminal-state and compatibility handling.

## 1.3.0-alpha.11 — 2026-10-23

- Fix zero-centered sweeps to follow 0 → End → Start → 0 within the entered bounds. Reject intervals excluding zero before enabling output; a zero endpoint is valid.
- Include exact segment endpoints when Step does not divide the span, using a shorter final interval without overshoot or duplicate junctions.
- Enforce software compliance on every acquired active-sweep conversion, including ramp/settling live readings. Send source OFF before further instrument queries or saving on a trip; protection cleanup never ramps with output enabled.
- Keep live protection samples out of normal CSV points and resistance fitting; retain trigger values and stop events in run metadata. Do not record the reset-to-zero level as a conversion-time source readback.
- Confirm source function, programmed range, zero setting and output ON across GS210 and the translated 7651 protocol. Unreadable source levels stop the run.
- Preview effective Step, automatic rounding, point count and actual route before Start, with cached planning and preserved compact layout.
- Apply shared protection to every supported meter model and simulation. Software checks act on acquired readings and do not replace instrument hardware limits or interlocks.

## 1.3.0-alpha.10 — 2026-10-23

新規保存をmain CSV・raw CSV・run JSONへ整理しました。mainは18列の`analysis_v3`で、測定ID・平均化採用数・標本標準偏差・品質フラグを保存します。rawは改行を保持できる共通CSVで、旧JSONL付きデータのコピーを維持します。終了時にmain/rawのSHA-256を記録し、設定・出力確認の通信証跡と任意の実験条件を保存します。

- Replace the provenance JSONL for new runs with a multiline-safe `raw.csv`, while retaining legacy JSONL when saving old runs.
- Write an 18-column `analysis_v3` CSV with run identity, accepted-reading count, sample standard deviation and explicit quality flags.
- Record SHA-256 hashes for the completed analysis and raw CSV files in `.run.json`.

## 1.3.0-alpha.9

- Timestamp each saved CSV row at host-observed completion of its final meter conversion, before later provenance, display, source-readback, or disk work.
- Record explicit host UTC/monotonic timing for audited queries, averaging windows, and flushed CSV writes; no device-clock timestamp or uncertainty is inferred.

## 1.3.0-alpha.8

- Preserve the existing 14-column CSV schema and shortest round-trip numeric values.
- Add a flushed `.readings.jsonl` provenance companion with raw per-repeat replies, available range/resolution, instrument identity/resource, and partial-run evidence.
- Keep display precision separate from recorded values; resolution and raw reply spelling do not claim measurement uncertainty.

## 1.3.0-alpha.7

- Centralize instrument capabilities, specifications and measurement definitions without changing supported models, controls or measurement behavior.
- Share ramp/cleanup sequencing and use typed execution, output and error states while retaining existing safety decisions and protocol strings.
- Version settings, run metadata and Notebook API compatibility; accept legacy configurations/clients and protect newer settings from accidental overwrite.
- Split desktop formatting, settings, history and widgets into focused modules, with cross-platform driver contracts and regression coverage.
- Document all supported instruments and adapters, include an actual GUI screenshot, and update portable, Setup and build-kit sources.
- Preserve the 14-column CSV format, numeric output, device command ordering and output-off behavior.

## 1.3.0-alpha.6

- Find the installed Rust engine automatically from Windows installer registration and native Program Files, including 32-bit Notebook kernels, or macOS user/system Applications folders.
- Accept a CLI path, installation directory or Mac app bundle as an explicit client override; retain deterministic override priority and clear missing-engine errors.
- Include an executable Notebook migration tutorial, updated client instructions and automatic discovery tests in portable packages, Setup and the build kit.
- Preserve measurement, CSV, driver and output protection behavior.

## 1.3.0-alpha.5

- Rename the desktop executable to `kohdalab-iv.exe` on Windows and `kohdalab-iv` on macOS; name the Mac bundle `KohdaLab IV.app`. No legacy GUI executable alias is shipped.
- Update shortcuts, metadata, packaging, CLI launch and signing inputs for the new name. Preserve existing configuration, device leases and Python `iv-cli` automation.
- Keep old/new process detection and shortcut migration for safe installation updates.

## 1.3.0-alpha.4

- Add separate rounded outlines around Output and Run while preserving the three-column layout.
- Collapse Source/Meter model, resource, connection controls and Meter NPLC inside Device. The selected model, connection status and live readouts remain visible.
- Preserve measurement, CSV, instrument and safety behavior.

## 1.3.0-alpha.3

- Prepare public code-signing and privacy policies for a SignPath application; acceptance and signatures remain pending.
- Add versioned Windows PE product metadata and an unsigned GitHub-hosted source build workflow with metadata/signature reports.
- Keep upstream signing boundaries explicit and preserve all redistribution sources/licenses.
- Publish Windows Setup/portable, Mac portable and source kit as a development preview; new model hardware validation remains pending.


## Earlier 1.3.0 development changes (alpha.1–alpha.2)

- Reduce sweep precision queries by caching resolution only while the observed range stays unchanged; still check autorange after each conversion and preserve source readback, every-sample compliance and CSV flush.
- Show readings per point and NPLC beside Run controls. Zero wait never starts an extra live conversion.

- Port legacy 7651, 34401A, 34465A and 7461A model protocols from Python into the Rust engine; physical validation remains pending.
- Select model-specific range, compliance, NPLC, identity and safe shutdown behavior in GUI, CLI and Notebook control.
- Add 34465A USBTMC enumeration/setup, and ADCMT USB ADC-language reads via official VISA without replacing vendor drivers.
- Document unsigned Windows release limitations and publisher signing requirements.

## 1.2.0 (2026-10-16)

- Add a graphical Windows Setup.exe, shared desktop/Start-menu shortcuts and Windows Installed apps registration, while keeping portable distribution.
- Verify updates and preserve configuration, measurements and instrument drivers during uninstall; retain a verified archive of app-local files.
- Make the thin Python client pip/uv-installable, with automatic discovery of the installed Windows Rust engine.

- Prefer installed Windows 64-bit VISA per USB/GPIB resource, keeping WinUSB for other devices.
- Keep vendor DLLs outside the distribution and load them in a separate bounded helper process.
- Preserve existing manufacturer/third-party driver assignments during USB setup. Do not switch backends or replay commands after a VISA failure.

## 1.1.0 (2026-10-16)

- Refresh the existing three-column UI with neutral charcoal cards, restrained borders and clearer text hierarchy.
- Distinguish editable settings from action buttons, align Mode/Sweep and quantity-unit right edges, and keep the central measurement view fixed.
- Show compact path/resource summaries with full-value hover and click-to-edit behavior; make connection and output states explicit.
- Reduce plot grid/legend emphasis and replace full-height panel toggles with compact controls. Keep Data and Log visible together.
- Preserve the measurement engine, scientific CSV output, device communication and output safety behavior.

## 1.0.1 (2026-10-15)

- Include an optional Windows app installer for Program Files placement and a desktop shortcut with a writable user working directory. Keep direct portable use available.
- Verify files before installation, retain existing installation access controls, and protect settings, measurement data and driver cache during updates.
- Document install/update steps and include the installer in portable packages and the source build kit.

## 1.0.0 (2026-10-15)

- Verify real source output OFF and zero before every sweep configuration, including retries after failures. Refuse Start when verification fails.
- Return compliance-stop results promptly to Python/Notebook callers instead of waiting for a timeout.

- Hide the separate Output Off button. Retain output_off IPC, priority handling, and automatic source cleanup on Stop, disconnect and shutdown.
- Publish matching Mac and Windows portable applications and the source build kit, retaining complete-data Save, automatic source setup, per-point Repeat averaging and exact scientific CSV output.

## 0.4.3 (2026-10-15)

- Use Start / Stop / Save in Run. Save exports the complete, exact CSV and companion run settings after cleanup; keep automatic recording and move Output Off into Source.
- Treat Repeat as N meter readings averaged at each fixed source target, with one settling wait and one CSV row per target. Preserve legacy average_count as a fallback rather than multiplying counts.
- Check software compliance after every conversion and preserve the triggering reading, unit and partial acquisition count on early stop.
- Correct unit-conversion alignment noise, resolve hardware step/ramp settings automatically, and explain blocked Start requests in actionable English dialogs without raising limits.
- Avoid unnecessary intermediate ramp steps between adjacent sweep targets. Retain explicit settling, NPLC and protection settings.

## 0.4.2 (2026-10-14)

- Remove unverified 82357B firmware acquisition and upload. Only pre-initialized adapters are supported; boot devices are refused without changing their driver.
- Document component GPL/LGPL terms, standalone GPIB protocol, complete corresponding source and library relinking/replacement.
- Include current native third-party notices and redistribution instructions outside the app bundle; reject firmware payloads and obsolete downloaders during packaging.

## 0.4.1 (2026-10-14)

- Distinguish overlapping forward/backward curves with blue solid and purple dashed lines, plus filled-circle/open-diamond markers.
- Match Field/Value to the native CSV's 14 names, order and exact scientific values using one shared formatter.

## 0.4.0 (2026-10-14)

- Coordinated amber/teal electrical channels and blue/purple sweep directions with a plot legend.
- Mode changes switch live instrument functions after verified output OFF and zero levels.
- Live acquisition continues during settling and each averaging conversion without adding CSV rows.
- Join turning points and render separate finite paths so sweep endpoints remain connected.
- Ignore ordinary wheel movement in the central graph; retain explicit zoom/pan controls.
- Support Python/Notebook automation through a thin standard-library client for Rust.


- Config at the top; compact I/V readouts, filled time-history trends and zero-centered level bars embedded in Source and Meter.
- Continuous idle monitoring after both instruments connect, using one worker
  with sweeps. No source output enable during monitoring.
- Native 14-column CSV and typed display data; removed the legacy CSV API.
- Rust-owned instrument TOML, Rust tests/CI and Cargo dependency audit.
- Removed the former Python application, notebooks and PyInstaller builds.


## Unreleased

- Fix Windows USB installer log sharing so SetupAPI log replacement no longer causes repeated 34-second waits.

- Fix native NI GPIB-USB-HS+ initialization on Windows when only the communication interface has WinUSB. Omit the optional analyzer-interface request; retain device/LED initialization and all required-transfer error handling. macOS behavior is unchanged.
- Recheck Windows USB drivers before Refresh, recover missing NI communication device nodes after uninstall, and wait for driver activation before scanning. Failed setup does not start instrument discovery.

## Portable v0.2.3 (2026-10-05)

- Publish matching macOS arm64 and Windows x64 portable distributions, English documentation, source build kits and checksums. Preserve the original Python v0.2.3 release.

- Simplify the Windows user distribution to a root GUI executable, README.txt, runtime and support folders; archive corresponding source and keep diagnostics/licenses in support. Validate the relocated GUI before packaging.

- Verify actual instrument identity before accepting hardware connections, reject mismatched Device/Resource selections on USB and GPIB, and close unverified handles without model-specific output commands.

- Avoid external vendor VISA discovery in the native portable, and display Refresh phases and elapsed seconds. Verify timeout recovery and repeated scans.

### Added

- Add PyInstaller build scripts and instructions for standalone GUI and CLI apps with Python included, with a writable per-user working directory for the frozen GUI.
- Add an optional persistent NI USB-GPIB userspace helper for Apple Silicon builds, with shared device access, configured timeouts, addressed local control, and explicit source-distribution instructions.
- Add a bundled direct USBTMC helper for the Agilent 34411A, including serial selection and USB descriptor discovery.
- Discover unregistered GPIB listeners at primary addresses 1–30 through the NI helper, and run Refresh in a background thread with measurement/discovery coordination.
- Add an Agilent 82357B userspace transport for macOS with automatic adapter selection, checked FX2 firmware loading, primary listener discovery, and an initial firmware download script. Identification has been verified on a remote Apple Silicon Mac.
- Check physical ATN on 82357B and interpret write-completion counts without an undocumented response prefix; include raw status diagnostics for remote verification.
- Release the 82357B external TI reset before AUX register setup so reopening after close can initialize the GPIB controller; verify repeated reopen/query cycles with a reset-aware USB simulator.
- Accept the physical 82357B's six-byte transfer-status response while preserving completion and exact byte-count checks; reject responses that omit any byte of the count.
- Release and verify ATN after GPIB addressing and before Agilent NO_ADDRESS data send/read, matching the Linux-GPIB common-layer standby transition.

- Support direct USB for GS210/GS200 alongside 34411A, with per-device USB ID and serial matching, and verified two-session isolation in native mock tests.
- Prepare an English macOS arm64 portable release package with source, licenses, diagnostic tools, USB/GPIB profiles, release notes and SHA256 checksums.
- Add a Windows x64 native build kit with statically linked USBTMC/NI/82357B helpers, Windows pipe handling, SHA256-verified firmware setup, WinUSB instructions, corresponding source and build-time executable checks. Windows hardware validation remains pending.
- Add a dedicated Windows USB setup candidate using a replaceable libwdi DLL, fixed instrument/adapter selection, exact-instance driver assignment, automatic first-launch checks and 82357B firmware/staging. Real Windows installation remains unverified.

### Fixed

- Avoid register polling between 82357B standby and receive requests, resolving observed GS210 first-byte loss on the remote Mac.
- Widen all dropdown menus only while open, preserving original input and panel widths.

- Provide identification and connection status for simulated instruments so the GUI can connect them without errors.
- Populate resource selectors from native USB discovery even when no external VISA runtime is installed, merge available VISA resources, and retain manually entered addresses when refreshing.

## [0.2.3] - 2026-09-15

### Changed

- Move Wait time, Average count, and both Compliance limits into a popup opened by the Timing / Compliance button below Step, retaining the original dark theme.
- Apply popup edits with OK, restore values with Cancel or close, and prevent editing during measurements. Config Save persists the applied settings.
- Consolidate pending dependency updates: IPython 9.15.0, Matplotlib 3.11.1, pre-commit 4.6.1, Ruff 0.16.0, Twine 7.0.0, setuptools >=83.0.0, and actions/upload-artifact v7.

## [0.2.2] - 2026-09-15

### Added

- Added editable GUI current and voltage stop thresholds, defaulting to 1 mA for voltage sourcing and 1 V for current sourcing.
- Preserve both thresholds and their units across mode changes, config saves, and reloads; retain legacy compliance values.
- Validate positive finite compliance settings and test positive/negative threshold boundaries and output cleanup.

### Changed

- Store the editable GUI default at `~/.kohdalab/config/iv.json`, alongside TRKR configs with a separate filename.
- Remember the selected config in `~/.kohdalab/last_iv_config.json`, retain legacy-path fallback, and support shared `KOHDALAB_STATE_DIR` and home-directory expansion.
- Document averaged-reading software stop behavior and the distinction from instrument hardware protection.
- Update locked cryptography, JupyterLab, pip, and Tornado dependencies to address the release-time vulnerability audit.

## [0.2.1] - 2026-07-14

### Added

- Added public Python API examples and a conservative real-hardware smoke-test guide.
- Added GUI run-state, measurement-boundary, instrument-edge, and compatibility regression tests.
- Added versioned config files and a packaged JSON Schema for editor and external-tool validation.
- Added a guarded version-bump command with dry-run support and release-metadata synchronization.
- Added a tag-gated release workflow that verifies, archives, and attaches distributions to a draft GitHub Release.
- Added one cross-platform project-check command shared by local development, CI, and release builds.
- Added `init-config` for safely creating an editable local config from the packaged default.
- Added CLI version reporting and text/JSON `doctor` diagnostics for config and VISA troubleshooting.
- Added shared Ruff formatting, EditorConfig, and commit/push hooks for consistent local and CI checks.
- Added locked dependency auditing and a weekly security workflow; updated Pillow to the patched release identified by the first audit.
- Added Mypy source checking to local quality checks, commit hooks, and CI.

### Changed

- Raised the supported Python baseline to 3.13 and removed the Python 3.10 compatibility layer and CI jobs.
- Expanded the README with configuration resolution, supported instruments, local installation, and development verification.
- Updated setup, usage, measurement-specification, and contribution guides to use the packaged default and explicit local configs.
- Removed the duplicated Python default so normalization, GUI, CLI, Notebook, and tests share the packaged JSON as the single source of truth.
- Raised enforced statement and branch coverage to 100%.
- Simplified unreachable configuration, scan, GUI, ADCMT, Keysight, and VISA branches without changing supported behavior.

## [0.2.0] - 2026-07-13

### Added

- Added simulated source and meter drivers for hardware-free measurements.
- Added config preflight validation and full-provenance CSV output.
- Added end-to-end measurement and safety-cleanup regression tests.
- Added hardware-free CLI and Notebook regression tests.
- Added release-metadata and distribution-artifact verification.
- Added CLI and packaging smoke tests to CI.
- Added package metadata, linting, coverage, and packaging-focused regression tests.
- Added project governance and safety baseline files.
- Added GitHub issue and pull request templates.
- Added CI and Dependabot configuration.

### Changed

- Unified CLI, GUI, and Notebook configuration-path resolution.
- Expanded CI across Python 3.10 and 3.13 on Ubuntu and Windows.
- Moved the default configuration into the installable package.
- Derived the runtime version from installed package metadata and displayed it in the GUI title.

- Show the Windows GUI before checking USB drivers, with asynchronous setup, phase messages and elapsed seconds. Keep the GUI open after setup failure or cancellation.

---

# 変更履歴（日本語）

## 1.3.0-alpha.14 — 2026-10-28

- GUI外部制御の停止条件・例外型・処理結果をTRKRと統一し、IVの既定保護と正常終了時のOutput OFF確認を維持。
- 通信断・Python例外・待機タイムアウトの停止を個別指定可能に。継続を明示選択した切離し処理も完了までGUIの編集をロック。
- 出力確認付き`stop_and_wait()`を追加し、安全停止失敗を停止済みの成功とせずエラーで報告。CSV・設定schemaは維持。
- 利用方法・停止条件の英日文書を更新。自動テスト・模擬実験は新しい実機適合確認を意味しません。

## 1.3.0-alpha.13 — 2026-10-28

- 2台の電圧Sourceを整数比で正確に配分するDual V-I測定を追加し、各Sourceの独立した安全停止と、差動値 `V1 - V2` を明示します。
- 左パネルに複数の保存済みSource/Meterを管理するインベントリを追加しました。未使用機器は暗く表示し、使用する役割はSettingsで指定します。
- 既存の18列解析CSVとraw CSVの契約を維持します。Dualの比率と各Sourceの設定値／読み戻し値はrun JSONへ保存し、取得できない端子電圧を実測値として記録しません。
- TRKR形式の所有権、UUIDジョブreceipt、非同期結果、設定／接続操作、機械判定可能なエラーコードにGUI API protocol 2を揃え、IVのStop／Output OFFの動作を維持します。
- このpreviewは自動・シミュレーション検証を含みますが、新しい実機認定は主張しません。

- Dualの正の待機時間をSource状態確認で使い切っても、取得時期に達した保護用ライブ変換を省略しません。wait=0の経路は維持し、制限到達点を保存する前に停止します。

- Gate／接続操作の成否に直前の測定のStopped状態を使わず、その操作の完了を判定します。実際の安全停止・測定保護停止は成功扱いにしません。

## 1.3.0-alpha.12 — 2026-10-24

- Source/Meterのライブ表示に制限到達の点滅警告を保持し、停止時の実設定値・トリガー・OFF確認結果・復旧案をポップアップ表示します。OFF失敗を明示し、次の実行要求（設定エラーを含む）で前回の警告を解除します。

- **Settings… / Measurement Settings**でSource側のハードウェア保護と外部Meter側のソフトウェア停止を分離し、4端子配線の説明と実効レンジ・保護値を表示します。
- GS210のH/L・EMRを/MON不要の非消去条件レジスタで検出します。0秒待機のランプも監視し、停止時は証跡保存より先にOFFを要求します。制限到達点は通常CSV・fitから除外し、生応答と停止情報を保持します。
- 出力ON前にGS210の実設定保護値を確認します。10/100 mVレンジの200 mA固定保護を扱い、より小さい電流保護を明示した場合は対応レンジを選択します。旧設定のレンジ・分解能は保護欄を編集するまで維持します。
- Settings schemaを2、run metadata schemaを7に更新し、18列の解析CSVは維持します。Pythonの終了状態と互換性判定も更新します。

## 1.3.0-alpha.11 — 2026-10-23

- ゼロ中心の経路を 0 → End → Start → 0 に修正し、入力区間外への反転を防ぎます。ゼロを含まない区間は出力前に拒否します。
- Step で割り切れない区間も端点に正確に到達し、実効 Step・丸め・点数・経路を Run に表示します。
- ランプ・待機中を含む取得済みの全読み値を保護判定し、制限・取得異常時は追加照会や保存より先に OFF を要求します。通常 Stop のゼロ復帰は維持します。
- GS210・7651 の機能・設定レンジ・ゼロ・ON を照合し、全対応メーター・両モードに共通適用します。
- 停止を起こした読み値・通信エラー・時刻を保持し、OFF 後のゼロを変換時の読み値として保存しません。analysis/raw CSV と実行記録の現行形式を維持します。

## 1.3.0-alpha.9

- 各保存CSV行のtimestampを、後続のprovenance・表示・source readback・disk処理より前の、最終meter conversionのhost観測完了時刻に設定します。
- 監査対象query、average window、flush済みCSV writeのhost UTC/monotonic timingを明示的に記録します。device clock timestampやuncertaintyを推定しません。

## 1.3.0-alpha.8

- 既存の14列CSV schemaと最短往復可能な数値を維持します。
- repeatごとのraw reply、利用可能なrange/resolution、機器identity/resource、partial runの証跡を含む、flush済み`.readings.jsonl` provenance companionを追加します。
- 表示precisionを記録値から分離します。resolutionやraw replyの表記はmeasurement uncertaintyを主張しません。

## 1.3.0-alpha.7

- 対応 model、control、測定挙動を変えずに、instrument capability、specification、measurement definition を集約。
- 既存の safety decision/protocol string を保持しつつ、ramp/cleanup sequence を共有し、typed execution/output/error state を使用。
- setting、run metadata、Notebook API compatibility を version 化し、legacy configuration/client を受け入れ、新しい setting の偶発的上書きを防止。
- desktop formatting、setting、history、widget を目的別 module に分割し、cross-platform driver contract と regression coverage を追加。
- 全対応 instrument/adapter を文書化し、実際の GUI screenshot を含め、portable、Setup、build-kit source を更新。
- 14-column CSV format、numeric output、device command ordering、output-off behavior を保持。

## 1.3.0-alpha.6

- Windows installer registration/native Program Files（32-bit Notebook kernel を含む）、または macOS user/system Applications folder から installed Rust engine を自動検出。
- CLI path、installation directory、Mac app bundle を明示 client override として受け入れ、deterministic override priority と明瞭な missing-engine error を保持。
- 実行可能な Notebook migration tutorial、更新した client instruction、automatic discovery test を portable package、Setup、build kit に含める。
- measurement、CSV、driver、output protection behavior を保持。

## 1.3.0-alpha.5

- desktop executable を Windows では `kohdalab-iv.exe`、macOS では `kohdalab-iv` に改名し、Mac bundle を `KohdaLab IV.app` とする。legacy GUI executable alias は配布しない。
- 新名称に shortcut、metadata、packaging、CLI launch、signing input を更新。既存 configuration、device lease、Python `iv-cli` automation は保持。
- 安全な installation update のため old/new process detection と shortcut migration を保持。

## 1.3.0-alpha.4

- three-column layout を保ったまま Output と Run の周囲に別々の rounded outline を追加。
- Source/Meter の model、resource、connection control、Meter NPLC を Device 内に折り畳む。選択 model、connection status、live readout は表示を維持。
- measurement、CSV、instrument、safety behavior を保持。

## 1.3.0-alpha.3

- SignPath application 向けの public code-signing/privacy policy を準備。acceptance/signature は未了。
- version 化した Windows PE product metadata と、metadata/signature report を含む unsigned GitHub-hosted source build workflow を追加。
- upstream signing boundary を明示し、全 redistribution source/license を保持。
- development preview として Windows Setup/portable、Mac portable、source kit を公開。新 model の hardware validation は未了。

## 1.3.0 の初期開発変更（alpha.1–alpha.2）

- observed range が変わらない間だけ resolution を cache して sweep precision query を減らす。各 conversion 後に autorange を確認し、source readback、every-sample compliance、CSV flush は保持。
- Run control のそばに point ごとの reading と NPLC を表示。zero wait は extra live conversion を開始しない。
- legacy 7651、34401A、34465A、7461A model protocol を Python から Rust engine に port。physical validation は未了。
- GUI、CLI、Notebook control で model 固有の range、compliance、NPLC、identity、safe shutdown behavior を選択。
- 34465A USBTMC enumeration/setup と、vendor driver を置換しない official VISA 経由の ADCMT USB ADC-language read を追加。
- unsigned Windows release の制限と publisher signing requirement を文書化。

## 1.2.0（2026-10-16）

- graphical Windows Setup.exe、共通 desktop/Start-menu shortcut、Windows Installed apps registration を追加し、portable distribution を維持。
- update を検証し uninstall 中も configuration、measurement、instrument driver を保持。app-local file の検証済み archive を保持。
- thin Python client を pip/uv-installable にし、installed Windows Rust engine の自動検出を追加。
- USB/GPIB resource ごとに installed Windows 64-bit VISA を優先し、他 device は WinUSB を維持。
- vendor DLL を distribution 外に置き、別の bounded helper process で load。
- USB setup 中の既存 manufacturer/third-party driver assignment を保持。VISA failure 後に backend を切替えたり command を replay したりしない。

## 1.1.0（2026-10-16）

- neutral charcoal card、抑制した border、明瞭な text hierarchy で既存 three-column UI を刷新。
- editable setting と action button を区別し、Mode/Sweep と quantity-unit の右端を揃え、central measurement view を固定。
- full-value hover/click-to-edit を持つ compact path/resource summary を表示し、connection/output state を明示。
- plot grid/legend の強調を弱め、full-height panel toggle を compact control に置換。Data と Log を同時に表示。
- measurement engine、scientific CSV output、device communication、output safety behavior を保持。

## 1.0.1（2026-10-15）

- Program Files 配置と writable user working directory の desktop shortcut を持つ optional Windows app installer を含め、direct portable use を維持。
- installation 前に file を検証し、既存 installation access control を維持、update 中の setting/measurement data/driver cache を保護。
- install/update step を文書化し、installer を portable package/source build kit に含める。

## 1.0.0（2026-10-15）

- failure 後 retry を含む全 sweep configuration 前に、real source output OFF と zero を検証。検証失敗時は Start を拒否。
- compliance-stop result を timeout 待ちせず Python/Notebook caller に速やかに返す。
- separate Output Off button を隠す。output_off IPC、priority handling、Stop/disconnect/shutdown 時の automatic source cleanup を保持。
- matching Mac/Windows portable application と source build kit を公開し、complete-data Save、automatic source setup、point ごとの Repeat averaging、正確な scientific CSV output を保持。

## 0.4.3（2026-10-15）

- Run で Start / Stop / Save を使用。Save は cleanup 後に complete/exact CSV と companion run setting を export し、automatic recording を保ち Output Off を Source に移動。
- Repeat を各 fixed source target の N meter reading を平均するものとし、one settling wait と target ごとの one CSV row を使用。count を乗算せず legacy `average_count` を fallback として保持。
- 各 conversion 後に software compliance を確認し、early stop 時に triggering reading、unit、partial acquisition count を保持。
- unit-conversion alignment noise を修正し、hardware step/ramp setting を自動解決し、limit を上げず blocked Start request を actionable English dialog で説明。
- adjacent sweep target 間の不要な intermediate ramp step を避け、explicit settling、NPLC、protection setting を保持。

## 0.4.2（2026-10-14）

- 未検証の 82357B firmware acquisition/upload を削除。pre-initialized adapter のみ対応し、boot device は driver を変えず拒否。
- component GPL/LGPL term、standalone GPIB protocol、complete corresponding source、library relinking/replacement を文書化。
- current native third-party notice/redistribution instruction を app bundle 外に含め、firmware payload/obsolete downloader を packaging 時に拒否。

## 0.4.1（2026-10-14）

- overlap する forward/backward curve を blue solid/purple dashed line と filled-circle/open-diamond marker で区別。
- Field/Value を native CSV の 14 name、order、exact scientific value に shared formatter で一致させる。

## 0.4.0（2026-10-14）

- amber/teal electrical channel と blue/purple sweep direction を plot legend と協調。
- verified output OFF/zero level の後、Mode change が live instrument function を切替。
- settling と各 averaging conversion 中も live acquisition を継続し、CSV row は追加しない。
- turning point を結び separate finite path を描画し sweep endpoint を接続したままにする。
- central graph の通常 wheel movement を無視し、explicit zoom/pan control を保持。
- Rust 用 thin standard-library client で Python/Notebook automation を対応。
- Config を上部に置き、compact I/V readout、filled time-history trend、zero-centered level bar を Source/Meter に埋込み。
- 両 instrument 接続後の continuous idle monitoring を sweep と一つの worker で実行。monitoring 中に source output を enable しない。
- native 14-column CSV と typed display data を使用し legacy CSV API を削除。
- Rust-owned instrument TOML、Rust test/CI、Cargo dependency audit。
- 旧 Python application、notebook、PyInstaller build を削除。

## Unreleased / 未リリース

- Windows USB installer log sharing を修正し、SetupAPI log replacement による繰返し 34-second wait を防止。
- Windows で communication interface のみが WinUSB の場合の native NI GPIB-USB-HS+ initialization を修正。optional analyzer-interface request を省略し、device/LED initialization と required-transfer error handling を保持。macOS behavior は不変。
- Refresh 前に Windows USB driver を再確認し、uninstall 後の missing NI communication device node を復旧、scan 前に driver activation を待機。setup failure は instrument discovery を開始しない。

## Portable v0.2.3（2026-10-05）

- matching macOS arm64/Windows x64 portable distribution、English documentation、source build kit、checksum を公開。original Python v0.2.3 release を保持。
- Windows user distribution を root GUI executable、README.txt、runtime/support folder に簡潔化。対応 source を archive し、diagnostic/license を support に置き、relocated GUI を packaging 前に検証。
- hardware connection を受け入れる前に実 instrument identity を検証し、USB/GPIB の mismatched Device/Resource selection を拒否。unverified handle は model-specific output command を出さず閉じる。
- native portable で external vendor VISA discovery を避け、Refresh phase/elapsed second を表示。timeout recovery/repeated scan を検証。

### Added / 追加

- Python を同梱する standalone GUI/CLI app 用 PyInstaller build script/instruction と、frozen GUI の writable per-user working directory を追加。
- Apple Silicon build 向け optional persistent NI USB-GPIB userspace helper を追加。shared device access、configured timeout、addressed local control、explicit source-distribution instruction を備える。
- Agilent 34411A 用 bundled direct USBTMC helper（serial selection/USB descriptor discovery を含む）を追加。
- NI helper 経由で primary address 1–30 の unregistered GPIB listener を discover し、measurement/discovery coordination を持つ background thread で Refresh。
- macOS 向け Agilent 82357B userspace transport を追加（automatic adapter selection、checked FX2 firmware loading、primary listener discovery、initial firmware download script）。remote Apple Silicon Mac で identification を検証。
- 82357B の physical ATN を確認し、undocumented response prefix なしに write-completion count を解釈。remote verification 用 raw status diagnostic を含める。
- 82357B external TI reset を AUX register setup 前に release し、close 後 reopen して GPIB controller を initialize 可能にする。reset-aware USB simulator で repeated reopen/query cycle を検証。
- physical 82357B の six-byte transfer-status response を受け入れ、completion/exact byte-count check を保持。count byte が一つでも欠ける response を拒否。
- GPIB addressing 後かつ Agilent NO_ADDRESS data send/read 前に ATN を release/verify し、Linux-GPIB common-layer standby transition に一致。
- 34411A と並んで GS210/GS200 の direct USB に対応し、device ごとの USB ID/serial matching と native mock test の verified two-session isolation を追加。
- source/license/diagnostic tool/USB-GPIB profile/release note/SHA256 checksum を含む English macOS arm64 portable release package を準備。
- statically linked USBTMC/NI/82357B helper、Windows pipe handling、SHA256-verified firmware setup、WinUSB instruction、corresponding source、build-time executable check を持つ Windows x64 native build kit を追加。Windows hardware validation は未了。
- replaceable libwdi DLL、fixed instrument/adapter selection、exact-instance driver assignment、automatic first-launch check、82357B firmware/staging を持つ dedicated Windows USB setup candidate を追加。real Windows installation は未検証。

### Fixed / 修正

- 82357B standby/receive request 間の register polling を避け、remote Mac で観測した GS210 first-byte loss を解消。
- dropdown menu は開いている間だけ幅を広げ、元の input/panel width を保持。
- simulated instrument の identification/connection status を供給し、GUI が error なしに接続可能にする。
- external VISA runtime がなくても native USB discovery から resource selector を設定し、available VISA resource を merge、Refresh 時も manually entered address を保持。

## [0.2.3] - 2026-09-15

### Changed / 変更

- Step 下の Timing / Compliance button で開く popup に Wait time、Average count、両 Compliance limit を移し、original dark theme を保持。
- popup edit は OK で適用、Cancel/close で復元し、measurement 中の編集を防止。Config Save は適用済み setting を保存。
- pending dependency update を統合: IPython 9.15.0、Matplotlib 3.11.1、pre-commit 4.6.1、Ruff 0.16.0、Twine 7.0.0、setuptools >=83.0.0、actions/upload-artifact v7。

## [0.2.2] - 2026-09-15

### Added / 追加

- editable GUI current/voltage stop threshold を追加。voltage sourcing は default 1 mA、current sourcing は 1 V。
- mode change/config save/reload をまたいで両 threshold と unit を保持し、legacy compliance value を維持。
- positive finite compliance setting を validate し、positive/negative threshold boundary と output cleanup を test。

### Changed / 変更

- editable GUI default を `~/.kohdalab/config/iv.json` に保存し、TRKR config と同居させつつ filename は分離。
- selected config を `~/.kohdalab/last_iv_config.json` に記憶し、legacy-path fallback、shared `KOHDALAB_STATE_DIR`、home-directory expansion を対応。
- averaged-reading software stop behavior と instrument hardware protection の違いを文書化。
- release-time vulnerability audit に対応して locked cryptography、JupyterLab、pip、Tornado dependency を更新。

## [0.2.1] - 2026-07-14

### Added / 追加

- public Python API example と conservative real-hardware smoke-test guide を追加。
- GUI run-state、measurement-boundary、instrument-edge、compatibility regression test を追加。
- editor/external-tool validation 用の versioned config file と packaged JSON Schema を追加。
- dry-run support/release-metadata synchronization を持つ guarded version-bump command を追加。
- verification/archive/draft GitHub Release への distribution attach を行う tag-gated release workflow を追加。
- local development/CI/release build で共有する cross-platform project-check command を追加。
- packaged default から editable local config を安全に作る `init-config` を追加。
- config/VISA troubleshooting 用 CLI version reporting と text/JSON `doctor` diagnostic を追加。
- 一貫した local/CI check 用 shared Ruff formatting、EditorConfig、commit/push hook を追加。
- locked dependency audit と weekly security workflow を追加し、first audit で特定した patched Pillow release に更新。
- local quality check、commit hook、CI に Mypy source checking を追加。

### Changed / 変更

- supported Python baseline を 3.13 に上げ、Python 3.10 compatibility layer/CI job を削除。
- README を configuration resolution、supported instrument、local installation、development verification で拡張。
- setup/usage/measurement-specification/contribution guide を packaged default/explicit local config に更新。
- duplicated Python default を削除し、normalization、GUI、CLI、Notebook、test が packaged JSON を single source of truth として共有。
- enforced statement/branch coverage を 100% に上げる。
- supported behavior を変えず、unreachable configuration/scan/GUI/ADCMT/Keysight/VISA branch を簡潔化。

## [0.2.0] - 2026-07-13

### Added / 追加

- hardware-free measurement 用 simulated source/meter driver を追加。
- config preflight validation と full-provenance CSV output を追加。
- end-to-end measurement/safety-cleanup regression test を追加。
- hardware-free CLI/Notebook regression test を追加。
- release-metadata/distribution-artifact verification を追加。
- CLI/packaging smoke test を CI に追加。
- package metadata、linting、coverage、packaging-focused regression test を追加。
- project governance/safety baseline file を追加。
- GitHub issue/pull request template を追加。
- CI/Dependabot configuration を追加。

### Changed / 変更

- CLI、GUI、Notebook の configuration-path resolution を統一。
- Ubuntu/Windows の Python 3.10/3.13 で CI を拡張。
- default configuration を installable package に移動。
- runtime version を installed package metadata から導出し GUI title に表示。
- USB driver check 前に Windows GUI を表示し、asynchronous setup、phase message、elapsed second を使用。setup failure/cancellation 後も GUI を開いたままにする。
