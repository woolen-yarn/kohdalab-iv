[CmdletBinding()]
param(
 [Parameter(Mandatory=$true)][string]$SetupPath,
 [Parameter(Mandatory=$true)][string]$ExpectedVersion,
 [switch]$ConfirmSystemChanges
)
# This test updates/removes/reinstalls the real Program Files application.
# Run only on an authorized Windows validation PC with IV closed.
if(!$ConfirmSystemChanges){throw 'This test changes the real app installation. Pass -ConfirmSystemChanges only on the authorized test PC.'}
$ErrorActionPreference='Stop' 
$ProgressPreference='SilentlyContinue'
$setup=[IO.Path]::GetFullPath($SetupPath)
$install=Join-Path ([Environment]::GetFolderPath('ProgramFiles')) 'Kohdalab\KohdaLab IV'
$key='HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\KohdaLabIV'
$out=Join-Path $env:USERPROFILE 'Documents\KohdaLab IV\validation\installer-2026-10-16'
New-Item -ItemType Directory -Path $out -Force | Out-Null
$results=[Collections.Generic.List[string]]::new()
$stage='start'
function Require($condition,[string]$message){if(!$condition){throw $message}}
function Run-Setup {
 $p=Start-Process -FilePath $setup -ArgumentList '/S' -PassThru
 if(!$p.WaitForExit(60000)){throw "Setup still running: $($p.Id)"}
 Require ($p.ExitCode -eq 0) "Setup exit $($p.ExitCode); review installer log."
}
function Snapshot-Files {
 $hashes=@{}
 foreach($root in @((Join-Path $env:USERPROFILE '.kohdalab\config'),(Join-Path $env:USERPROFILE '.kohdalab\results'),(Join-Path $env:LOCALAPPDATA 'KohdaLab IV'))){
  if(Test-Path -LiteralPath $root){foreach($f in Get-ChildItem -LiteralPath $root -Recurse -File){$hashes[$f.FullName]=(Get-FileHash -LiteralPath $f.FullName -Algorithm SHA256).Hash}}
 }
 return ,$hashes
}
function Driver-Snapshot {
 return (@(Get-CimInstance Win32_PnPSignedDriver | Where-Object {$_.DeviceID -match 'VID_(3923|0957|0B21)'} | Sort-Object DeviceID | Select-Object DeviceID,InfName,DriverVersion,DriverProviderName) | ConvertTo-Json -Compress)
}
function Verify-Preservation {
 foreach($path in $before.Keys){Require ((Test-Path -LiteralPath $path) -and (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash -eq $before[$path]) "User file changed: $path"}
 Require ((Driver-Snapshot) -eq $drivers) 'Instrument driver assignment changed.'
}
function Verify-Install {
 Require (Test-Path -LiteralPath $key) 'Windows app registration missing.'
 Require ((Get-ItemProperty -LiteralPath $key).DisplayVersion -eq $ExpectedVersion) 'Wrong installed app version.'
 Require ((Get-ItemProperty -LiteralPath $key).InstallLocation -eq $install) 'Wrong Program Files path.'
 $version=& (Join-Path $install 'iv-cli.exe') --version
 Require ($version -like ("*"+$ExpectedVersion+"*")) 'Installed Rust engine is not rc.1.'
 $known=Get-Content -LiteralPath (Join-Path $install 'FILES.json') -Raw | ConvertFrom-Json
 foreach($entry in $known.PSObject.Properties){Require ((Get-FileHash -LiteralPath (Join-Path $install $entry.Name) -Algorithm SHA256).Hash -eq $entry.Value) "Installed file hash differs: $($entry.Name)"}
 $wsh=New-Object -ComObject WScript.Shell
 foreach($shortcut in @((Join-Path ([Environment]::GetFolderPath('CommonDesktopDirectory')) 'KohdaLab IV.lnk'),(Join-Path ([Environment]::GetFolderPath('CommonPrograms')) 'KohdaLab IV\KohdaLab IV.lnk'))){
  Require (Test-Path -LiteralPath $shortcut) 'Shared application shortcut missing.'
  Require ($wsh.CreateShortcut($shortcut).TargetPath -eq (Join-Path $install 'kohdalab-iv-next.exe')) 'Shared shortcut target is wrong.'
 }
 Verify-Preservation
}
$before=Snapshot-Files
$drivers=Driver-Snapshot
$engine=$null
try {
 Require ((Get-Process -Name kohdalab-iv-next,iv-cli -ErrorAction SilentlyContinue | Measure-Object).Count -eq 0) 'Close IV before lifecycle verification.'
 $stage='update old portable installation'
 Run-Setup
 Verify-Install
 $results.Add('Upgrade from previous Program Files installation, app registration, shared shortcuts and all installed hashes: passed')
 $stage='uninstall'
 $u=Start-Process -FilePath (Join-Path $install 'Uninstall.exe') -ArgumentList '/S' -Wait -PassThru
 $deadline=[DateTime]::UtcNow.AddSeconds(30)
 while((Test-Path -LiteralPath $install) -and [DateTime]::UtcNow -lt $deadline){Start-Sleep -Milliseconds 250}
 Require (!(Test-Path -LiteralPath $install)) 'App folder remains after uninstall.'
 Require (!(Test-Path -LiteralPath $key)) 'Windows app registration remains after uninstall.'
 foreach($shortcut in @((Join-Path ([Environment]::GetFolderPath('CommonDesktopDirectory')) 'KohdaLab IV.lnk'),(Join-Path ([Environment]::GetFolderPath('CommonPrograms')) 'KohdaLab IV\KohdaLab IV.lnk'))){Require (!(Test-Path -LiteralPath $shortcut)) 'App shortcut remains after uninstall.'}
 Verify-Preservation
 $results.Add('Uninstall removed app/registration/shortcuts and retained configuration/results/drivers: passed')
 $stage='fresh install'
 Run-Setup
 Verify-Install
 $results.Add('Fresh install after uninstall: passed')
 $stage='repeat update'
 Run-Setup
 Verify-Install
 $results.Add('Re-running current installer safely updates the registered app: passed')
 $stage='running engine refusal'
 $psi=[Diagnostics.ProcessStartInfo]::new()
 $psi.FileName=Join-Path $install 'iv-cli.exe';$psi.Arguments='--engine';$psi.UseShellExecute=$false;$psi.CreateNoWindow=$true;$psi.RedirectStandardInput=$true;$psi.RedirectStandardOutput=$true;$psi.RedirectStandardError=$true
 $engine=[Diagnostics.Process]::Start($psi)
 Start-Sleep -Milliseconds 300
 $blocked=Start-Process -FilePath (Join-Path $install 'Uninstall.exe') -ArgumentList '/S' -Wait -PassThru
 Start-Sleep -Milliseconds 700
 Require (Test-Path -LiteralPath (Join-Path $install 'iv-cli.exe')) 'Uninstall removed a running engine.'
 Require (Test-Path -LiteralPath $key) 'Blocked uninstall changed registration.'
 Require (!$engine.HasExited) 'Installer stopped the running engine.'
 $engine.StandardInput.WriteLine('{"command":"shutdown"}');$engine.StandardInput.Flush()
 Require ($engine.WaitForExit(10000)) 'Test engine did not shut down.'
 Verify-Install
 $results.Add('Uninstall refuses a running engine without stopping it or changing registration: passed')
 [pscustomobject]@{result='passed';checks=@($results);preservedFiles=$before.Count;driverSnapshotUnchanged=$true;installed=$install;version=$ExpectedVersion} | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath (Join-Path $out 'lifecycle.json') -Encoding UTF8
 Write-Output (@($results) -join "`n")
} catch {
 [pscustomobject]@{result='failed';stage=$stage;error=$_.Exception.Message;completed=@($results)} | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath (Join-Path $out 'lifecycle.json') -Encoding UTF8
 throw
} finally {
 if($engine -and !$engine.HasExited){$engine.StandardInput.WriteLine('{"command":"shutdown"}');$engine.StandardInput.Flush();[void]$engine.WaitForExit(10000)}
}
