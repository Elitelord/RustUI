#Requires -Version 7
<#
.SYNOPSIS
    Measure one build of a desktop app: startup time, idle memory, idle CPU, and exe size.

.DESCRIPTION
    Each run launches the exe and waits for its main window to become visible
    (startup). It then lets the app idle for -WarmupSec, samples the whole
    process tree over -SampleSec (CPU, then memory at the end), and closes it.

    The whole process tree is measured. A WebView2 app's renderer, GPU and
    utility processes are children of the app, so they count against it here,
    the same way TaskManagerPlus's docs/LITE_MODE.md counted them.

    Startup is measured from process creation (the kernel's start time) to a
    visible main window, so the script's own polling overhead is excluded.
    That is not "first frame painted": a toolkit that shows its window before
    painting will look slightly faster than it is. Every run after the first is
    a warm-cache start.

    Appends one JSON line per invocation to -Results (bench/results.jsonl by
    default). Turn it into a table with bench/report.ps1.

.EXAMPLE
    ./bench/measure.ps1 -Exe "$env:LOCALAPPDATA\TaskManagerPlus\taskmanagerplus-lite.exe" -Label "egui lite 2.7.2 (installed, live data)"
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory)] [string] $Exe,
    [string] $Label,
    [string[]] $ArgumentList = @(),
    [ValidateRange(1, 20)] [int] $Runs = 3,
    [ValidateRange(0, 600)] [int] $WarmupSec = 5,
    [ValidateRange(1, 600)] [int] $SampleSec = 30,
    [ValidateRange(1, 300)] [int] $TimeoutSec = 30,
    [string] $Results = (Join-Path $PSScriptRoot 'results.jsonl')
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$Exe = (Resolve-Path -LiteralPath $Exe).Path
if (-not $Label) { $Label = [IO.Path]::GetFileNameWithoutExtension($Exe) }

# A second copy of a single-instance app would just focus the first and exit.
$running = @(Get-Process | Where-Object { $_.Path -eq $Exe })
if ($running) {
    throw "$Exe is already running (PID $($running.Id -join ', ')). Close it first."
}

# The root process plus everything it spawned, as (Id, StartTime) pairs.
# Children must be no older than the root, so a reused parent PID can't pull
# in an unrelated older process.
function Get-Tree([System.Diagnostics.Process] $Root) {
    $all = Get-CimInstance Win32_Process -Property ProcessId, ParentProcessId, CreationDate
    $kids = @{}
    foreach ($p in $all) {
        $parent = [int] $p.ParentProcessId
        if (-not $kids.ContainsKey($parent)) { $kids[$parent] = [Collections.Generic.List[object]]::new() }
        $kids[$parent].Add($p)
    }
    $tree = [Collections.Generic.List[object]]::new()
    $tree.Add([pscustomobject]@{ Id = $Root.Id; Start = $Root.StartTime })
    $queue = [Collections.Generic.Queue[int]]::new()
    $queue.Enqueue($Root.Id)
    while ($queue.Count) {
        $id = $queue.Dequeue()
        if (-not $kids.ContainsKey($id)) { continue }
        foreach ($c in $kids[$id]) {
            $cid = [int] $c.ProcessId
            if ($cid -eq $id -or $c.CreationDate -lt $Root.StartTime) { continue }
            $tree.Add([pscustomobject]@{ Id = $cid; Start = $c.CreationDate })
            $queue.Enqueue($cid)
        }
    }
    $tree
}

# The live process for a tree entry, or $null if it exited (or the PID was reused).
function Get-Live($Entry) {
    $p = Get-Process -Id $Entry.Id -ErrorAction SilentlyContinue
    if ($p -and [Math]::Abs(($p.StartTime - $Entry.Start).TotalSeconds) -lt 1) { $p } else { $null }
}

function Get-CpuTimes($Tree) {
    $t = @{}
    foreach ($e in $Tree) {
        $p = Get-Live $e
        if ($p -and $p.TotalProcessorTime) { $t[$e.Id] = $p.TotalProcessorTime.TotalSeconds }
    }
    $t
}

function Get-Median([double[]] $Values) {
    $s = @($Values | Sort-Object)
    $mid = [int][Math]::Floor($s.Count / 2)
    if ($s.Count % 2) { $s[$mid] } else { ($s[$mid - 1] + $s[$mid]) / 2 }
}

# Power state changes CPU clocks and timer behavior, so record it with the results.
Add-Type -AssemblyName System.Windows.Forms
$onAc = [Windows.Forms.SystemInformation]::PowerStatus.PowerLineStatus -eq 'Online'
if (-not $onAc) { Write-Warning 'Running on battery. Results are not comparable with runs on AC.' }

$logical = [Environment]::ProcessorCount
$runsOut = [Collections.Generic.List[object]]::new()

