#Requires -Version 7
<#
.SYNOPSIS
    Print the latest measurement of each build in results.jsonl as a Markdown table.

.EXAMPLE
    ./bench/report.ps1
    ./bench/report.ps1 | Set-Clipboard
#>
[CmdletBinding()]
param([string] $Results = (Join-Path $PSScriptRoot 'results.jsonl'))

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

if (-not (Test-Path -LiteralPath $Results)) { throw "No results yet: $Results" }

$latest = Get-Content -LiteralPath $Results |
    Where-Object { $_.Trim() } |
    ForEach-Object { $_ | ConvertFrom-Json } |
    Group-Object label |
    ForEach-Object { $_.Group | Sort-Object { [DateTimeOffset] $_.measured } | Select-Object -Last 1 } |
    Sort-Object { $_.median.private_ws_mb }

'| Build | Startup (ms) | Memory, private WS (MB) | Private bytes (MB) | Idle CPU (% of one core) | Idle CPU (% of machine) | Processes | Exe (MB) | Power | Measured |'
'|---|---:|---:|---:|---:|---:|---:|---:|---|---|'
foreach ($r in $latest) {
    $m = $r.median
    $power = if ($r.machine.on_ac) { 'AC' } else { 'battery' }
    $date = ([DateTimeOffset] $r.measured).ToString('yyyy-MM-dd')
    "| $($r.label) | $($m.startup_ms) | $($m.private_ws_mb) | $($m.private_bytes_mb) | $($m.cpu_pct_one_core) | $($m.cpu_pct_machine) | $($m.processes) | $($r.exe_mb) | $power | $date |"
}
''
$first = $latest | Select-Object -First 1
"Median of $($first.method.runs) runs; $($first.method.warmup_sec) s warmup, $($first.method.sample_sec) s idle sample. $($first.machine.cpu), $($first.machine.logical) logical processors, $($first.machine.ram_gb) GB."
