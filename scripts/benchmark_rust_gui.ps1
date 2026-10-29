# Compare before/after Rust portable GUI binaries on a logged-in Windows desktop.
# No instrument connections are made. Both apps use isolated simulated configs.
[CmdletBinding()]
param(
    [Parameter(Mandatory=$true)][string]$BeforePackage,
    [Parameter(Mandatory=$true)][string]$AfterPackage,
    [Parameter(Mandatory=$true)][string]$OutputDir,
    [int]$Repeats=5,
    [double]$IdleSettleSeconds=1.0,
    [switch]$SkipWarmup
)
$ErrorActionPreference='Stop'
$ProgressPreference='SilentlyContinue'
$utf8=[Text.UTF8Encoding]::new($false)
if (Test-Path (Join-Path $OutputDir 'gui-benchmark.json')) { throw 'Existing benchmark: preserve it and choose a new output directory' }
New-Item -ItemType Directory -Force $OutputDir | Out-Null
$rustProfile=Join-Path $OutputDir 'rust-profile'
# Use the application-owned configuration directory in an isolated profile.
$rustConfigDir=Join-Path $rustProfile '.kohdalab\iv'
New-Item -ItemType Directory -Force $rustConfigDir | Out-Null
$rustSettings=@{source='';meter='';source_model='SIMULATED_SOURCE';meter_model='SIMULATED_METER';simulation=$true;output_dir=(Join-Path $OutputDir 'unused-results');filename='unused';auto_suffix=$true}
[IO.File]::WriteAllText((Join-Path $rustConfigDir 'config.json'),($rustSettings|ConvertTo-Json -Depth 6),$utf8)
function Find-Gui([string]$Package) {
 foreach($name in @('kohdalab-iv.exe','kohdalab-iv-next.exe')) {
  $path=Join-Path $Package $name
  if(Test-Path -LiteralPath $path -PathType Leaf) { return $path }
 }
 throw 'GUI executable not found'
}
$beforeExe=Find-Gui $BeforePackage
$afterExe=Find-Gui $AfterPackage
$report=[ordered]@{
    started_utc=[DateTime]::UtcNow.ToString('o'); repeats=$Repeats
    before_exe=$beforeExe; before_sha256=(Get-FileHash $beforeExe).Hash.ToLowerInvariant()
    after_exe=$afterExe; after_sha256=(Get-FileHash $afterExe).Hash.ToLowerInvariant()
    os=[Environment]::OSVersion.VersionString
    cpu=(Get-CimInstance Win32_Processor | Select-Object Name,NumberOfCores,NumberOfLogicalProcessors)
    machine=(Get-CimInstance Win32_ComputerSystem | Select-Object Model,TotalPhysicalMemory)
    conditions="1240x760 window, simulated config, no connections, driver setup disabled in both, no GPU memory counter, 25ms startup polling, $IdleSettleSeconds s settle + 2s idle sample; no cache purge"
    script_sha256=(Get-FileHash $PSCommandPath).Hash.ToLowerInvariant()
    rows=@()
}
$reportPath=Join-Path $OutputDir 'gui-benchmark.json'
function Save-Report {
    [IO.File]::WriteAllText($reportPath,($report|ConvertTo-Json -Depth 12),$utf8)
}
function Get-TreeSnapshot([int[]]$ProcessIds) {
    $processes=@(foreach ($benchId in $ProcessIds) { Get-Process -Id $benchId -ErrorAction SilentlyContinue })
    $working=0L;$private=0L;$cpu=0.0;$details=@()
    foreach ($process in $processes) {
        $process.Refresh()
        $working += $process.WorkingSet64
        $private += $process.PrivateMemorySize64
        $cpu += $process.TotalProcessorTime.TotalSeconds
        $details += @{pid=$process.Id;name=$process.ProcessName;working_set_bytes=$process.WorkingSet64;private_commit_bytes=$process.PrivateMemorySize64}
    }
    return @{working_set_bytes=$working; private_commit_bytes=$private; cpu_s=$cpu; process_count=$processes.Count;processes=$details}
}
function Invoke-GuiRun([string]$Engine,[int]$Repeat,[bool]$Warmup) {
    $info=[Diagnostics.ProcessStartInfo]::new()
    $info.UseShellExecute=$false
    if ($Engine -eq 'before') {
        $info.FileName=$beforeExe;$info.WorkingDirectory=$BeforePackage
    } else {
        $info.FileName=$afterExe;$info.WorkingDirectory=$AfterPackage
    }
    $info.EnvironmentVariables['USERPROFILE']=$rustProfile
    $info.EnvironmentVariables.Remove('QT_QPA_PLATFORM')
    $process=[Diagnostics.Process]::new();$process.StartInfo=$info
    $started=[DateTime]::UtcNow
    $clock=[Diagnostics.Stopwatch]::StartNew()
    [void]$process.Start()
    $benchProcessId=$process.Id
    $ready=$false
    while ($clock.Elapsed.TotalSeconds -lt 30) {
        $process.Refresh()
        if ($process.HasExited) { throw "$Engine GUI exited before its window appeared" }
        # A visible main-window handle is an OS-level startup proxy, not first paint.
        if ($process.MainWindowHandle -ne 0 -and $process.MainWindowTitle -like 'KohdaLab IV v*') { $ready=$true;break }
        Start-Sleep -Milliseconds 25
    }
    if (!$ready) { throw "GUI startup timed out at PID $benchProcessId; inspect it before retrying" }
    $startup=$clock.Elapsed.TotalSeconds
    Start-Sleep -Milliseconds ([int]($IdleSettleSeconds*1000))
    $all=@(Get-CimInstance Win32_Process | Select-Object ProcessId,ParentProcessId)
    $ids=[Collections.Generic.HashSet[int]]::new();[void]$ids.Add($benchProcessId)
    do {
        $changed=$false
        foreach ($child in $all) { if ($ids.Contains([int]$child.ParentProcessId) -and $ids.Add([int]$child.ProcessId)) { $changed=$true } }
    } while ($changed)
    $treeIds=@($ids)
    $base=Get-TreeSnapshot $treeIds
    $idleClock=[Diagnostics.Stopwatch]::StartNew()
    $samples=@()
    while ($idleClock.Elapsed.TotalSeconds -lt 2.0) {
        $sample=Get-TreeSnapshot $treeIds
        $samples += [ordered]@{relative_s=$idleClock.Elapsed.TotalSeconds;working_set_bytes=$sample.working_set_bytes;private_commit_bytes=$sample.private_commit_bytes;cpu_s=$sample.cpu_s;process_count=$sample.process_count;processes=$sample.processes}
        Start-Sleep -Milliseconds 100
    }
    $last=Get-TreeSnapshot $treeIds
    $idleSeconds=$idleClock.Elapsed.TotalSeconds
    $row=[ordered]@{engine=$Engine;repeat=$Repeat;warmup=$Warmup;pid=$benchProcessId;tree_pids=$treeIds;started_utc=$started.ToString('o');window_visible_s=$startup;idle_s=$idleSeconds;idle_cpu_s=$last.cpu_s-$base.cpu_s;idle_single_core_cpu_percent=100.0*($last.cpu_s-$base.cpu_s)/$idleSeconds;idle_samples=$samples;termination='controlled simulated, unconnected GUI process terminated after observation'}
    # Process lifecycle control only; simulation was never connected or started.
    Stop-Process -Id $benchProcessId -ErrorAction Stop
    [void]$process.WaitForExit(5000)
    $deadline=[DateTime]::UtcNow.AddSeconds(8)
    do {
        $remaining=@(foreach ($childId in $treeIds) { Get-Process -Id $childId -ErrorAction SilentlyContinue })
        if (!$remaining.Count) { break }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    if ($remaining.Count) { throw "Child processes still running: $($remaining.Id -join ',')" }
    return $row
}
Save-Report
if (!$SkipWarmup) { foreach ($engine in @('before','after')) { $report.rows += Invoke-GuiRun $engine 0 $true; Save-Report } }
for ($repeat=1;$repeat -le $Repeats;$repeat++) {
    $order=if ($repeat%2) { @('before','after') } else { @('after','before') }
    foreach ($engine in $order) {
        $report.rows += Invoke-GuiRun $engine $repeat $false
        Save-Report
    }
}
$report.ended_utc=[DateTime]::UtcNow.ToString('o')
Save-Report
Write-Output "Completed: $reportPath"
