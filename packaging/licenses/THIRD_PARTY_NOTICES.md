# Third-party notices — native portable

KohdaLab IV is a distribution of separately licensed native programs and
libraries. The root MIT license does not cover third-party or GPL-only code.
See REDISTRIBUTION.md for obligations and SOURCE.zip/native/REBUILDING.md for
rebuilding/relinking and replacing libraries.

- Rust application / CLI / thin Python control client: MIT; all target-specific
  dependency and font license notices are supplied in licenses/rust/. No Qt,
  PySide, PyInstaller, Python interpreter, NumPy or PyVISA runtime is bundled.
- kohdalab-gpib-helper[.exe]: GPL-2.0-only for the combined executable.
  KAME's userspace NI driver and Linux-GPIB permit GPLv2 or later; the Agilent
  adaptation permits GPLv2 only. Original attribution is retained in source.
  Upstream Linux-GPIB portions include Frank Mori Hess's work. GPL-2.0.txt and
  complete corresponding source/build scripts are supplied.
- kohdalab-usbtmc-helper[.exe]: MIT application source, statically linked with
  libusb 1.0.30 (LGPL-2.1-or-later, Copyright libusb contributors).
- libusb 1.0.30 is also statically linked by the GPIB helper. Its unmodified
  source archive, COPYING, helper source and relinking instructions are in
  SOURCE.zip. LGPL-2.1.txt is also supplied separately.
- Windows USB-Setup.exe: MIT application, dynamically linked with the replaceable
  libwdi.dll (LGPL-3.0-or-later, Pete Batard and contributors). Full libwdi source,
  configuration, dated changes and rebuild instructions are in SOURCE.zip.
  GPL-3.0.txt and LGPL-3.0.txt are supplied. This uses Windows' inbox WinUSB,
  not a bundled Microsoft kernel driver or WDK coinstaller.
- Windows MinGW runtime notices and GCC runtime exception are supplied in
  licenses/. System OS APIs are obtained from the installed operating system.

82357B firmware is NOT licensed by the above open-source licenses merely
because a GPL driver can upload it. This distribution does not include, obtain
or upload that firmware. Only an already initialized adapter is supported.
Do not add manufacturer firmware or vendor VISA installers to this package.

Keep SOURCE.zip, licenses, notices and REDISTRIBUTION.md with redistributed
binaries. If you modify a GPL/LGPL binary, provide its actual modified source
and update the modification notices. No manufacturer endorsement is implied.

---

# 第三者通知 — native portable（日本語）

KohdaLab IV は別々の license を持つ native program/library の distribution です。root MIT license は third-party/GPL-only code を対象にしません。obligation は REDISTRIBUTION.md、library の rebuild/relink/replace は SOURCE.zip/native/REBUILDING.md を参照してください。

- Rust application / CLI / thin Python control client: MIT。target-specific dependency/font の全 license notice は licenses/rust/ に供給します。Qt、PySide、PyInstaller、Python interpreter、NumPy、PyVISA runtime は同梱しません。
- kohdalab-gpib-helper[.exe]: combined executable は GPL-2.0-only。KAME userspace NI driver/Linux-GPIB は GPLv2-or-later、Agilent adaptation は GPLv2-only を許可します。original attribution は source に保持し、upstream Linux-GPIB portion は Frank Mori Hess の work を含みます。GPL-2.0.txt と complete corresponding source/build script を供給します。
- kohdalab-usbtmc-helper[.exe]: MIT application source。libusb 1.0.30（LGPL-2.1-or-later、Copyright libusb contributors）と static link します。
- libusb 1.0.30 は GPIB helper にも static link します。unmodified source archive、COPYING、helper source、relinking instruction は SOURCE.zip、LGPL-2.1.txt は別途供給します。
- Windows USB-Setup.exe: MIT application。replaceable libwdi.dll（LGPL-3.0-or-later、Pete Batard and contributors）と dynamic link します。full libwdi source/configuration/dated change/rebuild instruction は SOURCE.zip、GPL-3.0.txt/LGPL-3.0.txt は同梱します。bundled Microsoft kernel driver/WDK coinstaller ではなく Windows inbox WinUSB を使います。
- Windows MinGW runtime notice/GCC runtime exception は licenses/ に供給します。system OS API は installed operating system から取得します。

82357B firmware は GPL driver が upload できても上記 open-source license で license されるものではありません。この distribution は firmware を含めず、取得/upload もしません。既に initialized された adapter のみ対応します。manufacturer firmware/vendor VISA installer をこの package に追加してはいけません。

redistributed binary には SOURCE.zip、license、notice、REDISTRIBUTION.md を同梱してください。GPL/LGPL binary を変更した場合は実際の modified source を提供し modification notice を更新します。manufacturer endorsement は含意しません。
