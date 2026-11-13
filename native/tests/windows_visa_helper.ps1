# SPDX-License-Identifier: MIT
# Tests the isolated VISA helper against its test-only fake VISA runtime.
# This script does not compile code, change drivers, or touch instruments.
#
# Build MockHelper with a compile-time *absolute* DLL path, for example:
#   -DKOHDALAB_VISA_TEST_DLL='L"C:\\kohdalab-visa-test\\windows_visa_mock.dll"'
# Then pass the resulting helper and the ordinary production helper below.
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)] [ValidateScript({ Test-Path -LiteralPath $_ -PathType Leaf })] [string] $ProductionHelper,
    [Parameter(Mandatory = $true)] [ValidateScript({ Test-Path -LiteralPath $_ -PathType Leaf })] [string] $MockHelper
)

$ErrorActionPreference = 'Stop'
$usb = 'USB0::0x0957::0x0A07::MOCK34411::INSTR'
$gpib = 'GPIB0::7::INSTR'

function Require([bool] $condition, [string] $message) {
    if (-not $condition) { throw "Assertion failed: $message" }
}
function Decode-Hex([string] $hex) {
    if ($hex.Length % 2 -ne 0 -or $hex -notmatch '^[0-9a-f]*$') { throw "Invalid helper hex: $hex" }
    $bytes = [byte[]]::new($hex.Length / 2)
    for ($i = 0; $i -lt $bytes.Length; ++$i) { $bytes[$i] = [Convert]::ToByte($hex.Substring($i * 2, 2), 16) }
    return [Text.Encoding]::UTF8.GetString($bytes)
}
function Parse-Frames([object[]] $output) {
    $frames = @()
    foreach ($line in $output) {
        $parts = ([string] $line).Split("`t", 3)
        Require ($parts.Count -eq 3 -and $parts[0] -eq 'KIV') "invalid KIV frame: $line"
        $frames += [PSCustomObject]@{ Status = $parts[1]; Payload = (Decode-Hex $parts[2]) }
    }
    return $frames
}
function Invoke-Session([string] $helper, [string] $resource, [string[]] $requests) {
    return @(Parse-Frames (@($requests | & $helper $resource)))
}
function Require-Ok([object] $frame, [string] $context) {
    Require ($frame.Status -eq 'OK') "$context expected KIV OK, got $($frame.Status): $($frame.Payload)"
}
function Require-Err([object] $frame, [string] $fragment) {
    Require ($frame.Status -eq 'ERR') "expected KIV ERR, got $($frame.Status)"
    Require ($frame.Payload.Contains($fragment)) "error did not contain '$fragment': $($frame.Payload)"
}

# A normal build may have no VISA runtime or a vendor runtime with any set of
# attached devices.  Its outputs must still have the documented, safe shape.
$available = @(& $ProductionHelper --available)
Require ($available.Count -eq 1 -and ($available[0] -eq '0' -or $available[0] -eq '1')) 'production --available must print exactly 0 or 1'
$productionList = @(Parse-Frames (@(& $ProductionHelper --list)))
Require ($productionList.Count -eq 1) 'production --list must emit one KIV frame'
Require-Ok $productionList[0] 'production --list'

$mockAvailable = @(& $MockHelper --available)
Require ($mockAvailable.Count -eq 1 -and $mockAvailable[0] -eq '1') 'mock --available must print 1'
$listed = @(Parse-Frames (@(& $MockHelper --list)))
Require ($listed.Count -eq 1) 'mock --list must emit one KIV frame'
Require-Ok $listed[0] 'mock --list'
Require ($listed[0].Payload -eq "USB0::0x0957::0x0A07::MOCK34411::INSTR`tOfficial VISA`nGPIB0::7::INSTR`tOfficial VISA`n") 'six-field USB VISA resource was not canonicalized'

foreach ($resource in @($usb, $gpib)) {
    $frames = Invoke-Session $MockHelper $resource @(
        'QUERY 0 1000 2a49444e3f0a', # *IDN?\n
        'CLEAR 0 1000 -',
        'LOCAL 0 1000 -',
        'REN 0 1000 30', # ASCII "0"
        'EXIT 0 1000 -'
    )
    Require ($frames.Count -eq 6) "$resource expected startup plus five responses"
    Require-Ok $frames[0] "$resource startup"
    Require-Ok $frames[1] "$resource query"
    Require ($frames[1].Payload -eq "MOCK,VISA,1,1`n") "$resource query payload"
    2..5 | ForEach-Object { Require-Ok $frames[$_] "$resource operation $_" }
}

$shortWrite = Invoke-Session $MockHelper $usb @(
    'WRITE 0 1000 53484f525457524954450a', # SHORTWRITE\n
    'QUERY 0 1000 2a49444e3f0a',
    'EXIT 0 1000 -'
)
Require-Ok $shortWrite[0] 'short-write startup'
Require-Err $shortWrite[1] 'fewer bytes'
Require-Err $shortWrite[2] 'previous VISA transfer failed'
Require-Ok $shortWrite[3] 'short-write exit'

$timeout = Invoke-Session $MockHelper $usb @(
    'QUERY 0 1000 54494d454f55543f0a', # TIMEOUT?\n
    'READ 0 1000 -',
    'EXIT 0 1000 -'
)
Require-Ok $timeout[0] 'timeout startup'
Require-Err $timeout[1] 'VISA read failed'
Require-Err $timeout[2] 'previous VISA transfer failed'
Require-Ok $timeout[3] 'timeout exit'

$oversize = Invoke-Session $MockHelper $usb @(
    'QUERY 0 1000 4f56455253495a453f0a', # OVERSIZE?\n
    'READ 0 1000 -',
    'EXIT 0 1000 -'
)
Require-Ok $oversize[0] 'oversize startup'
Require-Err $oversize[1] '16 KiB limit'
Require-Err $oversize[2] 'previous VISA transfer failed'
Require-Ok $oversize[3] 'oversize exit'

$tooLong = 'WRITE 0 1000 ' + ('00' * 32768)
$oversizeLine = Invoke-Session $MockHelper $usb @($tooLong, 'EXIT 0 1000 -')
Require-Ok $oversizeLine[0] 'oversize-line startup'
Require-Err $oversizeLine[1] 'exceeds 64 KiB'
Require-Ok $oversizeLine[2] 'oversize-line exit'

Write-Output 'windows_visa_helper.ps1 passed.'
