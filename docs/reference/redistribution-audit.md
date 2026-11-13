# Redistribution audit — 2026-10-14 / 再配布監査 — 2026-10-14

> **日本語の案内:** 本文はライセンス調査と配布境界の証跡です。法的な解釈は同梱する各ライセンス
> 原文および必要に応じた専門家の助言に従ってください。

Scope: the native Rust portable and source build kit, not every historical
Python release or arbitrary user-added file. This is an evidence record and
distribution policy, not legal clearance from the firmware copyright holder.

## GPL and LGPL evidence

- `native/agilent_82357b.cpp/.hpp` is GPL-2.0-only, adapted from Linux v6.17
  `drivers/staging/gpib/agilent_82357a/` and `include/tms9914.h` (Frank Mori Hess).
- `native/kohdalab_gpib_helper.cpp` and KAME's NI userspace driver permit GPLv2
  or later. The combined native GPIB executable is distributed under GPLv2,
  the common allowed version. The Rust program has no GPL source/header/link
  dependency; it exchanges generic instrument bytes with that utility.
- GPL applicability depends on both communication and program integration,
  not just separate process IDs. The GPL FAQ permits aggregation of independent
  programs and describes ordinary pipes/command-line communication as an
  indicator of separate programs. The standalone utility interface, replaceable
  executable, independent IDN mode, and USB/simulation paths are recorded in
  `native_helper_protocol.md`. We do not relicense GPL-only source as MIT/GPLv3.
- libusb 1.0.30's LGPL-2.1-or-later source archive is supplied. Full helper source
  and relinking instructions allow rebuilding its static links; they are not
  merely a link to an upstream download. The USBTMC helper is MIT application
  code plus that LGPL library. GPLv2 and LGPL-2.1-or-later admit this combination.
- libwdi is LGPL-3.0-or-later and dynamically linked by the independent MIT
  Windows setup program. Its complete source, changes, LGPL/GPL texts and
  replacement instructions are included. It is not linked into the GPLv2-only
  GPIB helper. The inbox-WinUSB build embeds no Microsoft kernel driver or WDK
  coinstaller; Windows supplies WinUSB. The build configuration excludes WDK,
  libusb0 and libusbK binary embedding.
- libwdi commit `90278c538a8fb5fd82aab25ae7f5a9887ca468ce` is upstream's
  2023-03-01 v1.5.0 commit. Its configure.ac still says 1.3.1 upstream; resource
  metadata/ChangeLog say 1.5.0. This discrepancy is upstream, not a different
  vendored release. The exact commit and modified source define our provenance.
- Rust dependencies and bundled fonts have their own notices; the package
  includes a target-filtered dependency index and supplied license texts.

Primary references (checked 2026-10-14):

- https://www.gnu.org/licenses/gpl-faq.en.html#MereAggregation
- https://www.gnu.org/licenses/old-licenses/gpl-2.0.html (sections 2 and 3)
- https://www.gnu.org/licenses/old-licenses/lgpl-2.1.html (section 6)
- https://www.gnu.org/licenses/lgpl-3.0.html (section 4)
- https://github.com/libusb/libusb/releases/tag/v1.0.30
- https://github.com/pbatard/libwdi/commit/90278c538a8fb5fd82aab25ae7f5a9887ca468ce
- https://github.com/northriv/KAME/tree/b413cc7102e984fa8e5e13bd4993549a6e4584cc/modules/charinterface/usermode-linux-gpib
- https://git.kernel.org/pub/scm/linux/kernel/git/torvalds/linux.git/tree/drivers/staging/gpib?h=v6.17

## 82357B firmware rights: not established, therefore excluded

The earlier downloader used `measat_releaseX1.8.hex` from
`fmhess/linux_gpib_firmware`, commit
`8a5c6c2c1a2adac4c770763a7236452a871a0113`. Its repository and Agilent directory
have instructions and firmware blobs, but no explicit license granting
redistribution of that file. GitHub's repository license endpoint returned 404;
this alone is not proof of prohibition, but provides no affirmative permission.
The Agilent README explains uploading and a two-stage boot issue, not licensing.

Keysight's published IO Libraries release notes grant use subject to the
applicable license agreement; they do not establish redistribution rights for
this particular GitHub firmware blob. A general EULA for another Keysight
product is not treated as the license of this file. Public availability and
checksums do not establish copyright permission.

No written permission was obtained from a rights holder, and no request was
sent on the user's behalf. This release therefore removes firmware download,
cached-file upload, override-path upload and initial boot-driver replacement.
The restriction is operational: an initialized `0957:0718` adapter remains
supported, while boot `0957:0518` is refused without opening/uploading it.
NI HS+ and direct USB do not use this firmware path.

References:

