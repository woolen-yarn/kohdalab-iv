# SPDX-License-Identifier: MIT
[CmdletBinding()]
param([Parameter(Mandatory=$true)][string]$InstallationDirectory)
$ErrorActionPreference='Stop'
$ProgressPreference='SilentlyContinue'
$logDirectory=Join-Path $env:LOCALAPPDATA 'KohdaLab IV Installer'
New-Item -ItemType Directory -Path $logDirectory -Force | Out-Null
Start-Transcript -Path (Join-Path $logDirectory 'uninstall.log') -Append -Force | Out-Null
Add-Type -AssemblyName System.IO.Compression.FileSystem
$expected=[IO.Path]::GetFullPath((Join-Path ([Environment]::GetFolderPath('ProgramFiles')) 'Kohdalab\KohdaLab IV')).TrimEnd('\')
$installed=[IO.Path]::GetFullPath($InstallationDirectory).TrimEnd('\')
if(![string]::Equals($installed,$expected,[StringComparison]::OrdinalIgnoreCase)){throw 'Refusing an unexpected installation directory.'}
$key='HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\KohdaLabIV'
if(!(Test-Path -LiteralPath $installed -PathType Container)){throw 'The installed application directory was not found.'}
$vendor=Split-Path -Parent $installed
function Assert-SafeAncestry([string]$Path) {
 $cursor=[IO.Path]::GetFullPath($Path)
 while($cursor){
  if((Test-Path -LiteralPath $cursor) -and ((Get-Item -LiteralPath $cursor -Force).Attributes -band [IO.FileAttributes]::ReparsePoint)){throw "Refusing reparse-point ancestor: $cursor"}
  $parentPath=[IO.Directory]::GetParent($cursor)
  $cursor=if($parentPath){$parentPath.FullName}else{$null}
 }
}
Assert-SafeAncestry $installed
$menu=Join-Path ([Environment]::GetFolderPath('CommonPrograms')) 'KohdaLab IV'
Assert-SafeAncestry $menu
$shortcuts=@((Join-Path ([Environment]::GetFolderPath('CommonDesktopDirectory')) 'KohdaLab IV.lnk'),(Join-Path $menu 'KohdaLab IV.lnk'))
$shortcutBackup=@{}
foreach($path in $shortcuts){
 if(Test-Path -LiteralPath $path){
  if((Get-Item -LiteralPath $path -Force).Attributes -band [IO.FileAttributes]::ReparsePoint){throw 'Refusing a reparse-point application shortcut.'}
  $shortcutBackup[$path]=[IO.File]::ReadAllBytes($path)
 }
}
$oldRegistry=$null
if(Test-Path -LiteralPath $key){
 $reg=Get-Item -LiteralPath $key
 if($reg.GetValue('InstallLocation') -ne $installed){throw 'The registered installation directory did not match.'}
 $oldRegistry=@{};foreach($name in $reg.GetValueNames()){$oldRegistry[$name]=@($reg.GetValue($name,$null,[Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames),$reg.GetValueKind($name).ToString())}
}
$quarantine=Join-Path $vendor ('.KohdaLab-IV-uninstall-'+[Guid]::NewGuid().ToString('N'))
$moved=$false
$committed=$false
$lock=$null
try {
 $lock=[IO.File]::Open((Join-Path $vendor '.KohdaLab-IV-install.lock'),[IO.FileMode]::OpenOrCreate,[IO.FileAccess]::ReadWrite,[IO.FileShare]::None)
 foreach($item in @(Get-Item -LiteralPath $installed -Force)+@(Get-ChildItem -LiteralPath $installed -Force -Recurse)){
  if($item.Attributes -band [IO.FileAttributes]::ReparsePoint){throw 'Refusing an installation containing a reparse point.'}
 }
 $state=Get-Content -LiteralPath (Join-Path $installed 'installer-state.json') -Raw | ConvertFrom-Json
 if($state.application -ne 'org.kohdalab.iv' -or $state.installation -ne $installed){throw 'Installed application identity did not match.'}
 foreach($process in @(Get-Process -Name kohdalab-iv-next,iv-cli -ErrorAction SilentlyContinue)){
  if(!$process.Path){throw 'Could not verify that IV is closed. Close IV before uninstalling.'}
  if($process.Path.StartsWith($installed+'\',[StringComparison]::OrdinalIgnoreCase)){throw 'KohdaLab IV is running. Stop measurement and close IV before uninstalling.'}
 }
 # Archive and verify the whole application tree, including user-added files.
 # Settings, results and USB-driver caches outside Program Files are never removed.
 $archiveDir=Join-Path ([Environment]::GetFolderPath('MyDocuments')) 'KohdaLab IV\archive'
 New-Item -ItemType Directory -Path $archiveDir -Force | Out-Null
 if((Get-Item -LiteralPath $archiveDir).Attributes -band [IO.FileAttributes]::ReparsePoint){throw 'Refusing a reparse-point backup directory.'}
 $archive=Join-Path $archiveDir ('KohdaLab-IV-uninstall-backup-'+(Get-Date -Format 'yyyyMMdd-HHmmss-fff')+'.zip')
 $hashes=@{}
 foreach($file in Get-ChildItem -LiteralPath $installed -Recurse -File){$hashes[$file.FullName.Substring($installed.Length+1).Replace('\','/')]=(Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash}
 [IO.Compression.ZipFile]::CreateFromDirectory($installed,$archive,[IO.Compression.CompressionLevel]::Optimal,$false)
 $zip=[IO.Compression.ZipFile]::OpenRead($archive)
 try {
  if(@($zip.Entries | Where-Object {!$_.FullName.Replace('\','/').EndsWith('/')}).Count -ne $hashes.Count){throw 'Uninstall backup file count did not match.'}
  foreach($entry in $zip.Entries){
   # .NET Framework on Windows writes backslashes in ZIP entry names.
   $entryName=$entry.FullName.Replace('\','/')
   if($entryName.EndsWith('/')){continue}
   $stream=$entry.Open();$sha=[Security.Cryptography.SHA256]::Create()
   try{$observed=[BitConverter]::ToString($sha.ComputeHash($stream)).Replace('-','')}finally{$stream.Dispose();$sha.Dispose()}
   if(!$hashes.ContainsKey($entryName) -or $observed -ne $hashes[$entryName]){throw 'Uninstall backup checksum verification failed.'}
  }
 } finally {$zip.Dispose()}
 # Verify the snapshot is still current before changing any installation file.
 $current=@(Get-ChildItem -LiteralPath $installed -Recurse -File)
 if($current.Count -ne $hashes.Count){throw 'Installation changed while its backup was created.'}
 foreach($file in $current){if((Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash -ne $hashes[$file.FullName.Substring($installed.Length+1).Replace('\','/')]){throw 'Installation changed while backing up.'}}
 # Recheck immediately before changing any app or registration state.
 foreach($process in @(Get-Process -Name kohdalab-iv-next,iv-cli -ErrorAction SilentlyContinue)){
  if(!$process.Path -or $process.Path.StartsWith($installed+'\',[StringComparison]::OrdinalIgnoreCase)){throw 'Close KohdaLab IV before uninstalling.'}
 }
 Set-Location -LiteralPath $env:TEMP
 Move-Item -LiteralPath $installed -Destination $quarantine
 $moved=$true
 $shell=New-Object -ComObject WScript.Shell
 foreach($shortcut in $shortcuts){
  if(Test-Path -LiteralPath $shortcut){if($shell.CreateShortcut($shortcut).TargetPath -eq (Join-Path $installed 'kohdalab-iv-next.exe')){Remove-Item -LiteralPath $shortcut}}
 }
 if(Test-Path -LiteralPath $key){Remove-Item -LiteralPath $key -Recurse}
 $committed=$true
 # File cleanup is after the recoverable registration transaction. A locked
 # residual file is retained with the verified archive, never silently lost.
 try { Remove-Item -LiteralPath $quarantine -Recurse -Force }
 catch { Write-Warning "The application was unregistered. Residual files were retained at $quarantine. Verified backup: $archive" }
 if((Test-Path -LiteralPath $menu) -and !(Get-ChildItem -LiteralPath $menu -Force)){Remove-Item -LiteralPath $menu -ErrorAction SilentlyContinue}
 Write-Output "KohdaLab IV was removed. Settings, measurements and device drivers were retained. Verified app backup: $archive"
} catch {
 $failure=$_
 if($moved -and !$committed){
  try {
   Move-Item -LiteralPath $quarantine -Destination $installed
   foreach($path in $shortcutBackup.Keys){[IO.File]::WriteAllBytes($path,[byte[]]$shortcutBackup[$path])}
   if($null -ne $oldRegistry){
    New-Item -Path $key -Force | Out-Null
    foreach($name in $oldRegistry.Keys){New-ItemProperty -Path $key -Name $name -Value $oldRegistry[$name][0] -PropertyType $oldRegistry[$name][1] -Force | Out-Null}
   }
  } catch { Write-Warning "Uninstall recovery requires attention: $_. Verified backup: $archive" }
 }
 throw $failure
} finally {if($lock){$lock.Dispose()};Stop-Transcript | Out-Null}
