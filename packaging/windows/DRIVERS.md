# Windows USB setup

The development preview first uses an installed 64-bit official VISA runtime
for instruments it enumerates. Manufacturer driver packages alone are not a
VISA runtime. Existing WinUSB devices use the native helpers as before. A busy
or failed VISA connection is reported; it is never retried through native I/O.

Connect supported instruments. If a device has no assigned driver, run
`support/USB-Setup.exe` and approve any Windows administrator/driver prompts.
The Rust GUI does not run the setup executable automatically. Afterwards,
launch `kohdalab-iv.exe`, click Refresh, select the
actual Source and Meter resources and connect them. No separate Python, VISA,
manufacturer installer or Zadig download is needed for these native connections.

Direct USB communication and GPIB communication with both adapters have been
reported working on the user's Windows instrument PC. The NI HS+ Windows fix was
verified with physical open-output GS210/34411A sweeps through GPIB and direct
USB on 2026-10-07 (three points, -1 to +1 mV, 1 mA hardware current limit).
Windows may request publisher/driver confirmation, a reconnect, or a restart.
A system administrator's policy may prohibit the generated driver package.

Run `support/USB-Setup.exe` again if a device driver was removed in Device
Manager. The setup tool can recover a missing NI communication child when its
USB parent is present. It verifies device startup/interface GUIDs and waits up
to 15 seconds for activation. Follow its reconnect/restart message if setup
fails, then retry Refresh in the GUI. Initial driver installation may take
longer than an already-configured check; installation requires Windows approval.

## Supported devices

| Device | USB vendor:product ID | Required mode |
| --- | --- | --- |
| YOKOGAWA GS210 (may appear as GS200) | 0B21:0039 | TMC |
| Agilent/Keysight 34411A | 0957:0A07 | USBTMC |
| NI GPIB-USB-HS+ | 3923:7618 | GPIB adapter |
| Agilent 82357B | 0957:0718 only | Already initialized under separately obtained firmware rights |

Set GS210 to TMC mode before connecting. Storage-mode devices, nonzero composite
interfaces and other USB IDs are excluded from this setup's selection. Do not
change to Storage mode while using the native measurement connection.
The NI HS+ Analyzer interface can remain without a driver; it is not used by
this application. The communication interface (MI_00) must use WinUSB.
Only one supported GPIB adapter may be connected at a time. GPIB0 uses controller
address 0 and scans listeners at primary addresses 1-30, including unregistered
models. Secondary addresses and multiple adapters are not supported.

## What initial setup does

- Checks the fixed supported-device allowlist and existing WinUSB interface GUID.
- Preserves every existing non-WinUSB driver assignment. It does not decide that
  a vendor driver is disposable merely because VISA cannot currently see it.
- Requests administrator privileges only when configuration is required.
- Uses a bundled, replaceable libwdi DLL to generate, sign and install a small
  device-specific package pointing to Windows' own WinUSB driver. It installs
  the generated publisher certificate in the machine certificate stores.
- Assigns the driver to the selected PnP instance, not every matching device.
- Configures only an already initialized 82357B (0957:0718). It never downloads
  or uploads firmware, configures boot 0957:0518, or stages a driver for firmware
  re-enumeration. Other supported devices are still configured if a boot
  82357B is connected. No Internet connection is needed for these setup steps.

The app and installer do not disable Windows driver-signature enforcement or
boot security settings. Setup does not replace an existing manufacturer driver. Older native-only
releases could assign WinUSB in its place; restore the manufacturer driver
manually if you want those devices to use official VISA. The original system
restore settings are preserved. No SCPI commands or measurement sweeps are
sent by USB setup.

Existing WinUSB configurations with an interface GUID are reused. To check or
retry manually, run `USB-Setup.exe` inside `support`. Keep `libwdi.dll` beside USB-Setup.exe. This is a one-time
USB configuration, not an installation of the KohdaLab application.

## Logs and excluded firmware

Setup logs are in `%LOCALAPPDATA%\KohdaLab IV`:

```text
usb-setup.log
usb-drivers/
```

The setup log records target device IDs and previous driver services. Windows
also records driver installation in its SetupAPI log. Keep exact error messages
if setup fails. Do not repeatedly start setup while a driver installation is
still running. Resolve pending Windows installation/restart requests first.

