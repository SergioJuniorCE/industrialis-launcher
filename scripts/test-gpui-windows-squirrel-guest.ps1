# Runs only inside the explicitly marked disposable Windows Sandbox.
$ErrorActionPreference = 'Stop'
$inputRoot = 'C:\GpuiAcceptanceInput'
$outputRoot = 'C:\GpuiAcceptanceOutput'
$marker = Get-Content -LiteralPath (Join-Path $inputRoot 'marker.json') -Raw | ConvertFrom-Json
if ($env:USERNAME -ne 'WDAGUtilityAccount' -or $env:COMPUTERNAME -eq $marker.hostComputer -or
    $marker.kind -ne 'gpui-expendable-squirrel-acceptance-v1') { throw 'Disposable guest identity required' }

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
function Close-ExpendableLegacy([string]$Root) {
  foreach ($process in (Owned-Processes $Root)) {
    try { [Diagnostics.Process]::GetProcessById($process.ProcessId).CloseMainWindow() | Out-Null } catch { }
  }
  $deadline = [DateTime]::UtcNow.AddSeconds(30)
  while ((Owned-Processes $Root).Count -gt 0 -and [DateTime]::UtcNow -lt $deadline) { Start-Sleep -Milliseconds 250 }
  $remaining = @(Owned-Processes $Root)
  foreach ($item in $remaining) {
    $process = [Diagnostics.Process]::GetProcessById($item.ProcessId)
    if (-not $process.Path.StartsWith($Root + '\',[StringComparison]::OrdinalIgnoreCase) -or
        [Math]::Abs(($process.StartTime - $item.CreationDate).TotalMilliseconds) -gt 20) { throw 'Guest process identity changed' }
    $process.Kill()
    if (-not $process.WaitForExit(10000)) { throw 'Expendable legacy process did not exit' }
  }
  if ((Owned-Processes $Root).Count -gt 0) { throw 'Legacy fixture remains running' }
  if ($remaining.Count -gt 0) { return 'forced fixture closure after normal-close deadline' }
  return 'normal window closure'
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
function Wait-GuestWindow($Process) {
  $deadline = [DateTime]::UtcNow.AddSeconds(60)
  do {
    Start-Sleep -Milliseconds 250
    $Process.Refresh()
    if ($Process.HasExited -or [DateTime]::UtcNow -gt $deadline) { throw 'Owned legacy fixture did not open a native window' }
  } while ($Process.MainWindowHandle -eq 0)
}
function Install-WithRunningLegacy([string]$LegacyExecutable, [string]$LegacyRoot, [string]$GuestDataDirectory) {
  Progress 'Opening the actual released Electron window before replacement'
  $old = Start-Process -FilePath $LegacyExecutable -ArgumentList @('--disable-gpu') -WindowStyle Normal -PassThru
  $oldHandle = $old.Handle
  Wait-GuestWindow $old
  $report.runningLegacyProcessId = $old.Id
  $report.runningLegacyWindow = $old.MainWindowHandle.ToString()
  # These files are launcher-managed; Chromium is allowed to update its own state
  # during a legitimate normal close. Snapshot those bytes after close separately.
  $criticalBefore = Snapshot $GuestDataDirectory
  $beforeLegacyClose = @{} + $criticalBefore
  $criticalBefore = @{} + $criticalBefore
  foreach ($name in @($criticalBefore.Keys)) {
    if ($name -notmatch '^instances\\[^\\]+\\(\.minecraft\\|persistent-minecraft\\|mmc-pack\.json$|instance\.cfg$)') { $criticalBefore.Remove($name) }
  }
  $stateFile = Join-Path $GuestDataDirectory 'Local State'
  $state = Get-Content -LiteralPath $stateFile -Raw | ConvertFrom-Json
  $originalKey = $state.os_crypt.encrypted_key
  $setup = Start-Process -FilePath (Join-Path $inputRoot 'gpui-setup.exe') -ArgumentList @('/S') -WindowStyle Hidden -PassThru
  $setupHandle = $setup.Handle
  # NSIS launches a worker executable from its temporary directory; follow only
  # the verified installer's descendants when locating its retirement helper.
  $deadline = [DateTime]::UtcNow.AddSeconds(90)
  do {
    $rows = @(Get-CimInstance Win32_Process)
    $descendants = New-Object 'Collections.Generic.HashSet[int]'
    $descendants.Add($setup.Id) | Out-Null
    do {
      $added = $false
      foreach ($row in $rows) {
        if ($descendants.Contains([int]$row.ParentProcessId) -and $descendants.Add([int]$row.ProcessId)) { $added = $true }
      }
    } while ($added)
    $helpers = @($rows | Where-Object { $descendants.Contains([int]$_.ProcessId) -and $_.Name -eq 'powershell.exe' -and $_.CommandLine -like '*retire-electron.ps1* -Apply*' })
    if ($setup.HasExited -or $old.HasExited -or [DateTime]::UtcNow -gt $deadline) { throw 'Actual installer did not wait for the running legacy window' }
    Start-Sleep -Milliseconds 250
  } while ($helpers.Count -ne 1)
  Start-Sleep -Seconds 2
  if ($setup.HasExited -or $old.HasExited -or -not (Test-Path -LiteralPath $LegacyExecutable)) { throw 'Retirement removed or stopped the running legacy app' }
  if (@(Get-CimInstance Win32_Process -Filter "Name = 'industrialis.exe'").Count -ne 0) { throw 'Replacement started beside an uncooperative legacy writer' }
  $report.actualInstallerWaitedForLegacy = $true
  Progress 'Closing only the owned released Electron window normally'
  if (-not $old.CloseMainWindow() -or -not $old.WaitForExit(60000) -or $old.ExitCode -ne 0) { throw 'Released Electron window did not close normally' }
  $deadline = [DateTime]::UtcNow.AddSeconds(30)
  while ((Owned-Processes $LegacyRoot).Count -gt 0) {
    if ([DateTime]::UtcNow -gt $deadline) { throw 'Actual Electron child remained after normal close' }
    Start-Sleep -Milliseconds 100
  }
  Assert-Snapshot $GuestDataDirectory $criticalBefore
  $state = Get-Content -LiteralPath $stateFile -Raw | ConvertFrom-Json
  if ($originalKey -and $state.os_crypt.encrypted_key -ne $originalKey) { throw 'Electron encryption key changed on normal close' }
  $afterClose = Snapshot $GuestDataDirectory
  $report.legacyMetadataChangesOnNormalBootClose = @($beforeLegacyClose.Keys | Where-Object { $afterClose[$_] -ne $beforeLegacyClose[$_] })
  $instanceSettings = Get-Content -LiteralPath (Join-Path $GuestDataDirectory 'instances/FixtureOne/instance.json') -Raw | ConvertFrom-Json
  if ($instanceSettings.name -ne 'GPUI Sandbox fixture') { throw 'Original launcher lost the generated instance name' }
  if (-not $setup.WaitForExit(300000) -or $setup.ExitCode -ne 0) { throw ('Actual running-legacy installer failed: ' + $setup.ExitCode) }
  Assert-Snapshot $GuestDataDirectory $afterClose
  $report.runningLegacyNormalClose = $true
  $report.runningLegacyEncryptionKeyRetained = [bool]$originalKey
  $report.profilePreservedAfterLegacyClose = $afterClose.Count
  return $afterClose
}
function Legacy-Shortcuts([string]$Root) {
  $shell = New-Object -ComObject WScript.Shell
  try {
    foreach ($directory in @([Environment]::GetFolderPath('DesktopDirectory'),[Environment]::GetFolderPath('StartMenu'))) {
      foreach ($file in (Get-ChildItem -LiteralPath $directory -Filter '*.lnk' -Recurse -File -ErrorAction SilentlyContinue)) {
        $shortcut = $shell.CreateShortcut($file.FullName)
        if ($shortcut.TargetPath.StartsWith($Root + '\',[StringComparison]::OrdinalIgnoreCase)) { $file.FullName }
      }
    }
  } finally { [Runtime.InteropServices.Marshal]::ReleaseComObject($shell) | Out-Null }
}
function Save-AppDiagnostics($Process) {
  Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;
public static class GpuiGuestWindows {
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
     mainWindow=$Process.MainWindowHandle.ToString(); windows=@([GpuiGuestWindows]::ForProcess($Process.Id));
     threads=@($Process.Threads | ForEach-Object { @{ id=$_.Id; state=$_.ThreadState.ToString() } }) } |
    ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $outputRoot 'gpui-process.json') -Encoding UTF8
}
try {
  Progress 'Verifying public legacy and local stable installer hashes'
  foreach ($pair in @(@('legacy-setup.exe',$marker.legacySha256),@('gpui-setup.exe',$marker.gpuiInstallerSha256))) {
    if ((Get-FileHash -LiteralPath (Join-Path $inputRoot $pair[0])).Hash -ne $pair[1]) { throw 'Installer provenance changed' }
  }
  $local = [Environment]::GetFolderPath('LocalApplicationData')
  $legacy = Join-Path $local 'IndustrialisLauncher'
  $dataDirectory = Join-Path ([Environment]::GetFolderPath('ApplicationData')) 'industrialis-launcher'
  if (Test-Path -LiteralPath $legacy) { throw 'Guest already contains a legacy installation' }
  Progress 'Installing actual launcher-v0.3.0 Squirrel release in fresh guest'
  Run-Installer (Join-Path $inputRoot 'legacy-setup.exe') @('--silent')
  $deadline = [DateTime]::UtcNow.AddSeconds(60)
  while (-not (Test-Path -LiteralPath (Join-Path $legacy 'app-0.3.0/resources/app.asar'))) {
    if ([DateTime]::UtcNow -gt $deadline) { throw 'Actual legacy installation did not appear' }
    Start-Sleep -Milliseconds 500
  }
  $report.legacyFixtureClosure = Close-ExpendableLegacy $legacy
  $oldShortcuts = @(Legacy-Shortcuts $legacy)
  $oldUninstallKey = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\IndustrialisLauncher'
  $oldUninstallRegistered = Test-Path -LiteralPath $oldUninstallKey
  $oldExecutables = @(Get-ChildItem -LiteralPath $legacy -Recurse -Filter 'industrialis-launcher.exe' -File | ForEach-Object FullName)
  if ($oldExecutables.Count -eq 0 -or -not (Test-Path -LiteralPath (Join-Path $legacy 'Update.exe'))) { throw 'Actual Squirrel layout missing' }
  $report.legacyInstalled = $true
  $report.oldExecutableCount = $oldExecutables.Count
  New-Item -ItemType Directory -Path (Join-Path $dataDirectory 'instances/FixtureOne/.minecraft/saves/acceptance') -Force | Out-Null
  [IO.File]::WriteAllText((Join-Path $dataDirectory 'accounts.json'), '[{"account_type":"offline","format_version":3,"id":"20000000-0000-4000-8000-000000000001","minecraft_profile":{"id":"20000000-0000-4000-8000-000000000002","name":"GpuiAcceptance"},"acceptanceExtension":{"preserve":true}},{"account_type":"offline","format_version":3,"id":"20000000-0000-4000-8000-000000000003","minecraft_profile":{"id":"20000000-0000-4000-8000-000000000004","name":"GpuiKeyboard"}}]')
  [IO.File]::WriteAllText((Join-Path $dataDirectory 'launcher-settings.json'), '{"default_account_id":"20000000-0000-4000-8000-000000000001","acceptanceExtension":{"preserve":true}}')
  [IO.File]::WriteAllText((Join-Path $dataDirectory 'instances/instgroups.json'), '{"acceptanceExtension":{"preserve":true}}')
  [IO.File]::WriteAllText((Join-Path $dataDirectory 'instances/FixtureOne/instance.cfg'), "InstanceType=OneSix`nname=GPUI Sandbox fixture`n")
  [IO.File]::WriteAllText((Join-Path $dataDirectory 'instances/FixtureOne/mmc-pack.json'), '{"formatVersion":1,"components":[]}')
  [IO.File]::WriteAllText((Join-Path $dataDirectory 'instances/FixtureOne/instance.json'), '{"name":"GPUI Sandbox fixture","acceptanceExtension":{"preserve":true}}')
  [IO.File]::WriteAllText((Join-Path $dataDirectory 'instances/FixtureOne/.minecraft/saves/acceptance/world.dat'), 'Generated acceptance player data; never a user world')
  New-Item -ItemType Directory -Path (Join-Path $dataDirectory 'instances/FixtureOne/.minecraft/config'),(Join-Path $dataDirectory 'instances/FixtureOne/.minecraft/mods') -Force | Out-Null
  [IO.File]::WriteAllText((Join-Path $dataDirectory 'instances/FixtureOne/.minecraft/config/acceptance.cfg'), "general {`n I:amount=1`n S:labels <`n  original`n >`n}`n")
  [IO.File]::WriteAllText((Join-Path $dataDirectory 'instances/FixtureOne/.minecraft/mods/pack-owned.jar'), 'Generated pack mod bytes')
  $report.legacyCreatedLocalState = Test-Path -LiteralPath (Join-Path $dataDirectory 'Local State')
  if (-not $report.legacyCreatedLocalState) {
    [IO.File]::WriteAllText((Join-Path $dataDirectory 'Local State'), '{"acceptanceExtension":{"preserve":true}}')
  }
  $beforeInstall = Snapshot $dataDirectory
  # Set an actual installed legacy command even if its initial app did not finish
  # protocol registration before closing. This is exclusively the guest registry.
  $protocolKey = 'HKCU:\Software\Classes\industrialislauncher\shell\open\command'
  New-Item -Path $protocolKey -Force | Out-Null
  Set-Item -LiteralPath $protocolKey -Value ('"' + $oldExecutables[0] + '" "%1"')
  Progress 'Installing actual stable GPUI package and delegating retirement to actual Squirrel updater'
  if ($marker.runningLegacy) {
    $releasedExecutable = Join-Path $legacy 'app-0.3.0/industrialis-launcher.exe'
    $beforeInstall = Install-WithRunningLegacy $releasedExecutable $legacy $dataDirectory
  } else {
    Run-Installer (Join-Path $inputRoot 'gpui-setup.exe') @('/S')
  }
  $installed = Join-Path $local 'Programs/Industrialis Launcher/industrialis.exe'
  if ((Get-FileHash -LiteralPath $installed).Hash -ne $marker.gpuiExecutableSha256) { throw 'Installed GPUI bytes differ from compiled input' }
  foreach ($exe in $oldExecutables) { if (Test-Path -LiteralPath $exe) { throw 'Actual legacy executable survived retirement' } }
  foreach ($shortcut in $oldShortcuts) { if (Test-Path -LiteralPath $shortcut) { throw 'Actual legacy shortcut survived retirement' } }
  if ($oldUninstallRegistered -and (Test-Path -LiteralPath $oldUninstallKey)) { throw 'Legacy uninstall registration survived retirement' }
  if ((Owned-Processes $legacy).Count -gt 0) { throw 'Legacy process survived retirement' }
  Assert-Snapshot $dataDirectory $beforeInstall
  if ((Default-Protocol) -ne ('"' + $installed + '" "%1"')) { throw 'Installed protocol does not belong to GPUI' }
  $report.actualSquirrelRetirement = $true
  $report.oldShortcutsRemoved = $oldShortcuts.Count
  $report.oldUninstallRegistrationRemoved = [bool]$oldUninstallRegistered
  $report.profilePreservedBeforeFirstBoot = $beforeInstall.Count
  $report.protocolRegistered = $true
  $report.systemVcruntimePresent = Test-Path -LiteralPath (Join-Path $env:WINDIR 'System32/vcruntime140.dll')
  Progress 'Booting installed GPUI against same generated original-location profile'
  # The host Sandbox client stays hidden; the guest app must expose its window
  # so MainWindowHandle and native normal-close exercise a visible guest window.
  $app = Start-Process -FilePath $installed -WindowStyle Normal -PassThru `
    -RedirectStandardOutput (Join-Path $outputRoot 'gpui-native.stdout.log') `
    -RedirectStandardError (Join-Path $outputRoot 'gpui-native.stderr.log')
  $appHandle = $app.Handle
  $report.nativeProcessId = $app.Id
  $deadline = [DateTime]::UtcNow.AddSeconds(60)
  do {
    Start-Sleep -Milliseconds 250
    $app.Refresh()
    if ($app.HasExited) {
      $app.WaitForExit()
      $report.nativeExitCode = $app.ExitCode
      throw ('Installed GPUI exited before opening a native window: ' + $app.ExitCode)
    }
    if ([DateTime]::UtcNow -gt $deadline) {
      Save-AppDiagnostics $app
      throw 'Installed GPUI native window did not appear'
    }
  } while ($app.MainWindowHandle -eq 0)
  Start-Sleep -Seconds 3
  if ($app.HasExited) { throw 'Installed GPUI exited during initial boot' }
  # Only this owned guest window is minimized; the physical host is untouched.
  Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
using System.Text;
public static class GpuiGuestActivation {
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
  [GpuiGuestActivation]::GetClassName($window,$windowClass,$windowClass.Capacity) | Out-Null
  if ($windowClass.ToString() -ne 'Zed::Window') { throw ('Installed GPUI opened a startup dialog instead of its renderer: ' + $windowClass) }
  $report.installedNativeBoot = $true
  [GpuiGuestActivation]::ShowWindow($window,6) | Out-Null
  if (-not [GpuiGuestActivation]::IsIconic($window)) { throw 'Owned guest window did not minimize' }
  Progress 'Launching registered protocol to restore and activate the same installed owner'
  Start-Process -FilePath 'industrialislauncher://sandbox-acceptance' -WindowStyle Normal
  $deadline = [DateTime]::UtcNow.AddSeconds(15)
  while ([GpuiGuestActivation]::IsIconic($window) -or [GpuiGuestActivation]::GetForegroundWindow() -ne $window) {
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
  $rect = New-Object GpuiGuestActivation+Rect
  if (-not [GpuiGuestActivation]::GetWindowRect($window,[ref]$rect)) { throw 'Owned guest window bounds unavailable' }
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
    $bitmap.Save((Join-Path $outputRoot 'gpui-native-render.png'),[Drawing.Imaging.ImageFormat]::Png)
    $report.renderedColors = $colors.Count
    if ($colors.Count -lt 12) { throw 'Owned GPUI window rendered insufficient content' }
    $report.installedNativeRender = $true
  } finally { $graphics.Dispose(); $bitmap.Dispose() }
  $app.CloseMainWindow() | Out-Null
  if (-not $app.WaitForExit(60000) -or $app.ExitCode -ne 0) { throw 'Installed GPUI did not close normally' }
  $settings = Get-Content -LiteralPath (Join-Path $dataDirectory 'launcher-settings.json') -Raw | ConvertFrom-Json
  if ($settings.acceptanceExtension.preserve -ne $true) { throw 'Generated unknown settings lost on native close' }
  $report.nativeNormalClose = $true
  $afterBoot = Snapshot $dataDirectory
  Progress 'Uninstalling actual GPUI package and checking profile and dead-protocol cleanup'
  Run-Installer (Join-Path $local 'Programs/Industrialis Launcher/Uninstall.exe') @('/S')
  $deadline = [DateTime]::UtcNow.AddSeconds(30)
  while (Test-Path -LiteralPath $installed) {
    if ([DateTime]::UtcNow -gt $deadline) { throw 'GPUI executable survived uninstall' }
    Start-Sleep -Milliseconds 250
  }
  Assert-Snapshot $dataDirectory $afterBoot
  if ((Default-Protocol)) { throw 'Uninstall retained or restored a dead legacy protocol' }
  $report.gpuiUninstall = $true
  $report.profilePreservedAfterUninstall = $afterBoot.Count
  $report.deadLegacyProtocolNotRestored = $true
  $report.limits = 'Generated offline profile in Windows Sandbox; controlled released Electron window handoff and owned native UI checks when enabled; no original credentials, physical input, published update or older Windows baseline'
  $report.passed = $true
  Progress 'Actual Squirrel retirement and GPUI lifecycle checks passed'
} catch {
  $report.error = $_.Exception.Message
  Progress ('Failure: ' + $report.error)
} finally {
  $setupLog = Join-Path ([Environment]::GetFolderPath('LocalApplicationData')) 'SquirrelTemp/SquirrelSetup.log'
  if (Test-Path -LiteralPath $setupLog) {
    try { Copy-Item -LiteralPath $setupLog -Destination (Join-Path $outputRoot 'legacy-squirrel.log') } catch { }
  }
  $report | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath (Join-Path $outputRoot 'result.json') -Encoding UTF8
  # Guest identity was checked before entering this block. Never shut down a host.
  & shutdown.exe /s /t 0
}
