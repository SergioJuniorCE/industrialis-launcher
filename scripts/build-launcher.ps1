[CmdletBinding()]
param(
    [ValidateSet("all", "nsis", "msi", "portable")]
    [string]$Target = "all",

    [string]$OutputDirectory
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent $PSScriptRoot
$launcherDirectory = Join-Path $repoRoot "apps\launcher"
$tauriDirectory = Join-Path $launcherDirectory "src-tauri"
$releaseDirectory = Join-Path $tauriDirectory "target\release"

if ([string]::IsNullOrWhiteSpace($OutputDirectory)) {
    $OutputDirectory = Join-Path $repoRoot "artifacts\launcher"
} elseif (-not [System.IO.Path]::IsPathRooted($OutputDirectory)) {
    $OutputDirectory = Join-Path $repoRoot $OutputDirectory
}

if (-not (Get-Command pnpm -ErrorAction SilentlyContinue)) {
    throw "pnpm is required but was not found on PATH."
}

New-Item -ItemType Directory -Path $OutputDirectory -Force | Out-Null

function Invoke-Pnpm {
    param([Parameter(Mandatory)][string[]]$Arguments)

    & pnpm @Arguments
    if ($LASTEXITCODE -ne 0) {
        throw "pnpm exited with code $LASTEXITCODE."
    }
}

function Copy-BundleArtifacts {
    param(
        [Parameter(Mandatory)][string]$BundleName,
        [Parameter(Mandatory)][string]$Filter
    )

    $bundleDirectory = Join-Path $releaseDirectory "bundle\$BundleName"
    $files = @(Get-ChildItem -LiteralPath $bundleDirectory -Filter $Filter -File -ErrorAction SilentlyContinue)
    if ($files.Count -eq 0) {
        throw "No $BundleName artifact was produced in $bundleDirectory."
    }

    foreach ($file in $files) {
        Copy-Item -LiteralPath $file.FullName -Destination $OutputDirectory -Force
    }
}

function New-PortableArchive {
    $portableExecutable = Join-Path $releaseDirectory "industrialis-launcher.exe"
    if (-not (Test-Path -LiteralPath $portableExecutable -PathType Leaf)) {
        throw "Portable executable was not produced at $portableExecutable."
    }

    $package = Get-Content -LiteralPath (Join-Path $launcherDirectory "package.json") -Raw | ConvertFrom-Json
    $portableArchive = Join-Path $OutputDirectory "industrialis-launcher-$($package.version)-windows-portable.zip"
    Compress-Archive -LiteralPath $portableExecutable -DestinationPath $portableArchive -Force
}

Push-Location $launcherDirectory
try {
    switch ($Target) {
        "all" {
            # Preserve an unpatched executable before Tauri marks the binary for each installer type.
            Invoke-Pnpm -Arguments @("tauri", "build", "--no-bundle")
            New-PortableArchive
            Invoke-Pnpm -Arguments @("tauri", "build", "--bundles", "nsis,msi")
            Copy-BundleArtifacts -BundleName "nsis" -Filter "*.exe"
            Copy-BundleArtifacts -BundleName "msi" -Filter "*.msi"
        }
        "nsis" {
            Invoke-Pnpm -Arguments @("tauri", "build", "--bundles", "nsis")
            Copy-BundleArtifacts -BundleName "nsis" -Filter "*.exe"
        }
        "msi" {
            Invoke-Pnpm -Arguments @("tauri", "build", "--bundles", "msi")
            Copy-BundleArtifacts -BundleName "msi" -Filter "*.msi"
        }
        "portable" {
            Invoke-Pnpm -Arguments @("tauri", "build", "--no-bundle")
            New-PortableArchive
        }
    }
} finally {
    Pop-Location
}

Write-Host "Launcher artifacts are available in $OutputDirectory"
Get-ChildItem -LiteralPath $OutputDirectory -File | Select-Object Name, Length, LastWriteTime
