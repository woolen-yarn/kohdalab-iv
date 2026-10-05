#!/usr/bin/env bash
# Rebuild the dedicated x64 installer and replaceable LGPL libwdi DLL.
set -euo pipefail
cd "$(dirname "$0")/.."
TASK_ROOT="$PWD"
TASK_BUILD="$TASK_ROOT/build/usb-setup"
TASK_SOURCE="$TASK_ROOT/native/vendor/libwdi"
TASK_CC="${KOHDALAB_WINDOWS_CC:-gcc}"
TASK_CXX="${KOHDALAB_WINDOWS_CXX:-g++}"
TASK_RC="${KOHDALAB_WINDOWS_RC:-windres}"
TASK_HOST_CC="${KOHDALAB_HOST_CC:-cc}"
mkdir -p "$TASK_BUILD"
cp -R "$TASK_SOURCE/libwdi" "$TASK_BUILD/"
cp "$TASK_SOURCE/config.h" "$TASK_BUILD/"
cd "$TASK_BUILD/libwdi"
"$TASK_CC" -O2 -static -s -DUNICODE installer.c -lsetupapi -lnewdev -lole32 -o installer_x64.exe
"$TASK_HOST_CC" -O2 -I.. embedder.c -o embedder
./embedder embedded.h
"$TASK_RC" -i libwdi.rc -o libwdi_rc.o
"$TASK_CC" -std=gnu11 -O2 -shared -static-libgcc -s -Wno-incompatible-pointer-types \
    -DLIBWDI_DLL_EXPORT -I.. logging.c tokenizer.c vid_data.c pki.c libwdi_dlg.c \
    libwdi.c libwdi_rc.o -lsetupapi -lole32 -lntdll -o ../libwdi.dll \
    -Wl,--out-implib,../libwdi.dll.a
cd ..
"$TASK_CXX" -std=c++17 -O2 -static -s -municode -mwindows \
    -Ilibwdi "$TASK_ROOT/native/windows_usb_setup.cpp" libwdi.dll.a \
    -ladvapi32 -lshell32 -lole32 -lwinhttp -lbcrypt -o USB-Setup.exe
echo "Built $TASK_BUILD/USB-Setup.exe and libwdi.dll"
