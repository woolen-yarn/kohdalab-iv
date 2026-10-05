# Build the GUI and CLI, validate them, and assemble the Windows portable ZIP.
$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest
$TaskRoot = Split-Path -Parent $PSScriptRoot
if ($env:OS -ne "Windows_NT") { throw "Run this script on a Windows x64 PC." }
if (-not (Get-Command uv -ErrorAction SilentlyContinue)) {
    throw "uv is required on the build PC. Install it with: winget install --id astral-sh.uv -e; then reopen PowerShell."
}
Set-Location $TaskRoot
New-Item -ItemType Directory -Force -Path "build" | Out-Null
$TaskLog = Join-Path $TaskRoot "build\windows-portable-build.log"
Start-Transcript -Path $TaskLog -Force | Out-Null
try {
    & uv sync --extra gui --frozen --python 3.13
    if ($LASTEXITCODE -ne 0) { throw "Dependency installation failed. See $TaskLog" }
    & uv run --no-sync python -c "import platform,struct,sys; assert sys.platform == 'win32' and struct.calcsize('P') == 8 and platform.machine().upper() in ('AMD64','X86_64'), 'Use Windows x64 and x64 Python'"
    if ($LASTEXITCODE -ne 0) { throw "The build requires x64 Python on Windows." }
    & uv run --no-sync python scripts/windows_native.py --execute
    if ($LASTEXITCODE -ne 0) { throw "Native helper validation failed. See $TaskLog" }
    & uv run --extra gui --with pyinstaller==6.22.3 python scripts/build_standalone.py --target all --native-gpib-helper portable/windows/native/kohdalab-gpib-helper.exe --native-usb-helper portable/windows/native/kohdalab-usbtmc-helper.exe
    if ($LASTEXITCODE -ne 0) { throw "EXE build failed. See $TaskLog" }
    & uv run --no-sync python scripts/package_windows_portable.py
    if ($LASTEXITCODE -ne 0) { throw "Portable validation or packaging failed. See $TaskLog" }
    Write-Host "Done. The release ZIP and checksum are under dist\releases."
} finally {
    Stop-Transcript | Out-Null
}
