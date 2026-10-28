param(
  [string]$AppDirectory = (Join-Path $PSScriptRoot '../apps/launcher'),
  [ValidateSet('preview', 'stable')][string]$Channel = 'preview',
  [string]$ArtifactDirectory
)
$ErrorActionPreference = 'Stop'
$appRoot = (Resolve-Path -LiteralPath $AppDirectory).Path
$expectedVersion = (Get-Content -LiteralPath (Join-Path $appRoot 'package.json') -Raw | ConvertFrom-Json).version
$binary = Join-Path $appRoot 'target/release/industrialis.exe'
$runtimeJson = & node (Join-Path $PSScriptRoot 'windows-runtime.mjs') $binary
if ($LASTEXITCODE -ne 0) { throw 'Windows executable runtime verification failed' }
$runtime = $runtimeJson | ConvertFrom-Json
$artifactRoot = if ($ArtifactDirectory) { (Resolve-Path -LiteralPath $ArtifactDirectory).Path } else { Join-Path $appRoot 'dist' }
$installerName = if ($Channel -eq 'stable') { 'IndustrialisLauncherSetup.exe' } else { 'IndustrialisLauncherPreviewSetup-x64.exe' }
$installer = Join-Path $artifactRoot $installerName
$portable = Join-Path $artifactRoot 'IndustrialisLauncher-windows-x64.zip'
Add-Type -AssemblyName System.IO.Compression.FileSystem
$archive = [System.IO.Compression.ZipFile]::OpenRead($portable)
try {
  $expectedNames = @('industrialis.exe', 'LICENSE', 'README.txt', 'THIRD_PARTY_NOTICES.txt')
  $actualNames = @($archive.Entries | ForEach-Object { $_.FullName })
  if ($actualNames.Count -ne $expectedNames.Count -or (Compare-Object $expectedNames $actualNames)) {
    throw 'Portable archive has missing, duplicate or unexpected entries'
  }
  foreach ($name in @('industrialis.exe', 'LICENSE')) {
    $stream = $archive.GetEntry($name).Open()
    $hasher = [System.Security.Cryptography.SHA256]::Create()
    try { $actualHash = ([BitConverter]::ToString($hasher.ComputeHash($stream))).Replace('-', '') }
    finally { $stream.Dispose(); $hasher.Dispose() }
    $reference = if ($name -eq 'industrialis.exe') { $binary } else { Join-Path $appRoot 'LICENSE' }
    if ($actualHash -ne (Get-FileHash -LiteralPath $reference -Algorithm SHA256).Hash) {
      throw "Portable $name does not match its build input"
    }
  }
  $reader = [System.IO.StreamReader]::new($archive.GetEntry('THIRD_PARTY_NOTICES.txt').Open())
  try { $notices = $reader.ReadToEnd() } finally { $reader.Dispose() }
  $artworkLicense = [System.IO.File]::ReadAllText((Join-Path $appRoot 'crates/app/assets/LICENSE-LUCIDE'))
  if (-not $notices.Contains($artworkLicense) -or -not $notices.Contains('gpui-pre')) {
    throw 'Dependency or complete artwork notices are missing from the portable artifact'
  }
} finally { $archive.Dispose() }
$metadata = (Get-Item -LiteralPath $binary).VersionInfo
if ($metadata.ProductVersion -ne $expectedVersion -or $metadata.FileVersion -ne $expectedVersion) {
  throw 'Launcher executable version differs from the package version'
}
$installerMetadata = (Get-Item -LiteralPath $installer).VersionInfo
if ($installerMetadata.FileVersion -ne $expectedVersion) { throw 'Installer version differs from the package version' }
$report = [ordered]@{
  version = $expectedVersion
  channel = $Channel
  portableEntries = $actualNames
  executableSha256 = (Get-FileHash -LiteralPath $binary -Algorithm SHA256).Hash
  installerSha256 = (Get-FileHash -LiteralPath $installer -Algorithm SHA256).Hash
  portableSha256 = (Get-FileHash -LiteralPath $portable -Algorithm SHA256).Hash
  completeArtworkNotice = $true
  externalMsvcRuntime = $runtime.externalMsvcRuntime
  importedLibraries = $runtime.imports
} | ConvertTo-Json -Depth 3

[IO.File]::WriteAllText((Join-Path $artifactRoot "verification.json"), $report)
$report
