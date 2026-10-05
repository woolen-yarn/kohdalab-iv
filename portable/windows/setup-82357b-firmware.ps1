# Download upstream 82357B firmware without redistributing the firmware binary.
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
if (-not $env:LOCALAPPDATA) { throw 'LOCALAPPDATA is unavailable.' }
$TaskDirectory = Join-Path $env:LOCALAPPDATA 'KohdaLab IV'
New-Item -ItemType Directory -Force -Path $TaskDirectory | Out-Null
$TaskTemporary = Join-Path $TaskDirectory ([guid]::NewGuid().ToString() + '.download')
try {
    [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
    Invoke-WebRequest -UseBasicParsing -TimeoutSec 60 -Uri 'https://raw.githubusercontent.com/fmhess/linux_gpib_firmware/8a5c6c2c1a2adac4c770763a7236452a871a0113/agilent_82357a/measat_releaseX1.8.hex' -OutFile $TaskTemporary
    $TaskExpected = '2c40213a3d0d8b7d7cc083b155a5ed094e29767214a9b8aa745486f35ef58664'
    if ((Get-FileHash -Algorithm SHA256 -LiteralPath $TaskTemporary).Hash.ToLowerInvariant() -ne $TaskExpected) {
        throw 'Firmware checksum mismatch. Existing firmware was not changed.'
    }
    Move-Item -LiteralPath $TaskTemporary -Destination (Join-Path $TaskDirectory 'measat_releaseX1.8.hex') -Force
    Write-Host '82357B firmware ready. Set up WinUSB for both adapter USB IDs as described in DRIVERS.md.'
} finally {
    if (Test-Path -LiteralPath $TaskTemporary) { Remove-Item -LiteralPath $TaskTemporary -Force }
}
