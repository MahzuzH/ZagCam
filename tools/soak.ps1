# Sprint 12 soak monitor.
#
#   .\tools\soak.ps1 -Minutes 240
#   .\tools\soak.ps1 -Minutes 10 -SampleSeconds 5   # short smoke
#
# A real consumer must already be reading ZagCam (Windows Camera, Meet, Teams, OBS...) and the
# tray app must be running. The script samples RSS/private bytes/handles/threads and writes a
# Markdown summary plus a CSV. It does not create a fake consumer.

param(
    [int]$Minutes = 240,
    [int]$SampleSeconds = 10,
    [string]$ProcessName = "ZagCam",
    [double]$MaxGrowthMiB = 25,
    [string]$OutDir = ""
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
if (-not $OutDir) { $OutDir = Join-Path $root "soak" }
New-Item -ItemType Directory -Force -Path $OutDir | Out-Null

$process = Get-Process -Name $ProcessName -ErrorAction SilentlyContinue | Select-Object -First 1
if (-not $process) {
    throw "ZagCam is not running; start the tray app and open a real consumer first"
}
$exe = $process.Path
$version = (& $exe --version 2>&1 | Out-String).Trim()
$status = (& $exe vcam status 2>&1 | Out-String)
if ($status -notmatch "media source:\s+live") {
    throw "no live media source; open ZagCam in a real consumer, then rerun`n$status"
}

$log = Get-ChildItem (Join-Path $env:ProgramData "ZagCam") -Filter "vcam-*.log" `
    -ErrorAction SilentlyContinue | Sort-Object LastWriteTime -Descending | Select-Object -First 1
$logLines = if ($log) { @(Get-Content $log.FullName).Count } else { 0 }
$cpuStart = $process.CPU
$start = Get-Date
$deadline = $start.AddMinutes($Minutes)
$stamp = $start.ToString("yyyyMMdd-HHmmss")
$samples = New-Object System.Collections.Generic.List[object]
$crashed = $false
$consumerLost = $false

while ((Get-Date) -lt $deadline) {
    $process.Refresh()
    if ($process.HasExited) { $crashed = $true; break }
    $watch = (& $exe vcam watch --seconds 0 2>&1 | Out-String)
    if ($watch -notmatch "client reading:\s+\d+x\d+") { $consumerLost = $true; break }
    $samples.Add([pscustomobject]@{
        Time = (Get-Date).ToString("HH:mm:ss")
        WorkingSetMiB = [math]::Round($process.WorkingSet64 / 1MB, 1)
        PrivateMiB = [math]::Round($process.PrivateMemorySize64 / 1MB, 1)
        Handles = $process.HandleCount
        Threads = $process.Threads.Count
    })
    Start-Sleep -Seconds $SampleSeconds
}

function Get-Median([object[]]$values) {
    if (-not $values -or $values.Count -eq 0) { return 0 }
    $ordered = @($values | Sort-Object)
    return $ordered[[int]($ordered.Count / 2)]
}

$first = @($samples | Select-Object -First ([math]::Max(1, [math]::Min(6, $samples.Count))))
$last = @($samples | Select-Object -Last ([math]::Max(1, [math]::Min(6, $samples.Count))))
$firstRam = Get-Median @($first | ForEach-Object WorkingSetMiB)
$lastRam = Get-Median @($last | ForEach-Object WorkingSetMiB)
$growth = [math]::Round($lastRam - $firstRam, 1)
$peak = if ($samples.Count) { ($samples | Measure-Object WorkingSetMiB -Maximum).Maximum } else { 0 }
$cpuSeconds = try { [math]::Round($process.CPU - $cpuStart, 1) } catch { 0 }
$logErrors = @()
if ($log) {
    $logErrors = @(Get-Content $log.FullName | Select-Object -Skip $logLines |
        Where-Object { $_ -match "(?i)error|panic|fail" })
}
$pass = (-not $crashed) -and (-not $consumerLost) -and ($samples.Count -gt 0) -and ($growth -le $MaxGrowthMiB)

$csv = Join-Path $OutDir "soak-$stamp.csv"
$samples | Export-Csv -NoTypeInformation -Path $csv
$report = Join-Path $OutDir "soak-$stamp.md"
$lines = @(
    "# ZagCam soak $stamp",
    "",
    "- Version: $version",
    "- Process: $($process.Id) $exe",
    "- Duration: $([math]::Round(((Get-Date) - $start).TotalMinutes, 1)) min, $($samples.Count) samples",
    "- Consumer: active for the whole run: $(-not $consumerLost)",
    "- Process exited during run: $crashed",
    "- Result: $(if ($pass) { 'PASS' } else { 'FAIL' })",
    "",
    "| Metric | First | Last | Peak / delta |",
    "| --- | ---: | ---: | ---: |",
    "| Working set MiB | $firstRam | $lastRam | peak $peak, growth $growth |",
    "| CPU seconds during run |  |  | $cpuSeconds |",
    "| Provider log | $($log.FullName) |  | new error/panic/fail lines: $($logErrors.Count) |",
    "",
    "Limit: growth must be <= $MaxGrowthMiB MiB and the process must not exit.",
    "Samples: $csv",
    "",
    "## New provider log findings",
    ""
)
if ($logErrors.Count) { $lines += ($logErrors | ForEach-Object { "- $_" }) } else { $lines += "- none" }
Set-Content -Path $report -Value $lines -Encoding UTF8
Get-Content $report
if (-not $pass) { exit 1 }
