#Requires -Version 7
<#
.SYNOPSIS
    List every toggleable control (checkbox, switch) in a window, with the name
    Windows UI Automation reports for it.

.DESCRIPTION
    Launches the exe (with optional arguments), waits for its window, then finds
    every element that supports UIA's TogglePattern. That covers checkboxes and
    switches. An empty Name is what a screen reader has nothing to announce for.

.EXAMPLE
    # gpui-kit's story gallery, opened on the Settings story
    ./toggles.ps1 -Exe ..\..\..\gpui-kit\target\debug\gpui-component-story.exe -Arguments settings
#>
param(
    [Parameter(Mandatory)] [string] $Exe,
    [string[]] $Arguments = @(),
    [int] $SettleSec = 6
)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes

$Exe = (Resolve-Path -LiteralPath $Exe).Path
$start = @{ FilePath = $Exe; PassThru = $true; WorkingDirectory = (Split-Path $Exe) }
if ($Arguments.Count) { $start.ArgumentList = $Arguments }
$proc = Start-Process @start
try {
    $deadline = [DateTime]::UtcNow.AddSeconds(60)
    do { Start-Sleep -Milliseconds 200; $proc.Refresh() } until ($proc.MainWindowHandle -ne 0 -or [DateTime]::UtcNow -gt $deadline)
    Start-Sleep -Seconds $SettleSec
    # Debug builds are console apps, so .NET's "main window" can be the console or zero.
    # Pick the process's titled top-level window that isn't a console instead.
    $ofProcess = New-Object Windows.Automation.PropertyCondition(
        [Windows.Automation.AutomationElement]::ProcessIdProperty, $proc.Id)
    $deadline = [DateTime]::UtcNow.AddSeconds(15)
    do {
        $root = [Windows.Automation.AutomationElement]::RootElement.FindAll(
            [Windows.Automation.TreeScope]::Children, $ofProcess) |
            Where-Object { $_.Current.Name -and $_.Current.ClassName -notmatch 'Console' } |
            Select-Object -First 1
        if (-not $root) { Start-Sleep -Milliseconds 250 }
    } until ($root -or [DateTime]::UtcNow -gt $deadline)
    if (-not $root) { throw "No app window found for PID $($proc.Id)" }
    $toggleable = New-Object Windows.Automation.PropertyCondition(
        [Windows.Automation.AutomationElement]::IsTogglePatternAvailableProperty, $true)
    $found = $root.FindAll([Windows.Automation.TreeScope]::Descendants, $toggleable)
    "Window `"$($root.Current.Name)`": $($found.Count) toggleable controls"
    foreach ($el in $found) {
        $c = $el.Current
        $state = $el.GetCurrentPattern([Windows.Automation.TogglePattern]::Pattern).Current.ToggleState
        $type = if ($c.ControlType) { $c.ControlType.ProgrammaticName -replace '^ControlType\.', '' } else { '(none)' }
        $name = if ($c.Name) { "`"$($c.Name)`"" } else { '(no name)' }
        "  {0,-9} {1,-4} {2}" -f $type, $state, $name
    }
}
finally {
    if (-not $proc.HasExited) { $null = $proc.CloseMainWindow(); if (-not $proc.WaitForExit(5000)) { Stop-Process -Id $proc.Id -Force } }
}
