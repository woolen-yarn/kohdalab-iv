# Redistributing KohdaLab IV / KohdaLab IV の再配布

> **日本語の案内:** 本文は再配布するバイナリー、対応ソース、第三者ライセンス、82357B
> ファームウェアの扱いを定めます。ライセンス条件そのものは英語の原文および同梱ライセンスが
> 優先します。翻訳でライセンス上の権利・義務を変更することはありません。

This distribution contains separately licensed programs. The repository's MIT
license applies to KohdaLab's original application code; it does not replace
the licenses of the native helpers, libraries, fonts, or other dependencies.

## What to distribute

Distribute the complete portable ZIP, including `support/SOURCE.zip`,
`support/licenses/`, `support/THIRD_PARTY_NOTICES.md`, this file, and the file
manifest. On macOS these documents are beside the app in `support/`, rather
than inside the signed app bundle. Do not deliver only the GUI executable.
If you change a GPL/LGPL component, replace the corresponding source with the
source of your actual modified binary and record your changes and their date.
Do not restrict copying/modification of these components or reverse engineering
for debugging modifications to LGPL libraries.

| Component | Distribution license | Corresponding source / notices |
| --- | --- | --- |
| Rust GUI, engine, CLI, thin Python client | MIT, plus dependency licenses | `SOURCE.zip`: `Cargo.toml`, `Cargo.lock`, `crates/`, `bindings/`; `licenses/rust/DEPENDENCIES.json` and individual notices |
| Combined NI/Agilent GPIB helper | GPL-2.0-only for the combined executable | `SOURCE.zip`: `native/kohdalab_gpib_helper.cpp`, `native/agilent_82357b.*`, `native/vendor/ni-gpib/`, build scripts; `licenses/GPL-2.0.txt` |
| USBTMC helper | MIT application code, statically linked LGPL-2.1-or-later libusb | `SOURCE.zip`: `native/usbtmc_helper.cpp`, headers, scripts, libusb source archive; `licenses/LGPL-2.1.txt` |
| Windows USB-Setup | MIT application, dynamically linked LGPL-3.0-or-later libwdi | `SOURCE.zip`: `native/windows_usb_setup.cpp`, headers, `native/vendor/libwdi/`, build script; `licenses/LGPL-3.0.txt` and `GPL-3.0.txt` |
| libusb 1.0.30 | LGPL-2.1-or-later | `SOURCE.zip`: `native/vendor/libusb-1.0.30.tar.bz2` (unmodified upstream source, including COPYING) |
| Windows MinGW runtimes | Respective permissive notices / GCC runtime exception | `licenses/mingw-w64-COPYING.txt`, `winpthreads-COPYING.txt`, `GCC-RUNTIME-EXCEPTION.txt` |
| GUI dependencies and fonts | Per-dependency licenses | `licenses/rust/` (including font license texts) |

The GPIB executable combines GPL-2.0-or-later NI code and GPL-2.0-only Agilent
code. Version 2 is the common permitted GPL version for that executable; the
Agilent part cannot simply be upgraded to GPLv3. Preserve the original notices.

## Independent native programs

The Rust executable does not link the GPL driver or include its source/header
definitions. It runs a replaceable GPIB program through standard input/output.
That program also runs independently (`--available`, `--idn`, framed byte-I/O).
The interface exchanges instrument addresses, timeouts, raw command bytes and
raw response bytes, not Rust application structures, plans, plots, or records.
Its interface is documented in `docs/reference/helper-protocol.md` in SOURCE.zip.
`KOHDALAB_IV_HELPER_DIR` selects an alternative helper directory. Direct USB
transfers use the USBTMC program rather than the GPL driver; simulation opens
no native helper. The portable lookup expects both named helper files in its
helper directory, even when a measurement uses only direct USB.

This is a distribution of independent programs with their own license terms,
not a claim that process separation automatically exempts every combination
from the GPL. See the recorded evidence in `docs/reference/redistribution-audit.md`.

## Rebuilding and replacing LGPL components

Extract `support/SOURCE.zip` into a writable directory. Complete helper and
library source plus the actual build scripts are provided, rather than only a
link or a time-limited written offer. See `native/REBUILDING.md` for commands to
rebuild/relink with a modified libusb or replace `libwdi.dll`. Code signing
uses no private distributor key: the Mac helper can be ad hoc signed locally.
Do not remove these materials when you pass on the binaries.

## 82357B firmware is excluded

No third-party 82357B firmware, automatic firmware downloader, firmware cache,
or firmware upload function is provided by this release. A public GitHub URL
and a checksum do not grant redistribution permission. We have not established
a license authorizing redistribution of `measat_releaseX1.8.hex`.

Only an already initialized adapter (`0957:0718`) can be used. An uninitialized
adapter (`0957:0518`) produces a clear error without uploading firmware or
replacing its boot driver. Firmware initialization must be obtained separately
from a source that grants the necessary rights. KohdaLab does not distribute
manufacturer software or promise that any external initialization workflow is
available on every OS. Do not add a firmware file to a portable ZIP or build kit.

These notes document the implemented licensing precautions and obligations;
they are not a warranty covering every jurisdiction, patent, trademark, or
third-party device/software agreement. No vendor endorsement is implied.

---

# KohdaLab IV の再配布

