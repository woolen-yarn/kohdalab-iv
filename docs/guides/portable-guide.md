# Rust desktop release and validation / Rust デスクトップ配布と検証

> **日本語の案内:** このガイドのコマンド、アーカイブ名、パスは英語原文のまま入力してください。

> Historical reference for an earlier edition. For Rust v1.0.0 operation, use
> [the current desktop guide](application-guide.md) and `specs/`. The current
> GUI has Start / Stop / Save; output-off remains an engine/API function.


Version: **0.3.4**. This is the native desktop release,
separate from the stable Python portable. The full migration plan is in
[next_version_plan.md](../development/next-version-plan.md).

Version 0.3.4 adds nonnegative timing inputs, automatic engineering units on
graph axes, and display precision from shared instrument TOML specifications.
CSV uses 17-digit scientific notation without display rounding. See
[instrument_precision.md](../reference/instrument-precision.md) for reviewed models, sources
and limitations; literal front-panel matching remains unverified.

Alpha.3 adds the blue/orange I-V application icon to both Windows executables,
the Mac app bundle and the GUI window. Editable SVG and platform icon assets
are included in the source; `scripts/build_iv_icons.py` rebuilds them.

Alpha.2 optimizes CSV allocation, repeated sweep planning, engine IPC and graph
updates while preserving measurement and persistence settings. The
[measured optimization report](../development/rust-optimization.md) compares it with alpha.1;
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
- App-specific settings at `~/.kohdalab/iv/config.json`, with first-launch import of valid native Rust `~/.kohdalab/config/iv-next.json` settings. The original is preserved. Python GUI `iv.json` files use a different schema and are not automatically imported. See [settings storage](../reference/settings-storage.md).
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

The [Python and Rust performance comparison](../validation/performance-comparison.md) records
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

The release ZIPs are built into `dist/` and published as `portable-v0.3.0`
in the Kohdalab repository and personal mirror. The earlier measurements above
retain their preview-version context; release promotion does not imply that
the remaining acceptance work has been completed.

---

# Rust デスクトップ配布と検証

> 以前の版についての履歴資料です。Rust v1.0.0 の操作には[現在のデスクトップガイド](application-guide.md)
> と `specs/` を使用してください。現在の GUI は Start / Stop / Save を持ち、output-off はエンジン／API 機能として残ります。

バージョンは **0.3.4** です。これは安定した Python ポータブル版とは別のネイティブデスクトップリリースです。
完全な移行計画は[次期バージョン計画](../development/next-version-plan.md)にあります。0.3.4 は非負の時間入力、
グラフ軸の自動工学単位、共有する計測器 TOML 仕様に基づく表示精度を追加します。CSV は表示丸めをせず
17 桁の科学表記を用います。レビュー済みモデル、出典、制約は[計測器精度](../reference/instrument-precision.md)
を参照してください。前面パネルとの文字どおりの一致は未検証です。Alpha.3 は青／橙の I-V アイコンを
Windows 実行ファイル、Mac アプリバンドル、GUI ウィンドウへ追加しました。編集可能 SVG とプラットフォーム
アイコン資産をソースに含み、`scripts/build_iv_icons.py` で再生成します。Alpha.2 は測定・永続化設定を維持して
CSV 割り当て、反復スイープ計画、エンジン IPC、グラフ更新を最適化しました。測定済みの比較は
[最適化報告](../development/rust-optimization.md)にあり、下記の履歴上の受入観測は当時の文脈を保持します。

## 実装済み