- https://github.com/fmhess/linux_gpib_firmware/tree/8a5c6c2c1a2adac4c770763a7236452a871a0113/agilent_82357a
- https://raw.githubusercontent.com/fmhess/linux_gpib_firmware/master/agilent_82357a/README
- https://www.keysight.com/cz/en/assets/9924-02109/release-notes/ReleaseNotes-IOLS2024U2.pdf

Until a rights holder supplies an applicable grant, do not bundle this blob,
reintroduce automatic acquisition/upload, or claim its rights are cleared.
Packagers validate their outputs to prevent an accidental firmware inclusion.

## 日本語 — 全文対訳

対象はネイティブRust portableとソースbuild kitであり、歴代Python配布すべてや利用者が追加した任意ファイルではありません。これは証跡と配布方針の記録で、ファームウェア権利者からの法的許諾ではありません。

### GPL・LGPLの根拠

- `native/agilent_82357b.cpp/.hpp`はGPL-2.0-onlyで、Linux v6.17の`drivers/staging/gpib/agilent_82357a/`と`include/tms9914.h`（Frank Mori Hess）由来です。
- `native/kohdalab_gpib_helper.cpp`とKAMEのNIユーザー空間driverはGPLv2以降を許可します。結合GPIB実行ファイルは共通して許されるGPLv2で配布します。RustはGPLのソース・ヘッダー・リンクに依存せず、汎用機器バイトを交換します。
- GPLの適用は通信とプログラム統合に依存し、PIDの分離だけでは決まりません。GPL FAQは独立プログラムの集合を認め、通常のpipe・CLI通信を独立性の指標として説明します。独立utility、置換可能な実行ファイル、独立IDN、USB・simulation経路を`native_helper_protocol.md`に記録しています。GPL-onlyコードをMIT・GPLv3へ再ライセンスしません。
- libusb 1.0.30のLGPL-2.1-or-later完全ソースアーカイブを同梱します。全helperソースと再リンク手順で静的リンクを再構築でき、単なる上流リンクではありません。USBTMC helperはMITのアプリコードとLGPL libraryです。GPLv2とLGPL-2.1-or-laterはこの組合せを許容します。
- libwdiはLGPL-3.0-or-laterで、独立MIT Windows setupから動的リンクします。完全ソース、変更、LGPL/GPL本文、置換手順を同梱し、GPLv2-only GPIB helperにはリンクしません。inbox-WinUSBビルドはMicrosoft kernel driver・WDK coinstallerを埋め込まずWindowsのWinUSBを使います。WDK・libusb0・libusbKバイナリ埋込みを除外します。
- libwdi commit `90278c538a8fb5fd82aab25ae7f5a9887ca468ce`は上流の2023-03-01 v1.5.0です。configure.acは上流でも1.3.1のままで、resource metadata・ChangeLogは1.5.0です。これは上流の不一致で別版の取り込みではありません。正確なcommitと変更ソースが由来を定義します。
- Rust依存物とフォントは個別の権利表記を持ち、対象プラットフォームで絞った依存索引と提供されたライセンス本文を同梱します。

一次資料は上の「Primary references」（2026-10-14確認）の全8件を参照してください。GNU GPL FAQのMereAggregation、GPL-2.0の2・3節、LGPL-2.1の6節、LGPL-3.0の4節、libusb v1.0.30、libwdiの該当commit、KAMEの該当tree、Linux v6.17 GPIBソースです。URL・参照revisionは英語原文と共通です。

### 82357Bファームウェア：権利未確認のため除外

旧downloaderは`fmhess/linux_gpib_firmware` commit `8a5c6c2c1a2adac4c770763a7236452a871a0113`の`measat_releaseX1.8.hex`を使っていました。リポジトリとAgilentディレクトリに手順とblobはありますが、そのファイルの再配布を明示的に認めるライセンスはありません。GitHub license endpointの404だけで禁止の証明にはなりませんが、積極的な許諾の根拠にもなりません。READMEはuploadと2段階bootの説明で、許諾ではありません。

Keysight IO Librariesの公開release notesは該当利用契約に従う使用を認めますが、このGitHub blobの再配布権を確立しません。他製品の一般EULAもこのファイルのライセンスとして扱いません。公開されていることとchecksumは著作権上の許可を意味しません。

書面による許可は得ておらず、利用者に代わる照会も送っていません。このためdownload、cached-file upload、override-path upload、初回boot-driver置換を削除します。初期化済み`0957:0718`は対応し、boot状態`0957:0518`はopen・uploadせず拒否します。NI HS+とdirect USBはこのfirmware経路を使いません。

firmwareの3つの参照URLは英語側「References」と共通です。該当linux_gpib_firmware tree、上流Agilent README、Keysight IO Libraries2024U2 release notesを参照してください。

権利者から適用可能な許諾が示されるまで、このblobの同梱、自動取得・uploadの再導入、権利確認済みとの主張をしません。配布処理は誤混入を防ぐため出力を検証します。
