# Build the Rust desktop using a Windows Rust / Visual C++ development environment.
$ErrorActionPreference = 'Stop'
Set-Location $PSScriptRoot
cargo test --workspace --locked
if ($LASTEXITCODE -ne 0) { throw 'Rust tests failed' }
cargo build --release --locked
if ($LASTEXITCODE -ne 0) { throw 'Rust build failed' }
python ../scripts/package_rust_preview.py --platform windows-x64 --binaries-dir "$PSScriptRoot/target/release"
if ($LASTEXITCODE -ne 0) { throw 'Portable packaging failed' }
