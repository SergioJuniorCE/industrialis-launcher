# Runs only inside the explicitly marked disposable Windows Sandbox.
$ErrorActionPreference = 'Stop'
$inputRoot = 'C:\LauncherAcceptanceInput'
$outputRoot = 'C:\LauncherAcceptanceOutput'
$marker = Get-Content -LiteralPath (Join-Path $inputRoot 'marker.json') -Raw | ConvertFrom-Json
if ($env:USERNAME -ne 'WDAGUtilityAccount' -or $env:COMPUTERNAME -eq $marker.hostComputer -or
    $marker.kind -ne 'launcher-expendable-installed-acceptance-v1') { throw 'Disposable guest identity required' }

$report = [ordered]@{ nonce=$marker.nonce; guest=$true; osBuild=[Environment]::OSVersion.Version.ToString(); passed=$false }
function Progress([string]$Message) {
  ('{0:o} {1}' -f [DateTime]::UtcNow,$Message) | Add-Content -LiteralPath (Join-Path $outputRoot 'progress.log')
}
function Run-Installer([string]$File, [string[]]$Arguments) {
  $name = [IO.Path]::GetFileNameWithoutExtension($File)
  $process = Start-Process -FilePath $File -ArgumentList $Arguments -WindowStyle Hidden -PassThru `
    -RedirectStandardOutput (Join-Path $outputRoot ($name + '.stdout.log')) `
    -RedirectStandardError (Join-Path $outputRoot ($name + '.stderr.log'))
  # Windows PowerShell can lose redirected child exit codes without a retained handle.
  $nativeHandle = $process.Handle
  Progress ('Started ' + $name + ' PID ' + $process.Id)
  if (-not $process.WaitForExit(300000)) { throw 'Owned acceptance installer exceeded its deadline' }
  Progress ($name + ' exit code ' + $process.ExitCode)
  if ($process.ExitCode -ne 0) { throw ('Owned acceptance installer failed: ' + $process.ExitCode) }
}
function Owned-Processes([string]$Root) {
  @(Get-CimInstance Win32_Process | Where-Object {
    $_.ExecutablePath -and $_.ExecutablePath.StartsWith($Root + '\',[StringComparison]::OrdinalIgnoreCase)
  })
}
function Snapshot([string]$Root) {
  $snapshot = @{}
  foreach ($file in (Get-ChildItem -LiteralPath $Root -Recurse -File -Force)) {
    $snapshot[$file.FullName.Substring($Root.Length + 1)] = (Get-FileHash -LiteralPath $file.FullName).Hash
  }
  return $snapshot
}
function Assert-Snapshot([string]$Root, $Snapshot) {
  foreach ($name in $Snapshot.Keys) {
    $path = Join-Path $Root $name
    if (-not (Test-Path -LiteralPath $path -PathType Leaf) -or (Get-FileHash -LiteralPath $path).Hash -ne $Snapshot[$name]) {
      throw ('Generated profile bytes changed: ' + $name)
    }
  }
}
function Default-Protocol {
  $key = 'HKCU:\Software\Classes\industrialislauncher\shell\open\command'
  if (Test-Path -LiteralPath $key) { return (Get-Item -LiteralPath $key).GetValue('') }
  return ''
}
function Save-AppDiagnostics($Process) {
  Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;
public static class LauncherGuestWindows {
  delegate bool Callback(IntPtr window, IntPtr parameter);
  [DllImport("user32.dll")] static extern bool EnumWindows(Callback callback, IntPtr parameter);
  [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr window, out uint pid);
  [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr window);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] static extern int GetWindowText(IntPtr window, StringBuilder text, int count);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] static extern int GetClassName(IntPtr window, StringBuilder text, int count);
  public static string[] ForProcess(uint pid) {
    var found = new List<string>();
    EnumWindows((window, parameter) => {
      uint owner; GetWindowThreadProcessId(window, out owner);
      if (owner == pid) {
        var title = new StringBuilder(512); var kind = new StringBuilder(256);
        GetWindowText(window,title,title.Capacity); GetClassName(window,kind,kind.Capacity);
        found.Add(window + " visible=" + IsWindowVisible(window) + " class=" + kind + " title=" + title);
      }
      return true;
    },IntPtr.Zero);
    return found.ToArray();
  }
}
'@
  $Process.Refresh()
  @{ pid=$Process.Id; exited=$Process.HasExited; cpuSeconds=$Process.TotalProcessorTime.TotalSeconds;
     mainWindow=$Process.MainWindowHandle.ToString(); windows=@([LauncherGuestWindows]::ForProcess($Process.Id));
     threads=@($Process.Threads | ForEach-Object { @{ id=$_.Id; state=$_.ThreadState.ToString() } }) } |
    ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $outputRoot 'launcher-process.json') -Encoding UTF8
}
try {
  Progress 'Verifying local stable installer hash'
  if ((Get-FileHash -LiteralPath (Join-Path $inputRoot 'launcher-setup.exe')).Hash -ne $marker.launcherInstallerSha256) { throw 'Installer provenance changed' }
  $local = [Environment]::GetFolderPath('LocalApplicationData')
  $dataDirectory = Join-Path ([Environment]::GetFolderPath('ApplicationData')) 'industrialis-launcher'
  if (Test-Path -LiteralPath (Join-Path $local 'Programs/Industrialis Launcher')) { throw 'Guest already contains a launcher installation' }
  New-Item -ItemType Directory -Path (Join-Path $dataDirectory 'instances/FixtureOne/.minecraft/saves/acceptance') -Force | Out-Null
  [IO.File]::WriteAllText((Join-Path $dataDirectory 'accounts.json'), '[{"account_type":"offline","format_version":3,"id":"20000000-0000-4000-8000-000000000001","minecraft_profile":{"id":"20000000-0000-4000-8000-000000000002","name":"LauncherAcceptance"},"acceptanceExtension":{"preserve":true}},{"account_type":"offline","format_version":3,"id":"20000000-0000-4000-8000-000000000003","minecraft_profile":{"id":"20000000-0000-4000-8000-000000000004","name":"LauncherKeyboard"}}]')
  [IO.File]::WriteAllText((Join-Path $dataDirectory 'launcher-settings.json'), '{"default_account_id":"20000000-0000-4000-8000-000000000001","acceptanceExtension":{"preserve":true}}')
  [IO.File]::WriteAllText((Join-Path $dataDirectory 'instances/instgroups.json'), '{"acceptanceExtension":{"preserve":true}}')
  [IO.File]::WriteAllText((Join-Path $dataDirectory 'instances/FixtureOne/instance.cfg'), "InstanceType=OneSix`nname=Launcher Sandbox fixture`n")
  [IO.File]::WriteAllText((Join-Path $dataDirectory 'instances/FixtureOne/mmc-pack.json'), '{"formatVersion":1,"components":[]}')
  [IO.File]::WriteAllText((Join-Path $dataDirectory 'instances/FixtureOne/instance.json'), '{"name":"Launcher Sandbox fixture","acceptanceExtension":{"preserve":true}}')
  [IO.File]::WriteAllText((Join-Path $dataDirectory 'instances/FixtureOne/.minecraft/saves/acceptance/world.dat'), 'Generated acceptance player data; never a user world')
  New-Item -ItemType Directory -Path (Join-Path $dataDirectory 'instances/FixtureOne/.minecraft/config'),(Join-Path $dataDirectory 'instances/FixtureOne/.minecraft/mods') -Force | Out-Null
  [IO.File]::WriteAllText((Join-Path $dataDirectory 'instances/FixtureOne/.minecraft/config/acceptance.cfg'), "general {`n I:amount=1`n S:labels <`n  original`n >`n}`n")
  [IO.File]::WriteAllText((Join-Path $dataDirectory 'instances/FixtureOne/.minecraft/mods/pack-owned.jar'), 'Generated pack mod bytes')
  $beforeInstall = Snapshot $dataDirectory
  Progress 'Installing launcher package in fresh guest'
  Run-Installer (Join-Path $inputRoot 'launcher-setup.exe') @('/S')
  $installed = Join-Path $local 'Programs/Industrialis Launcher/industrialis.exe'
  if ((Get-FileHash -LiteralPath $installed).Hash -ne $marker.launcherExecutableSha256) { throw 'Installed launcher bytes differ from compiled input' }
  Assert-Snapshot $dataDirectory $beforeInstall
  if ((Default-Protocol) -ne ('"' + $installed + '" "%1"')) { throw 'Installed protocol does not belong to the launcher' }
  $report.profilePreservedBeforeFirstBoot = $beforeInstall.Count
  $report.protocolRegistered = $true
  $report.systemVcruntimePresent = Test-Path -LiteralPath (Join-Path $env:WINDIR 'System32/vcruntime140.dll')
  Progress 'Booting installed launcher against same generated profile'
  # The host Sandbox client stays hidden; the guest app must expose its window
  # so MainWindowHandle and native normal-close exercise a visible guest window.
  $app = Start-Process -FilePath $installed -WindowStyle Normal -PassThru `
    -RedirectStandardOutput (Join-Path $outputRoot 'launcher-native.stdout.log') `
    -RedirectStandardError (Join-Path $outputRoot 'launcher-native.stderr.log')
  $appHandle = $app.Handle
  $report.nativeProcessId = $app.Id
  $deadline = [DateTime]::UtcNow.AddSeconds(60)
  do {
    Start-Sleep -Milliseconds 250
    $app.Refresh()
    if ($app.HasExited) {
      $app.WaitForExit()
      $report.nativeExitCode = $app.ExitCode
      throw ('Installed launcher exited before opening a native window: ' + $app.ExitCode)
    }
    if ([DateTime]::UtcNow -gt $deadline) {
      Save-AppDiagnostics $app
      throw 'Installed launcher native window did not appear'
    }
  } while ($app.MainWindowHandle -eq 0)
  Start-Sleep -Seconds 3
  if ($app.HasExited) { throw 'Installed launcher exited during initial boot' }
  # Only this owned guest window is minimized; the physical host is untouched.
  Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
using System.Text;
public static class LauncherGuestActivation {
  [StructLayout(LayoutKind.Sequential)] public struct Rect { public int Left,Top,Right,Bottom; }
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr window, int command);
  [DllImport("user32.dll")] public static extern bool IsIconic(IntPtr window);
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr window, out Rect rect);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetClassName(IntPtr window, StringBuilder text, int count);
}
'@
  $window = $app.MainWindowHandle
  $windowClass = New-Object Text.StringBuilder 256
  [LauncherGuestActivation]::GetClassName($window,$windowClass,$windowClass.Capacity) | Out-Null
  if ($windowClass.ToString() -ne 'Zed::Window') { throw ('Installed launcher opened a startup dialog instead of its renderer: ' + $windowClass) }
  $report.installedNativeBoot = $true
  [LauncherGuestActivation]::ShowWindow($window,6) | Out-Null
  if (-not [LauncherGuestActivation]::IsIconic($window)) { throw 'Owned guest window did not minimize' }
  Progress 'Launching registered protocol to restore and activate the same installed owner'
  Start-Process -FilePath 'industrialislauncher://sandbox-acceptance' -WindowStyle Normal
  $deadline = [DateTime]::UtcNow.AddSeconds(15)
  while ([LauncherGuestActivation]::IsIconic($window) -or [LauncherGuestActivation]::GetForegroundWindow() -ne $window) {
    if ($app.HasExited -or [DateTime]::UtcNow -gt $deadline) { throw 'Installed protocol did not restore and foreground the owner' }
    Start-Sleep -Milliseconds 100
  }
  Start-Sleep -Seconds 1
  $owners = @(Owned-Processes (Split-Path -Parent $installed))
  if ($owners.Count -ne 1 -or $owners[0].ProcessId -ne $app.Id) { throw 'Protocol activation left a duplicate installed process' }
  $report.protocolLaunchSameOwner = $true
  $report.protocolRestoresMinimizedWindow = $true
  $report.protocolForegroundActivation = $true
  if ($marker.nativeUi) {
    . (Join-Path $inputRoot 'native-ui.ps1')
    Invoke-GuestNativeUi $app $window $dataDirectory $report
  }
  Add-Type -AssemblyName System.Drawing,System.Windows.Forms
  $rect = New-Object LauncherGuestActivation+Rect
  if (-not [LauncherGuestActivation]::GetWindowRect($window,[ref]$rect)) { throw 'Owned guest window bounds unavailable' }
  $bounds = [Drawing.Rectangle]::FromLTRB($rect.Left,$rect.Top,$rect.Right,$rect.Bottom)
  $visible = [Drawing.Rectangle]::Intersect($bounds,[Windows.Forms.SystemInformation]::VirtualScreen)
  if ($visible.Width -lt 600 -or $visible.Height -lt 400) { throw 'Owned guest renderer is outside its desktop' }
  $bitmap = New-Object Drawing.Bitmap $visible.Width,$visible.Height
  $graphics = [Drawing.Graphics]::FromImage($bitmap)
  try {
    $graphics.CopyFromScreen($visible.Location,[Drawing.Point]::Empty,$visible.Size)
    $colors = New-Object 'Collections.Generic.HashSet[int]'
    for ($y=0; $y -lt $bitmap.Height; $y+=8) {
      for ($x=0; $x -lt $bitmap.Width; $x+=8) { $colors.Add($bitmap.GetPixel($x,$y).ToArgb()) | Out-Null }
    }
    $bitmap.Save((Join-Path $outputRoot 'launcher-native-render.png'),[Drawing.Imaging.ImageFormat]::Png)
    $report.renderedColors = $colors.Count
    if ($colors.Count -lt 12) { throw 'Owned launcher window rendered insufficient content' }
    $report.installedNativeRender = $true
  } finally { $graphics.Dispose(); $bitmap.Dispose() }
  $app.CloseMainWindow() | Out-Null
  if (-not $app.WaitForExit(60000) -or $app.ExitCode -ne 0) { throw 'Installed launcher did not close normally' }
  $settings = Get-Content -LiteralPath (Join-Path $dataDirectory 'launcher-settings.json') -Raw | ConvertFrom-Json
  if ($settings.acceptanceExtension.preserve -ne $true) { throw 'Generated unknown settings lost on native close' }
  $report.nativeNormalClose = $true
  $afterBoot = Snapshot $dataDirectory
  Progress 'Uninstalling actual launcher package and checking profile and protocol cleanup'
  Run-Installer (Join-Path $local 'Programs/Industrialis Launcher/Uninstall.exe') @('/S')
  $deadline = [DateTime]::UtcNow.AddSeconds(30)
  while (Test-Path -LiteralPath $installed) {
    if ([DateTime]::UtcNow -gt $deadline) { throw 'Launcher executable survived uninstall' }
    Start-Sleep -Milliseconds 250
  }
  Assert-Snapshot $dataDirectory $afterBoot
  if ((Default-Protocol)) { throw 'Uninstall retained an unowned protocol handler' }
  $report.launcherUninstall = $true
  $report.profilePreservedAfterUninstall = $afterBoot.Count
  $report.protocolRemovedOnUninstall = $true
  $report.limits = 'Generated offline profile in Windows Sandbox; owned native UI checks when enabled; no original credentials, physical input, published update or older Windows baseline'
  $report.passed = $true
  Progress 'Installed launcher lifecycle checks passed'
} catch {
  $report.error = $_.Exception.Message
  Progress ('Failure: ' + $report.error)
} finally {
  $report | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath (Join-Path $outputRoot 'result.json') -Encoding UTF8
  # Guest identity was checked before entering this block. Never shut down a host.
  & shutdown.exe /s /t 0
}
