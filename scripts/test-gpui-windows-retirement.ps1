param([string]$MakeNSIS)
$ErrorActionPreference = 'Stop'
$appRoot = Join-Path (Split-Path -Parent $PSScriptRoot) 'apps/launcher-gpui'
. (Join-Path $appRoot 'packaging/retire-electron.ps1')
$fixture = Join-Path $appRoot ('target/retirement-test-' + [guid]::NewGuid().ToString('N'))
$local = Join-Path $fixture 'Local'
$profile = Join-Path $fixture 'Roaming/industrialis-launcher'
$root = Join-Path $local 'IndustrialisLauncher'
$version = Join-Path $root 'app-1.0.0'
New-Item -ItemType Directory -Path (Join-Path $version 'resources'), $profile | Out-Null
$exe = Join-Path $version 'industrialis-launcher.exe'
$stub = Join-Path $root 'industrialis-launcher.exe'
[IO.File]::WriteAllText($exe, 'fixture executable')
[IO.File]::WriteAllText($stub, 'fixture Squirrel shortcut stub')
[IO.File]::WriteAllText((Join-Path $version 'resources/app.asar'), 'fixture Electron package')
[IO.File]::WriteAllText((Join-Path $root 'Update.exe'), 'fixture updater')
[IO.File]::WriteAllText((Join-Path $profile 'accounts.json'), '{"opaque":"preserve exactly"}')
$before = (Get-FileHash -LiteralPath (Join-Path $profile 'accounts.json')).Hash