- 計画／永続化、計測器転送、デスクトップの分離 Rust ワークスペース。アプリケーションクレートは unsafe を禁止します。
- KohdaLab Camera Native と同じ GUI 基盤の egui/eframe デスクトップ。左に計測器・プロファイル、中央にスイープ／出力／実行とグラフ、右にログと Field / Value を置く 3 列 IV レイアウトを再現します。黒テーマ、グループボックス、コンパクト操作、背の高いサイドパネル切替を再現し、Source と Meter は独立接続します。Timing と制限は中央から開き、Config と出力 Browse はネイティブファイルダイアログを使います。
- 分離された Rust 測定プロセス。デバイス転送は GUI 外で実行し、ネイティブ C/C++ ヘルパーは分離サブプロセスのままです。Rust が外部ライブラリーをメモリー安全にするわけではありません。
- 明示的シミュレーション、実計測器の識別確認、選択リソースロック、上限付き通信フレーム／キュー、ヘルパー転送期限、2 秒 GUI heartbeat リース。
- IV／VI 計画、4 種のスイープパターン、反復、ソース範囲／分解能検証、ハードウェアコンプライアンス、ソフトウェア閾値、待機、NPLC、平均化、割込み可能ランプ、ストリーム抵抗フィット。
- 既存の 46 列 CSV レイアウト、排他的ファイル作成、点ごとのフラッシュ、設定・識別・物理リソースアドレス・結果・警告を記録する run JSON。フラッシュは停電からの保存を保証しません。
- 設定は `~/.kohdalab/iv/config.json` に分離します。旧Rust版 `~/.kohdalab/config/iv-next.json` は初回に引き継ぎ、元ファイルを残します。Python版 `iv.json` は別形式のため自動取り込みしません。[設定の保存先](../reference/settings-storage.md)を参照してください。
- 緊急停止ではレベルを戻す前に Output Off を送ります。破損ヘルパーはデバイスロックを保持した上限付き復旧期限内で置換します。OFF 再試行前に以前確認したソースの一致が必要です。ゼロレベルコマンド失敗でも、確認済み OFF 状態を破棄しません。
- ポータブル配布、依存ライセンス、ネイティブヘルパーライセンス、完全なソースとビルドスクリプト、ファイルハッシュ、ad hoc 署名済み Mac バンドル。

## 2026-10-13 に記録した検証

Windows テストホストは NEEDLEFISH でした。NI GPIB-USB-HS+ の計測器インターフェースは **WINUSB**
サービスと **KohdaLab** ドライバープロバイダーを使いました。従ってテストは Rust 経路でベンダー VISA
を使わず、確立済みのポータブルドライバーとヘルパーを使いました。

| 確認項目 | 観測結果 |
| --- | --- |
| Rust ユニット・偽ヘルパーテスト | Mac で 17 件成功、strict Clippy 成功 |
| 完全なシミュレーションエンジンプロセス | 3 点完了、既存 46 CSV 列、1 kOhm フィット、出力 OFF |
| シミュレーションの長い待機中 Stop | 約 0.10 s で出力 OFF |
| シミュレーション中の GUI heartbeat 喪失 | 約 2.00 s で出力 OFF・切断 |
| 実 Mac GUI | 41 点シミュレーション、CSV 保存、結果プロット、出力 OFF |
| Mac 最初の GUI 更新 | 開発 Mac で約 0.14 s。これは起動プロキシでありコールドスタートのベンチマークではない |
| Windows NI GPIB 識別 | アドレス 5 の GS210 とアドレス 9 の 34411A を識別、ソース出力 OFF |
| Windows NI GPIB 測定 | -1 mV、0、+1 mV の 3 点、1 mA コンプライアンス、readback 一致、CSV 保存、出力 OFF 確認 |
| Windows GUI と Rust エンジン | 実 NI GPIB 3 点スイープ成功、既存ログイン済みセッションで最初の GUI 更新約 0.12 s |
| Windows リソース検出の反復 | 3 回とも両リソースを返し、約 3.27 s、1.00 s、1.01 s |
| 誤った source/meter 割当 | アドレス 5 と 9 の反転を実際の識別確認で拒否 |
| パッケージ済み Mac アプリ | 署名済みバンドル起動、41 シミュレーション点保存、出力 OFF、最初の更新約 0.15 s |
| パッケージ済み Windows アプリ | ZIP と全ファイルハッシュ一致、実 NI 3 点スイープと出力 OFF、最初の更新約 0.13 s |
| Windows 測定メモリー試料 | プロセスツリーのピーク working set 約 156 MiB、private memory 約 125 MiB |

実機テストはユーザーが明示的に許可した**開放回路**を使いました。ソースは YOKOGAWA GS210、
serial 91T729315、firmware 2.02、メーターは Agilent 34411A、serial MY53000981、firmware
2.40-2.40-0.09-46-09 です。3 点エンジン実行は約 **2.17 s**、接続とクリーンアップを含む CLI 全体は
約 **3.04 s** でした。パッケージ済みエンジンのシミュレーションは Stop を約 **0.06 s**、heartbeat
喪失クリーンアップを約 **2.00 s** で通過しました。これらは短いテスト固有の観測であり、故障デバイスに
対する保証期限ではありません。Windows メモリー試料は 3 点実行中 100 ms ごとに GUI、Rust エンジン、
ネイティブヘルパーを含めて採取しました。working-set の合計は共有ページを複数回数え得ます。GPU メモリーは
計測しておらず、アイドルメモリー目標もこのピーク測定では確立していません。

