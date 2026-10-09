#Requires -Version 7
<#
.SYNOPSIS
    Dump what an app exposes to screen readers (its UI Automation tree) and score it.

.DESCRIPTION
    Launches the exe, waits for its window and -SettleSec more for the first
    data, then walks the UIA control view under the main window. That is the
    tree Narrator, NVDA and Accessibility Insights read. Records:
      - elements:   UIA elements under the window
      - named:      elements with a non-empty Name (a screen reader can say something)
      - focusable:  elements reachable by keyboard
      - by_type:    count per control type (Button, DataItem, Text, ...)
    Appends a summary to bench/a11y.jsonl and saves the full tree next to it
    as a11y-trees/<label>.txt. This is a scripted proxy, not a replacement
    for an Accessibility Insights FastPass or listening with NVDA.

.EXAMPLE
    ./bench/a11y.ps1 -Exe "$env:LOCALAPPDATA\TaskManagerPlus\taskmanagerplus-lite.exe" -Label "egui lite 2.7.2 (installed, live data)"
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory)] [string] $Exe,
    [string] $Label,
    [ValidateRange(0, 120)] [int] $SettleSec = 3,
    [ValidateRange(1, 300)] [int] $TimeoutSec = 30,
    [ValidateRange(1, 20000)] [int] $MaxElements = 5000,
    [string] $Results = (Join-Path $PSScriptRoot 'a11y.jsonl')
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes

$Exe = (Resolve-Path -LiteralPath $Exe).Path
if (-not $Label) { $Label = [IO.Path]::GetFileNameWithoutExtension($Exe) }
if (@(Get-Process | Where-Object { $_.Path -eq $Exe })) { throw "$Exe is already running. Close it first." }

$proc = Start-Process -FilePath $Exe -WorkingDirectory (Split-Path $Exe) -PassThru
try {
    $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSec)
    while ($true) {
        $proc.Refresh()
        if ($proc.HasExited) { throw "exited during startup (code $($proc.ExitCode))" }
        if ($proc.MainWindowHandle -ne [IntPtr]::Zero) { break }
        if ([DateTime]::UtcNow -gt $deadline) { throw "no visible window after $TimeoutSec s" }
        Start-Sleep -Milliseconds 50
    }
    Start-Sleep -Seconds $SettleSec

    # Re-read the handle: some toolkits replace their first window during startup.
    $proc.Refresh()
    $root = [Windows.Automation.AutomationElement]::FromHandle($proc.MainWindowHandle)
    $walker = [Windows.Automation.TreeWalker]::ControlViewWalker
    $lines = [Collections.Generic.List[string]]::new()
    $byType = @{}
    $count = 0; $named = 0; $focusable = 0

    # Depth-first walk under the window (the window itself isn't counted).
    $stack = [Collections.Generic.Stack[object]]::new()
    $child = $walker.GetFirstChild($root)
    while ($child) { $stack.Push(@($child, 1)); $child = $walker.GetNextSibling($child) }
    while ($stack.Count -and $count -lt $MaxElements) {
        $el, $depth = $stack.Pop()
        try { $c = $el.Current } catch { continue }   # element vanished mid-walk
        $count++
        $type = if ($c.ControlType) { $c.ControlType.ProgrammaticName -replace '^ControlType\.', '' } else { '(none)' }
        $byType[$type] = 1 + ($byType[$type] ?? 0)
        if ($c.Name) { $named++ }
        if ($c.IsKeyboardFocusable) { $focusable++ }
        $lines.Add(('  ' * $depth) + "$type `"$($c.Name)`"" + $(if ($c.IsKeyboardFocusable) { ' [focusable]' } else { '' }))
        $kids = [Collections.Generic.List[object]]::new()
        $k = $walker.GetFirstChild($el)
        while ($k) { $kids.Add($k); $k = $walker.GetNextSibling($k) }
        for ($i = $kids.Count - 1; $i -ge 0; $i--) { $stack.Push(@($kids[$i], $depth + 1)) }
    }
    $windowName = $root.Current.Name
}
finally {
    if (-not $proc.HasExited) {
        $null = $proc.CloseMainWindow()
        if (-not $proc.WaitForExit(5000)) { Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue }
    }
}

$treeDir = Join-Path (Split-Path $Results) 'a11y-trees'
New-Item -ItemType Directory -Force -Path $treeDir | Out-Null
$safe = ($Label -replace '[^\w\.\- ]', '').Trim() -replace '\s+', '-'
$treeFile = Join-Path $treeDir "$safe.txt"
@("Window `"$windowName`"") + $lines | Set-Content -LiteralPath $treeFile -Encoding utf8

$sortedTypes = [ordered]@{}
foreach ($t in $byType.GetEnumerator() | Sort-Object Value -Descending) { $sortedTypes[$t.Key] = $t.Value }

$record = [ordered]@{
    label     = $Label
    version   = (Get-Item -LiteralPath $Exe).VersionInfo.ProductVersion
    measured  = (Get-Date).ToString('o')
    window    = $windowName
    elements  = $count
    named     = $named
    focusable = $focusable
    truncated = $count -ge $MaxElements
    by_type   = $sortedTypes
}
$record | ConvertTo-Json -Depth 4 -Compress | Add-Content -LiteralPath $Results -Encoding utf8

Write-Host "[$Label] window `"$windowName`": $count elements, $named named, $focusable keyboard-focusable" -ForegroundColor Green
$byType.GetEnumerator() | Sort-Object Value -Descending | Format-Table Name, Value -AutoSize | Out-String | Write-Host
Write-Host "Tree: $treeFile"
