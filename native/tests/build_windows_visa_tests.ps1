# SPDX-License-Identifier: MIT
# Build and run the VISA helper protocol tests with Visual Studio's x64 toolset.
# It compiles only into the supplied output directory and does not touch drivers.
[CmdletBinding()]
param(
    [string] $OutputDirectory = (Join-Path (Split-Path -Parent (Split-Path -Parent $PSScriptRoot)) 'build\visa-tests')
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$source = Join-Path $root 'native'
$mockSource = Join-Path $source 'tests\windows_visa_mock.cpp'
$helperSource = Join-Path $source 'visa_helper.cpp'
$runner = Join-Path $source 'tests\windows_visa_helper.ps1'
$vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
if (-not (Test-Path -LiteralPath $vswhere -PathType Leaf)) { throw "Visual Studio locator was not found: $vswhere" }
$installation = @(& $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath)
if ($installation.Count -ne 1 -or -not $installation[0]) { throw 'Visual Studio C++ x64 build tools were not found.' }
$devCmd = Join-Path $installation[0] 'Common7\Tools\VsDevCmd.bat'
if (-not (Test-Path -LiteralPath $devCmd -PathType Leaf)) { throw "Visual Studio developer command script was not found: $devCmd" }

# Import only the Visual Studio command environment into this process.  This is
# equivalent to opening the x64 Native Tools prompt and does not alter policy.
$environment = @(& cmd.exe /d /s /c "call `"$devCmd`" -arch=amd64 -host_arch=amd64 >nul && set")
foreach ($entry in $environment) {
    $equals = $entry.IndexOf('=')
    if ($equals -gt 0) {
        [Environment]::SetEnvironmentVariable($entry.Substring(0, $equals), $entry.Substring($equals + 1), 'Process')
    }
}
if (-not (Get-Command cl.exe -ErrorAction SilentlyContinue)) { throw 'cl.exe was not available after VsDevCmd.' }

New-Item -ItemType Directory -Force -Path $OutputDirectory | Out-Null
$out = (Resolve-Path -LiteralPath $OutputDirectory).Path
$mockDll = Join-Path $out 'windows_visa_mock.dll'
$production = Join-Path $out 'visa_helper_production.exe'
$mockHelper = Join-Path $out 'visa_helper_mock.exe'
$common = @('/nologo', '/std:c++17', '/W4', '/WX', '/EHsc', '/MT', '/D_CRT_SECURE_NO_WARNINGS')

& cl.exe @common '/LD' "/Fe$mockDll" "/Fo$(Join-Path $out 'windows_visa_mock.obj')" $mockSource
if ($LASTEXITCODE -ne 0) { throw "Mock VISA DLL compilation failed ($LASTEXITCODE)." }
& cl.exe @common "/Fe$production" "/Fo$(Join-Path $out 'visa_helper_production.obj')" $helperSource
if ($LASTEXITCODE -ne 0) { throw "Production VISA helper compilation failed ($LASTEXITCODE)." }

# A test-only forced header avoids compiler/PowerShell macro quote differences.
$dllLiteral = $mockDll.Replace('\', '\\')
$testHeader = Join-Path $out 'visa_test_runtime.h'
[IO.File]::WriteAllText($testHeader, '#define KOHDALAB_VISA_TEST_DLL L"' + $dllLiteral + '"')
& cl.exe @common "/FI$testHeader" "/Fe$mockHelper" "/Fo$(Join-Path $out 'visa_helper_mock.obj')" $helperSource
if ($LASTEXITCODE -ne 0) { throw "Mock VISA helper compilation failed ($LASTEXITCODE)." }

# Do not invoke the .ps1 as a file: this keeps the test independent of a user's
# execution policy while preserving its normal parameters and assertions.
$test = [ScriptBlock]::Create([IO.File]::ReadAllText($runner))
& $test -ProductionHelper $production -MockHelper $mockHelper
if ($LASTEXITCODE -ne 0) { throw "VISA helper test process failed ($LASTEXITCODE)." }
Write-Output "Built and tested VISA helpers in $out"
