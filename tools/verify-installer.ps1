# Verifies the unsigned MSI install, upgrade, and uninstall.
#
#   .\tools\verify-installer.ps1 -Msi dist\ZagCam-0.5.0-beta.1.msi -Install
#   .\tools\verify-installer.ps1 -Msi ... -Upgrade
#   .\tools\verify-installer.ps1 -Msi ... -Uninstall
#
# Run -Install/-Upgrade/-Uninstall from an elevated PowerShell on the test machine.

param(
    [string]$Msi = "",
    [switch]$Install,
    [switch]$Upgrade,
    [switch]$Uninstall
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
if (-not $Msi) {
    $Msi = Get-ChildItem (Join-Path $root "dist") -Filter "ZagCam-*.msi" -ErrorAction SilentlyContinue |
        Sort-Object LastWriteTime -Descending |
        Select-Object -First 1 -ExpandProperty FullName
}
if (-not $Msi -or -not (Test-Path -LiteralPath $Msi)) { throw "MSI not found; pass -Msi" }
$Msi = (Resolve-Path -LiteralPath $Msi).Path
$installDir = Join-Path $env:ProgramFiles "ZagCam"
$exe = Join-Path $installDir "ZagCam.exe"
$provider = Join-Path $installDir "zagcam_vcam.dll"
$model = Join-Path $installDir "models\selfie_segmentation_landscape.onnx"
$settings = Join-Path $env:APPDATA "ZagCam\settings.json"

function Test-Admin {
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    $principal = New-Object Security.Principal.WindowsPrincipal($identity)
    return $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
}

function Invoke-Msi([string[]]$Arguments) {
    $process = Start-Process msiexec.exe -ArgumentList $Arguments -Wait -PassThru
    if ($process.ExitCode -ne 0 -and $process.ExitCode -ne 3010) {
        throw "msiexec failed with exit code $($process.ExitCode)"
    }
}

function Get-Registered([string]$Status) {
    if ($Status -match "registered DLL:\s+(.+)$") { return $Matches[1].Trim() }
    return "none"
}

if (Get-Process ZagCam -ErrorAction SilentlyContinue) {
    throw "quit ZagCam before running installer checks"
}

Write-Output "msi: $Msi"
Write-Output "size: $([math]::Round((Get-Item -LiteralPath $Msi).Length / 1MB, 2)) MiB"

if ($Install -or $Upgrade -or $Uninstall) {
    if (-not (Test-Admin)) { throw "run this script from an elevated PowerShell" }
}

if ($Install -or $Upgrade) {
    Invoke-Msi @("/i", "`"$Msi`"", "/qn", "/norestart")
    if ($Upgrade) {
        Invoke-Msi @("/i", "`"$Msi`"", "/qn", "/norestart")
    }
    foreach ($required in @($exe, $provider, $model)) {
        if (-not (Test-Path -LiteralPath $required)) { throw "missing installed file: $required" }
    }
    $status = (& $exe vcam status 2>&1 | Out-String)
    $registered = Get-Registered $status
    if ($registered -eq "none" -or -not (Test-Path -LiteralPath $registered)) {
        throw "provider was not registered correctly`n$status"
    }
    if (-not $registered.StartsWith($installDir, [StringComparison]::OrdinalIgnoreCase)) {
        throw "provider registered outside the install directory: $registered"
    }
    Write-Output "installed; provider: $registered"
}

if ($Uninstall) {
    $before = if (Test-Path -LiteralPath $settings) { (Get-FileHash -LiteralPath $settings).Hash } else { "" }
    Invoke-Msi @("/x", "`"$Msi`"", "/qn", "/norestart")
    if (Test-Path -LiteralPath $installDir) { throw "install directory still exists after uninstall" }
    $run = Get-ItemProperty "HKCU:\Software\Microsoft\Windows\CurrentVersion\Run" -Name ZagCam -ErrorAction SilentlyContinue
    if ($run) { throw "auto-start entry still exists after uninstall" }
    $after = if (Test-Path -LiteralPath $settings) { (Get-FileHash -LiteralPath $settings).Hash } else { "" }
    if ($before -ne $after) { throw "settings changed during uninstall" }
    Write-Output "uninstalled; settings preserved"
}

Write-Output "done"