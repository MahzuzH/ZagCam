# Builds the unsigned beta zip for Sprint 13.
#
#   .\tools\package-beta.ps1
#   .\tools\package-beta.ps1 -SkipBuild
#
# Output: dist\ZagCam-<version>.zip and .zip.sha256

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
$commit = (& git rev-parse --short HEAD 2>$null | Out-String).Trim()
if (-not $commit) { $commit = "unknown" }

$stage = Join-Path $OutDir "ZagCam-$version"
if (Test-Path -LiteralPath $stage) { Remove-Item -LiteralPath $stage -Recurse -Force }
New-Item -ItemType Directory -Force -Path (Join-Path $stage "models") | Out-Null

$files = @(
    @{ From = (Join-Path $release "ZagCam.exe"); To = "ZagCam.exe" },
    @{ From = (Join-Path $release "zagcam_vcam.dll"); To = "zagcam_vcam.dll" },
    @{ From = (Join-Path $root "models\selfie_segmentation_landscape.onnx"); To = "models\selfie_segmentation_landscape.onnx" },
    @{ From = (Join-Path $root "models\LICENSE"); To = "models\LICENSE" },
    @{ From = (Join-Path $root "THIRD_PARTY_NOTICES.md"); To = "THIRD_PARTY_NOTICES.md" },
    @{ From = (Join-Path $root "beta\QUICKSTART.md"); To = "QUICKSTART.md" },
    @{ From = (Join-Path $root "beta\FEEDBACK.md"); To = "FEEDBACK.md" },
    @{ From = (Join-Path $root "beta\SETUP_SURVEY.md"); To = "SETUP_SURVEY.md" },
    @{ From = (Join-Path $root "compat\known-issues.md"); To = "KNOWN-ISSUES.md" },
    @{ From = (Join-Path $root "tools\collect-logs.ps1"); To = "collect-logs.ps1" }
)
foreach ($file in $files) {
    if (-not (Test-Path -LiteralPath $file.From)) { throw "missing package input: $($file.From)" }
    Copy-Item -LiteralPath $file.From -Destination (Join-Path $stage $file.To) -Force
}

$manifest = @(
    "ZagCam $version beta",
    "Built $(Get-Date -Format 'yyyy-MM-dd HH:mm:ss')",
    "Commit $commit",
    "Unsigned beta. Windows may show SmartScreen; choose More info -> Run anyway.",
    "Install: .\ZagCam.exe vcam install (admin), then .\ZagCam.exe",
    "Uninstall: quit the tray app, then .\ZagCam.exe vcam uninstall (admin)"
)
Set-Content -LiteralPath (Join-Path $stage "VERSION.txt") -Value $manifest -Encoding UTF8

$zip = Join-Path $OutDir "ZagCam-$version.zip"
if (Test-Path -LiteralPath $zip) { Remove-Item -LiteralPath $zip -Force }
Compress-Archive -Path (Join-Path $stage "*") -DestinationPath $zip -CompressionLevel Optimal
$hash = (Get-FileHash -LiteralPath $zip -Algorithm SHA256).Hash
Set-Content -LiteralPath "$zip.sha256" -Value "$hash  $(Split-Path -Leaf $zip)" -Encoding ASCII

Write-Output "package: $zip"
Write-Output "sha256:  $hash"
Write-Output "size:    $([math]::Round((Get-Item -LiteralPath $zip).Length / 1MB, 2)) MiB"