Firmware rights have not been established. Firmware files, automatic acquisition,
cache loading and upload are excluded, even if an earlier release left a file
on the user's PC. A boot adapter is refused with an explanatory message. Obtain
initialization separately through an appropriately licensed workflow; do not
add its files to this distribution. See REDISTRIBUTION.md and
SOURCE.zip/docs/redistribution_audit.md. No source output is enabled during
setup. GPIB identification later initializes IFC/REN.

## Instrument checks

With the GUI closed, verify complete identities before a measurement. Replace
GPIB addresses 3 and 7 with the instruments' actual addresses:

```powershell
& '.\support\diagnostics\usbtmc-probe.exe' --idn
& '.\support\diagnostics\gpib-probe.exe' --idn 3 7
```

Diagnostic probes are rebuilt from the same source, but may be available only
in the build kit rather than the minimal portable. For an 82357B, an initialized
0957:0718 device must already be present; this app cannot initialize boot
0957:0518. Use the app's Refresh
and select the actual resources rather than retaining example serials/addresses.
Hardware identification, measurement, Stop and output-off need Windows tests.

## Source and alternatives

Complete setup/helper/dependency source, licenses and rebuild scripts are in
`native` and `scripts` after extracting `support/SOURCE.zip`. The libwdi
modifications are documented in `native/vendor/libwdi/KOHDALAB-MODIFICATIONS.md`.
The LGPL DLL can be replaced with a compatible modified build.

Zadig is an optional manual fallback if the dedicated installer fails; it is not
required by the normal flow. Use only the listed USB IDs and TMC interfaces.
- libwdi: https://github.com/pbatard/libwdi
- WinUSB: https://learn.microsoft.com/en-us/windows-hardware/drivers/usbcon/winusb-installation
- Optional Zadig fallback: https://zadig.akeo.ie/

The 2026-10-07 installer shares deletion access when reading Windows SetupAPI
logs so log rotation does not block installation. On the instrument test PC,
full NI driver-package removal/reinstallation dropped from about 70 seconds to
about 3 seconds. Timing varies with Windows policy, approval, and hardware.
Normal startup reuses active drivers. Installer phases remain visible in the GUI.

---

# Windows USB セットアップ

開発プレビューは、列挙する機器に対して最初にインストール済み 64-bit official VISA runtime を使います。
メーカー driver package だけは VISA runtime ではありません。既存 WinUSB device は従来どおり native helpers を
使います。busy または失敗した VISA connection は報告し、native I/O で再試行しません。

対応機器を接続します。device に割当 driver がなければ `support/USB-Setup.exe` を実行し、Windows の
administrator/driver prompt を承認します。Rust GUI が setup executable を自動実行することはありません。
その後 `kohdalab-iv.exe` を起動し、Refresh を押して実際の Source/Meter resources を選び接続します。
これら native connections には別の Python、VISA、manufacturer installer、Zadig download は不要です。

direct USB と両 adapter 経由 GPIB communication は、利用者の Windows instrument PC で動作報告があります。
NI HS+ Windows fix は 2026-10-07、GPIB と direct USB を通す physical open-output GS210/34411A sweep
（3 points、-1 から +1 mV、1 mA hardware current limit）で確認しました。Windows は publisher/driver 確認、
reconnect、restart を求め得ます。system administrator policy は生成 driver package を禁止し得ます。

Device Manager で device driver を削除した場合は `support/USB-Setup.exe` を再実行します。USB parent があれば
setup tool は失われた NI communication child を復旧できます。device startup/interface GUID を確認し、activation を
最大 15 seconds 待ちます。setup が失敗したら reconnect/restart message に従ってから GUI の Refresh を再試行します。
初回 driver installation は設定済み check より長くかかることがあり、Windows approval が必要です。

## 対応デバイス

| デバイス | USB vendor:product ID | 必要モード |
| --- | --- | --- |
| YOKOGAWA GS210（GS200 と表示される場合あり） | 0B21:0039 | TMC |
| Agilent/Keysight 34411A | 0957:0A07 | USBTMC |
| NI GPIB-USB-HS+ | 3923:7618 | GPIB adapter |
| Agilent 82357B | 0957:0718 のみ | 別途得た firmware rights で初期化済み |

接続前に GS210 を TMC mode に設定します。storage-mode devices、nonzero composite interfaces、その他 USB IDs は
この setup の選択対象外です。native measurement connection 中に Storage mode へ変更しないでください。NI HS+
Analyzer interface は本アプリで使わないため driver なしで構いません。communication interface (MI_00) は WinUSB を
使う必要があります。一度に接続できる対応 GPIB adapter は一台です。GPIB0 は controller address 0 を使い、
未登録モデルを含む primary addresses 1-30 の listeners を scan します。secondary addresses と multiple adapters は
未対応です。