function Assert-Throws([scriptblock]$Action) {
  $failed = $false
  try { & $Action } catch { $failed = $true }
  if (-not $failed) { throw 'Expected unsafe/failed retirement to be rejected' }
}
$install = Get-LegacyInstall $local $profile
if ($install.Executables.Count -ne 2 -or $install.Updater -ne (Join-Path $root 'Update.exe')) { throw 'Legacy detection failed to include app and shortcut stub' }
Assert-Throws { Get-LegacyInstall $local (Join-Path $root 'user-profile') }
[IO.File]::WriteAllText((Join-Path $root 'accounts.json'), '{}')
Assert-Throws { Get-LegacyInstall $local $profile }
Remove-Item -LiteralPath (Join-Path $root 'accounts.json')
Assert-Throws { Invoke-LegacyRetirement $install -Running { @() } -Uninstall { throw 'fixture uninstall failure' } }
if (-not (Test-Path -LiteralPath $exe)) { throw 'Failed uninstall altered the executable' }
Assert-Throws { Invoke-LegacyRetirement $install -Running { @() } -Uninstall { } }
Assert-Throws { Invoke-LegacyRetirement $install -Running { @() } -Uninstall { param($updater) Remove-Item -LiteralPath $exe } }
if (-not (Test-Path -LiteralPath $stub)) { throw 'Incomplete uninstall removed the remaining test stub' }
[IO.File]::WriteAllText($exe, 'fixture executable')
Assert-Throws { Invoke-LegacyRetirement $install -WaitSeconds 0 -Running { @('running fixture') } -Uninstall { throw 'must not execute while running' } }
# Only the test callback removes the one synthetic executable. The real helper
# invokes Squirrel, and never composes its own recursive application deletion.
Invoke-LegacyRetirement $install -Running { @() } -Uninstall { param($updater) Remove-Item -LiteralPath $exe, $stub }
if ($null -ne (Get-LegacyInstall $local $profile)) { throw 'Retired fixture remains discoverable' }
[IO.File]::WriteAllText($stub, 'orphan shortcut stub')
Assert-Throws { Get-LegacyInstall $local $profile }
Remove-Item -LiteralPath $stub
if ((Get-FileHash -LiteralPath (Join-Path $profile 'accounts.json')).Hash -ne $before) { throw 'Original profile bytes changed' }
$testKey = 'HKCU:\Software\IndustrialisGpuiRetirementTest-' + [guid]::NewGuid().ToString('N')
try {
  New-Item -Path $testKey | Out-Null
  Set-Item -LiteralPath $testKey -Value ('"' + $exe + '" "%1"')
  New-ItemProperty -LiteralPath $testKey -Name 'fixture' -Value 'retain other values' | Out-Null
  # Only an approved handler whose old executable is gone can be cleared.
  [IO.File]::WriteAllText($exe, 'still installed')
  Remove-DeadLegacyProtocol $install -Key $testKey
  if ($null -eq (Get-Item -LiteralPath $testKey).GetValue('')) { throw 'Live legacy handler was removed' }
  Remove-Item -LiteralPath $exe
  Remove-DeadLegacyProtocol $install -Key $testKey
  if ($null -ne (Get-Item -LiteralPath $testKey).GetValue('')) { throw 'Dead legacy handler was retained' }
  if ((Get-Item -LiteralPath $testKey).GetValue('fixture') -ne 'retain other values') { throw 'Unrelated registry value changed' }
  $otherHandler = '"C:\Unrelated Application\launcher.exe" "%1"'
  Set-Item -LiteralPath $testKey -Value $otherHandler
  Remove-DeadLegacyProtocol $install -Key $testKey
  if ((Get-Item -LiteralPath $testKey).GetValue('') -ne $otherHandler) { throw 'Unrelated protocol handler changed' }
} finally {
  # This unique key is owned entirely by this test, never the real protocol key.
  if (Test-Path -LiteralPath $testKey) { Remove-Item -LiteralPath $testKey }
}
if ($MakeNSIS) {
  $newSource = Join-Path $fixture 'new.rs'
  [IO.File]::WriteAllText($newSource, '#![windows_subsystem = "windows"] fn main() {}')
  $newBinary = Join-Path $fixture 'new.exe'
  & rustc --edition 2021 $newSource -o $newBinary
  if ($LASTEXITCODE -ne 0) { throw 'Retirement fixture compilation failed' }
  $failureScript = Join-Path $fixture 'retire-failure.ps1'
  [IO.File]::WriteAllText($failureScript, 'exit 7')
  $setup = Join-Path $fixture 'retire-failure-setup.exe'
  & $MakeNSIS /V2 /DSMOKE_TEST /DAPPVERSION=0.1.0 "/DBINARY=$newBinary" "/DOUTFILE=$setup" `
    "/DICONFILE=$(Join-Path $appRoot 'crates/app/assets/icon.ico')" "/DRETIRE_SCRIPT_FILE=$failureScript" (Join-Path $appRoot 'packaging/windows.nsi')
  if ($LASTEXITCODE -ne 0) { throw 'Replacement installer compilation failed' }
  foreach ($previous in @($false, $true)) {
    $destination = Join-Path $fixture "Install-$previous"
    New-Item -ItemType Directory -Path $destination | Out-Null
    $installed = Join-Path $destination 'industrialis.exe'
    if ($previous) { [IO.File]::WriteAllText($installed, 'previous GPUI bytes') }
    $process = Start-Process -FilePath $setup -ArgumentList @('/S', "/D=$destination") -WindowStyle Hidden -PassThru
    # Retain the native handle before waiting; Windows PowerShell can otherwise
    # lose a short-lived redirected child's exit code.
    $ownedHandle = $process.Handle
    if (-not $process.WaitForExit(30000)) { throw 'Retirement failure fixture did not finish' }
    if ($null -eq $process.ExitCode -or $process.ExitCode -eq 0) { throw 'Failed retirement did not report a nonzero exit code' }
    if ($previous) {
      if ([IO.File]::ReadAllText($installed) -ne 'previous GPUI bytes') { throw 'Previous GPUI executable was not restored' }
    } elseif (Test-Path -LiteralPath $installed) { throw 'Failed first cutover kept its replacement executable' }
    if (Test-Path -LiteralPath (Join-Path $destination 'gpui-install.json')) { throw 'Failed first cutover wrote an install marker' }
  }
}
[pscustomobject]@{ detection = 'passed'; overlapGuard = 'passed'; profileGuard = 'passed'; failureGuard = 'passed';
  remainingExecutableGuard = 'passed'; runningGuard = 'passed'; installerRollback = [bool]$MakeNSIS;
  protocolCleanup = 'passed'; profilePreserved = $true; actualSquirrelUninstall = 'not run' } | ConvertTo-Json -Compress
