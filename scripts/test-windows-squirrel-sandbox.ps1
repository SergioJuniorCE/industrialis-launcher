param(
  [Parameter(Mandatory=$true)][string]$LegacyInstaller,
  [Parameter(Mandatory=$true)][string]$StableArtifactDirectory,
  [switch]$RunningLegacy,
  [switch]$NativeUi,
  [switch]$Run
)
$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
$app = Join-Path $repo 'apps/launcher'
$legacy = (Resolve-Path -LiteralPath $LegacyInstaller).Path
$stable = (Resolve-Path -LiteralPath $StableArtifactDirectory).Path
$receipt = Get-Content -LiteralPath (Join-Path $stable 'verification.json') -Raw | ConvertFrom-Json
if ($receipt.channel -ne 'stable') { throw 'Use a verified stable artifact directory' }
if ($receipt.externalMsvcRuntime -ne $false) { throw 'Use a stable artifact verified to need no external MSVC runtime' }
$legacyHash = '5B6A7B5ED7B94510124F3867B1C1ADB30D5A40E4F817AB89D3427743F8A8C746'
if ((Get-FileHash -LiteralPath $legacy).Hash -ne $legacyHash) { throw 'Use the unchanged public launcher-v0.3.0 Windows installer' }
$setup = Join-Path $stable 'IndustrialisLauncherSetup.exe'
if ((Get-FileHash -LiteralPath $setup).Hash -ne $receipt.installerSha256) { throw 'Stable installer changed after verification' }
$fixture = Join-Path $app ('target/live-acceptance/squirrel-sandbox-' + [guid]::NewGuid().ToString('N'))
$sandboxInputs = Join-Path $fixture 'input'
$sandboxOutputs = Join-Path $fixture 'output'
New-Item -ItemType Directory -Path $sandboxInputs,$sandboxOutputs | Out-Null
Copy-Item -LiteralPath $legacy -Destination (Join-Path $sandboxInputs 'legacy-setup.exe')
Copy-Item -LiteralPath $setup -Destination (Join-Path $sandboxInputs 'launcher-setup.exe')
Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'test-windows-squirrel-guest.ps1') -Destination (Join-Path $sandboxInputs 'acceptance.ps1')
Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'test-windows-native-ui.ps1') -Destination (Join-Path $sandboxInputs 'native-ui.ps1')
Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'test-windows-editor-guest.ps1') -Destination (Join-Path $sandboxInputs 'editor.ps1')
Copy-Item -LiteralPath (Join-Path $app 'crates/app/assets/brand.png') -Destination (Join-Path $sandboxInputs 'acceptance.png')
$nonce = [guid]::NewGuid().ToString('N')
[ordered]@{ kind='launcher-expendable-squirrel-acceptance-v1'; nonce=$nonce; hostComputer=$env:COMPUTERNAME;
  legacySha256=$legacyHash; launcherInstallerSha256=$receipt.installerSha256; launcherExecutableSha256=$receipt.executableSha256;
  runningLegacy=[bool]$RunningLegacy; nativeUi=[bool]$NativeUi } |
  ConvertTo-Json | Set-Content -LiteralPath (Join-Path $sandboxInputs 'marker.json') -Encoding UTF8
$escapedInput = [Security.SecurityElement]::Escape($sandboxInputs)
$escapedOutput = [Security.SecurityElement]::Escape($sandboxOutputs)
$configuration = @"
<Configuration>
  <vGPU>Disable</vGPU>
  <Networking>Disable</Networking><ClipboardRedirection>Disable</ClipboardRedirection>
  <AudioInput>Disable</AudioInput><VideoInput>Disable</VideoInput><PrinterRedirection>Disable</PrinterRedirection>
  <MemoryInMB>4096</MemoryInMB>
  <MappedFolders>
    <MappedFolder><HostFolder>$escapedInput</HostFolder><SandboxFolder>C:\LauncherAcceptanceInput</SandboxFolder><ReadOnly>true</ReadOnly></MappedFolder>
    <MappedFolder><HostFolder>$escapedOutput</HostFolder><SandboxFolder>C:\LauncherAcceptanceOutput</SandboxFolder><ReadOnly>false</ReadOnly></MappedFolder>
  </MappedFolders>
  <LogonCommand><Command>powershell.exe -NoProfile -NonInteractive -WindowStyle Hidden -ExecutionPolicy Bypass -File C:\LauncherAcceptanceInput\acceptance.ps1</Command></LogonCommand>
