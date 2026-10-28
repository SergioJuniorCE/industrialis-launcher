param([string]$Executable = 'apps/launcher/target/debug/industrialis-dev.exe',
  [ValidateSet('caption','appearance','about','explorer','easy','files')][string]$Scenario = 'caption')
$ErrorActionPreference = 'Stop'
$binary = (Resolve-Path -LiteralPath $Executable).Path
$feature = Get-CimInstance Win32_OptionalFeature -Filter "Name = 'Containers-DisposableClientVM'"
if ($feature.InstallState -ne 1) { throw 'Windows Sandbox must already be enabled' }
if (@(Get-Process -Name 'WindowsSandbox*' -ErrorAction SilentlyContinue).Count -ne 0) { throw 'An existing Sandbox is in use; nothing was started' }
$nonce = [guid]::NewGuid().ToString('N')
$fixture = Join-Path (Resolve-Path -LiteralPath 'apps/launcher/target').Path "caption-acceptance-$nonce"
$inputRoot = Join-Path $fixture 'input'
$outputRoot = Join-Path $fixture 'output'
New-Item -ItemType Directory -Path $inputRoot,$outputRoot | Out-Null
Copy-Item -LiteralPath $binary -Destination "$inputRoot/industrialis-dev.exe"
# Development builds use the dynamic CRT; no installation or host mutation.
Copy-Item -LiteralPath "$env:WINDIR/System32/vcruntime140.dll" -Destination $inputRoot
$registry = $(if ($env:CARGO_HOME) { $env:CARGO_HOME } else { Join-Path $env:USERPROFILE '.cargo' })
$sources = @(Get-ChildItem -LiteralPath "$registry/registry/src" -Directory | ForEach-Object {
  Get-ChildItem -LiteralPath $_.FullName -Directory -Filter 'gpui-pre-windows-0.3.8'
})
if ($sources.Count -ne 1) { throw 'One pinned framework Windows source directory required for debug shaders' }
$shaderPath = Join-Path $sources[0].FullName 'src'
New-Item -ItemType Directory -Path "$inputRoot/shaders" | Out-Null
Get-ChildItem -LiteralPath $shaderPath -File | Where-Object { $_.Extension -in @('.hlsl','.h') } |
  Copy-Item -Destination "$inputRoot/shaders"
Copy-Item -LiteralPath "$PSScriptRoot/test-windows-caption-guest.ps1" -Destination "$inputRoot/acceptance.ps1"
Copy-Item -LiteralPath "$PSScriptRoot/test-windows-appearance-native.ps1" -Destination "$inputRoot/appearance.ps1"
Copy-Item -LiteralPath "$PSScriptRoot/test-windows-explorer-native.ps1" -Destination "$inputRoot/explorer.ps1"
Copy-Item -LiteralPath "$PSScriptRoot/test-windows-easy-native.ps1" -Destination "$inputRoot/easy.ps1"
Copy-Item -LiteralPath "$PSScriptRoot/test-windows-files-native.ps1" -Destination "$inputRoot/files.ps1"
@{kind='launcher-caption-acceptance-v1';nonce=$nonce;hostComputer=$env:COMPUTERNAME;shaderPath=$shaderPath;scenario=$Scenario;executableSha256=(Get-FileHash -LiteralPath $binary).Hash} | ConvertTo-Json |
  Set-Content -LiteralPath "$inputRoot/marker.json" -Encoding UTF8
$escapedInput = [Security.SecurityElement]::Escape($inputRoot)
$escapedOutput = [Security.SecurityElement]::Escape($outputRoot)
$wsb = Join-Path $fixture 'acceptance.wsb'
@"
<Configuration>
  <vGPU>Disable</vGPU><Networking>Disable</Networking><ClipboardRedirection>Disable</ClipboardRedirection>
  <AudioInput>Disable</AudioInput><VideoInput>Disable</VideoInput><PrinterRedirection>Disable</PrinterRedirection>
  <MemoryInMB>4096</MemoryInMB>
  <MappedFolders>
    <MappedFolder><HostFolder>$escapedInput</HostFolder><SandboxFolder>C:\LauncherCaptionInput</SandboxFolder><ReadOnly>true</ReadOnly></MappedFolder>
    <MappedFolder><HostFolder>$escapedOutput</HostFolder><SandboxFolder>C:\LauncherCaptionOutput</SandboxFolder><ReadOnly>false</ReadOnly></MappedFolder>
  </MappedFolders>
  <LogonCommand><Command>powershell.exe -NoProfile -NonInteractive -WindowStyle Hidden -ExecutionPolicy Bypass -File C:\LauncherCaptionInput\acceptance.ps1</Command></LogonCommand>
</Configuration>
"@ | Set-Content -LiteralPath $wsb -Encoding UTF8
Write-Output "Caption acceptance fixture: $fixture"
$started = [DateTime]::Now
$process = Start-Process -FilePath "$env:WINDIR/System32/WindowsSandbox.exe" -ArgumentList @('"'+$wsb+'"') -WindowStyle Hidden -PassThru
$ownedHandle = $process.Handle
try {
  $deadline = [DateTime]::UtcNow.AddMinutes(3)
  $result = Join-Path $outputRoot 'result.json'
  while (-not (Test-Path -LiteralPath $result)) {
    if ([DateTime]::UtcNow -gt $deadline) { throw "Caption acceptance timed out: $outputRoot" }
    if ($process.HasExited -and $process.ExitCode -ne 0) { throw "Sandbox failed: $($process.ExitCode)" }
    Start-Sleep -Seconds 1
  }
  Start-Sleep -Milliseconds 300
  $report = Get-Content -LiteralPath $result -Raw | ConvertFrom-Json
  if ($report.nonce -ne $nonce -or $report.guest -ne $true) { throw 'Caption guest identity failed' }
  $report | ConvertTo-Json
  if (-not $report.passed) { throw $report.error }
} finally {
  Start-Sleep -Seconds 2
  foreach ($client in (Get-CimInstance Win32_Process -Filter "Name = 'WindowsSandboxRemoteSession.exe'")) {
    if ($client.ParentProcessId -eq $process.Id -and $client.CreationDate -ge $started -and $client.CommandLine.Contains($wsb)) {
      $owned = [Diagnostics.Process]::GetProcessById($client.ProcessId)
      $identity = $owned.Handle
      if ($owned.Path -ne $client.ExecutablePath -or [Math]::Abs(($owned.StartTime - $client.CreationDate).TotalMilliseconds) -gt 20) { throw 'Sandbox identity changed' }
      $owned.Kill()
      if (-not $owned.WaitForExit(10000)) { throw 'Owned Sandbox client did not exit' }
    }
  }
  # Keep the small receipts and source fixture, not another executable cache.
  Remove-Item -LiteralPath "$inputRoot/industrialis-dev.exe","$inputRoot/vcruntime140.dll"
}
