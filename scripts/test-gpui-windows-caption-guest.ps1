$ErrorActionPreference = 'Stop'
$inputRoot = 'C:/GpuiCaptionInput'
$outputRoot = 'C:/GpuiCaptionOutput'
$marker = Get-Content -LiteralPath "$inputRoot/marker.json" -Raw | ConvertFrom-Json
if ($env:USERNAME -ne 'WDAGUtilityAccount' -or $env:COMPUTERNAME -eq $marker.hostComputer -or
    $marker.kind -ne 'gpui-caption-acceptance-v1') { throw 'Disposable caption guest required' }
$report = @{ nonce=$marker.nonce; guest=$true; passed=$false }
try {
  # GPUI debug builds compile shaders from their baked-in source path. Create
  # that path only inside this disposable guest, from read-only input copies.
  New-Item -ItemType Directory -Path $marker.shaderPath -Force | Out-Null
  Get-ChildItem -LiteralPath "$inputRoot/shaders" -File | Copy-Item -Destination $marker.shaderPath
  Add-Type -AssemblyName UIAutomationClient,UIAutomationTypes
  Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class CaptionGuest {
  [DllImport("user32.dll")] public static extern bool IsIconic(IntPtr hwnd);
  [DllImport("user32.dll")] public static extern bool IsZoomed(IntPtr hwnd);
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr hwnd, int command);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hwnd);
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hwnd, out uint pid);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint flags, uint dx, uint dy, uint data, UIntPtr extra);
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [DllImport("user32.dll")] public static extern IntPtr SendMessage(IntPtr hwnd, uint message, IntPtr wparam, IntPtr lparam);
  [DllImport("user32.dll")] public static extern bool MoveWindow(IntPtr hwnd, int x, int y, int width, int height, bool repaint);
  [StructLayout(LayoutKind.Sequential)] public struct Rect { public int Left, Top, Right, Bottom; }
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hwnd, out Rect rect);
}
'@
  [CaptionGuest]::SetProcessDPIAware() | Out-Null
  $env:INDUSTRIALIS_DATA_DIR = 'C:/GpuiCaptionProfile'
  $env:INDUSTRIALIS_DEV_WATCH = $null
  $env:GPUI_DISABLE_DIRECT_COMPOSITION = $null
  New-Item -ItemType Directory -Path $env:INDUSTRIALIS_DATA_DIR -Force | Out-Null
  $preferences = '{"launch_maximized":true,"theme_preset":"sandstone","theme_mode":"dark"}'
  if ($marker.scenario -in @('about','explorer','easy','files')) {
    # Flat imported-style colors reproduce the barely visible input/outline
    # boundary without any user profile, OAuth, or network dependency.
    $preferences = '{"launch_maximized":false,"theme_preset":"monochrome","theme_mode":"dark","theme_overrides":{"background":"#303446","foreground":"#c6d0f5","card":"#303446","card_foreground":"#c6d0f5","popover":"#292c3c","popover_foreground":"#c6d0f5","primary":"#ca9ee6","primary_foreground":"#232634","secondary":"#626880","secondary_foreground":"#c6d0f5","muted":"#414559","muted_foreground":"#b5bfe2","accent":"#414559","accent_foreground":"#c6d0f5","input":"#292c3c","border":"#292c3c","ring":"#ca9ee6"}}'
  }
  [IO.File]::WriteAllText("$env:INDUSTRIALIS_DATA_DIR/launcher-settings.json",$preferences)
  if ($marker.scenario -in @('explorer','easy','files')) {
    $instance = "$env:INDUSTRIALIS_DATA_DIR/instances/PerfPack"
    New-Item -ItemType Directory -Path "$instance/.minecraft/config" -Force | Out-Null
    [IO.File]::WriteAllText("$instance/instance.json",'{"name":"PerfPack","pack_version":"test"}')
    [IO.File]::WriteAllText("$instance/mmc-pack.json",'{}')
    if ($marker.scenario -eq 'files') {
      New-Item -ItemType Directory -Path "$instance/.minecraft/serverutilities/server" -Force | Out-Null
      [IO.File]::WriteAllText("$instance/.minecraft/serverutilities/server/ranks.txt","# ranks fixture`nplayer=test`n")
    } elseif ($marker.scenario -eq 'easy') {
      $config = "# Preserve this comment`ngeneral {`n B:enabled=true`n"
      for ($index=1; $index -lt 500; $index++) {
        $config += " # Description for setting $index`n I:setting$index=42`n"
      }
      [IO.File]::WriteAllText("$instance/.minecraft/config/easy-benchmark.cfg",$config+"}`n")
    } else { for ($index=0; $index -lt 5000; $index++) {
      [IO.File]::WriteAllText((Join-Path "$instance/.minecraft/config" ('file-{0:d5}.cfg' -f $index)),"general {`n B:enabled=true`n}`n")
    } }
  }
  $app = Start-Process -FilePath "$inputRoot/industrialis-dev.exe" -PassThru -RedirectStandardError "$outputRoot/stderr.log"
  $ownedHandle = $app.Handle
  $deadline = [DateTime]::UtcNow.AddSeconds(45)
  do {
    Start-Sleep -Milliseconds 200
    $app.Refresh()
    if ($app.HasExited) { throw "Launcher exited before caption test: $($app.ExitCode)" }
    if ([DateTime]::UtcNow -gt $deadline) { throw 'Launcher window did not appear' }
  } while ($app.MainWindowHandle -eq 0)
  $window = $app.MainWindowHandle
  if ($marker.scenario -eq 'files') {
    . "$inputRoot/files.ps1"
    return
  }
  if ($marker.scenario -eq 'easy') {
    . "$inputRoot/easy.ps1"
    return
  }
  if ($marker.scenario -eq 'explorer') {
    . "$inputRoot/explorer.ps1"
    return
  }
  if ($marker.scenario -in @('appearance','about')) {
    . "$inputRoot/appearance.ps1"
    return
  }
  Start-Sleep -Milliseconds 700
  $root = [Windows.Automation.AutomationElement]::FromHandle($window)
  $condition = New-Object Windows.Automation.PropertyCondition ([Windows.Automation.AutomationElement]::NameProperty),'Processes'
  $tab = $root.FindFirst([Windows.Automation.TreeScope]::Descendants,$condition)
  if ($null -eq $tab) { throw 'Processes tab missing' }
  $tab.SetFocus()
  Add-Type -AssemblyName System.Windows.Forms
  [Windows.Forms.SendKeys]::SendWait('{ENTER}')
  foreach ($maximized in @($false,$true)) {
    [CaptionGuest]::ShowWindow($window, $(if ($maximized) {3} else {9})) | Out-Null
    [CaptionGuest]::SetForegroundWindow($window) | Out-Null
    Start-Sleep -Milliseconds 700
    [uint32]$owner = 0
    [CaptionGuest]::GetWindowThreadProcessId($window,[ref]$owner) | Out-Null
    if ($owner -ne $app.Id -or [CaptionGuest]::GetForegroundWindow() -ne $window) { throw 'Owned foreground guest launcher required' }
    if ([CaptionGuest]::IsZoomed($window) -ne $maximized) { throw 'Could not establish requested window state' }
    $root = [Windows.Automation.AutomationElement]::FromHandle($window)
    $condition = New-Object Windows.Automation.PropertyCondition ([Windows.Automation.AutomationElement]::NameProperty),'Minimize window'
    $button = $root.FindFirst([Windows.Automation.TreeScope]::Descendants,$condition)
    if ($null -eq $button) { throw 'Minimize window button missing' }
    $bounds = $button.Current.BoundingRectangle
    if ($bounds.IsEmpty -or $bounds.Width -le 0) { throw 'Minimize window has no painted bounds' }
    [CaptionGuest]::SetCursorPos([int]($bounds.X+$bounds.Width/2),[int]($bounds.Y+$bounds.Height/2)) | Out-Null
    Start-Sleep -Milliseconds 150
    $x = [int]($bounds.X+$bounds.Width/2)
    $y = [int]($bounds.Y+$bounds.Height/2)
    $hit = [CaptionGuest]::SendMessage($window,0x84,[IntPtr]::Zero,[IntPtr](($y -shl 16) -bor ($x -band 65535)))
    if ($hit.ToInt64() -ne 8) { throw "Minimize is not a native caption target: hit-test=$hit (expected HTMINBUTTON=8)" }
    [CaptionGuest]::mouse_event(2,0,0,0,[UIntPtr]::Zero)
    Start-Sleep -Milliseconds 80
    [CaptionGuest]::mouse_event(4,0,0,0,[UIntPtr]::Zero)
    $deadline = [DateTime]::UtcNow.AddSeconds(3)
    while (-not [CaptionGuest]::IsIconic($window) -and [DateTime]::UtcNow -lt $deadline) { Start-Sleep -Milliseconds 50 }
    if (-not [CaptionGuest]::IsIconic($window)) { throw "Clicking minimize did not minimize the window (maximized=$maximized)" }
    Start-Sleep -Milliseconds 400
    if (-not [CaptionGuest]::IsIconic($window)) { throw 'Launcher unexpectedly restored itself after minimize' }
    $report[$(if ($maximized) {'maximizedPointerMinimize'} else {'windowedPointerMinimize'})] = $true
    [CaptionGuest]::ShowWindow($window, $(if ($maximized) {3} else {9})) | Out-Null
    [CaptionGuest]::SetForegroundWindow($window) | Out-Null
    Start-Sleep -Milliseconds 500
    $root = [Windows.Automation.AutomationElement]::FromHandle($window)
    $button = $root.FindFirst([Windows.Automation.TreeScope]::Descendants,$condition)
    if ($null -eq $button) { throw 'Minimize window button missing after restore' }
    $button.SetFocus()
    [Windows.Forms.SendKeys]::SendWait('{ENTER}')
    $deadline = [DateTime]::UtcNow.AddSeconds(3)
    while (-not [CaptionGuest]::IsIconic($window) -and [DateTime]::UtcNow -lt $deadline) { Start-Sleep -Milliseconds 50 }
    if (-not [CaptionGuest]::IsIconic($window)) { throw "Keyboard minimize failed (maximized=$maximized)" }
    $report[$(if ($maximized) {'maximizedKeyboardMinimize'} else {'windowedKeyboardMinimize'})] = $true
  }
  $report.passed = $true
} catch {
  $report.passed = $false
  $report.error = $_.Exception.Message
} finally {
  $report | ConvertTo-Json | Set-Content -LiteralPath "$outputRoot/result.json" -Encoding UTF8
  # Everything on this desktop belongs to the disposable guest.
  Stop-Computer -Force
}
