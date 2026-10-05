#!/bin/bash
# Run on an Apple Silicon Mac with Xcode Command Line Tools and uv installed.
set -euo pipefail
cd "$(dirname "$0")/.."
mkdir -p build
exec > >(tee build/macos-portable-build.log) 2>&1
if [ "$(uname -m)" != arm64 ]; then
    echo 'This release builder requires an Apple Silicon Mac.'
    exit 1
fi
command -v uv >/dev/null || { echo 'Install uv from https://docs.astral.sh/uv/ and retry.'; exit 1; }
uv sync --extra gui --frozen
bash scripts/build_native_gpib.sh
bash scripts/build_usbtmc_probe.sh
uv run --extra gui --with pyinstaller==6.22.3 python scripts/build_standalone.py --target gui \
    --native-gpib-helper build/native-gpib/kohdalab-gpib-helper \
    --native-usb-helper build/usbtmc-probe/kohdalab-usbtmc-helper
uv run --extra gui python scripts/package_macos_release.py
echo 'Portable ZIP and checksum are ready in dist/distribution.'
open dist/distribution
