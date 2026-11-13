#!/bin/bash
# Build a separate GPL helper. Requires clang and make on macOS.
set -euo pipefail
cd "$(dirname "$0")/.."
ROOT="$PWD"
TASK_BUILD="$ROOT/build/native-gpib"
mkdir -p "$TASK_BUILD"
DRIVER="$ROOT/native/vendor/ni-gpib"
cd "$TASK_BUILD"
cp "$ROOT/native/vendor/libusb-1.0.30.tar.bz2" .
echo 'fea36f34f9156400209595e300840767ab1a385ede1dc7ee893015aea9c6dbaf  libusb-1.0.30.tar.bz2' | shasum -a 256 -c -
if [ ! -d libusb-1.0.30 ]; then tar -xjf libusb-1.0.30.tar.bz2; fi
(cd libusb-1.0.30 && CFLAGS='-O2 -mmacosx-version-min=13.0' ./configure --disable-shared --enable-static && make -j4)
mkdir -p include/libusb-1.0 obj
cp libusb-1.0.30/libusb/libusb.h include/libusb-1.0/
COMMON=(-mmacosx-version-min=13.0 -I"$DRIVER" -I"$DRIVER/linux-gpib" -Iinclude -Wno-unused-function -Wno-visibility)
clang "${COMMON[@]}" -std=gnu11 -c "$DRIVER/linux-gpib/ni_usb_gpib.c" -o obj/ni_usb_gpib.o
clang "${COMMON[@]}" -std=gnu11 -c "$DRIVER/gpib_stubs.c" -o obj/gpib_stubs.o
clang++ "${COMMON[@]}" -std=gnu++17 -c "$DRIVER/NiGpibDriver.cpp" -o obj/NiGpibDriver.o
clang++ "${COMMON[@]}" -std=gnu++17 -c "$ROOT/native/agilent_82357b.cpp" -o obj/agilent_82357b.o
clang++ "${COMMON[@]}" -std=gnu++17 -c "$ROOT/native/kohdalab_gpib_helper.cpp" -o obj/main.o
clang++ -mmacosx-version-min=13.0 obj/*.o libusb-1.0.30/libusb/.libs/libusb-1.0.a -framework IOKit -framework CoreFoundation -framework Security -lpthread -o kohdalab-gpib-helper
codesign --force --sign - kohdalab-gpib-helper
echo "Built $TASK_BUILD/kohdalab-gpib-helper"
