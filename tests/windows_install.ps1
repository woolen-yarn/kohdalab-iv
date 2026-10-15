# End-to-end installer lifecycle checks; TestRoot prevents real installation.
param([string]$RepositoryRoot=(Split-Path -Parent $PSScriptRoot))
$ErrorActionPreference='Stop'
$ProgressPreference='SilentlyContinue'
Add-Type -AssemblyName System.IO.Compression.FileSystem
$directory=Join-Path ([IO.Path]::GetTempPath()) ('iv-install-tests-'+[Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory $directory | Out-Null
$package=Join-Path $directory 'download with spaces'
$testRoot=Join-Path $directory 'isolated user'
$installer=Join-Path $RepositoryRoot 'portable\windows\Install-Application.ps1'
function Assert-True($value,[string]$message){if(!$value){throw $message}}
function New-Package([string]$revision){
 New-Item -ItemType Directory (Join-Path $package 'support') -Force | Out-Null
 Set-Content (Join-Path $package 'kohdalab-iv-next.exe') "GUI fixture $revision" -Encoding ASCII
 Set-Content (Join-Path $package 'iv-cli.exe') "CLI fixture $revision" -Encoding ASCII
 Set-Content (Join-Path $package 'support\native.txt') "support $revision" -Encoding ASCII
 Set-Content (Join-Path $package 'README.md') "Install fixture $revision" -Encoding ASCII
 Set-Content (Join-Path $package 'REDISTRIBUTION.md') "Test notices $revision" -Encoding ASCII
 foreach($name in @('Install-Application.cmd','Install-Application.ps1')){Copy-Item (Join-Path $RepositoryRoot "portable\windows\$name") (Join-Path $package $name) -Force}
 $manifest=[ordered]@{}
 foreach($file in Get-ChildItem $package -Recurse -File | Where-Object Name -ne 'FILES.json'){$manifest[$file.FullName.Substring($package.Length+1).Replace('\','/')]=(Get-FileHash $file.FullName -Algorithm SHA256).Hash.ToLower()}
 $manifest | ConvertTo-Json | Set-Content (Join-Path $package 'FILES.json') -Encoding UTF8
}
function Install-Fixture{& ([ScriptBlock]::Create([IO.File]::ReadAllText($installer))) -PackageDirectory $package -TestRoot $testRoot}
function Expect-Refusal([string]$description){
 $refused=$false
 try{Install-Fixture}catch{$refused=$true;Write-Host "Expected refusal ($description): $($_.Exception.Message)"}
 Assert-True $refused "Installer accepted $description"
}
try {
 New-Package 'first'
 Install-Fixture
 $installed=Join-Path $testRoot 'ProgramFiles\Kohdalab\KohdaLab IV'
 $exe=Join-Path $installed 'kohdalab-iv-next.exe'
 Assert-True (Test-Path $exe) 'Fresh install is missing'
 Assert-True ((Get-FileHash $exe).Hash -eq (Get-FileHash (Join-Path $package 'kohdalab-iv-next.exe')).Hash) 'Fresh install differs from package'
 $shortcut=(New-Object -ComObject WScript.Shell).CreateShortcut((Join-Path $testRoot 'Desktop\KohdaLab IV.lnk'))
 Assert-True ($shortcut.TargetPath -eq $exe) 'Shortcut points at extracted download'
 Assert-True ($shortcut.WorkingDirectory -eq (Join-Path $testRoot 'UserData')) 'Shortcut working folder is not user-writable'
 # Re-running identical setup must also succeed.
 Install-Fixture
 $userData=Join-Path $testRoot 'UserData'
 New-Item -ItemType Directory (Join-Path $userData 'config') -Force | Out-Null
 $config=Join-Path $userData 'config\iv-next.json'
 Set-Content $config '{"keep":"user settings"}' -Encoding ASCII
 $measurement=Join-Path $userData 'results\user-measurement.csv'
 Set-Content $measurement 'voltage_V,current_A' -Encoding ASCII
 $userFile=Join-Path $installed 'user-added.csv'
 Set-Content $userFile 'keep old local data' -Encoding ASCII
 $protected=@($config,$measurement,$userFile) | ForEach-Object {[pscustomobject]@{path=$_;hash=(Get-FileHash $_).Hash}}
 New-Package 'second'
 Install-Fixture
 Assert-True ((Get-Content $exe -Raw) -match 'second') 'Update left the old executable'
 foreach($f in $protected | Where-Object path -ne $userFile){Assert-True ((Get-FileHash $f.path).Hash -eq $f.hash) 'Update changed user configuration/results'}
 $backups=@(Get-ChildItem (Join-Path $testRoot 'Documents') -Filter *.zip -Recurse)
 $foundUserData=$false
 foreach($file in $backups){$z=[IO.Compression.ZipFile]::OpenRead($file.FullName);try{foreach($entry in $z.Entries){if($entry.FullName.EndsWith('user-added.csv')){$stream=$entry.Open();$hash=[Security.Cryptography.SHA256]::Create();try{$observed=[BitConverter]::ToString($hash.ComputeHash($stream)).Replace('-','');if($observed -eq ($protected | Where-Object path -eq $userFile).hash){$foundUserData=$true}}finally{$stream.Dispose();$hash.Dispose()}}}}finally{$z.Dispose()}}
 Assert-True $foundUserData 'Update lost the unlisted local data instead of backing it up'
 $before=(Get-FileHash $exe).Hash
 Add-Content (Join-Path $package 'iv-cli.exe') 'tampered'
 Expect-Refusal 'a changed payload'
 Assert-True ((Get-FileHash $exe).Hash -eq $before) 'A rejected package changed the installed executable'
 New-Package 'third'
 $manifest=Get-Content (Join-Path $package 'FILES.json') -Raw | ConvertFrom-Json
 $manifest | Add-Member -NotePropertyName '../outside.txt' -NotePropertyValue ('0'*64)
 $manifest | ConvertTo-Json | Set-Content (Join-Path $package 'FILES.json') -Encoding UTF8
 Expect-Refusal 'a parent traversal manifest path'
 Assert-True ((Get-FileHash $exe).Hash -eq $before) 'Traversal refusal changed installation'
 New-Package 'incomplete'
 $manifest=Get-Content (Join-Path $package 'FILES.json') -Raw | ConvertFrom-Json
 $manifest.PSObject.Properties.Remove('iv-cli.exe')
 Remove-Item -LiteralPath (Join-Path $package 'iv-cli.exe')
 $manifest | ConvertTo-Json | Set-Content (Join-Path $package 'FILES.json') -Encoding UTF8
 Expect-Refusal 'an incomplete application manifest'
 Assert-True ((Get-FileHash $exe).Hash -eq $before) 'Incomplete package changed installation'
 New-Package 'locked'
 $lockPath=Join-Path $testRoot 'ProgramFiles\Kohdalab\.KohdaLab-IV-install.lock'
 $lock=[IO.File]::Open($lockPath,[IO.FileMode]::OpenOrCreate,[IO.FileAccess]::ReadWrite,[IO.FileShare]::None)
 try{Expect-Refusal 'a simultaneous installer'}finally{$lock.Dispose()}
 Assert-True ((Get-FileHash $exe).Hash -eq $before) 'Concurrent setup changed installation'
 New-Package 'failed update'
 $failed=$false
 try{& ([ScriptBlock]::Create([IO.File]::ReadAllText($installer))) -PackageDirectory $package -TestRoot $testRoot -TestCorruptAfterMove}catch{$failed=$true;Write-Host "Expected post-move verification failure: $($_.Exception.Message)"}
 Assert-True $failed 'Installer accepted corruption after activation'
 Assert-True ((Get-FileHash $exe).Hash -eq $before) 'Failed verification did not restore the working version'
 $shortcut=(New-Object -ComObject WScript.Shell).CreateShortcut((Join-Path $testRoot 'Desktop\KohdaLab IV.lnk'))
 Assert-True ($shortcut.TargetPath -eq $exe) 'Rollback lost the working shortcut'
 foreach($f in $protected | Where-Object path -ne $userFile){Assert-True ((Get-FileHash $f.path).Hash -eq $f.hash) 'Rollback changed user data'}
 Write-Host 'PASS: fresh install, repeat, update, backup preservation, shortcuts, user data, tamper/traversal refusal and failed-update rollback.'
} finally {
 Remove-Item -LiteralPath $directory -Recurse -Force
}
