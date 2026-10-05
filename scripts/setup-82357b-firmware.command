#!/bin/bash
# Download the 82357B firmware directly from the Linux-GPIB maintainer.
# The binary is deliberately not redistributed inside the application.
set -euo pipefail
TASK_DIRECTORY="$HOME/Library/Application Support/KohdaLab IV"
mkdir -p "$TASK_DIRECTORY"
TASK_TEMP=$(mktemp "$TASK_DIRECTORY/.82357b-firmware.XXXXXX")
trap 'rm -f "$TASK_TEMP"' EXIT
curl --fail --location --connect-timeout 15 --max-time 60 \
    https://raw.githubusercontent.com/fmhess/linux_gpib_firmware/8a5c6c2c1a2adac4c770763a7236452a871a0113/agilent_82357a/measat_releaseX1.8.hex \
    --output "$TASK_TEMP"
TASK_EXPECTED=2c40213a3d0d8b7d7cc083b155a5ed094e29767214a9b8aa745486f35ef58664
TASK_ACTUAL=$(shasum -a 256 "$TASK_TEMP")
if [ "${TASK_ACTUAL%% *}" != "$TASK_EXPECTED" ]; then
    echo "Firmware checksum mismatch. Existing firmware was not changed." >&2
    exit 1
fi
mv "$TASK_TEMP" "$TASK_DIRECTORY/measat_releaseX1.8.hex"
echo "82357B firmware ready. Connect only the 82357B adapter, then open KohdaLab IV and click Refresh."
