# SPDX-License-Identifier: MIT
# CI-only Setup.exe lifecycle check: silent install, payload hash comparison
# against the portable ZIP, --version plus a simulated measurement, silent
# uninstall and removal check. No instrument is opened and no driver is
# installed (USB-Setup.exe is only hashed, never run).
#
# Setup.exe deliberately ignores /D and always installs to
# "%ProgramFiles%\Kohdalab\KohdaLab IV". This script therefore refuses to run
# outside a disposable GitHub-hosted runner. All working files (extracted
# portable payload, simulated CSV, report) stay in a fresh temporary directory.
[CmdletBinding()]
param(
 [Parameter(Mandatory=$true)][string]$SetupPath,
 [Parameter(Mandatory=$true)][string]$PortableZip,
 [Parameter(Mandatory=$true)][string]$ExpectedVersion,
 [Parameter(Mandatory=$true)][string]$ReportPath
)
$ErrorActionPreference='Stop'
$ProgressPreference='SilentlyContinue'
function Require($condition,[string]$message){if(!$condition){throw $message}}
if($env:GITHUB_ACTIONS -ne 'true' -or $env:RUNNER_ENVIRONMENT -ne 'github-hosted'){throw 'This check installs into Program Files; run it only on a disposable GitHub-hosted runner.'}

$setup=[IO.Path]::GetFullPath($SetupPath)
$zip=[IO.Path]::GetFullPath($PortableZip)
$report=[IO.Path]::GetFullPath($ReportPath)
$tempRoot=if($env:RUNNER_TEMP){$env:RUNNER_TEMP}else{[IO.Path]::GetTempPath()}
$work=Join-Path $tempRoot ('iv-setup-lifecycle-'+[Guid]::NewGuid().ToString('N'))
$install=Join-Path ([Environment]::GetFolderPath('ProgramFiles')) 'Kohdalab\KohdaLab IV'
$key='HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\KohdaLabIV'
$shortcuts=@((Join-Path ([Environment]::GetFolderPath('CommonDesktopDirectory')) 'KohdaLab IV.lnk'),(Join-Path ([Environment]::GetFolderPath('CommonPrograms')) 'KohdaLab IV\KohdaLab IV.lnk'))
# The same first-party binaries uploaded as signing input.
$firstParty=@('kohdalab-iv.exe','iv-cli.exe','support\kohdalab-usbtmc-helper.exe','support\kohdalab-visa-helper.exe','support\USB-Setup.exe')
$checks=[Collections.Generic.List[string]]::new()
$stage='prepare'

