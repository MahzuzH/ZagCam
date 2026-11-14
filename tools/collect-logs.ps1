# Collects the non-video diagnostics needed for a ZagCam beta report.
#
#   .\collect-logs.ps1
#   .\collect-logs.ps1 -OutFile C:\Temp\zagcam-support.txt
#
# The text file contains versions, camera names, settings, and provider logs. No frames, meeting
# content, account details, or telemetry are collected.

param([string]$Exe = "", [string]$OutFile = "")

$ErrorActionPreference = "Continue"

if (-not $Exe) {
    $candidates = @(
        (Join-Path (Get-Location) "ZagCam.exe"),
        (Join-Path $PSScriptRoot "ZagCam.exe"),
        (Join-Path (Split-Path -Parent $PSScriptRoot) "target\release\ZagCam.exe")
    )
    $Exe = $candidates | Where-Object { Test-Path -LiteralPath $_ } | Select-Object -First 1
}
if (-not $OutFile) {
    $desktop = Join-Path $env:USERPROFILE "Desktop"
    if (-not (Test-Path -LiteralPath $desktop)) { $desktop = (Get-Location).Path }
    $OutFile = Join-Path $desktop ("zagcam-support-{0}.txt" -f (Get-Date -Format "yyyyMMdd-HHmmss"))
}

$lines = New-Object System.Collections.Generic.List[string]
function Add-Section([string]$Title, [string]$Body) {
    $script:lines.Add("")
    $script:lines.Add("===== $Title =====")
    $script:lines.Add($Body.TrimEnd())
}

Add-Section "Collected" (Get-Date -Format "yyyy-MM-dd HH:mm:ss")
Add-Section "Machine" @"
User: $env:USERNAME
OS: $((Get-CimInstance Win32_OperatingSystem).Caption)
Version: $((Get-CimInstance Win32_OperatingSystem).Version)
Build: $((Get-CimInstance Win32_OperatingSystem).BuildNumber)
Arch: $env:PROCESSOR_ARCHITECTURE
"@
Add-Section "Executable" ($(if ($Exe) { "$Exe`n$(& $Exe --version 2>&1 | Out-String)" } else { "ZagCam.exe not found" }))
Add-Section "Cameras" ($(if ($Exe) { & $Exe --list 2>&1 | Out-String } else { "not available" }))
Add-Section "Virtual camera status" ($(if ($Exe) { & $Exe vcam status 2>&1 | Out-String } else { "not available" }))

$settings = Join-Path $env:APPDATA "ZagCam\settings.json"
if (Test-Path -LiteralPath $settings) {
    Add-Section "Settings" (Get-Content -LiteralPath $settings -Raw)
} else {
    Add-Section "Settings" "missing: $settings"
}

$logs = Get-ChildItem (Join-Path $env:ProgramData "ZagCam") -Filter "vcam-*.log" -ErrorAction SilentlyContinue
if ($logs) {
    foreach ($log in $logs) {
        Add-Section "Provider log $($log.Name)" ((Get-Content -LiteralPath $log.FullName -Tail 500) -join "`r`n")
    }
} else {
    Add-Section "Provider logs" "none in $env:ProgramData\ZagCam"
}

$events = @(Get-WinEvent -FilterHashtable @{
    LogName = "Application"
    Level = 1, 2
    StartTime = (Get-Date).AddDays(-2)
} -ErrorAction SilentlyContinue | Where-Object { $_.Message -match "ZagCam" } | Select-Object -First 20)
if ($events.Count) {
    Add-Section "Recent application errors" (($events | ForEach-Object {
        "$($_.TimeCreated) [$($_.ProviderName)] $($_.Message)"
    }) -join "`r`n")
} else {
    Add-Section "Recent application errors" "none"
}

Set-Content -LiteralPath $OutFile -Value $lines -Encoding UTF8
Write-Output "Wrote $OutFile"