## 初期セットアップが行うこと

- 固定 supported-device allowlist と既存 WinUSB interface GUID を確認します。
- 既存の非 WinUSB driver assignment をすべて保持します。VISA が現在見えないからといって vendor driver を廃棄可能とは判断しません。
- configuration が必要な場合だけ administrator privileges を要求します。
- bundled で置換可能な libwdi DLL を用い、Windows 自身の WinUSB driver を指す小さい device-specific package を生成、署名、インストールします。生成 publisher certificate を machine certificate stores に入れます。
- すべての一致 device ではなく、選択した PnP instance に driver を割り当てます。
- 初期化済み 82357B (0957:0718) だけを設定します。firmware を download/upload せず、boot 0957:0518 を設定せず、firmware re-enumeration 用 driver を stage しません。boot 82357B 接続中でも他の対応 devices は設定します。これらの setup steps に Internet connection は不要です。

アプリと installer は Windows driver-signature enforcement や boot security settings を無効化しません。Setup は
既存 manufacturer driver を置換しません。古い native-only release はその代わりに WinUSB を割り当てることが
ありました。これらの device を official VISA で使うには manufacturer driver を手動復元してください。元の
system restore settings は保存されます。USB setup は SCPI commands や measurement sweeps を送信しません。
interface GUID を持つ既存 WinUSB configurations は再利用します。手動確認／再試行は `support` 内の `USB-Setup.exe`
を実行し、`libwdi.dll` を USB-Setup.exe の隣に保ちます。これは一回限りの USB configuration で、KohdaLab
application の installation ではありません。

## Logs と除外 firmware

Setup logs は `%LOCALAPPDATA%\KohdaLab IV` にあります。

```text
usb-setup.log
usb-drivers/
```

setup log は target device IDs と previous driver services を記録します。Windows も SetupAPI log に driver
installation を記録します。setup 失敗時は正確な error messages を保持します。driver installation 実行中に
setup を繰返し開始せず、pending Windows installation/restart requests を先に解決してください。

firmware rights は確立していません。以前の release が利用者 PC に file を残していても、firmware files、
automatic acquisition、cache loading、upload は除外します。boot adapter は説明 message と共に拒否します。
適切に licensed workflow で別途 initialization を得て、その files を本 distribution に追加しないでください。
`REDISTRIBUTION.md` と `SOURCE.zip/docs/redistribution_audit.md` を参照してください。setup 中に source output は
有効化せず、後の GPIB identification が IFC/REN を初期化します。

## 計測器確認

GUI を閉じた状態で測定前に完全な identities を確認します。GPIB addresses 3 と 7 は実際の instruments の
addresses へ置換してください。

```powershell
& '.\support\diagnostics\usbtmc-probe.exe' --idn
& '.\support\diagnostics\gpib-probe.exe' --idn 3 7
```

diagnostic probes は同じ source から再ビルドしますが、minimal portable でなく build kit にしかない場合があります。
82357B には初期化済み 0957:0718 device が既に必要で、本アプリは boot 0957:0518 を初期化できません。example
serials/addresses を残さず、app の Refresh と実際の resources を使います。hardware identification、measurement、
Stop、output-off には Windows tests が必要です。

## Source と代替手段

`support/SOURCE.zip` 展開後、完全な setup/helper/dependency source、licenses、rebuild scripts は `native` と
`scripts` にあります。libwdi modifications は `native/vendor/libwdi/KOHDALAB-MODIFICATIONS.md` に記録します。
LGPL DLL は互換のある modified build に置換できます。専用 installer が失敗する場合、Zadig は任意の manual
fallback で通常経路には不要です。listed USB IDs と TMC interfaces だけを使います。

- libwdi: https://github.com/pbatard/libwdi
- WinUSB: https://learn.microsoft.com/en-us/windows-hardware/drivers/usbcon/winusb-installation
- Optional Zadig fallback: https://zadig.akeo.ie/

2026-10-07 installer は Windows SetupAPI logs 読取り時に deletion access を共有するため、log rotation が
installation を block しません。instrument test PC では完全な NI driver-package removal/reinstallation が約
70 seconds から約 3 seconds へ短縮しました。timing は Windows policy、approval、hardware により変動します。
通常 startup は active drivers を再利用し、installer phases は GUI に表示されたままです。
