# Rebuilding and relinking native components

All paths below are relative to the root of the extracted `support/SOURCE.zip`.
The archives include application/helper source, dependency source, configuration,
local modifications and build scripts. Install compiler/build tools separately.
The generated binary's license depends on its components, not on the license
of the build script. Retain original notices when modifying these files.

## macOS Apple Silicon

With Xcode Command Line Tools and Make available:

```sh
bash scripts/build_native_gpib.sh
bash scripts/build_usbtmc_probe.sh
```

The outputs are in `build/native-gpib/` and `build/usbtmc-probe/`.
To use a modified libusb, first run these scripts to extract its source, edit
the respective `build/.../libusb-1.0.30/` tree, then rebuild the static library:

```sh
cd build/usbtmc-probe/libusb-1.0.30
make clean
make
cd ../../..
bash scripts/build_usbtmc_probe.sh
```

For the GPIB helper, edit `build/native-gpib/libusb-1.0.30/` and rerun
`scripts/build_native_gpib.sh`. The script builds that library and relinks the
helper. The supplied source archive's checksum protects extraction of the
original source; it does not reject edits in the already extracted build tree.
Ad hoc signing is included in the build scripts and needs no secret key.

Use an explicit replacement without modifying a signed app:

```sh
mkdir -p build/replacement-helpers
cp build/usbtmc-probe/kohdalab-usbtmc-helper build/replacement-helpers/
cp build/native-gpib/kohdalab-gpib-helper build/replacement-helpers/
export KOHDALAB_IV_HELPER_DIR="$PWD/build/replacement-helpers"
```

Launch the GUI/CLI from that environment. Alternatively replace the helper
inside `KohdaLab IV.app/Contents/MacOS/support/` and ad hoc re-sign the app.

## Windows x64

Use MSYS2 UCRT64 with GCC/G++, Make, Bash and binutils (`windres`):

```sh
bash scripts/build_native_windows.sh
bash scripts/build_usb_setup.sh
```

Outputs are `build/native-windows/` and `build/usb-setup/`.
For a modified libusb, edit `build/native-windows/libusb-1.0.30/`, run
`make clean && make` there, and rerun `scripts/build_native_windows.sh` to
relink the helpers with the changed library. Its full source is in
`native/vendor/libusb-1.0.30.tar.bz2`.

For a modified libwdi, edit `native/vendor/libwdi/` and run
`scripts/build_usb_setup.sh`. Replace `support/libwdi.dll` with a compatible
build; the setup executable dynamically imports that DLL. No application EULA
forbids debugging or replacing the LGPL library. Rebuild the MIT setup executable
as needed using the included source and script.

Set `KOHDALAB_IV_HELPER_DIR` to a directory containing both replacement helper
executables, or replace their files in portable `support/` after closing the
application. In PowerShell, use `$env:KOHDALAB_IV_HELPER_DIR = "C:\path\to\helpers"`.
The distributor's FILES.json records original files; locally modified builds
are expected to have different hashes.

## VISA helper test (Windows only)

`visa_helper.cpp` dynamically loads an already installed official 64-bit VISA
runtime from Windows System32. It does not need, import, or package a VISA SDK.
For a reproducible protocol test without a VISA installation or an instrument,
build the fake DLL and a separate test helper with an absolute DLL path:

```sh
x86_64-w64-mingw32-g++ -std=c++17 -Wall -Wextra -Werror -static \
  -shared native/tests/windows_visa_mock.cpp -o C:/kohdalab-visa-test/windows_visa_mock.dll
x86_64-w64-mingw32-g++ -std=c++17 -Wall -Wextra -Werror -static \
  '-DKOHDALAB_VISA_TEST_DLL=L"C:\\kohdalab-visa-test\\windows_visa_mock.dll"' \
  native/visa_helper.cpp -o C:/kohdalab-visa-test/visa_helper_mock.exe
```

The `KOHDALAB_VISA_TEST_DLL` macro is test-only; do not define it for a
portable release. It is a compile-time absolute path, not an environment
variable or a production DLL search path. Run the test alongside a normal
production helper:

```powershell
$script = [IO.File]::ReadAllText((Resolve-Path native/tests/windows_visa_helper.ps1))
& ([ScriptBlock]::Create($script)) `
  -ProductionHelper C:\path\to\kohdalab-visa-helper.exe `
  -MockHelper C:\kohdalab-visa-test\visa_helper_mock.exe
```

The script changes no driver bindings and sends no commands to real instruments.

---

# ネイティブコンポーネントの再ビルドと再リンク

