# Verifies signed update manifests without installing anything.
#
#   .\tools\verify-update.ps1
#
# Requires target\release\ZagCam.exe and .secrets\zagcam-update.key.

param(
    [string]$Exe = "",
    [string]$KeyPath = ""
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
if (-not $Exe) { $Exe = Join-Path $root "target\release\ZagCam.exe" }
if (-not $KeyPath) { $KeyPath = Join-Path $root ".secrets\zagcam-update.key" }
if (-not (Test-Path -LiteralPath $Exe)) { throw "missing release exe: $Exe" }
if (-not (Test-Path -LiteralPath $KeyPath)) { throw "missing update key: $KeyPath" }

$signer = Join-Path $root "target\release\zagcam-sign-manifest.exe"
if (-not (Test-Path -LiteralPath $signer)) {
    & cargo build -p zagcam-sign-manifest --release
    if ($LASTEXITCODE -ne 0) { throw "signer build failed" }
}

$temp = Join-Path $env:TEMP "ZagCam-verify-update"
if (Test-Path -LiteralPath $temp) { Remove-Item -LiteralPath $temp -Recurse -Force }
New-Item -ItemType Directory -Force -Path $temp | Out-Null
$manifest = Join-Path $temp "latest.json"
$signature = "$manifest.sig"

function Write-Manifest([string]$Version) {
    $json = [ordered]@{
        version   = $Version
        url       = "https://example.test/ZagCam-$Version.msi"
        sha256    = ("a" * 64)
        notes_url = "https://example.test/releases"
    } | ConvertTo-Json -Compress
    [System.IO.File]::WriteAllText($manifest, $json, [System.Text.Encoding]::ASCII)
}

function Invoke-Check([string]$Feed) {
    $ErrorActionPreference = "Continue"
    $output = (& $Exe update check --feed $Feed 2>&1 | Out-String)
    return [pscustomobject]@{ Code = $LASTEXITCODE; Output = $output }
}

Write-Manifest "9.9.9"
& $signer sign $KeyPath $manifest
if ($LASTEXITCODE -ne 0) { throw "manifest signing failed" }

$good = Invoke-Check $manifest
if ($good.Code -ne 0 -or $good.Output -notmatch "9\.9\.9 is available") {
    throw "valid manifest was rejected`n$($good.Output)"
}
Write-Output "valid manifest: accepted"

$bytes = [System.IO.File]::ReadAllBytes($manifest)
$bytes[0] = $bytes[0] -bxor 1
[System.IO.File]::WriteAllBytes($manifest, $bytes)
$tampered = Invoke-Check $manifest
if ($tampered.Code -eq 0) { throw "tampered manifest was accepted`n$($tampered.Output)" }
Write-Output "tampered manifest: rejected"

Write-Manifest "9.9.10"
& $signer sign $KeyPath $manifest
if ($LASTEXITCODE -ne 0) { throw "manifest signing failed" }
$signatureBytes = [System.IO.File]::ReadAllBytes($signature)
$signatureBytes[0] = $signatureBytes[0] -bxor 1
[System.IO.File]::WriteAllBytes($signature, $signatureBytes)
$badSignature = Invoke-Check $manifest
if ($badSignature.Code -eq 0) { throw "tampered signature was accepted`n$($badSignature.Output)" }
Write-Output "tampered signature: rejected"

& $signer sign $KeyPath $manifest
if ($LASTEXITCODE -ne 0) { throw "manifest signing failed" }
$fixed = Invoke-Check $manifest
if ($fixed.Code -ne 0 -or $fixed.Output -notmatch "9\.9\.10 is available") {
    throw "re-signed manifest was rejected`n$($fixed.Output)"
}
Write-Output "re-signed manifest: accepted"
Write-Output "done"