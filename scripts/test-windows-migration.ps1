param([Parameter(Mandatory = $true)][string]$MakeNSIS)
$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
$app = Join-Path $repo 'apps/launcher'
$fixture = Join-Path $app ('target/migration-smoke-' + [guid]::NewGuid().ToString('N'))
$programs = Join-Path $env:LOCALAPPDATA 'Programs'
$suffix = [guid]::NewGuid().ToString('N').Substring(0, 12)
$legacyName = "Industrialis GPUI Smoke $suffix"
$currentName = "Industrialis Launcher Smoke $suffix"
$legacy = Join-Path $programs $legacyName
$current = Join-Path $programs $currentName
$setup = Join-Path $fixture 'setup.exe'
$ready = Join-Path $fixture 'installer-ready'
$locks = @()
$processes = @()
$child = $null

function New-LegacyInstall {
  Remove-Item -LiteralPath $legacy -Recurse -Force -ErrorAction SilentlyContinue
  New-Item -ItemType Directory -Path $legacy -Force | Out-Null
  [IO.File]::WriteAllText((Join-Path $legacy 'industrialis.exe'), 'old launcher')
  [IO.File]::WriteAllText((Join-Path $legacy 'gpui-install.json'), '{"format":1,"kind":"nsis"}')
  [IO.File]::WriteAllText((Join-Path $legacy 'Uninstall.exe'), 'old uninstaller')
  [IO.File]::WriteAllText((Join-Path $legacy 'LICENSE'), 'old license')
}

