#Requires -Version 7
<#
.SYNOPSIS
    Run one repro example and print what Windows UI Automation sees.

.DESCRIPTION
    Dumps the UIA control view under the window in tree order, which is the
    order screen readers read. With -SetValue, it also calls
    ValuePattern.SetValue on the first Edit control, then prints the field's
    value and the UI's own "Current value" label, so you can see whether the
    app received the text.

.EXAMPLE
    ./probe.ps1 table
    ./probe.ps1 order
    ./probe.ps1 text_edit -SetValue hello
    ./probe.ps1 text_edit -SetValue hello -Exe target\fork\release\examples\text_edit.exe
#>
param(
    [Parameter(Mandatory)] [ValidateSet('table', 'table_virtual', 'text_edit', 'order')] [string] $Example,
    [string] $SetValue,
    # Probe a different build, e.g. one patched against a local egui checkout.
    [string] $Exe
)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes

$exe = if ($Exe) { $Exe } else { Join-Path $PSScriptRoot "target\release\examples\$Example.exe" }
if (-not (Test-Path $exe)) { throw "Build first: cargo build --release --examples" }

$proc = Start-Process $exe -PassThru
try {
    $deadline = [DateTime]::UtcNow.AddSeconds(20)
    do { Start-Sleep -Milliseconds 100; $proc.Refresh() } until ($proc.MainWindowHandle -ne 0 -or [DateTime]::UtcNow -gt $deadline)
    Start-Sleep -Seconds 2
    $proc.Refresh()
    $root = [Windows.Automation.AutomationElement]::FromHandle($proc.MainWindowHandle)
    $walker = [Windows.Automation.TreeWalker]::ControlViewWalker

    function Show-Tree($el, [int] $depth) {
        $child = $walker.GetFirstChild($el)
        while ($child) {
            $c = $child.Current
            $type = if ($c.ControlType) { $c.ControlType.ProgrammaticName -replace '^ControlType\.', '' } else { '(no control type)' }
            "{0}{1} `"{2}`"" -f ('  ' * $depth), $type, $c.Name
            Show-Tree $child ($depth + 1)
            $child = $walker.GetNextSibling($child)
        }
    }
    "Window `"$($root.Current.Name)`""
    Show-Tree $root 1

    if ($SetValue) {
        $editCond = New-Object Windows.Automation.PropertyCondition(
            [Windows.Automation.AutomationElement]::ControlTypeProperty, [Windows.Automation.ControlType]::Edit)
        $edit = $root.FindFirst([Windows.Automation.TreeScope]::Descendants, $editCond)
        if (-not $edit) { throw 'No Edit control found' }
        $vp = $edit.GetCurrentPattern([Windows.Automation.ValuePattern]::Pattern)
        ''
        "ValuePattern.IsReadOnly = $($vp.Current.IsReadOnly)"
        "Calling ValuePattern.SetValue('$SetValue')"
        $vp.SetValue($SetValue)
        Start-Sleep -Seconds 1
        "Edit value after:  '$($vp.Current.Value)'"
        $label = $root.FindAll([Windows.Automation.TreeScope]::Descendants, [Windows.Automation.Condition]::TrueCondition) |
            Where-Object { $_.Current.Name -like 'Current value*' } | Select-Object -First 1
        "App label after:   $(if ($label) { $label.Current.Name } else { '(not found)' })"
    }
}
finally {
    if (-not $proc.HasExited) { $null = $proc.CloseMainWindow(); if (-not $proc.WaitForExit(5000)) { Stop-Process -Id $proc.Id -Force } }
}
