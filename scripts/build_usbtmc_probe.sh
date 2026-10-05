#!/bin/bash
# Build the direct USB identification probe for macOS, without Python.
set -euo pipefail
cd "$(dirname "$0")/.."
TASK_ROOT="$PWD"
TASK_BUILD="$TASK_ROOT/build/usbtmc-probe"
mkdir -p "$TASK_BUILD"
cd "$TASK_BUILD"
cp "$TASK_ROOT/native/vendor/libusb-1.0.30.tar.bz2" .
echo 'fea36f34f9156400209595e300840767ab1a385ede1dc7ee893015aea9c6dbaf  libusb-1.0.30.tar.bz2' | shasum -a 256 -c -
if [ ! -d libusb-1.0.30 ]; then tar -xjf libusb-1.0.30.tar.bz2; fi
if [ ! -f libusb-1.0.30/libusb/.libs/libusb-1.0.a ]; then
    (cd libusb-1.0.30 && CFLAGS='-O2 -mmacosx-version-min=13.0' ./configure --disable-shared --enable-static && make -j4)
fi
clang++ -std=c++17 -O2 -Wall -Wextra -Werror -mmacosx-version-min=13.0 \
    -Ilibusb-1.0.30/libusb "$TASK_ROOT/native/usbtmc_probe.cpp" \
    libusb-1.0.30/libusb/.libs/libusb-1.0.a \
    -framework IOKit -framework CoreFoundation -framework Security -lpthread -o usbtmc-probe
codesign --force --sign - usbtmc-probe
./usbtmc-probe --self-test
clang++ -std=c++17 -O2 -Wall -Wextra -Werror -mmacosx-version-min=13.0 \
    -Ilibusb-1.0.30/libusb "$TASK_ROOT/native/usbtmc_helper.cpp" \
    libusb-1.0.30/libusb/.libs/libusb-1.0.a \
    -framework IOKit -framework CoreFoundation -framework Security -lpthread -o kohdalab-usbtmc-helper
codesign --force --sign - kohdalab-usbtmc-helper
./kohdalab-usbtmc-helper --self-test
echo "Built $TASK_BUILD/usbtmc-probe"