function Invoke-Migration([bool]$LockOldExe) {
  Remove-Item -LiteralPath $current -Recurse -Force -ErrorAction SilentlyContinue
  New-LegacyInstall
  New-Item -ItemType Directory -Path $current -Force | Out-Null
  [IO.File]::WriteAllText((Join-Path $current 'keep-user-data.txt'), 'preserve')
  $script:locks = @()
  if ($LockOldExe) {
    # A game broker maps the retired executable without delete sharing.
    $script:locks = @(
      [IO.File]::Open(
        (Join-Path $legacy 'industrialis.exe'),
        [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::Read
      )
    )
  }
  # The handoff waits for the old launcher PID after writing readiness.
  $script:child = Start-Process -FilePath "$env:SystemRoot\System32\ping.exe" `
    -ArgumentList @('-n', '6', '127.0.0.1') -WindowStyle Hidden -PassThru

  $psi = New-Object System.Diagnostics.ProcessStartInfo
  $psi.FileName = $setup
  $psi.Arguments = "/S /UPDATEPID=$($script:child.Id) /READYFILE=$ready /D=$legacy"
  $psi.UseShellExecute = $false
  $psi.CreateNoWindow = $true
  $installer = [System.Diagnostics.Process]::Start($psi)
  if (-not $installer.WaitForExit(60000)) { throw 'Migration installer did not finish' }
  if ($installer.ExitCode -ne 0) {
    throw "Migration installer failed with exit code $($installer.ExitCode)"
  }

  if (-not (Test-Path -LiteralPath (Join-Path $current 'industrialis.exe'))) {
    throw 'Legacy installation was not migrated to the current directory'
  }
  if (-not (Test-Path -LiteralPath (Join-Path $current 'install.json'))) {
    throw 'Migrated installation lacks the current marker'
  }
  if (-not (Test-Path -LiteralPath (Join-Path $current 'keep-user-data.txt'))) {
    throw 'Migration removed unrelated files from the target directory'
  }
  $deadline = [DateTime]::UtcNow.AddSeconds(10)
  while (-not (Test-Path -LiteralPath (Join-Path $current 'restarted-version.txt'))) {
    if ([DateTime]::UtcNow -ge $deadline) { throw 'Migrated launcher was not restarted' }
    Start-Sleep -Milliseconds 100
  }
  if ((Get-Content -LiteralPath (Join-Path $current 'restarted-version.txt') -Raw) -ne '0.4.4') {
    throw 'Migrated launcher was not the new build'
  }

  if ($LockOldExe) {
    if (-not (Test-Path -LiteralPath $legacy)) {
      throw 'Locked retired directory unexpectedly disappeared'
    }
    if (-not (Test-Path -LiteralPath (Join-Path $legacy 'industrialis.exe'))) {
      throw 'Locked retired executable was not kept'
    }
  } elseif (Test-Path -LiteralPath $legacy) {
    throw 'Retired installation directory was not removed'
  }

  foreach ($held in $script:locks) { $held.Dispose() }
  $script:locks = @()
  if (-not $script:child.HasExited) { $script:child.Kill(); $script:child.WaitForExit() }
  $script:child = $null
}

function Invoke-LockedRollback {
  Remove-Item -LiteralPath $current -Recurse -Force -ErrorAction SilentlyContinue
  New-Item -ItemType Directory -Path $current -Force | Out-Null
  [IO.File]::WriteAllText((Join-Path $current 'industrialis.exe'), 'current launcher')
  [IO.File]::WriteAllText((Join-Path $current 'install.json'), '{"format":1,"kind":"nsis"}')
  Copy-Item (Join-Path $fixture 'sleeper.exe') (Join-Path $current 'industrialis.exe.previous') -Force
  # A broker from a still-running game maps the default rollback copy. Mapped
  # images allow a rename but refuse deletion, so the installer must choose a
  # unique rollback name and keep going.
  $rollbackPsi = New-Object System.Diagnostics.ProcessStartInfo
  $rollbackPsi.FileName = Join-Path $current 'industrialis.exe.previous'
  $rollbackPsi.UseShellExecute = $false
  $rollbackPsi.CreateNoWindow = $true
  $script:processes = @([System.Diagnostics.Process]::Start($rollbackPsi))
  Start-Sleep -Milliseconds 300
  $script:child = Start-Process -FilePath "$env:SystemRoot\System32\ping.exe" `
    -ArgumentList @('-n', '6', '127.0.0.1') -WindowStyle Hidden -PassThru

  $psi = New-Object System.Diagnostics.ProcessStartInfo
  $psi.FileName = $setup
  $psi.Arguments = "/S /UPDATEPID=$($script:child.Id) /READYFILE=$ready /D=$current"
  $psi.UseShellExecute = $false
  $psi.CreateNoWindow = $true
  $installer = [System.Diagnostics.Process]::Start($psi)
  if (-not $installer.WaitForExit(60000)) { throw 'Locked rollback installer did not finish' }
  if ($installer.ExitCode -ne 0) {
    throw "Locked rollback installer failed with exit code $($installer.ExitCode)"
  }
  if (-not (Test-Path -LiteralPath (Join-Path $current 'industrialis.exe'))) {
    throw 'Locked rollback update did not install the new launcher'
  }
  $deadline = [DateTime]::UtcNow.AddSeconds(10)
  while (-not (Test-Path -LiteralPath (Join-Path $current 'restarted-version.txt'))) {
    if ([DateTime]::UtcNow -ge $deadline) { throw 'Locked rollback update did not restart the launcher' }
    Start-Sleep -Milliseconds 100
  }
  if (-not (Test-Path -LiteralPath (Join-Path $current 'industrialis.exe.previous'))) {
    throw 'Locked rollback update removed the mapped rollback copy'
  }
  $uniqueRollbacks = @(Get-ChildItem -LiteralPath $current -File | Where-Object {
    $_.Name -like 'industrialis.exe.previous.*'
  })
  if ($uniqueRollbacks.Count -gt 0) {
    throw 'Locked rollback update left a unique rollback copy behind'
  }
  foreach ($running in $script:processes) {
    if (-not $running.HasExited) { $running.Kill(); $running.WaitForExit() }
  }
  $script:processes = @()
  if (-not $script:child.HasExited) { $script:child.Kill(); $script:child.WaitForExit() }
  $script:child = $null
}

try {
  New-Item -ItemType Directory -Path $fixture -Force | Out-Null

  $source = @'
#![windows_subsystem = "windows"]
fn main() {
    let root = std::env::current_exe().unwrap().parent().unwrap().to_path_buf();
    std::fs::write(root.join("restarted-version.txt"), "0.4.4").unwrap();
}
'@
  [IO.File]::WriteAllText((Join-Path $fixture 'fixture.rs'), $source, (New-Object Text.UTF8Encoding($false)))
  & rustc --edition 2021 (Join-Path $fixture 'fixture.rs') -o (Join-Path $fixture 'new-launcher.exe')
  if ($LASTEXITCODE -ne 0) { throw 'Migration fixture compilation failed' }

  $sleeper = @'
#![windows_subsystem = "windows"]
fn main() { std::thread::sleep(std::time::Duration::from_secs(30)); }
'@
  [IO.File]::WriteAllText((Join-Path $fixture 'sleeper.rs'), $sleeper, (New-Object Text.UTF8Encoding($false)))
  & rustc --edition 2021 (Join-Path $fixture 'sleeper.rs') -o (Join-Path $fixture 'sleeper.exe')
  if ($LASTEXITCODE -ne 0) { throw 'Sleeper fixture compilation failed' }

  & $MakeNSIS /V2 /DSMOKE_TEST /DAPPVERSION=0.4.4 `
    "/DBINARY=$(Join-Path $fixture 'new-launcher.exe')" "/DOUTFILE=$setup" `
    "/DICONFILE=$(Join-Path $app 'crates/app/assets/icon.ico')" `
    "/DCURRENT_FOLDER_NAME=$currentName" "/DLEGACY_FOLDER_NAME=$legacyName" `
    (Join-Path $app 'packaging/windows.nsi')
  if ($LASTEXITCODE -ne 0) { throw 'Migration installer compilation failed' }

  Invoke-Migration -LockOldExe $false
  Invoke-Migration -LockOldExe $true
  Invoke-LockedRollback
} finally {
  foreach ($held in $script:locks) { $held.Dispose() }
  foreach ($running in $script:processes) {
    if (-not $running.HasExited) { $running.Kill() }
  }
  if ($script:child -and -not $script:child.HasExited) { $script:child.Kill() }
  Start-Sleep -Milliseconds 300
  foreach ($path in @($legacy, $current)) {
    if (Test-Path -LiteralPath $path) { Remove-Item -LiteralPath $path -Recurse -Force }
  }
  if (Test-Path -LiteralPath $fixture) { Remove-Item -LiteralPath $fixture -Recurse -Force }
}
[pscustomobject]@{ legacyMarkerAccepted = 'passed'; unlockedMigration = 'passed'; lockedMigration = 'passed'; lockedRollbackName = 'passed'; targetPreserved = 'passed'; restarted = 'passed' } | ConvertTo-Json -Compress
