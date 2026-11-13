# SPDX-License-Identifier: MIT
[CmdletBinding()]
param(
 [Parameter(Mandatory=$true)][string]$PackageDirectory,
 [Parameter(Mandatory=$true)][string]$UninstallerSource,
 [Parameter(Mandatory=$true)][ValidatePattern('^\d+\.\d+\.\d+(?:-[A-Za-z0-9.-]+)?$')][string]$ApplicationVersion
)
$ErrorActionPreference='Stop'
$ProgressPreference='SilentlyContinue'
$logDirectory=Join-Path $env:LOCALAPPDATA 'KohdaLab IV Installer'
New-Item -ItemType Directory -Path $logDirectory -Force | Out-Null
Start-Transcript -Path (Join-Path $logDirectory 'setup.log') -Append -Force | Out-Null
$package=[IO.Path]::GetFullPath($PackageDirectory)
$parent=Split-Path -Parent $package
$prepared=Join-Path $parent ('prepared-'+[Guid]::NewGuid().ToString('N'))
$identity=[Security.Principal.WindowsIdentity]::GetCurrent()
if(!([Security.Principal.WindowsPrincipal]$identity).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)){throw 'Administrator approval is required for installation.'}
$installed=Join-Path ([Environment]::GetFolderPath('ProgramFiles')) 'Kohdalab\KohdaLab IV'
$key='HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\KohdaLabIV'
$desktop=[Environment]::GetFolderPath('CommonDesktopDirectory')
$menu=Join-Path ([Environment]::GetFolderPath('CommonPrograms')) 'KohdaLab IV'
function Assert-SafeAncestry([string]$Path) {
 $cursor=[IO.Path]::GetFullPath($Path)
 while($cursor){
  if((Test-Path -LiteralPath $cursor) -and ((Get-Item -LiteralPath $cursor -Force).Attributes -band [IO.FileAttributes]::ReparsePoint)){throw "Refusing reparse-point ancestor: $cursor"}
  $parentPath=[IO.Directory]::GetParent($cursor)
  $cursor=if($parentPath){$parentPath.FullName}else{$null}
 }
}
Assert-SafeAncestry $installed
Assert-SafeAncestry $menu
$oldRegistry=$null
if(Test-Path -LiteralPath $key){
 $reg=Get-Item -LiteralPath $key
 $oldRegistry=@{};foreach($name in $reg.GetValueNames()){$oldRegistry[$name]=@($reg.GetValue($name,$null,[Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames),$reg.GetValueKind($name).ToString())}
}
$menuLink=Join-Path $menu 'KohdaLab IV.lnk'
$menuExisted=Test-Path -LiteralPath $menu
Assert-SafeAncestry $menuLink
$oldMenu=if(Test-Path -LiteralPath $menuLink){[IO.File]::ReadAllBytes($menuLink)}else{$null}
$activationStarted=$false
$manifest=[ordered]@{}
try {
 foreach($item in @(Get-Item -LiteralPath $package -Force)+@(Get-ChildItem -LiteralPath $package -Recurse -Force)){
  if($item.Attributes -band [IO.FileAttributes]::ReparsePoint){throw 'Refusing a reparse-point installation package.'}
 }
 $original=Get-Content -LiteralPath (Join-Path $package 'FILES.json') -Raw | ConvertFrom-Json
 foreach($entry in $original.PSObject.Properties){
  $name=[string]$entry.Name
  if([IO.Path]::IsPathRooted($name) -or $name.Contains(':') -or @($name.Replace('\','/').Split('/') | Where-Object {$_ -in @('','..','.')}).Count){throw 'Unsafe package manifest path.'}
  if($entry.Value -notmatch '^[0-9a-fA-F]{64}$'){throw 'Invalid package checksum.'}
  $source=Join-Path $package $name
  if((Get-FileHash -LiteralPath $source -Algorithm SHA256).Hash -ne $entry.Value){throw "Package checksum mismatch: $name"}
  $manifest[$name]=$entry.Value
 }
 $actual=@(Get-ChildItem -LiteralPath $package -Recurse -File | Where-Object Name -ne 'FILES.json')
 if($actual.Count -ne $manifest.Count){throw 'Unexpected package files.'}
 if(!(Test-Path -LiteralPath $UninstallerSource -PathType Leaf) -or ((Get-Item -LiteralPath $UninstallerSource).Attributes -band [IO.FileAttributes]::ReparsePoint)){throw 'The generated uninstaller is missing or unsafe.'}
 New-Item -ItemType Directory -Path $prepared | Out-Null
 foreach($item in Get-ChildItem -LiteralPath $package -Force){Copy-Item -LiteralPath $item.FullName -Destination $prepared -Recurse -Force}
 Copy-Item -LiteralPath $UninstallerSource -Destination (Join-Path $prepared 'Uninstall.exe')
 $manifest['Uninstall.exe']=(Get-FileHash -LiteralPath (Join-Path $prepared 'Uninstall.exe') -Algorithm SHA256).Hash.ToLowerInvariant()
 $state=[ordered]@{application='org.kohdalab.iv';version=$ApplicationVersion;installation=$installed;desktop=(Join-Path $desktop 'KohdaLab IV.lnk');menu=$menu}
 $state | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $prepared 'installer-state.json') -Encoding UTF8
 $manifest['installer-state.json']=(Get-FileHash -LiteralPath (Join-Path $prepared 'installer-state.json') -Algorithm SHA256).Hash.ToLowerInvariant()
 $manifest | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath (Join-Path $prepared 'FILES.json') -Encoding UTF8
 $register = {
  param($InstallPath)
 # Verify activation before creating Windows application registration.
 if((Get-FileHash -LiteralPath (Join-Path $installed 'Uninstall.exe')).Hash -ne $manifest['Uninstall.exe']){throw 'Installed uninstaller verification failed.'}
 New-Item -ItemType Directory -Path $menu -Force | Out-Null
 $shell=New-Object -ComObject WScript.Shell
 foreach($path in @((Join-Path $menu 'KohdaLab IV.lnk'),(Join-Path $desktop 'KohdaLab IV.lnk'))){
  $shortcut=$shell.CreateShortcut($path)
  $shortcut.TargetPath=Join-Path $installed 'kohdalab-iv.exe'
  $shortcut.WorkingDirectory=$env:WINDIR
  $shortcut.Description='KohdaLab IV'
  $shortcut.Save()
 }
 $values=@{DisplayName='KohdaLab IV';DisplayVersion=$ApplicationVersion;Publisher='KohdaLab';InstallLocation=$installed;DisplayIcon=(Join-Path $installed 'kohdalab-iv.exe');UninstallString=('"'+(Join-Path $installed 'Uninstall.exe')+'"');QuietUninstallString=('"'+(Join-Path $installed 'Uninstall.exe')+'" /S');URLInfoAbout='https://github.com/Kohdalab/kohdalab-iv'}
 New-Item -Path $key -Force | Out-Null
 foreach($name in $values.Keys){New-ItemProperty -Path $key -Name $name -Value $values[$name] -PropertyType String -Force | Out-Null}
 foreach($name in @('NoModify','NoRepair')){New-ItemProperty -Path $key -Name $name -Value 1 -PropertyType DWord -Force | Out-Null}
 $size=[int][Math]::Ceiling(((Get-ChildItem -LiteralPath $installed -Recurse -File | Measure-Object Length -Sum).Sum)/1024)
 New-ItemProperty -Path $key -Name EstimatedSize -Value $size -PropertyType DWord -Force | Out-Null
 }.GetNewClosure()
 $script=[IO.File]::ReadAllText((Join-Path $prepared 'Install-Application.ps1'))
 $activationStarted=$true
 & ([ScriptBlock]::Create($script)) -PackageDirectory $prepared -DesktopPath $desktop -AfterInstall $register
 # Cleanup of an old per-user alias is optional, after the transaction commits.
 try {
 $shell=New-Object -ComObject WScript.Shell
 # Remove an older per-user shortcut only when it refers to this same application.
 $old=Join-Path ([Environment]::GetFolderPath('Desktop')) 'KohdaLab IV.lnk'
 if($old -ne (Join-Path $desktop 'KohdaLab IV.lnk') -and (Test-Path -LiteralPath $old)){
  if($shell.CreateShortcut($old).TargetPath -in @((Join-Path $installed 'kohdalab-iv.exe'),(Join-Path $installed 'kohdalab-iv-next.exe'))){Remove-Item -LiteralPath $old}
 }
 } catch { Write-Warning "The old per-user shortcut was retained: $_" }
 Write-Output "Installed KohdaLab IV $ApplicationVersion; registered Windows uninstall and shortcuts."
} catch {
 $failure=$_
 if($activationStarted){
  try {
   if(Test-Path -LiteralPath $key){Remove-Item -LiteralPath $key -Recurse}
   if($null -ne $oldRegistry){
    New-Item -Path $key -Force | Out-Null
    foreach($name in $oldRegistry.Keys){New-ItemProperty -Path $key -Name $name -Value $oldRegistry[$name][0] -PropertyType $oldRegistry[$name][1] -Force | Out-Null}
   }
   if($null -ne $oldMenu){[IO.File]::WriteAllBytes($menuLink,[byte[]]$oldMenu)}elseif(Test-Path -LiteralPath $menuLink){Remove-Item -LiteralPath $menuLink}
   if(!$menuExisted -and (Test-Path -LiteralPath $menu) -and !(Get-ChildItem -LiteralPath $menu -Force)){Remove-Item -LiteralPath $menu}
  } catch { Write-Warning "Installer registration recovery requires attention: $_" }
 }
 throw $failure
} finally {
 if(Test-Path -LiteralPath $prepared){Remove-Item -LiteralPath $prepared -Recurse -Force -ErrorAction SilentlyContinue}
 Stop-Transcript | Out-Null
}
