#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
bash scripts/build_native_gpib.sh
bash scripts/build_usbtmc_probe.sh
cargo test --workspace --locked
cargo build --release --locked
python3 scripts/package_rust_preview.py --platform macos-arm64
