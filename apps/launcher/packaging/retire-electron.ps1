param([switch]$Apply)
$ErrorActionPreference = 'Stop'

function Assert-PlainInstallPath([string]$Path, [string]$Boundary) {
  $itemPath = [IO.Path]::GetFullPath($Path)
  $boundaryPath = [IO.Path]::GetFullPath($Boundary).TrimEnd('\')
  if (-not $itemPath.StartsWith($boundaryPath + '\', [StringComparison]::OrdinalIgnoreCase)) {
    throw 'Legacy installation escaped the local application directory'
  }
  while ($itemPath -ne $boundaryPath) {
    if (Test-Path -LiteralPath $itemPath) {
      $item = Get-Item -LiteralPath $itemPath -Force
      if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'Legacy installation contains a redirected path' }
    }
    $itemPath = Split-Path -Parent $itemPath
  }
}

function Get-LegacyInstall([string]$LocalDirectory, [string]$ProfileDirectory) {
  $root = [IO.Path]::GetFullPath((Join-Path $LocalDirectory 'IndustrialisLauncher'))
  $profile = [IO.Path]::GetFullPath($ProfileDirectory).TrimEnd('\')
  if ($profile -eq $root -or $profile.StartsWith($root + '\', [StringComparison]::OrdinalIgnoreCase) -or
      $root.StartsWith($profile + '\', [StringComparison]::OrdinalIgnoreCase)) {
    throw 'Legacy installation overlaps the user profile'
  }
  if (-not (Test-Path -LiteralPath $root)) { return $null }
  Assert-PlainInstallPath $root $LocalDirectory
  # Never delegate removal of a directory containing a launcher profile.
  foreach ($name in @('accounts.json', 'launcher-settings.json', 'Local State', 'instances')) {
    if (Test-Path -LiteralPath (Join-Path $root $name)) { throw 'User data was found inside the legacy installation; manual review is required' }
  }
  $executables = @()
  foreach ($directory in (Get-ChildItem -LiteralPath $root -Directory -Filter 'app-*')) {
    $exe = Join-Path $directory.FullName 'industrialis-launcher.exe'
    $asar = Join-Path $directory.FullName 'resources/app.asar'
    if (Test-Path -LiteralPath $exe) {
      Assert-PlainInstallPath $exe $LocalDirectory
      Assert-PlainInstallPath $asar $LocalDirectory
      if (-not (Test-Path -LiteralPath $asar -PathType Leaf)) { throw 'Legacy executable is missing its Electron package' }
      $executables += $exe
    }
  }
  $stub = Join-Path $root 'industrialis-launcher.exe'
  if ($executables.Count -eq 0) {
    if (Test-Path -LiteralPath $stub) { throw 'An orphan legacy shortcut stub remains; manual review is required' }
    return $null
  }
  if (Test-Path -LiteralPath $stub) {
    Assert-PlainInstallPath $stub $LocalDirectory
    if (-not (Test-Path -LiteralPath $stub -PathType Leaf)) { throw 'Legacy shortcut stub is not a file' }
    $executables += $stub
  }
  $updater = Join-Path $root 'Update.exe'
  Assert-PlainInstallPath $updater $LocalDirectory
  if (-not (Test-Path -LiteralPath $updater -PathType Leaf)) { throw 'Legacy Squirrel updater is missing; no application files were removed' }
  [pscustomobject]@{ Root = $root; Updater = $updater; Executables = $executables; Profile = $profile }
}

function Get-RunningLaunchers([string]$Root) {
  @(Get-CimInstance Win32_Process | Where-Object {
    $_.ExecutablePath -and ($_.ExecutablePath.StartsWith($Root + '\', [StringComparison]::OrdinalIgnoreCase) -or
      $_.Name -eq 'industrialis.exe')
  })
}

function Invoke-LegacyRetirement($Install,
    [ValidateRange(0, 120)][int]$WaitSeconds = 120,
    [scriptblock]$Running = { param($root) Get-RunningLaunchers $root },
    [scriptblock]$Uninstall = {
      param($updater)
      $process = Start-Process -FilePath $updater -ArgumentList @('--uninstall', '--silent') -WindowStyle Hidden -PassThru
      if (-not $process.WaitForExit(120000)) { throw 'Squirrel uninstall is still running. The launcher was not started; wait before retrying.' }
      if ($process.ExitCode -ne 0) { throw "Squirrel uninstall failed ($($process.ExitCode)). The launcher was not started." }
    }) {
  if ($null -eq $Install) { return }
  # Do not force-stop applications. The old updater starts its installer before
  # it drains operations and quits, so allow that normal close to finish first.
  $deadline = [DateTime]::UtcNow.AddSeconds($WaitSeconds)
  while (@(& $Running $Install.Root).Count -gt 0) {
    if ([DateTime]::UtcNow -ge $deadline) { throw 'Close launcher windows and retry. No process was stopped or application removed.' }
    Start-Sleep -Milliseconds 250
  }
  & $Uninstall $Install.Updater
  foreach ($exe in $Install.Executables) {
    if (Test-Path -LiteralPath $exe) { throw 'An old launcher executable remains. The launcher was not started.' }
  }
}

function Remove-DeadLegacyProtocol($Install,
    [string]$Key = 'HKCU:\Software\Classes\industrialislauncher\shell\open\command') {
  if ($null -eq $Install) { return }
  if (-not (Test-Path -LiteralPath $Key)) { return }
  $command = (Get-Item -LiteralPath $Key).GetValue('')
  if ($command -match '^"([^"]+)"') {
    $exe = $Matches[1]
    if ($Install.Executables -contains $exe -and -not (Test-Path -LiteralPath $exe)) {
      # An uninstalled handler must not be restored by the future launcher uninstall.
      $writableKey = (Get-Item -LiteralPath $Key).OpenSubKey('', $true)
      if ($null -eq $writableKey) { throw 'Unable to open the dead legacy protocol handler for cleanup' }
      try { $writableKey.DeleteValue('', $false) } finally { $writableKey.Dispose() }
    }
  }
}

if ($MyInvocation.InvocationName -ne '.') {
  try {
    $local = [Environment]::GetFolderPath('LocalApplicationData')
    $profile = Join-Path ([Environment]::GetFolderPath('ApplicationData')) 'industrialis-launcher'
    $install = Get-LegacyInstall $local $profile
    if ($Apply) {
      Invoke-LegacyRetirement $install
      Remove-DeadLegacyProtocol $install
    }
    [pscustomobject]@{ LegacyPresent = ($null -ne $install); Applied = [bool]$Apply; UserProfilePreserved = $true } | ConvertTo-Json -Compress
  } catch {
    Write-Error $_
    exit 1
  }
}
