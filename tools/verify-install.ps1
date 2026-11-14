# Verifies the beta package install, upgrade/idempotence, and uninstall.
#
#   .\tools\verify-install.ps1                    # non-destructive package checks
#   .\tools\verify-install.ps1 -Install           # admin: register provider, check status
#   .\tools\verify-install.ps1 -Upgrade           # admin: install twice, check idempotence
#   .\tools\verify-install.ps1 -Uninstall         # admin: unregister, verify settings kept
#
# Install/uninstall change HKLM and %ProgramFiles%; run only on the test machine.

param(
    [switch]$Install,
    [switch]$Upgrade,
    [switch]$Uninstall,
    [string]$Exe = ""
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
if (-not $Exe) {
    $candidates = @(
        (Join-Path $root "ZagCam.exe"),
        (Join-Path $root "target\release\ZagCam.exe")
    )
    $Exe = $candidates | Where-Object { Test-Path -LiteralPath $_ } | Select-Object -First 1
}
if (-not $Exe -or -not (Test-Path -LiteralPath $Exe)) { throw "ZagCam.exe not found; pass -Exe" }
$Exe = (Resolve-Path -LiteralPath $Exe).Path
$dir = Split-Path -Parent $Exe
$settings = Join-Path $env:APPDATA "ZagCam\settings.json"

function Test-Admin {
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    $principal = New-Object Security.Principal.WindowsPrincipal($identity)
    return $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
}
function Invoke-ZagCam([string[]]$Arguments) {
    & $Exe @Arguments
    if ($LASTEXITCODE -ne 0) { throw "ZagCam $($Arguments -join ' ') failed ($LASTEXITCODE)" }
}
function Get-Registered([string]$Status) {
    if ($Status -match "registered DLL:\s+(.+)$") { return $Matches[1].Trim() }
    return "none"
}

Write-Output "exe: $Exe"
Write-Output "version: $(& $Exe --version)"
$provider = Join-Path $dir "zagcam_vcam.dll"
$model = Join-Path $dir "models\selfie_segmentation_landscape.onnx"
if (-not (Test-Path -LiteralPath $model)) {
    $model = Join-Path $root "models\selfie_segmentation_landscape.onnx"
}
foreach ($required in @($provider, $model)) {
    if (-not (Test-Path -LiteralPath $required)) { throw "missing package file: $required" }
}
Write-Output "package files: OK"

if ($Install -or $Upgrade) {
    if (-not (Test-Admin)) { throw "run this script from an elevated PowerShell" }
    Invoke-ZagCam @("vcam", "install")
    if ($Upgrade) {
        Invoke-ZagCam @("vcam", "install")
    }
    $status = (& $Exe vcam status 2>&1 | Out-String)
    $registered = Get-Registered $status
    if ($registered -eq "none" -or -not (Test-Path -LiteralPath $registered)) {
        throw "provider was not registered correctly`n$status"
    }
    Write-Output "provider registered: $registered"
}

if ($Uninstall) {
    if (-not (Test-Admin)) { throw "run this script from an elevated PowerShell" }
    $before = if (Test-Path -LiteralPath $settings) { (Get-FileHash -LiteralPath $settings).Hash } else { "" }
    Invoke-ZagCam @("vcam", "uninstall")
    $status = (& $Exe vcam status 2>&1 | Out-String)
    if ((Get-Registered $status) -ne "none") { throw "provider still registered after uninstall`n$status" }
    $after = if (Test-Path -LiteralPath $settings) { (Get-FileHash -LiteralPath $settings).Hash } else { "" }
    if ($before -ne $after) { throw "settings changed during uninstall" }
    Write-Output "uninstalled; settings preserved"
}

Write-Output "status:"
Write-Output (& $Exe vcam status 2>&1 | Out-String)
Write-Output "done"
