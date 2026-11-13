# Builds and registers the ZagCam virtual camera media source (zagcam_vcam.dll).
#
#   .\tools\vcam\install-dev.ps1                 # build release DLL + register (admin)
#   .\tools\vcam\install-dev.ps1 -Configuration debug
#   .\tools\vcam\install-dev.ps1 -Uninstall
#
# Registration writes HKLM\Software\Classes\CLSID\{8E7B2A4C-1D9F-4E3B-9C5A-6F0D2B7E4A19},
# so it asks for elevation once.

param(
    [ValidateSet("debug", "release")]
    [string]$Configuration = "release",
    [switch]$Uninstall
)

$ErrorActionPreference = "Stop"

$repo = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
$profileDir = if ($Configuration -eq "release") { "release" } else { "debug" }
$dll = Join-Path $repo "target\$profileDir\zagcam_vcam.dll"

function Test-Admin {
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    (New-Object Security.Principal.WindowsPrincipal($identity)).IsInRole(
        [Security.Principal.WindowsBuiltInRole]::Administrator
    )
}

if (-not (Test-Admin)) {
    Write-Host "Elevating (registration writes to HKLM)..."
    $arguments = @("-NoProfile", "-ExecutionPolicy", "Bypass", "-File", $PSCommandPath, "-Configuration", $Configuration)
    if ($Uninstall) { $arguments += "-Uninstall" }
    $process = Start-Process -FilePath "powershell" -ArgumentList $arguments -Verb RunAs -PassThru
    $process.WaitForExit()
    exit $process.ExitCode
}

if ($Uninstall) {
    if (Test-Path -LiteralPath $dll) {
        & regsvr32.exe /s /u $dll
    } else {
        Write-Host "DLL not found at $dll; nothing to unregister."
    }
    Write-Host "Unregistered (if it was registered)."
    exit 0
}

Push-Location $repo
try {
    cargo build -p zagcam-vcam --$Configuration
    if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }
} finally {
    Pop-Location
}

if (-not (Test-Path -LiteralPath $dll)) { throw "DLL not found at $dll" }

& regsvr32.exe /s $dll
if ($LASTEXITCODE -ne 0) { throw "regsvr32 failed with $LASTEXITCODE" }

Write-Host "Registered $dll"
Write-Host "Create the camera itself with the engine (Sprint 2) or the Windows Settings camera page."
