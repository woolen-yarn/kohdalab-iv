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
