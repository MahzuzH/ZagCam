# Builds the unsigned per-machine MSI for Sprint 17.
#
#   .\tools\build-installer.ps1
#   .\tools\build-installer.ps1 -SkipBuild
#
# Output: dist\ZagCam-<version>.msi

param(
    [string]$OutDir = "",
    [switch]$SkipBuild
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
if (-not $OutDir) { $OutDir = Join-Path $root "dist" }
$release = Join-Path $root "target\release"

if (-not $SkipBuild) {
    & cargo build --release
    if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }
    & cargo build --release -p zagcam-vcam
    if ($LASTEXITCODE -ne 0) { throw "provider build failed" }
}

$metadata = (& cargo metadata --no-deps --format-version 1 | Out-String) | ConvertFrom-Json
$version = ($metadata.packages | Where-Object { $_.name -eq "ZagCam" }).version
if (-not $version) { throw "cannot read the package version from cargo metadata" }

# Windows Installer only accepts a numeric ProductVersion. Keep the semver
# prerelease in the release tag, not in the MSI version.
$parts = ($version -split "[-+]")[0].Split(".")
while ($parts.Count -lt 3) { $parts += "0" }
$productVersion = ($parts[0..2] -join ".")
if ([int]$parts[0] -gt 255 -or [int]$parts[1] -gt 255 -or [int]$parts[2] -gt 65535) {
    throw "version $version cannot be represented as an MSI ProductVersion"
}

$wix = (Get-Command wix -ErrorAction SilentlyContinue).Source
if (-not $wix) {
    $fallback = "C:\Program Files\WiX Toolset v7.0\bin\wix.exe"
    if (Test-Path -LiteralPath $fallback) { $wix = $fallback }
}
if (-not $wix) { throw "WiX CLI not found; install WiXToolset.WiXCLI" }

New-Item -ItemType Directory -Force -Path $OutDir | Out-Null
$msi = Join-Path $OutDir "ZagCam-$version.msi"
if (Test-Path -LiteralPath $msi) { Remove-Item -LiteralPath $msi -Force }

& $wix build -acceptEula wix7 -arch x64 `
    -d "ReleaseDir=$release" `
    -d "ProductVersion=$productVersion" `
    -pdbtype none `
    -o $msi `
    (Join-Path $root "installer\ZagCam.wxs")
if ($LASTEXITCODE -ne 0) { throw "WiX build failed" }

$size = (Get-Item -LiteralPath $msi).Length
if ($size -gt 30MB) { throw "installer is $([math]::Round($size / 1MB, 2)) MiB; budget is 30 MiB" }

Write-Output "installer: $msi"
Write-Output "version:   $version (MSI $productVersion)"
Write-Output "size:      $([math]::Round($size / 1MB, 2)) MiB"
Write-Output "signed:    no (signing exception)"