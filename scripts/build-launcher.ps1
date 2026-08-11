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
    # Electron's Windows runtime files can have pre-1980 timestamps, which the ZIP DOS format cannot encode.
    $stagingDirectory = Join-Path ([System.IO.Path]::GetTempPath()) "industrialis-launcher-portable-$([guid]::NewGuid().ToString('N'))"
    $zipSafeTimestamp = [datetime]::SpecifyKind([datetime]'2000-01-01T00:00:00', [DateTimeKind]::Local)

    New-Item -ItemType Directory -Path $stagingDirectory -Force | Out-Null
    try {
        Copy-Item -Path (Join-Path $packageDirectory "*") -Destination $stagingDirectory -Recurse -Force

        foreach ($entry in @(Get-ChildItem -LiteralPath $stagingDirectory -Recurse -Force)) {
            $entry.CreationTime = $zipSafeTimestamp
            $entry.LastAccessTime = $zipSafeTimestamp
            $entry.LastWriteTime = $zipSafeTimestamp
        }

        Compress-Archive -Path (Join-Path $stagingDirectory "*") -DestinationPath $portableArchive -Force
    } finally {
        if (Test-Path -LiteralPath $stagingDirectory) {
            [System.IO.Directory]::Delete($stagingDirectory, $true)
        }
    }
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