function Driver-Snapshot {
 # Driver packages and device bindings; must be unchanged by Setup and uninstall.
 $packages=@(Get-CimInstance Win32_PnPSignedDriver | Sort-Object DeviceID,InfName | ForEach-Object {"$($_.DeviceID)|$($_.InfName)|$($_.DriverVersion)"})
 return ($packages -join "`n")
}
function Hash([string]$path){(Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant()}
function Read-Manifest([string]$directory){
 $table=@{}
 $json=Get-Content -LiteralPath (Join-Path $directory 'FILES.json') -Raw | ConvertFrom-Json
 foreach($entry in $json.PSObject.Properties){$table[$entry.Name.Replace('/','\')]=([string]$entry.Value).ToLowerInvariant()}
 return $table
}
function Wait-Removed([int]$seconds){
 $deadline=[DateTime]::UtcNow.AddSeconds($seconds)
 while(((Test-Path -LiteralPath $install) -or (Test-Path -LiteralPath $key)) -and [DateTime]::UtcNow -lt $deadline){Start-Sleep -Milliseconds 500}
}

try {
 New-Item -ItemType Directory -Path $work | Out-Null
 Require (Test-Path -LiteralPath $setup -PathType Leaf) "Setup not found: $setup"
 Require (Test-Path -LiteralPath $zip -PathType Leaf) "Portable ZIP not found: $zip"
 Require (!(Test-Path -LiteralPath $install)) 'Program Files installation already exists; runner is not clean.'
 Require (!(Test-Path -LiteralPath $key)) 'Windows uninstall registration already exists; runner is not clean.'
 $drivers=Driver-Snapshot

 $stage='extract portable reference'
 $portableRoot=Join-Path $work 'portable'
 Expand-Archive -LiteralPath $zip -DestinationPath $portableRoot
 $portable=@(Get-ChildItem -LiteralPath $portableRoot -Directory)
 Require ($portable.Count -eq 1) 'Portable ZIP must contain exactly one top-level folder.'
 $portable=$portable[0].FullName
 $portableManifest=Read-Manifest $portable
 foreach($name in $portableManifest.Keys){Require ((Hash (Join-Path $portable $name)) -eq $portableManifest[$name]) "Portable ZIP content differs from its FILES.json: $name"}
 foreach($name in $firstParty){Require $portableManifest.ContainsKey($name) "Portable FILES.json lacks first-party binary: $name"}
 $checks.Add("Portable ZIP extracted and all $($portableManifest.Count) files match its FILES.json")

 $stage='silent install'
 # /D is an existing NSIS option that this Setup intentionally ignores; pass a
 # temporary target to prove that a caller cannot redirect the installation.
 $ignoredTarget=Join-Path $work 'ignored-install-target'
 $p=Start-Process -FilePath $setup -ArgumentList @('/S',"/D=$ignoredTarget") -PassThru
 if(!$p.WaitForExit(300000)){try{$p.Kill()}catch{};throw 'Setup did not finish within 5 minutes.'}
 Require ($p.ExitCode -eq 0) "Setup exit code $($p.ExitCode)."
 Require (!(Test-Path -LiteralPath $ignoredTarget)) 'Setup wrote to the caller-supplied /D path.'
 Require (Test-Path -LiteralPath $install -PathType Container) 'Installation directory missing after Setup.'
 $reg=Get-ItemProperty -LiteralPath $key
 Require ($reg.DisplayVersion -eq $ExpectedVersion) "Registered version $($reg.DisplayVersion) != $ExpectedVersion."
 Require ($reg.InstallLocation -eq $install) "Registered location $($reg.InstallLocation) != $install."
 foreach($shortcut in $shortcuts){Require (Test-Path -LiteralPath $shortcut) "Shortcut missing: $shortcut"}
 $checks.Add('Setup /S exited 0, ignored /D, registered the expected version and created shortcuts')

 $stage='payload hashes'
 $installedManifest=Read-Manifest $install
 foreach($name in $installedManifest.Keys){Require ((Hash (Join-Path $install $name)) -eq $installedManifest[$name]) "Installed file differs from installed FILES.json: $name"}
 foreach($name in $portableManifest.Keys){
  $target=Join-Path $install $name
  Require (Test-Path -LiteralPath $target -PathType Leaf) "Installed payload lacks portable file: $name"
  Require ((Hash $target) -eq $portableManifest[$name]) "Installed file differs from portable ZIP: $name"
 }
 $firstPartyHashes=[ordered]@{}
 foreach($name in $firstParty){$firstPartyHashes[$name.Replace('\','/')]=Hash (Join-Path $install $name)}
 $checks.Add("Installed first-party binaries ($($firstParty.Count)) and all portable files are byte-identical to the portable ZIP")

 $stage='installed version'
 $cli=Join-Path $install 'iv-cli.exe'
 $version=(& $cli --version 2>&1 | Out-String).Trim()
 Require ($LASTEXITCODE -eq 0) "iv-cli --version exit code $LASTEXITCODE."
 Require ($version -eq "KohdaLab IV $ExpectedVersion") "Unexpected version output: $version"
 $checks.Add("Installed iv-cli --version: $version")

 $stage='simulated measurement'
 $csv=Join-Path $work 'simulated measurement.csv'
 $simulation=(& $cli --measure --simulate --output $csv 2>&1 | Out-String).Trim()
 Require ($LASTEXITCODE -eq 0) "Simulated measurement failed ($LASTEXITCODE): $simulation"
 Require (Test-Path -LiteralPath $csv -PathType Leaf) 'Simulated measurement wrote no CSV.'
 Require (@(Get-Content -LiteralPath $csv | Where-Object {$_.Trim()}).Count -ge 2) 'Simulated measurement CSV has no data rows.'
 $checks.Add('Installed iv-cli --measure --simulate completed and wrote a CSV (no instrument opened)')

 $stage='silent uninstall'
 $deadline=[DateTime]::UtcNow.AddSeconds(30)
 while(@(Get-Process -Name kohdalab-iv,iv-cli -ErrorAction SilentlyContinue | Where-Object {$_.Path -and $_.Path.StartsWith($install+'\',[StringComparison]::OrdinalIgnoreCase)}).Count -and [DateTime]::UtcNow -lt $deadline){Start-Sleep -Milliseconds 250}
 $installedFiles=@(Get-ChildItem -LiteralPath $install -Recurse -File | ForEach-Object FullName)
 $u=Start-Process -FilePath (Join-Path $install 'Uninstall.exe') -ArgumentList '/S' -Wait -PassThru
 Require ($u.ExitCode -eq 0) "Uninstall.exe exit code $($u.ExitCode)."
 # NSIS re-launches the uninstaller from a temporary copy; wait for it to finish.
 Wait-Removed 180
 Require (!(Test-Path -LiteralPath $install)) 'Installation directory remains after uninstall.'
 Require (!(Test-Path -LiteralPath $key)) 'Windows uninstall registration remains after uninstall.'
 foreach($file in $installedFiles){Require (!(Test-Path -LiteralPath $file)) "Installed file remains: $file"}
 foreach($shortcut in $shortcuts){Require (!(Test-Path -LiteralPath $shortcut)) "Shortcut remains: $shortcut"}
 $vendor=Split-Path -Parent $install
 if(Test-Path -LiteralPath $vendor){Require (!@(Get-ChildItem -LiteralPath $vendor -Directory -Force -Filter '.KohdaLab-IV-uninstall-*')) 'Uninstall left a quarantine folder.'}
 $checks.Add("Uninstall /S removed all $($installedFiles.Count) installed files, registration and shortcuts")

 $stage='driver state'
 Require ((Driver-Snapshot) -eq $drivers) 'Driver packages or device bindings changed during the lifecycle.'
 $checks.Add('Windows driver bindings unchanged (no driver installed)')

 New-Item -ItemType Directory -Force -Path (Split-Path -Parent $report) | Out-Null
 [ordered]@{result='passed';version=$ExpectedVersion;setup=(Split-Path -Leaf $setup);setupSha256=(Hash $setup);portableZip=(Split-Path -Leaf $zip);installLocation=$install;firstPartySha256=$firstPartyHashes;checks=@($checks)} | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath $report -Encoding UTF8
 Write-Output (@($checks) -join "`n")
} catch {
 $failure=$_
 $logs=Join-Path $env:LOCALAPPDATA 'KohdaLab IV Installer'
 foreach($log in @('setup.log','uninstall.log')){$path=Join-Path $logs $log;if(Test-Path -LiteralPath $path){Write-Host "----- $log -----";Get-Content -LiteralPath $path | Write-Host}}
 try {
  New-Item -ItemType Directory -Force -Path (Split-Path -Parent $report) | Out-Null
  [ordered]@{result='failed';stage=$stage;error=$failure.Exception.Message;completed=@($checks)} | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath $report -Encoding UTF8
 } catch {}
 throw $failure
} finally {
 if(Test-Path -LiteralPath $work){Remove-Item -LiteralPath $work -Recurse -Force -ErrorAction SilentlyContinue}
}
