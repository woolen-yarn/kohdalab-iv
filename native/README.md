# Native instrument programs

The Rust application uses independent native executables for USBTMC and GPIB
byte I/O. No manufacturer VISA runtime or kernel driver binary is bundled.
Windows assigns its inbox WinUSB to the supported communication interfaces.

## License boundaries

- The combined NI/Agilent GPIB executable is distributed under GPLv2.
  The NI userspace port permits GPL-2.0-or-later; the Linux-derived Agilent
  adaptation permits GPL-2.0-only. This is not an MIT driver.
- USBTMC application source is MIT and statically links LGPL-2.1-or-later libusb.
  The GPIB executable also links that library. Full source and relinking
  instructions accompany the binaries.
- The independent Windows setup program is MIT and dynamically links a
  replaceable LGPLv3 libwdi DLL. This build embeds no WDK coinstaller or
  Microsoft kernel binary. It points to the installed Windows WinUSB driver.

See root [REDISTRIBUTION.md](../REDISTRIBUTION.md),
[licensing evidence](../docs/reference/redistribution-audit.md),
[provenance](vendor/README.md), and [rebuild/relink instructions](REBUILDING.md).
The standalone framed byte protocol is documented in
[docs/native_helper_protocol.md](../docs/reference/helper-protocol.md).

## Supported connections

- NI GPIB-USB-HS+ (3923:7618): native GPIB.
- Agilent 82357B: only an already initialized 0957:0718 adapter.
- GS210/GS200 USBTMC (0B21:0039), TMC mode; storage mode is excluded.
- Agilent/Keysight 34411A USBTMC (0957:0A07).

An uninitialized 82357B (0957:0518) is refused without opening it, replacing
its boot driver or uploading firmware. Firmware redistribution rights have
not been established; this release has no downloader or uploader. Obtain any
initialization separately under appropriate rights. External manufacturer
software and operating-system availability are outside this distribution.

Only one supported GPIB adapter may be connected at a time. Controller address
is 0; primary instrument addresses are 1-30. Listener discovery sends GPIB
addressing/control operations, but no SCPI commands, trigger or source-output
commands. USB descriptor discovery does not enable an instrument output.
Opening a GPIB adapter initializes IFC/REN, so it is not a passive bus operation.

## Building and replacing helpers

Use `scripts/build_native_gpib.sh` / `scripts/build_usbtmc_probe.sh` on macOS
or `scripts/build_native_windows.sh` / `scripts/build_usb_setup.sh` on Windows
(MSYS2 UCRT64). Build-host compiler tools are not required on a user's PC.
See REBUILDING.md for full commands and modified-library relinking.

`KOHDALAB_IV_HELPER_DIR` selects a replacement directory containing both named
helper executables. Otherwise the Rust portable uses executables in its support
folder. The measurement engine owns transfers and cleanup; the optional thin
Python client calls that Rust engine. Device/transport errors remain checked.
A software output-off request cannot guarantee shutdown after cable/hardware
loss; retain instrument hardware limits.

Development history is retained in Git and historical release notes. Previous
firmware downloader instructions do not describe the current distribution.

---

# ネイティブ計測器プログラム

Rust アプリケーションは USBTMC および GPIB のバイト I/O に、独立したネイティブ実行ファイルを
使用します。メーカーの VISA ランタイムやカーネルドライバーバイナリーは同梱しません。Windows は
対応する通信インターフェースへ標準 WinUSB を割り当てます。

## ライセンス境界

- 結合された NI/Agilent GPIB 実行ファイルは GPLv2 で配布します。NI ユーザー空間移植は
  GPL-2.0-or-later を許可し、Linux 由来の Agilent 適応は GPL-2.0-only を許可します。これは MIT ドライバーではありません。
- USBTMC アプリケーションソースは MIT で、LGPL-2.1-or-later の libusb を静的リンクします。GPIB 実行ファイルもこのライブラリーをリンクします。完全なソースと再リンク手順をバイナリーに添付します。
- 独立した Windows セットアッププログラムは MIT で、置換可能な LGPLv3 libwdi DLL を動的リンクします。このビルドは WDK coinstaller や Microsoft カーネルバイナリーを埋め込みません。インストール済みの Windows WinUSB ドライバーを指します。

ルートの [REDISTRIBUTION.md](../REDISTRIBUTION.md)、[ライセンス証跡](../docs/reference/redistribution-audit.md)、
[来歴](vendor/README.md)、[再ビルド／再リンク手順](REBUILDING.md)を参照してください。独立した
フレーム付きバイトプロトコルは[ヘルパープロトコル](../docs/reference/helper-protocol.md)に記載されています。

## 対応接続

- NI GPIB-USB-HS+ (3923:7618): ネイティブ GPIB。
- Agilent 82357B: すでに初期化された 0957:0718 アダプターのみ。
- GS210/GS200 USBTMC (0B21:0039)、TMC モード。ストレージモードは対象外。
- Agilent/Keysight 34411A USBTMC (0957:0A07)。

未初期化の 82357B (0957:0518) は、開く、ブートドライバーを置換する、ファームウェアをアップロードする
ことなく拒否されます。ファームウェア再配布権は確立されていないため、本リリースにはダウンローダーも
アップローダーもありません。必要な初期化は適切な権利のもとで別途取得してください。外部メーカーソフト
ウェアと OS の利用可否は本配布の対象外です。

接続できる対応 GPIB アダプターは一度に 1 台だけです。コントローラーアドレスは 0、計測器の一次
アドレスは 1–30 です。リスナー検出は GPIB のアドレス指定／制御操作を送りますが、SCPI コマンド、
トリガー、ソース出力コマンドは送信しません。USB 記述子の検出で計測器出力を有効化することもありません。
GPIB アダプターを開くと IFC/REN が初期化されるため、受動的なバス操作ではありません。

## ヘルパーのビルドと置換

macOS では `scripts/build_native_gpib.sh` / `scripts/build_usbtmc_probe.sh`、Windows（MSYS2 UCRT64）では
`scripts/build_native_windows.sh` / `scripts/build_usb_setup.sh` を使用します。利用者 PC にビルドホスト用の
コンパイラーツールは必要ありません。完全なコマンドと変更ライブラリーの再リンクは REBUILDING.md を参照してください。

`KOHDALAB_IV_HELPER_DIR` は、名前付きヘルパー実行ファイル両方を含む置換ディレクトリを選択します。
指定がなければ Rust ポータブル版は support フォルダーの実行ファイルを使用します。測定エンジンが転送と
クリーンアップを所有し、任意の薄い Python クライアントはその Rust エンジンを呼びます。デバイス／転送エラーは
引き続き検査されます。ケーブルまたはハードウェア喪失後にソフトウェア output-off 要求が停止を保証することは
できないため、計測器のハードウェア制限を保持してください。

開発履歴は Git と過去のリリースノートに残しています。以前のファームウェアダウンローダーの手順は、現在の
配布物を示しません。