開放回路の電流はゼロ付近で変動します。その抵抗フィットは既知抵抗器の精度測定ではありません。このテストが
確立するのは、このハードウェア構成での通信、設定／readback、データ永続化、確認済み出力停止です。計量精度を
確立するものではありません。ローカル証跡は無視対象の `next/validation/` と `build/iv-next-*` ログに置きます。
ハードウェア証跡には `ni-three-points-1.csv` と `.run.json` が含まれ、配布ソース・ポータブルアーカイブからは
意図的に除外します。

## 元 UI の再現

以前の 3 列解釈を、元の Python IV インターフェースを視覚基準として置換しました。黒テーマ、Config /
Source / Meter ボックス、入れ子の Measurement / Output / Run ボックス、独立した単位選択、コンパクトな
Status / Point / Resistance / Conductance ストリップ、青／橙プロット、簡素なログ、Field / Value 切替、
背の高い `<` / `>` パネル操作を再現します。初期ウィンドウは 1240 × 760 です。リソース欄はコンパクトに
保ち、開いたメニューでは完全なアドレスを表示します。Browse / Load / Save、Auto suffix、個別計測器接続は
機能します。未対応のレガシーモデルは表示したまま無効にします。この再現は、すべてのレガシーモデルに対する
設定形式または計測器ドライバー互換を意味しません。

独立接続では別の役割と GPIB セッションを維持します。拒否された識別は有効な役割を残します。Source 切断は
確認済み出力停止を実行し、heartbeat 喪失とアプリケーション終了も source-only 接続をクリーンアップします。
シミュレーションと実機を混在した組では測定を開始できません。再現後の検証では 25 ユニット／偽ヘルパー
テストと strict Clippy に成功しました。Mac と Windows の両 GUI は 41 シミュレーション点を完了して CSV を
保存し、出力 OFF を報告して正常終了しました。これらのウォームテストで最初の GUI 更新は Mac 約 0.14 s、
Windows 約 0.095 s でした。Mac UI 操作確認は両サイドパネル切替、Field / Value 切替、ネイティブ出力 Browse、
Cancel 時の以前の待機時間復元を検証しました。

Windows NI GPIB デバイスは source-only 接続、同じヘルパーへの meter 追加、source を乱さない重複リソースの
拒否、meter を維持した確認済み OFF の source 切断、両方の再接続／切断も通過しました。この接続確認で出力を
有効化していません。上記と同じアドレス 5 の GS210 とアドレス 9 の 34411A を使用しました。ローカルの
スクリーンショットとテストログは無視対象の `next/validation/gui-*-legacy.png` と
`build/iv-next-legacy-*` に置き、以前の測定証跡とは分けて保持します。

## 既知の制約と将来の受入作業

[Python と Rust の性能比較](../validation/performance-comparison.md)は、ウォーム状態の Windows ウィンドウ表示
約 0.125 s 対 0.503 s、シミュレーション Mac 処理ループの改善、60 秒後に Rust 110.7 MiB 対 Python 104.4 MiB
で残る常駐メモリー問題を記録します。これは特定ベンチマーク条件であり、すべての性能目標の受入ではありません。

- Rust 版の直接 USB、混在 USB/GPIB、Agilent 82357B の実デバイス受入。既存ネイティブ経路は再利用しますが、以前の Python 版結果を Rust 版受入とは扱いません。
- 非同期 Windows ドライバー設定／復旧と、クリーンなドライバー削除／再インストール受入。リリースには既存の手動 USB-Setup 支援ツールを含み、この実行ではドライバーをアンインストールしていません。
- レガシー設定のインポート、他の計測器モデル対応。
- コールドスタート、より広いプロセスメモリープロファイル、大グラフベンチマーク。Windows のアイドルプロセスツリー目標（最大 100 MiB）は未達です。
- 長時間実行、全実デバイススイープモード、抜き差し復旧、source または meter 電源喪失、実ハードウェアでのプロセス障害受入。
- インストーラー／署名／公証および一般リリース文書。

リリース ZIP は `dist/` にビルドされ、Kohdalab リポジトリと個人ミラーで `portable-v0.3.0` として公開されます。
上記の以前の測定はプレビュー版の文脈を保持します。リリース昇格は、残る受入作業が完了したことを意味しません。