</Configuration>
"@
$wsb = Join-Path $fixture 'acceptance.wsb'
[IO.File]::WriteAllText($wsb,$configuration)
Write-Output ('Prepared isolated acceptance fixture: ' + $fixture)
if (-not $Run) { return }
$feature = Get-CimInstance Win32_OptionalFeature -Filter "Name = 'Containers-DisposableClientVM'"
if ($feature.InstallState -ne 1) { throw 'Windows Sandbox must already be enabled; no host features were changed' }
if (@(Get-Process -Name 'WindowsSandbox*' -ErrorAction SilentlyContinue).Count -ne 0) { throw 'An existing Sandbox is in use; nothing was started' }
$started = [DateTime]::Now
$process = Start-Process -FilePath (Join-Path $env:WINDIR 'System32/WindowsSandbox.exe') -ArgumentList @('"' + $wsb + '"') -WindowStyle Hidden -PassThru
$sandboxHandle = $process.Handle
try {
$deadline = [DateTime]::UtcNow.AddMinutes(15)
$resultPath = Join-Path $sandboxOutputs 'result.json'
$sawOwnedClient = $false
$clientLostAt = $null
while (-not (Test-Path -LiteralPath $resultPath)) {
  if ([DateTime]::UtcNow -gt $deadline) { throw ('Owned Sandbox acceptance exceeded its deadline; inspect ' + $sandboxOutputs) }
  if ($process.HasExited -and $process.ExitCode -ne 0) { throw ('Sandbox launch failed: ' + $process.ExitCode) }
  $clients = @(Get-CimInstance Win32_Process -Filter "Name = 'WindowsSandboxRemoteSession.exe'" | Where-Object {
    $_.ParentProcessId -eq $process.Id -and $_.CreationDate -ge $started -and $_.CommandLine.Contains($wsb)
  })
  if ($clients.Count -gt 0) { $sawOwnedClient = $true; $clientLostAt = $null }
  elseif ($sawOwnedClient) {
    if ($null -eq $clientLostAt) { $clientLostAt = [DateTime]::UtcNow }
    elseif (([DateTime]::UtcNow - $clientLostAt).TotalSeconds -gt 15) { throw ('Owned Sandbox exited without an acceptance receipt: ' + $sandboxOutputs) }
  }
  Start-Sleep -Seconds 2
}
# Set-Content can create a file before its JSON is complete; wait briefly for a
# complete receipt. Guest shutdown leaves all private user data inside the VM.
$report = $null
for ($attempt=0; $attempt -lt 10 -and $null -eq $report; $attempt++) {
  try { $report = Get-Content -LiteralPath $resultPath -Raw | ConvertFrom-Json } catch { Start-Sleep -Milliseconds 200 }
  if ($null -eq $report) { Start-Sleep -Milliseconds 200 }
}
if ($null -eq $report -or $report.nonce -ne $nonce -or $report.guest -ne $true) { throw 'Owned guest result identity failed' }
$report | ConvertTo-Json -Depth 4
if ($report.passed -ne $true) { throw ('Actual Squirrel acceptance failed: ' + $report.error) }
} finally {
  # Allow guest shutdown, then close only this fixture's identified host client,
  # including failure/timeout paths. No existing user Sandbox is ever selected.
  Start-Sleep -Seconds 5
  foreach ($client in (Get-CimInstance Win32_Process -Filter "Name = 'WindowsSandboxRemoteSession.exe'")) {
    if ($client.ParentProcessId -eq $process.Id -and $client.CreationDate -ge $started -and $client.CommandLine.Contains($wsb)) {
      $owned = [Diagnostics.Process]::GetProcessById($client.ProcessId)
      $ownedHandle = $owned.Handle
      if ([Math]::Abs(($owned.StartTime - $client.CreationDate).TotalMilliseconds) -gt 20 -or
          $owned.Path -ne $client.ExecutablePath) { throw 'Sandbox client identity changed before cleanup' }
      $owned.Kill()
      if (-not $owned.WaitForExit(10000)) { throw 'Owned Sandbox client did not exit' }
    }
  }
}
