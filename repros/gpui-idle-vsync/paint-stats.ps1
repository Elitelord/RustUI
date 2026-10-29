#Requires -Version 7
# Runs the instrumented build with the vsync guard off and on. For a visible
# window and then a minimized one, reports per second: WM_PAINT messages,
# paints that found no frame callback, frame-callback calls, time spent in
# each, and the main thread's CPU time.
param(
    [Parameter(Mandatory)] [string] $Exe,
    [int] $WindowSec = 10,
    [int] $Runs = 3
)
$ErrorActionPreference = 'Stop'
Add-Type @'
using System; using System.Runtime.InteropServices;
public static class P {
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int cmd);
}
'@

function Read-Stats($file) {
    $line = Get-Content $file | Where-Object { $_ -like 't=*' } | Select-Object -Last 1
    $h = @{}
    foreach ($kv in $line -split ' ') { $k, $v = $kv -split '=', 2; $h[$k] = [double]$v }
    $h
}
function Main-Ms($p) {
    $p.Refresh()
    # The main thread is the process's first thread.
    ($p.Threads | Sort-Object StartTime | Select-Object -First 1).TotalProcessorTime.TotalMilliseconds
}
function Measure-Window($p, $file, $sec) {
    $a = Read-Stats $file; $m0 = Main-Ms $p
    Start-Sleep -Seconds $sec
    $b = Read-Stats $file; $m1 = Main-Ms $p
    $dt = $b['t'] - $a['t']
    $r = [ordered]@{}
    foreach ($k in 'vsync_ticks', 'invalidates', 'skipped_hidden', 'paint_msgs', 'paint_no_callback', 'def_paint', 'frame_callbacks', 'wnd_msgs', 'platform_msgs', 'loop_msgs') {
        $r[$k] = [math]::Round(($b[$k] - $a[$k]) / $dt, 1)
    }
    foreach ($k in 'paint_ns', 'def_paint_ns', 'frame_ns', 'wnd_ns', 'platform_ns') {
        $r[($k -replace '_ns', '_ms')] = [math]::Round(($b[$k] - $a[$k]) / 1e6 / $dt, 2)
    }
    $r['main_cpu_ms'] = [math]::Round(($m1 - $m0) / $dt, 2)
    [pscustomobject]$r
}

$rows = foreach ($run in 1..$Runs) {
    foreach ($guard in '0', '1') {
        $file = Join-Path $PSScriptRoot "paint-stats-$guard-$run.txt"
        Remove-Item $file -ErrorAction SilentlyContinue
        $env:GPUI_PAINT_STATS = $file; $env:GPUI_VSYNC_GUARD = $guard
        $p = Start-Process -FilePath (Resolve-Path $Exe).Path -PassThru
        $env:GPUI_PAINT_STATS = $null; $env:GPUI_VSYNC_GUARD = $null
        try {
            Start-Sleep -Seconds 6
            $v = Measure-Window $p $file $WindowSec
            $p.Refresh(); [void][P]::ShowWindow($p.MainWindowHandle, 6); Start-Sleep -Seconds 2
            $m = Measure-Window $p $file $WindowSec
            $v | Add-Member run $run -PassThru | Add-Member guard $guard -PassThru | Add-Member state visible -PassThru
            $m | Add-Member run $run -PassThru | Add-Member guard $guard -PassThru | Add-Member state minimized -PassThru
        }
        finally { if (-not $p.HasExited) { Stop-Process -Id $p.Id -Force } }
    }
}
$rows | Export-Csv (Join-Path $PSScriptRoot 'paint-stats.csv') -NoTypeInformation
$rows | Format-List | Out-String -Width 300
