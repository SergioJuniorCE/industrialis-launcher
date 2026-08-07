[CmdletBinding()]
param(
    [ValidateSet("all", "installer", "portable")]
    [string]$Target = "all",

    [string]$OutputDirectory
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent $PSScriptRoot
$launcherDirectory = Join-Path $repoRoot "apps\launcher"
$makeDirectory = Join-Path $launcherDirectory "out\make"

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

    Push-Location $launcherDirectory
    try {
        & pnpm @Arguments
        if ($LASTEXITCODE -ne 0) {
            throw "pnpm exited with code $LASTEXITCODE."
        }
    } finally {
        Pop-Location
    }
}

function Copy-MakeArtifacts {
    $files = @(Get-ChildItem -LiteralPath $makeDirectory -Recurse -File -ErrorAction SilentlyContinue)
    if ($files.Count -eq 0) {
        throw "Electron Forge did not produce artifacts in $makeDirectory."
    }

    foreach ($file in $files) {
        Copy-Item -LiteralPath $file.FullName -Destination $OutputDirectory -Force
    }
}

function New-PortableArchive {
    $packageDirectory = Join-Path $launcherDirectory "out\industrialis-launcher-win32-x64"
    if (-not (Test-Path -LiteralPath $packageDirectory -PathType Container)) {
        throw "Electron package was not produced at $packageDirectory."
    }

    $package = Get-Content -LiteralPath (Join-Path $launcherDirectory "package.json") -Raw | ConvertFrom-Json
    $portableArchive = Join-Path $OutputDirectory "industrialis-launcher-$($package.version)-windows-portable.zip"
    Compress-Archive -Path (Join-Path $packageDirectory "*") -DestinationPath $portableArchive -Force
}

switch ($Target) {
    "portable" {
        Invoke-Pnpm -Arguments @("package")
        New-PortableArchive
    }
    "all" {
        Invoke-Pnpm -Arguments @("make")
        Copy-MakeArtifacts
        New-PortableArchive
    }
    "installer" {
        Invoke-Pnpm -Arguments @("make")
        Copy-MakeArtifacts
    }
}

Write-Host "Launcher artifacts are available in $OutputDirectory"
Get-ChildItem -LiteralPath $OutputDirectory -File | Select-Object Name, Length, LastWriteTime
