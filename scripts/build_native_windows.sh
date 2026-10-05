#!/usr/bin/env bash
# Build Windows x64 native helpers in MSYS2 UCRT64, or cross-compile with MinGW.
set -euo pipefail
cd "$(dirname "$0")/.."
TASK_ROOT="$PWD"
TASK_BUILD="$TASK_ROOT/build/native-windows"
TASK_DRIVER="$TASK_ROOT/native/vendor/ni-gpib"
TASK_CC="${KOHDALAB_WINDOWS_CC:-gcc}"
TASK_CXX="${KOHDALAB_WINDOWS_CXX:-g++}"
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
TASK_LIBS=(libusb-1.0.30/libusb/.libs/libusb-1.0.a -lsetupapi -lcfgmgr32 -ladvapi32 -lole32 -lbcrypt)
"$TASK_CXX" -static -s obj/*.o "${TASK_LIBS[@]}" -o kohdalab-gpib-helper.exe
for source in usbtmc_probe usbtmc_helper; do
    name=usbtmc-probe
    if [ "$source" = usbtmc_helper ]; then name=kohdalab-usbtmc-helper; fi
    "$TASK_CXX" -std=c++17 -O2 -Wall -Wextra -Werror -static -s \
        -Ilibusb-1.0.30/libusb "$TASK_ROOT/native/$source.cpp" \
        "${TASK_LIBS[@]}" -o "$name.exe"
done
cp kohdalab-gpib-helper.exe gpib-probe.exe
echo "Built Windows helpers in $TASK_BUILD"