for ($run = 1; $run -le $Runs; $run++) {
    Write-Host "[$Label] run $run/$Runs" -ForegroundColor Cyan
    $start = @{ FilePath = $Exe; PassThru = $true; WorkingDirectory = (Split-Path $Exe) }
    if ($ArgumentList.Count) { $start.ArgumentList = $ArgumentList }
    $proc = Start-Process @start
    $tree = $null
    try {
        # Startup: process creation -> visible main window.
        $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSec)
        while ($true) {
            $proc.Refresh()
            if ($proc.HasExited) { throw "exited during startup (code $($proc.ExitCode))" }
            if ($proc.MainWindowHandle -ne [IntPtr]::Zero) { break }
            if ([DateTime]::UtcNow -gt $deadline) { throw "no visible window after $TimeoutSec s" }
            Start-Sleep -Milliseconds 2
        }
        $startupMs = ([DateTime]::UtcNow - $proc.StartTime.ToUniversalTime()).TotalMilliseconds

        Start-Sleep -Seconds $WarmupSec

        # Idle CPU across the tree over the sample window. Processes that start
        # during the window count in full. Processes that exit drop out.
        $tree0 = Get-Tree $proc
        $cpu0 = Get-CpuTimes $tree0
        $clock = [Diagnostics.Stopwatch]::StartNew()
        Start-Sleep -Seconds $SampleSec
        $tree = Get-Tree $proc
        $cpu1 = Get-CpuTimes $tree
        $elapsed = $clock.Elapsed.TotalSeconds
        $cpuSec = 0.0
        foreach ($id in $cpu1.Keys) {
            $cpuSec += $cpu1[$id] - ($cpu0.ContainsKey($id) ? $cpu0[$id] : 0.0)
        }

        # Memory at the end of the idle window.
        $private = 0L; $workingSet = 0L; $alive = 0
        foreach ($e in $tree) {
            $p = Get-Live $e
            if (-not $p) { continue }
            $alive++
            $private += $p.PrivateMemorySize64
            $workingSet += $p.WorkingSet64
        }
        # "Memory" in Task Manager is the private working set. It is only
        # exposed through perf counters.
        $ids = @($tree.Id)
        $privateWs = (Get-CimInstance Win32_PerfFormattedData_PerfProc_Process -Property IDProcess, WorkingSetPrivate |
            Where-Object { $ids -contains [int] $_.IDProcess } |
            Measure-Object -Property WorkingSetPrivate -Sum).Sum

        $r = [pscustomobject]@{
            run              = $run
            startup_ms       = [Math]::Round($startupMs, 1)
            cpu_pct_machine  = [Math]::Round($cpuSec / $elapsed / $logical * 100, 3)
            cpu_pct_one_core = [Math]::Round($cpuSec / $elapsed * 100, 2)
            private_ws_mb    = [Math]::Round($privateWs / 1MB, 1)
            private_bytes_mb = [Math]::Round($private / 1MB, 1)
            working_set_mb   = [Math]::Round($workingSet / 1MB, 1)
            processes        = $alive
        }
        $runsOut.Add($r)
        $r | Format-List | Out-String | Write-Host
    }
    finally {
        if (-not $proc.HasExited) {
            $null = $proc.CloseMainWindow()
            $null = $proc.WaitForExit(5000)
        }
        # Children can outlive the root, as WebView2 helpers sometimes do. Only
        # entries still matching their recorded start time are stopped, so a
        # reused PID is never killed.
        $leftovers = @($proc) + @(if ($tree) { foreach ($e in $tree) { Get-Live $e } })
        foreach ($p in $leftovers) {
            if ($p -and -not $p.HasExited) { Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue }
        }
    }
    if ($run -lt $Runs) { Start-Sleep -Seconds 2 }
}

$median = [ordered]@{}
foreach ($k in 'startup_ms', 'cpu_pct_machine', 'cpu_pct_one_core', 'private_ws_mb', 'private_bytes_mb', 'working_set_mb', 'processes') {
    $median[$k] = Get-Median @($runsOut | ForEach-Object { [double] $_.$k })
}

# Results get committed, so store the path without the user profile in it.
function Get-PortablePath([string] $Path) {
    foreach ($var in 'LOCALAPPDATA', 'APPDATA', 'USERPROFILE') {
        $root = [Environment]::GetEnvironmentVariable($var)
        if ($root -and $Path.StartsWith($root, [StringComparison]::OrdinalIgnoreCase)) {
            return "%$var%" + $Path.Substring($root.Length)
        }
    }
    $Path
}

$cpuName = (Get-CimInstance Win32_Processor | Select-Object -First 1).Name.Trim()
$record = [ordered]@{
    label      = $Label
    exe        = Get-PortablePath $Exe
    exe_mb     = [Math]::Round((Get-Item -LiteralPath $Exe).Length / 1MB, 2)
    version    = (Get-Item -LiteralPath $Exe).VersionInfo.ProductVersion
    measured   = (Get-Date).ToString('o')
    machine    = [ordered]@{
        cpu     = $cpuName
        logical = $logical
        ram_gb  = [Math]::Round((Get-CimInstance Win32_ComputerSystem).TotalPhysicalMemory / 1GB, 1)
        os      = [Environment]::OSVersion.VersionString
        on_ac   = $onAc
    }
    method     = [ordered]@{ runs = $Runs; warmup_sec = $WarmupSec; sample_sec = $SampleSec }
    median     = $median
    runs       = $runsOut
}
New-Item -ItemType Directory -Force -Path (Split-Path $Results) | Out-Null
$record | ConvertTo-Json -Depth 6 -Compress | Add-Content -LiteralPath $Results -Encoding utf8

Write-Host "[$Label] median of $Runs runs" -ForegroundColor Green
[pscustomobject] $median | Format-List | Out-String | Write-Host
Write-Host "Appended to $Results"