この配布物には別個にライセンスされたプログラムが含まれます。リポジトリの MIT ライセンスは
KohdaLab 独自のアプリケーションコードに適用され、ネイティブヘルパー、ライブラリー、フォント、
その他依存関係のライセンスを置き換えません。

## 配布するもの

`support/SOURCE.zip`、`support/licenses/`、`support/THIRD_PARTY_NOTICES.md`、このファイル、
ファイル manifest を含む完全な portable ZIP を配布します。macOS ではこれらの文書は署名済み app bundle の
内側ではなく、app の隣の `support/` にあります。GUI 実行ファイルだけを配布してはいけません。GPL/LGPL
コンポーネントを変更した場合、対応ソースを実際に変更したバイナリーのソースへ置き換え、変更内容と日付を
記録します。これらのコンポーネントについてコピー／変更、または LGPL ライブラリーの変更をデバッグする
リバースエンジニアリングを制限してはいけません。

| コンポーネント | 配布ライセンス | 対応ソース／通知 |
| --- | --- | --- |
| Rust GUI、engine、CLI、薄い Python client | MIT と依存ライセンス | `SOURCE.zip`: `Cargo.toml`, `Cargo.lock`, `crates/`, `bindings/`; `licenses/rust/DEPENDENCIES.json` と個別通知 |
| 結合 NI/Agilent GPIB helper | 結合 executable は GPL-2.0-only | `SOURCE.zip`: `native/kohdalab_gpib_helper.cpp`, `native/agilent_82357b.*`, `native/vendor/ni-gpib/`, build scripts; `licenses/GPL-2.0.txt` |
| USBTMC helper | MIT application code、静的リンク LGPL-2.1-or-later libusb | `SOURCE.zip`: `native/usbtmc_helper.cpp`, headers, scripts, libusb source archive; `licenses/LGPL-2.1.txt` |
| Windows USB-Setup | MIT application、動的リンク LGPL-3.0-or-later libwdi | `SOURCE.zip`: `native/windows_usb_setup.cpp`, headers, `native/vendor/libwdi/`, build script; `licenses/LGPL-3.0.txt` と `GPL-3.0.txt` |
| libusb 1.0.30 | LGPL-2.1-or-later | `SOURCE.zip`: `native/vendor/libusb-1.0.30.tar.bz2`（変更なしの上流 source、COPYING を含む） |
| Windows MinGW runtimes | 各 permissive notices / GCC runtime exception | `licenses/mingw-w64-COPYING.txt`, `winpthreads-COPYING.txt`, `GCC-RUNTIME-EXCEPTION.txt` |
| GUI dependencies と fonts | dependency ごとの licenses | `licenses/rust/`（font license texts を含む） |

GPIB executable は GPL-2.0-or-later の NI code と GPL-2.0-only の Agilent code を結合します。この executable では
version 2 が共通して許可される GPL version であり、Agilent 部分を単純に GPLv3 へ上げることはできません。
元の notices を保持してください。

## 独立したネイティブプログラム

Rust executable は GPL driver をリンクせず、source/header definitions も含みません。標準入出力を通じて
置換可能な GPIB program を実行します。その program も独立して実行します（`--available`、`--idn`、
framed byte-I/O）。interface が交換するのは instrument address、timeout、raw command bytes、raw response bytes で、
Rust application structures、plans、plots、records ではありません。interface は SOURCE.zip 内の
`docs/reference/helper-protocol.md` に記載します。`KOHDALAB_IV_HELPER_DIR` は alternative helper directory を
選択します。direct USB transfers は GPL driver でなく USBTMC program を使い、simulation は native helper を
開きません。portable lookup は測定で direct USB だけを使う場合でも helper directory 内に両 named helper files を
期待します。

これは各自の license terms を持つ独立 program の配布であり、process separation がすべての combination を
GPL から自動的に免除するという主張ではありません。記録済み証跡は
`docs/reference/redistribution-audit.md` を参照してください。

## LGPL コンポーネントの再ビルドと置換

`support/SOURCE.zip` を書込み可能なディレクトリーへ展開します。リンクや期間限定の written offer だけでなく、
完全な helper/library source と実際の build scripts を提供します。変更した libusb の再ビルド／再リンク、
libwdi.dll の置換コマンドは `native/REBUILDING.md` を参照してください。code signing は private distributor key を
使いません。Mac helper はローカルで ad hoc sign できます。バイナリーを渡す際にこれらの資料を削除してはいけません。

## 82357B firmware は除外

本リリースは第三者 82357B firmware、自動 firmware downloader、firmware cache、firmware upload function を
提供しません。公開 GitHub URL と checksum は再配布許可を与えません。`measat_releaseX1.8.hex` の再配布を
認可する license は確立していません。

すでに初期化済み adapter (`0957:0718`) だけを使えます。未初期化 adapter (`0957:0518`) は firmware の
upload や boot driver 置換なしに明確なエラーになります。firmware initialization は必要な権利を与える
source から別途取得しなければなりません。KohdaLab は manufacturer software を配布せず、外部 initialization
workflow がすべての OS で利用できることを約束しません。firmware file を portable ZIP や build kit に追加しては
いけません。

これらの注意は実装済み license precautions と obligations を記録するもので、全法域、patent、trademark、
第三者 device/software agreement を保証するものではありません。ベンダーの推奨は含意しません。
