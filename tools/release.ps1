# Builds the unsigned GitHub release assets for Sprint 17.
#
#   .\tools\release.ps1
#   .\tools\release.ps1 -SkipBuild
#
# Output: dist\release\v<version>\
#   ZagCam-<version>.msi
#   latest.json
#   latest.json.sig
#   SHA256SUMS

param(
    [string]$OutDir = "",
    [string]$Repo = "MahzuzH/ZagCam",
    [string]$KeyPath = "",
    [switch]$SkipBuild
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
if (-not $OutDir) { $OutDir = Join-Path $root "dist" }
if (-not $KeyPath) { $KeyPath = Join-Path $root ".secrets\zagcam-update.key" }
if (-not (Test-Path -LiteralPath $KeyPath)) {
    throw "update signing key not found: $KeyPath`nGenerate one with: cargo run -p zagcam-sign-manifest --release -- keygen $KeyPath"
}

& (Join-Path $PSScriptRoot "build-installer.ps1") -OutDir $OutDir -SkipBuild:$SkipBuild

$metadata = (& cargo metadata --no-deps --format-version 1 | Out-String) | ConvertFrom-Json
$version = ($metadata.packages | Where-Object { $_.name -eq "ZagCam" }).version
$msiName = "ZagCam-$version.msi"
$msi = Join-Path $OutDir $msiName
if (-not (Test-Path -LiteralPath $msi)) { throw "missing installer: $msi" }

$tag = "v$version"
$releaseDir = Join-Path $OutDir "release\$tag"
if (Test-Path -LiteralPath $releaseDir) { Remove-Item -LiteralPath $releaseDir -Recurse -Force }
New-Item -ItemType Directory -Force -Path $releaseDir | Out-Null

$releaseMsi = Join-Path $releaseDir $msiName
Copy-Item -LiteralPath $msi -Destination $releaseMsi -Force
$hash = (Get-FileHash -LiteralPath $releaseMsi -Algorithm SHA256).Hash.ToLowerInvariant()
$url = "https://github.com/$Repo/releases/download/$tag/$msiName"
$notes = "https://github.com/$Repo/releases/tag/$tag"

$manifest = [ordered]@{
    version   = $version
    url       = $url
    sha256    = $hash
    notes_url = $notes
} | ConvertTo-Json -Compress
$manifestPath = Join-Path $releaseDir "latest.json"
[System.IO.File]::WriteAllText($manifestPath, $manifest, [System.Text.Encoding]::ASCII)

& cargo run -p zagcam-sign-manifest --release -- sign $KeyPath $manifestPath
if ($LASTEXITCODE -ne 0) { throw "manifest signing failed" }

$assets = Get-ChildItem -LiteralPath $releaseDir -File | Sort-Object Name
$sums = foreach ($asset in $assets) {
    $digest = (Get-FileHash -LiteralPath $asset.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
    "$digest  $($asset.Name)"
}
[System.IO.File]::WriteAllLines(
    (Join-Path $releaseDir "SHA256SUMS"),
    $sums,
    [System.Text.Encoding]::ASCII
)

Write-Output "release: $releaseDir"
Write-Output "tag:     $tag"
Write-Output "assets:"
Get-ChildItem -LiteralPath $releaseDir -File | ForEach-Object {
    Write-Output "  $($_.Name)  $([math]::Round($_.Length / 1MB, 2)) MiB"
}
Write-Output ""
Write-Output "Upload these files to https://github.com/$Repo/releases/new?tag=$tag"
Write-Output "Publish as a normal release, not a pre-release, so /releases/latest resolves."
Write-Output "The MSI is unsigned; SmartScreen warnings are expected."