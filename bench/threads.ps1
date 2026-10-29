#Requires -Version 7
# Launch an exe, let it settle, then report CPU time per thread over a window,
# with each thread's description (Rust sets it from the thread name).
param(
    [Parameter(Mandatory)] [string] $Exe,
    [int] $SettleSec = 8,
    [int] $SampleSec = 20,
    [hashtable] $Env = @{},
    [switch] $Minimize
)
$ErrorActionPreference = 'Stop'
Add-Type @'
using System; using System.Runtime.InteropServices;
public static class T {
  [DllImport("kernel32.dll")] public static extern IntPtr OpenThread(uint access, bool inherit, uint id);
  [DllImport("kernel32.dll")] public static extern bool CloseHandle(IntPtr h);
  [DllImport("kernel32.dll", CharSet = CharSet.Unicode)] public static extern int GetThreadDescription(IntPtr h, out IntPtr desc);
  [DllImport("kernel32.dll")] public static extern IntPtr LocalFree(IntPtr p);
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int cmd);
  public static string Name(int id) {
    IntPtr h = OpenThread(0x0800 /* QUERY_LIMITED_INFORMATION */, false, (uint)id);
    if (h == IntPtr.Zero) return "";
    try {
      IntPtr p;
      if (GetThreadDescription(h, out p) < 0) return "";
      string s = Marshal.PtrToStringUni(p); LocalFree(p); return s ?? "";
    } finally { CloseHandle(h); }
  }
}
'@
foreach ($k in $Env.Keys) { Set-Item "env:$k" $Env[$k] }
$p = Start-Process -FilePath (Resolve-Path $Exe).Path -PassThru
foreach ($k in $Env.Keys) { Set-Item "env:$k" $null }
try {
    Start-Sleep -Seconds $SettleSec
    $p.Refresh()
    if ($Minimize) { [void][T]::ShowWindow($p.MainWindowHandle, 6); Start-Sleep -Seconds 2; $p.Refresh(); 'window minimized' }
    $before = @{}
    foreach ($t in $p.Threads) { $before[$t.Id] = $t.TotalProcessorTime.TotalMilliseconds }
    $total0 = $p.TotalProcessorTime.TotalMilliseconds
    Start-Sleep -Seconds $SampleSec
    $p.Refresh()
    $rows = foreach ($t in $p.Threads) {
        $b = if ($before.ContainsKey($t.Id)) { $before[$t.Id] } else { 0 }
        [pscustomobject]@{
            Id       = $t.Id
            Name     = [T]::Name($t.Id)
            CpuMs    = [math]::Round($t.TotalProcessorTime.TotalMilliseconds - $b, 1)
            Priority = $t.PriorityLevel
        }
    }
    $total = $p.TotalProcessorTime.TotalMilliseconds - $total0
    "process: {0:N0} ms CPU over {1} s = {2:N2}% of one core, {3} threads" -f $total, $SampleSec, ($total / ($SampleSec * 10)), @($p.Threads).Count
    $rows | Sort-Object CpuMs -Descending | Select-Object -First 12 | Format-Table -AutoSize | Out-String
}
finally { if (-not $p.HasExited) { Stop-Process -Id $p.Id -Force } }
