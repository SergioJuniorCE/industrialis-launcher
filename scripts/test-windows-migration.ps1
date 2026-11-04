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
$child = $null
try {
  New-Item -ItemType Directory -Path $fixture, $legacy -Force | Out-Null

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

  # A native installation created before the product folder rename.
  [IO.File]::WriteAllText((Join-Path $legacy 'industrialis.exe'), 'old launcher')
  [IO.File]::WriteAllText((Join-Path $legacy 'gpui-install.json'), '{"format":1,"kind":"nsis"}')
  [IO.File]::WriteAllText((Join-Path $legacy 'Uninstall.exe'), 'old uninstaller')
  [IO.File]::WriteAllText((Join-Path $legacy 'LICENSE'), 'old license')

  & $MakeNSIS /V2 /DSMOKE_TEST /DAPPVERSION=0.4.4 `
    "/DBINARY=$(Join-Path $fixture 'new-launcher.exe')" "/DOUTFILE=$setup" `
    "/DICONFILE=$(Join-Path $app 'crates/app/assets/icon.ico')" `
    "/DCURRENT_FOLDER_NAME=$currentName" "/DLEGACY_FOLDER_NAME=$legacyName" `
    (Join-Path $app 'packaging/windows.nsi')
  if ($LASTEXITCODE -ne 0) { throw 'Migration installer compilation failed' }

  # The handoff waits for the old launcher PID after writing the readiness
  # file. This child stands in for the running old launcher.
  $child = Start-Process -FilePath "$env:SystemRoot\System32\ping.exe" `
    -ArgumentList @('-n', '6', '127.0.0.1') -WindowStyle Hidden -PassThru

  $psi = New-Object System.Diagnostics.ProcessStartInfo
  $psi.FileName = $setup
  $psi.Arguments = "/S /UPDATEPID=$($child.Id) /READYFILE=$ready /D=$legacy"
  $psi.UseShellExecute = $false
  $psi.CreateNoWindow = $true
  $installer = [System.Diagnostics.Process]::Start($psi)
  if (-not $installer.WaitForExit(60000)) { throw 'Migration installer did not finish' }
  if ($installer.ExitCode -ne 0) { throw "Migration installer failed with exit code $($installer.ExitCode)" }

  if (-not (Test-Path -LiteralPath (Join-Path $current 'industrialis.exe'))) {
    throw 'Legacy installation was not migrated to the current directory'
  }
  if (-not (Test-Path -LiteralPath (Join-Path $current 'install.json'))) {
    throw 'Migrated installation lacks the current marker'
  }
  $deadline = [DateTime]::UtcNow.AddSeconds(10)
  while (-not (Test-Path -LiteralPath (Join-Path $current 'restarted-version.txt'))) {
    if ([DateTime]::UtcNow -ge $deadline) { throw 'Migrated launcher was not restarted' }
    Start-Sleep -Milliseconds 100
  }
  if (Test-Path -LiteralPath $legacy) { throw 'Retired installation directory was not removed' }
} finally {
  if ($child -and -not $child.HasExited) { $child.Kill() }
  Start-Sleep -Milliseconds 300
  foreach ($path in @($legacy, $current)) {
    if (Test-Path -LiteralPath $path) { Remove-Item -LiteralPath $path -Recurse -Force }
  }
  if (Test-Path -LiteralPath $fixture) { Remove-Item -LiteralPath $fixture -Recurse -Force }
}
[pscustomobject]@{ legacyMarkerAccepted = 'passed'; migratedDirectory = 'passed'; restarted = 'passed' } | ConvertTo-Json -Compress
