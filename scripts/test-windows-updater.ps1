param([Parameter(Mandatory = $true)][string]$MakeNSIS)
$ErrorActionPreference = 'Stop'
$taskRepo = Split-Path -Parent $PSScriptRoot
$taskApp = Join-Path $taskRepo 'apps/launcher'
$taskFixture = Join-Path $taskApp ('target/updater-smoke-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $taskFixture | Out-Null
$taskSource = @'
#![windows_subsystem = "windows"]
fn main() {
    let root = std::env::current_exe().unwrap().parent().unwrap().to_path_buf();
    std::fs::write(root.join("restarted-version.txt"), "0.1.1").unwrap();
}
'@
[IO.File]::WriteAllText((Join-Path $taskFixture 'fixture.rs'), $taskSource, (New-Object Text.UTF8Encoding($false)))
& rustc --edition 2021 (Join-Path $taskFixture 'fixture.rs') -o (Join-Path $taskFixture 'new-launcher.exe')
if ($LASTEXITCODE -ne 0) { throw 'Fixture compilation failed' }
$taskInstaller = Join-Path $taskFixture 'setup.exe'
& $MakeNSIS /V2 /DSMOKE_TEST /DAPPVERSION=0.1.1 "/DBINARY=$(Join-Path $taskFixture 'new-launcher.exe')" "/DOUTFILE=$taskInstaller" "/DICONFILE=$(Join-Path $taskApp 'crates/app/assets/icon.ico')" (Join-Path $taskApp 'packaging/windows.nsi')
if ($LASTEXITCODE -ne 0) { throw 'NSIS compilation failed' }
$taskPriorInstaller = $env:LAUNCHER_NSIS_SMOKE_INSTALLER
try {
    $env:LAUNCHER_NSIS_SMOKE_INSTALLER = $taskInstaller
    Push-Location $taskApp
    try {
        & cargo test -p industrialis-backend --jobs 1 --locked release_updater::tests::nsis_install_update_restart_and_uninstall -- --ignored --exact --nocapture
        if ($LASTEXITCODE -ne 0) { throw 'Windows update smoke test failed' }
    } finally { Pop-Location }
} finally { $env:LAUNCHER_NSIS_SMOKE_INSTALLER = $taskPriorInstaller }
