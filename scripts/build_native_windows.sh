#!/usr/bin/env bash
# Build Windows x64 native helpers in MSYS2 UCRT64, or cross-compile with MinGW.
set -euo pipefail
cd "$(dirname "$0")/.."
TASK_ROOT="$PWD"
TASK_BUILD="$TASK_ROOT/build/native-windows"
TASK_DRIVER="$TASK_ROOT/native/vendor/ni-gpib"
TASK_CC="${KOHDALAB_WINDOWS_CC:-gcc}"
TASK_CXX="${KOHDALAB_WINDOWS_CXX:-g++}"
TASK_RC="${KOHDALAB_WINDOWS_RC:-${TASK_CC%gcc}windres}"
TASK_PYTHON="${KOHDALAB_PYTHON:-python3}"
TASK_ICON="$TASK_ROOT/next/assets/kohdalab-iv.ico"
TASK_LIBUSB="$TASK_ROOT/native/vendor/libusb-1.0.30.tar.bz2"
TASK_DIGEST=$(sha256sum "$TASK_LIBUSB")
if [ "${TASK_DIGEST%% *}" != fea36f34f9156400209595e300840767ab1a385ede1dc7ee893015aea9c6dbaf ]; then
    echo 'libusb source checksum mismatch' >&2
    exit 1
fi
mkdir -p "$TASK_BUILD/obj" "$TASK_BUILD/include/libusb-1.0"
cd "$TASK_BUILD"
if [ ! -d libusb-1.0.30 ]; then tar -xjf "$TASK_LIBUSB"; fi
if [ ! -f libusb-1.0.30/libusb/.libs/libusb-1.0.a ]; then
    (cd libusb-1.0.30 && CC="$TASK_CC" CXX="$TASK_CXX" ./configure \
        --host=x86_64-w64-mingw32 --disable-shared --enable-static && make -j4)
fi
cp libusb-1.0.30/libusb/libusb.h include/libusb-1.0/
TASK_COMMON=(-O2 -I"$TASK_DRIVER" -I"$TASK_DRIVER/linux-gpib" -Iinclude)
"$TASK_CC" "${TASK_COMMON[@]}" -std=gnu11 -c "$TASK_DRIVER/linux-gpib/ni_usb_gpib.c" -o obj/ni_usb_gpib.o
"$TASK_CC" "${TASK_COMMON[@]}" -std=gnu11 -c "$TASK_DRIVER/gpib_stubs.c" -o obj/gpib_stubs.o
"$TASK_CXX" "${TASK_COMMON[@]}" -std=gnu++17 -c "$TASK_DRIVER/NiGpibDriver.cpp" -o obj/NiGpibDriver.o
"$TASK_CXX" "${TASK_COMMON[@]}" -std=gnu++17 -c "$TASK_ROOT/native/agilent_82357b.cpp" -o obj/agilent_82357b.o
"$TASK_CXX" "${TASK_COMMON[@]}" -std=gnu++17 -c "$TASK_ROOT/native/kohdalab_gpib_helper.cpp" -o obj/main.o
TASK_LIBS=(libusb-1.0.30/libusb/.libs/libusb-1.0.a -lsetupapi -lcfgmgr32 -ladvapi32 -lole32)
build_resource() {
    local name="$1" description="$2"
    "$TASK_PYTHON" "$TASK_ROOT/scripts/windows_version_resource.py" \
        --output "obj/$name.rc" --description "$description" \
        --original-filename "$name.exe" --internal-name "$name" --icon "$TASK_ICON"
    "$TASK_RC" -O coff -i "obj/$name.rc" -o "obj/$name.res.o"
}
build_resource kohdalab-gpib-helper 'KohdaLab IV GPIB Helper'
"$TASK_CXX" -static -s obj/main.o obj/ni_usb_gpib.o obj/gpib_stubs.o obj/NiGpibDriver.o obj/agilent_82357b.o obj/kohdalab-gpib-helper.res.o "${TASK_LIBS[@]}" -o kohdalab-gpib-helper.exe
build_resource gpib-probe 'KohdaLab IV GPIB Probe'
"$TASK_CXX" -static -s obj/main.o obj/ni_usb_gpib.o obj/gpib_stubs.o obj/NiGpibDriver.o obj/agilent_82357b.o obj/gpib-probe.res.o "${TASK_LIBS[@]}" -o gpib-probe.exe
for source in usbtmc_probe usbtmc_helper; do
    name=usbtmc-probe
    description='KohdaLab IV USBTMC Probe'
    if [ "$source" = usbtmc_helper ]; then name=kohdalab-usbtmc-helper; description='KohdaLab IV USBTMC Helper'; fi
    build_resource "$name" "$description"
    "$TASK_CXX" -std=c++17 -O2 -Wall -Wextra -Werror -static -s \
        -Ilibusb-1.0.30/libusb "$TASK_ROOT/native/$source.cpp" \
        "obj/$name.res.o" "${TASK_LIBS[@]}" -o "$name.exe"
done
build_resource kohdalab-visa-helper 'KohdaLab IV VISA Helper'
"$TASK_CXX" -std=c++17 -O2 -Wall -Wextra -Werror -static -s \
    "$TASK_ROOT/native/visa_helper.cpp" obj/kohdalab-visa-helper.res.o -o kohdalab-visa-helper.exe
echo "Built Windows helpers in $TASK_BUILD"