以下のすべてのパスは、展開した `support/SOURCE.zip` のルートからの相対パスです。アーカイブには
application/helper source、dependency source、configuration、local modifications、build scripts を含みます。
compiler/build tools は別途インストールしてください。生成 binary の license は build script の license ではなく
components によって決まります。これらのファイルを変更する際は元の notices を保持します。

## macOS Apple Silicon

Xcode Command Line Tools と Make がある場合は次を実行します。

```sh
bash scripts/build_native_gpib.sh
bash scripts/build_usbtmc_probe.sh
```

出力先は `build/native-gpib/` と `build/usbtmc-probe/` です。変更した libusb を使う場合は、まずこれらの
scripts を実行して source を展開し、該当する `build/.../libusb-1.0.30/` tree を編集してから static library を
再ビルドします。

```sh
cd build/usbtmc-probe/libusb-1.0.30
make clean
make
cd ../../..
bash scripts/build_usbtmc_probe.sh
```

GPIB helper では `build/native-gpib/libusb-1.0.30/` を編集し、`scripts/build_native_gpib.sh` を再実行します。
script は library をビルドして helper を再リンクします。添付 source archive の checksum は元 source の展開を
保護しますが、既に展開された build tree 内の編集を拒否するものではありません。ad hoc signing は build scripts に
含まれ secret key を必要としません。

署名済み app を変更せずに明示的な置換を使うには次を実行します。

```sh
mkdir -p build/replacement-helpers
cp build/usbtmc-probe/kohdalab-usbtmc-helper build/replacement-helpers/
cp build/native-gpib/kohdalab-gpib-helper build/replacement-helpers/
export KOHDALAB_IV_HELPER_DIR="$PWD/build/replacement-helpers"
```

この環境から GUI/CLI を起動します。代替として `KohdaLab IV.app/Contents/MacOS/support/` 内の helper を置換して
app を ad hoc 再署名できます。

## Windows x64

GCC/G++、Make、Bash、binutils (`windres`) を備えた MSYS2 UCRT64 を使用します。

```sh
bash scripts/build_native_windows.sh
bash scripts/build_usb_setup.sh
```

出力は `build/native-windows/` と `build/usb-setup/` です。変更した libusb には
`build/native-windows/libusb-1.0.30/` を編集し、そこで `make clean && make` を実行してから
`scripts/build_native_windows.sh` を再実行し、変更 library で helpers を再リンクします。完全な source は
`native/vendor/libusb-1.0.30.tar.bz2` にあります。

変更した libwdi には `native/vendor/libwdi/` を編集し、`scripts/build_usb_setup.sh` を実行します。
`support/libwdi.dll` を互換のある build に置換します。setup executable はその DLL を dynamic import します。
アプリケーション EULA は LGPL library の debugging や置換を禁止しません。添付 source/script で必要に応じて
MIT setup executable を再ビルドしてください。

`KOHDALAB_IV_HELPER_DIR` を両置換 helper executable を含む directory に設定するか、application を閉じた後に
portable `support/` のファイルを置換します。PowerShell では
`$env:KOHDALAB_IV_HELPER_DIR = "C:\path\to\helpers"` を使用します。distributor の `FILES.json` は元ファイルを
記録するため、ローカル変更 build の hashes が異なるのは想定どおりです。

## VISA helper test（Windows のみ）

`visa_helper.cpp` は既にインストール済み official 64-bit VISA runtime を Windows System32 から dynamic load します。
VISA SDK を必要とせず import/package もしません。VISA installation や instrument なしに再現可能な protocol test を
行うには、fake DLL と absolute DLL path の別 test helper をビルドします。

```sh
x86_64-w64-mingw32-g++ -std=c++17 -Wall -Wextra -Werror -static \
  -shared native/tests/windows_visa_mock.cpp -o C:/kohdalab-visa-test/windows_visa_mock.dll
x86_64-w64-mingw32-g++ -std=c++17 -Wall -Wextra -Werror -static \
  '-DKOHDALAB_VISA_TEST_DLL=L"C:\\kohdalab-visa-test\\windows_visa_mock.dll"' \
  native/visa_helper.cpp -o C:/kohdalab-visa-test/visa_helper_mock.exe
```

`KOHDALAB_VISA_TEST_DLL` macro は test-only です。portable release に定義してはいけません。これは compile-time の
absolute path であり、environment variable や production DLL search path ではありません。通常の production helper と
並べて test を実行します。

```powershell
$script = [IO.File]::ReadAllText((Resolve-Path native/tests/windows_visa_helper.ps1))
& ([ScriptBlock]::Create($script)) `
  -ProductionHelper C:\path\to\kohdalab-visa-helper.exe `
  -MockHelper C:\kohdalab-visa-test\visa_helper_mock.exe
```

この script は driver bindings を変更せず、実計測器へコマンドを送信しません。
