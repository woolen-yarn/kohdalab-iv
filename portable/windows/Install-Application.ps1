[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string]$PackageDirectory,
    [scriptblock]$AfterInstall,
    [string]$TestRoot,
    [switch]$Elevated,
    # Filled in before UAC elevation so an alternate administrator account never
    # receives the installing user's shortcut, settings, or results directory.
    [string]$UserProfilePath,
    [string]$DesktopPath,
    [string]$DocumentsPath,
    # Test-only fault injection for the lifecycle rollback test.  It is refused
    # for a normal installation.
    [switch]$TestCorruptAfterMove
)

# This script is deliberately self-contained.  Install-Application.cmd loads it
# through ScriptBlock::Create so it does not need a machine execution-policy
# change.  -TestRoot is for the repository lifecycle test only.
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'

function Get-FullPath([string]$Path) {
    return [IO.Path]::GetFullPath($Path)
}

function Test-ReparsePoint([string]$Path) {
    if (Test-Path -LiteralPath $Path) {
        return (((Get-Item -LiteralPath $Path -Force).Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0)
    }
    return $false
}

function Assert-NoReparsePoints([string]$Path, [string]$Description) {
    if (Test-ReparsePoint $Path) {
        throw "$Description contains a reparse point: $Path"
    }
    foreach ($item in @(Get-ChildItem -LiteralPath $Path -Force -Recurse -ErrorAction Stop)) {
        if (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
            throw "$Description contains a reparse point: $($item.FullName)"
        }
    }
}

function Assert-ContainedPath([string]$Root, [string]$Candidate, [string]$Description) {
    $rootFull = (Get-FullPath $Root).TrimEnd('\') + '\'
    $candidateFull = Get-FullPath $Candidate
    if (-not $candidateFull.StartsWith($rootFull, [StringComparison]::OrdinalIgnoreCase)) {
        throw "$Description resolves outside its allowed directory."
    }
    return $candidateFull
}

function Get-Sha256([string]$Path) {
    return (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
}

function Get-PackageManifest([string]$Root) {
    $manifestPath = Join-Path $Root 'FILES.json'
    if (-not (Test-Path -LiteralPath $manifestPath -PathType Leaf)) {
        throw 'FILES.json is missing. Download a complete portable package.'
    }
    $raw = [IO.File]::ReadAllText($manifestPath)
    try {
        $json = $raw | ConvertFrom-Json -ErrorAction Stop
    } catch {
        throw "FILES.json is invalid: $($_.Exception.Message)"
    }
    if ($null -eq $json) { throw 'FILES.json does not contain any files.' }

    $entries = @()
    $seen = New-Object 'System.Collections.Generic.HashSet[string]' ([StringComparer]::OrdinalIgnoreCase)
    foreach ($property in @($json.PSObject.Properties)) {
        $relative = [string]$property.Name
        $expected = ([string]$property.Value).ToLowerInvariant()
        if ([String]::IsNullOrWhiteSpace($relative) -or
            [IO.Path]::IsPathRooted($relative) -or
            $relative.Contains(':') -or
            $relative.StartsWith('/') -or
            $relative.StartsWith('\')) {
            throw "FILES.json has an unsafe path: $relative"
        }
        $segments = $relative.Replace('\', '/').Split('/')
        if (@($segments | Where-Object { $_ -eq '' -or $_ -eq '.' -or $_ -eq '..' }).Count -gt 0) {
            throw "FILES.json has an unsafe path: $relative"
        }
        if ($expected -notmatch '^[0-9a-f]{64}$') {
            throw "FILES.json has an invalid SHA-256 value for: $relative"
        }
        $normalised = $relative.Replace('/', '\')
        if (-not $seen.Add($normalised)) {
            throw "FILES.json has duplicate paths: $relative"
        }
        $source = Assert-ContainedPath $Root (Join-Path $Root $normalised) "Package path '$relative'"
        $entries += [PSCustomObject]@{ Relative = $normalised; Source = $source; Expected = $expected }
    }
    if ($entries.Count -eq 0) { throw 'FILES.json does not list any files.' }
    return ,$entries
}

function Assert-Package([string]$Root, $Entries) {
    if (-not (Test-Path -LiteralPath $Root -PathType Container)) {
        throw "Portable package directory does not exist: $Root"
    }
    Assert-NoReparsePoints $Root 'The portable package'
    $listed = New-Object 'System.Collections.Generic.HashSet[string]' ([StringComparer]::OrdinalIgnoreCase)
    foreach ($entry in $Entries) {
        if (-not (Test-Path -LiteralPath $entry.Source -PathType Leaf)) {
            throw "Package file is missing: $($entry.Relative)"
        }
        $actual = Get-Sha256 $entry.Source
        if ($actual -ne $entry.Expected) {
            throw "Package checksum verification failed: $($entry.Relative)"
        }
        [void]$listed.Add($entry.Relative)
    }
    foreach ($file in @(Get-ChildItem -LiteralPath $Root -File -Force -Recurse)) {
        $relative = $file.FullName.Substring($Root.TrimEnd('\').Length + 1)
        if ($relative -ieq 'FILES.json') { continue }
        if (-not $listed.Contains($relative)) {
            throw "Package contains a file that is not protected by FILES.json: $relative"
        }
    }
    foreach ($required in @('kohdalab-iv.exe', 'iv-cli.exe', 'Install-Application.cmd', 'Install-Application.ps1')) {
        if (-not $listed.Contains($required)) {
            throw "FILES.json is missing a required application file: $required"
        }
    }
}

function Assert-TreeMatchesManifest([string]$Root, $Entries, [string]$Description) {
    Assert-NoReparsePoints $Root $Description
    $listed = New-Object 'System.Collections.Generic.HashSet[string]' ([StringComparer]::OrdinalIgnoreCase)
    foreach ($entry in $Entries) {
        $path = Assert-ContainedPath $Root (Join-Path $Root $entry.Relative) "$Description path '$($entry.Relative)'"
        if (-not (Test-Path -LiteralPath $path -PathType Leaf) -or (Get-Sha256 $path) -ne $entry.Expected) {
            throw "$Description verification failed: $($entry.Relative)"
        }
        [void]$listed.Add($entry.Relative)
    }
    foreach ($file in @(Get-ChildItem -LiteralPath $Root -File -Force -Recurse)) {
        $relative = $file.FullName.Substring($Root.TrimEnd('\').Length + 1)
        if ($relative -ieq 'FILES.json') { continue }
        if (-not $listed.Contains($relative)) {
            throw "$Description contains an unexpected file: $relative"
        }
    }
}

function New-Directory([string]$Path) {
    if (Test-Path -LiteralPath $Path) {
        if (-not (Test-Path -LiteralPath $Path -PathType Container)) { throw "Expected a directory: $Path" }
        if (Test-ReparsePoint $Path) { throw "Refusing to use a reparse-point directory: $Path" }
    } else {
        New-Item -ItemType Directory -Path $Path -Force | Out-Null
    }
}

function Copy-PackageToStage([string]$Stage, [string]$Root, $Entries) {
    New-Directory $Stage
    foreach ($entry in $Entries) {
        $destination = Assert-ContainedPath $Stage (Join-Path $Stage $entry.Relative) "Staging path '$($entry.Relative)'"
        New-Directory (Split-Path -Parent $destination)
        Copy-Item -LiteralPath $entry.Source -Destination $destination -Force
    }
    Copy-Item -LiteralPath (Join-Path $Root 'FILES.json') -Destination (Join-Path $Stage 'FILES.json') -Force
    Assert-TreeMatchesManifest $Stage $Entries 'The staged installation'
}

function Get-TreeHashes([string]$Root) {
    Assert-NoReparsePoints $Root 'The previous installation'
    $hashes = @{}
    foreach ($file in @(Get-ChildItem -LiteralPath $Root -File -Force -Recurse)) {
        $relative = $file.FullName.Substring($Root.TrimEnd('\').Length + 1).Replace('\', '/')
        $hashes[$relative] = Get-Sha256 $file.FullName
    }
    return $hashes
}

function Assert-TreeHashes([string]$Root, $Expected, [string]$Description) {
    $actual = Get-TreeHashes $Root
    if ($actual.Count -ne $Expected.Count) {
        throw "$Description changed after its backup was verified."
    }
    foreach ($relative in $Expected.Keys) {
        if (-not $actual.ContainsKey($relative) -or $actual[$relative] -ne $Expected[$relative]) {
            throw "$Description changed after its backup was verified: $relative"
        }
    }
}

function Archive-PreviousInstall([string]$Previous, [string]$ArchiveDirectory) {
    $hashes = Get-TreeHashes $Previous
    New-Directory $ArchiveDirectory
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $archive = Join-Path $ArchiveDirectory ("KohdaLab-IV-install-backup-{0}.zip" -f (Get-Date -Format 'yyyyMMdd-HHmmss-fff'))
    [IO.Compression.ZipFile]::CreateFromDirectory($Previous, $archive, [IO.Compression.CompressionLevel]::Optimal, $false)
    $zip = [IO.Compression.ZipFile]::OpenRead($archive)
    try {
        $zipHashes = @{}
        foreach ($entry in $zip.Entries) {
            if ([String]::IsNullOrEmpty($entry.Name)) { continue }
            $stream = $entry.Open()
            try {
                $sha = [Security.Cryptography.SHA256]::Create()
                try {
                    $zipHashes[$entry.FullName.Replace('/', '\')] = ([BitConverter]::ToString($sha.ComputeHash($stream))).Replace('-', '').ToLowerInvariant()
                } finally { $sha.Dispose() }
            } finally { $stream.Dispose() }
        }
        if ($zipHashes.Count -ne $hashes.Count) { throw 'The previous installation backup has an unexpected file count.' }
        foreach ($relative in $hashes.Keys) {
            $zipKey = $relative.Replace('/', '\')
            if (-not $zipHashes.ContainsKey($zipKey) -or $zipHashes[$zipKey] -ne $hashes[$relative]) {
                throw "The previous installation backup failed verification: $relative"
            }
        }
    } finally { $zip.Dispose() }
    return [PSCustomObject]@{ Archive = $archive; Hashes = $hashes }
}

function Assert-NotRunning([string]$InstallPath, [bool]$IsTest) {
    # Keep the legacy process name here through the rename migration.  A running
    # old binary can still be executing from the installation directory that an
    # update is about to replace.
    $names = @('kohdalab-iv', 'kohdalab-iv-next', 'iv-cli', 'KohdaLab IV')
    foreach ($name in $names) {
        foreach ($process in @(Get-Process -Name $name -ErrorAction SilentlyContinue)) {
            if ($IsTest) {
                try {
                    if ($process.Path -and -not $process.Path.StartsWith((Get-FullPath $InstallPath), [StringComparison]::OrdinalIgnoreCase)) { continue }
                } catch { continue }
            }
            throw "Close KohdaLab IV and any iv-cli/Python automation before installing. Running process: $($process.ProcessName)"
        }
    }
}

function Backup-DesktopShortcut([string]$ShortcutPath, [string]$ArchiveDirectory) {
    if (Test-Path -LiteralPath $ShortcutPath -PathType Leaf) {
        New-Directory $ArchiveDirectory
        $backup = Join-Path $ArchiveDirectory ("KohdaLab-IV-shortcut-backup-{0}.lnk" -f (Get-Date -Format 'yyyyMMdd-HHmmss-fff'))
        Copy-Item -LiteralPath $ShortcutPath -Destination $backup -Force
        return $backup
    }
    return $null
}

function New-DesktopShortcut([string]$ShortcutPath, [string]$TargetPath, [string]$WorkingDirectory) {
    if (Test-Path -LiteralPath $ShortcutPath -PathType Leaf) {
        Remove-Item -LiteralPath $ShortcutPath -Force
    }
    $shell = New-Object -ComObject WScript.Shell
    $shortcut = $shell.CreateShortcut($ShortcutPath)
    $shortcut.TargetPath = $TargetPath
    $shortcut.WorkingDirectory = $WorkingDirectory
    $shortcut.Description = 'KohdaLab IV'
    $shortcut.Save()
}

$package = Get-FullPath $PackageDirectory
if (-not (Test-Path -LiteralPath $package -PathType Container)) { throw "Portable package directory does not exist: $package" }
if (Test-ReparsePoint $package) { throw "Refusing to install from a reparse-point package directory: $package" }

if ($TestRoot) {
    if (-not [IO.Path]::IsPathRooted($TestRoot)) { throw '-TestRoot must be an absolute path.' }
    $test = Get-FullPath $TestRoot
    $programFiles = Join-Path $test 'ProgramFiles'
    $desktop = Join-Path $test 'Desktop'
    $documents = Join-Path $test 'Documents'
    $userData = Join-Path $test 'UserData'
    $isTest = $true
} else {
    $isTest = $false
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    $principal = New-Object Security.Principal.WindowsPrincipal($identity)
    if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
        $scriptPath = Join-Path $package 'Install-Application.ps1'
        if (-not (Test-Path -LiteralPath $scriptPath -PathType Leaf)) { throw 'Install-Application.ps1 is missing from this portable package.' }
        $escapedScript = (Get-FullPath $scriptPath).Replace("'", "''")
        $escapedPackage = $package.Replace("'", "''")
        $originalProfile = $env:USERPROFILE
        $originalDesktop = [Environment]::GetFolderPath([Environment+SpecialFolder]::DesktopDirectory)
        $originalDocuments = [Environment]::GetFolderPath([Environment+SpecialFolder]::MyDocuments)
        $escapedProfile = $originalProfile.Replace("'", "''")
        $escapedDesktop = $originalDesktop.Replace("'", "''")
        $escapedDocuments = $originalDocuments.Replace("'", "''")
        $command = "& ([ScriptBlock]::Create([IO.File]::ReadAllText('$escapedScript'))) -PackageDirectory '$escapedPackage' -Elevated -UserProfilePath '$escapedProfile' -DesktopPath '$escapedDesktop' -DocumentsPath '$escapedDocuments'"
        $encoded = [Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($command))
        try {
            $child = Start-Process -FilePath 'powershell.exe' -Verb RunAs -ArgumentList @('-NoProfile', '-EncodedCommand', $encoded) -Wait -PassThru
        } catch {
            throw 'Administrator approval was cancelled or the elevated installer could not be started.'
        }
        if ($child.ExitCode -ne 0) { throw "The elevated installer did not finish successfully (exit code $($child.ExitCode))." }
        return
    }
    $programFiles = [Environment]::GetFolderPath([Environment+SpecialFolder]::ProgramFiles)
    $desktop = if ($DesktopPath) { Get-FullPath $DesktopPath } else { [Environment]::GetFolderPath([Environment+SpecialFolder]::DesktopDirectory) }
    $documents = if ($DocumentsPath) { Get-FullPath $DocumentsPath } else { [Environment]::GetFolderPath([Environment+SpecialFolder]::MyDocuments) }
    $profile = if ($UserProfilePath) { Get-FullPath $UserProfilePath } else { $env:USERPROFILE }
    $userData = Join-Path $profile '.kohdalab'
}

if ($TestCorruptAfterMove -and -not $isTest) {
    throw '-TestCorruptAfterMove is available only with -TestRoot.'
}

# Verify every package payload, including both installer files, before creating
# a target directory or changing an existing installation.
$manifest = Get-PackageManifest $package
Assert-Package $package $manifest

$vendorRoot = Join-Path $programFiles 'Kohdalab'
$installPath = Join-Path $vendorRoot 'KohdaLab IV'
$archiveDirectory = Join-Path (Join-Path $documents 'KohdaLab IV') 'archive'
$shortcutPath = Join-Path $desktop 'KohdaLab IV.lnk'

if ((Test-Path -LiteralPath (Join-Path $installPath 'installer-state.json')) -and
    -not (Test-Path -LiteralPath (Join-Path $package 'Uninstall.exe'))) {
    throw 'This app was installed with Setup.exe. Use Setup.exe to update it, or run the portable executable separately. No files were changed.'
}

New-Directory $vendorRoot
New-Directory $desktop
New-Directory $userData
New-Directory (Join-Path $userData 'results')
New-Directory $archiveDirectory

$stage = Join-Path $vendorRoot ('.KohdaLab-IV-stage-' + [Guid]::NewGuid().ToString('N'))
$previous = Join-Path $vendorRoot ('.KohdaLab-IV-previous-' + [Guid]::NewGuid().ToString('N'))
$hadPrevious = $false
$oldAcl = $null
$installLock = $null
$previousShortcutBackup = $null
$activated = $false
$shortcutChanged = $false
$committed = $false
try {
    # Keep this lock file so independent installer processes contend on a stable
    # path.  The exclusive handle, not deletion of the file, provides the lock.
    $installLock = [IO.File]::Open((Join-Path $vendorRoot '.KohdaLab-IV-install.lock'), [IO.FileMode]::OpenOrCreate, [IO.FileAccess]::ReadWrite, [IO.FileShare]::None)
    Assert-NotRunning $installPath $isTest
    $hadPrevious = Test-Path -LiteralPath $installPath
    Copy-PackageToStage $stage $package $manifest
    if ($hadPrevious) {
        if (-not (Test-Path -LiteralPath $installPath -PathType Container) -or (Test-ReparsePoint $installPath)) {
            throw "Refusing to replace an unsafe existing installation: $installPath"
        }
        $oldAcl = Get-Acl -LiteralPath $installPath
        # Retain root ACLs, including explicit deny entries, rather than widening access.
        Set-Acl -LiteralPath $stage -AclObject $oldAcl
        Assert-NotRunning $installPath $isTest
        Move-Item -LiteralPath $installPath -Destination $previous
        try {
            $previousBackup = Archive-PreviousInstall $previous $archiveDirectory
        } catch {
            Move-Item -LiteralPath $previous -Destination $installPath
            throw
        }
    }
    Move-Item -LiteralPath $stage -Destination $installPath
    $activated = $true
    if ($TestCorruptAfterMove) {
        Add-Content -LiteralPath (Join-Path $installPath 'kohdalab-iv.exe') -Value 'test corruption after staging move'
    }
    Assert-TreeMatchesManifest $installPath $manifest 'The installed application'
    $previousShortcutBackup = Backup-DesktopShortcut $shortcutPath $archiveDirectory
    $shortcutChanged = $true
    New-DesktopShortcut $shortcutPath (Join-Path $installPath 'kohdalab-iv.exe') $userData
    if ($hadPrevious -and (Test-Path -LiteralPath $previous)) {
        Assert-TreeHashes $previous $previousBackup.Hashes 'The previous installation'
    }
    # Installer registration belongs to the same rollback boundary as files.
    if ($AfterInstall) { & $AfterInstall $installPath }
    $committed = $true
    if ($hadPrevious -and (Test-Path -LiteralPath $previous)) {
        try { Remove-Item -LiteralPath $previous -Recurse -Force } catch {
            Write-Warning "Installation succeeded. Some old files could not be removed from $previous. The verified backup is $($previousBackup.Archive)."
        }
    }
    Write-Host "Installed KohdaLab IV to $installPath"
    Write-Host "Desktop shortcut: $shortcutPath"
    Write-Host "User settings and results remain in $userData"
} catch {
    $originalError = $_
    if (-not $committed) {
        try {
            if ($activated -and (Test-Path -LiteralPath $installPath)) {
                $failedPath = Join-Path $vendorRoot ('.KohdaLab-IV-failed-' + [Guid]::NewGuid().ToString('N'))
                Move-Item -LiteralPath $installPath -Destination $failedPath
                Write-Warning "Failed update files were retained at $failedPath."
            }
            if ($hadPrevious -and (Test-Path -LiteralPath $previous)) {
                Move-Item -LiteralPath $previous -Destination $installPath
            }
            if ($shortcutChanged) {
                if (Test-Path -LiteralPath $shortcutPath) { Remove-Item -LiteralPath $shortcutPath -Force }
                if ($previousShortcutBackup) {
                    Copy-Item -LiteralPath $previousShortcutBackup -Destination $shortcutPath -Force
                }
            }
        } catch {
            throw "Installation failed: $originalError Recovery also failed: $_. The previous installation or its verified backup is retained at $previous / $archiveDirectory. Close IV before recovering these files."
        }
    }
    throw $originalError
} finally {
    if (Test-Path -LiteralPath $stage) { Remove-Item -LiteralPath $stage -Recurse -Force -ErrorAction SilentlyContinue }
    if ($installLock) { $installLock.Dispose() }
}
