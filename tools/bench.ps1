# Repeatable release benchmark for the M4 performance budgets.
# Usage: powershell -ExecutionPolicy Bypass -File tools\bench.ps1
#        powershell -ExecutionPolicy Bypass -File tools\bench.ps1 -Seconds 10 -SkipIdle
[CmdletBinding()]
param(
    [int]$Seconds = 5,
    [string]$Background = "",
    [string]$OutFile = "",
    [switch]$SkipIdle
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
if (-not $OutFile) {
    $OutFile = Join-Path $root "bench\latest.md"
}
$exe = Join-Path $root "target\release\ZagCam.exe"
if (-not (Test-Path $exe)) {
    throw "release binary not found; run: cargo build --release"
}

Add-Type -AssemblyName System.Drawing
if (-not $Background) {
    $Background = Join-Path $env:TEMP "zagcam-bench-background.png"
    $bitmap = New-Object System.Drawing.Bitmap 1280, 720
    $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
    $graphics.Clear([System.Drawing.Color]::FromArgb(32, 64, 96))
    $graphics.Dispose()
    $bitmap.Save($Background, [System.Drawing.Imaging.ImageFormat]::Png)
    $bitmap.Dispose()
}

function Invoke-Probe {
    param([string]$Mode, [string]$Quality, [string]$Image)
    $probeArgs = @("--probe", "$Seconds", "--mode", $Mode, "--quality", $Quality)
    if ($Mode -eq "image") {
        $probeArgs += @("--background", $Image)
    }
    $output = (& $exe @probeArgs 2>&1 | Out-String)
    $row = [ordered]@{
        Case = "$Mode/$Quality"
        Fps = $null
        RamMiB = $null
        CpuPercent = $null
        Preprocess = $null
        Infer = $null
        Mask = $null
        Blur = $null
        Composite = $null
        FirstFrame = $null
        Interval = $null
        LatencyP50 = $null
        LatencyP95 = $null
    }
    if ($output -match "baseline: ([\d.]+) fps over [\d.]+s \| working set ([\d.]+) MiB \| CPU ([\d.]+)%") {
        $row.Fps = [double]$matches[1]
        $row.RamMiB = [double]$matches[2]
        $row.CpuPercent = [double]$matches[3]
    } else {
        throw "probe failed for $Mode/$Quality`n$output"
    }
    if ($output -match "preprocess ([\d.]+) ms \| infer[^ ]* ([\d.]+) ms \| mask ([\d.]+) ms \| blur ([\d.]+) ms \| composite ([\d.]+) ms") {
        $row.Preprocess = [double]$matches[1]
        $row.Infer = [double]$matches[2]
        $row.Mask = [double]$matches[3]
        $row.Blur = [double]$matches[4]
        $row.Composite = [double]$matches[5]
    }
    if ($output -match "first frame ([\d.]+) ms \| average interval ([\d.]+) ms \| capture-to-output p50 ([\d.]+) ms \| p95 ([\d.]+) ms") {
        $row.FirstFrame = [double]$matches[1]
        $row.Interval = [double]$matches[2]
        $row.LatencyP50 = [double]$matches[3]
        $row.LatencyP95 = [double]$matches[4]
    }
    return [pscustomobject]$row
}

$cases = @(
    @{ Mode = "off"; Quality = "balanced" },
    @{ Mode = "blur"; Quality = "eco" },
    @{ Mode = "blur"; Quality = "balanced" },
    @{ Mode = "blur"; Quality = "quality" },
    @{ Mode = "image"; Quality = "balanced" }
)

$rows = @()
foreach ($case in $cases) {
    Write-Host "probing $($case.Mode)/$($case.Quality)..."
    $rows += Invoke-Probe -Mode $case.Mode -Quality $case.Quality -Image $Background
    Start-Sleep -Milliseconds 750
}

$releaseDir = Join-Path $root "target\release"
$exeSize = (Get-Item $exe).Length
$binaries = Get-ChildItem $releaseDir -File |
    Where-Object { $_.Extension -in ".exe", ".dll" } |
    Measure-Object Length -Sum
$model = Get-Item (Join-Path $root "models\selfie_segmentation_landscape.onnx")
$zip = Join-Path $env:TEMP "zagcam-bench.zip"
$package = @($exe, (Join-Path $releaseDir "zagcam_vcam.dll"), $model.FullName,
    (Join-Path $root "THIRD_PARTY_NOTICES.md"))
if (Test-Path $zip) {
    Remove-Item $zip -Force
}
Compress-Archive -Path $package -DestinationPath $zip -CompressionLevel Optimal
$zipSize = (Get-Item $zip).Length

$idle = $null
if (-not $SkipIdle) {
    if (Get-Process ZagCam -ErrorAction SilentlyContinue) {
        Write-Warning "ZagCam is already running; skipping idle measurement"
    } else {
        $process = Start-Process -FilePath $exe -PassThru
        Start-Sleep -Seconds 3
        $process.Refresh()
        if ($process.HasExited) {
            Write-Warning "ZagCam exited during idle measurement; skipping"
        } else {
            # Idle tray means no settings window; close it and sample the tray process.
            $process.CloseMainWindow() | Out-Null
            Start-Sleep -Seconds 3
            $process.Refresh()
            $cpu1 = $process.CPU
            $time1 = Get-Date
            Start-Sleep -Seconds 3
            $process.Refresh()
            $cpu2 = $process.CPU
            $time2 = Get-Date
            $elapsed = ($time2 - $time1).TotalSeconds
            $idle = [pscustomobject]@{
                RamMiB = [math]::Round($process.WorkingSet64 / 1MB, 1)
                CpuPercent = if ($elapsed -gt 0) {
                    [math]::Round((($cpu2 - $cpu1) / $elapsed) * 100, 1)
                } else { 0 }
            }
            Stop-Process -Id $process.Id -Force
        }
    }
}

$lines = @()
$lines += "# ZagCam benchmark"
$lines += ""
$lines += "Generated $(Get-Date -Format 'yyyy-MM-dd HH:mm:ss'), ${Seconds}s probes, release build."
$lines += ""
$lines += "| Case | fps | RAM MiB | CPU % 1 core | first frame ms | interval ms | latency p50 ms | latency p95 ms | preprocess ms | infer ms | mask ms | blur ms | composite ms |"
$lines += "| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |"
foreach ($row in $rows) {
    $lines += "| $($row.Case) | $($row.Fps) | $($row.RamMiB) | $($row.CpuPercent) | $($row.FirstFrame) | $($row.Interval) | $($row.LatencyP50) | $($row.LatencyP95) | $($row.Preprocess) | $($row.Infer) | $($row.Mask) | $($row.Blur) | $($row.Composite) |"
}
$lines += ""
$lines += "Latency is decode plus effect for each probe frame; it excludes the camera frame wait."
$lines += ""
$lines += "## Sizes"
$lines += ""
$lines += "| Artifact | MiB |"
$lines += "| --- | ---: |"
$lines += "| ZagCam.exe | $([math]::Round($exeSize / 1MB, 2)) |"
$lines += "| release binaries (exe + DLLs) | $([math]::Round($binaries.Sum / 1MB, 2)) |"
$lines += "| model | $([math]::Round($model.Length / 1MB, 2)) |"
$lines += "| compressed package (exe, provider, model, notices) | $([math]::Round($zipSize / 1MB, 2)) |"
$lines += ""
$lines += "## Idle tray"
$lines += ""
if ($idle) {
    $lines += "| RAM MiB | CPU % 1 core |"
    $lines += "| ---: | ---: |"
    $lines += "| $($idle.RamMiB) | $($idle.CpuPercent) |"
} else {
    $lines += "Not measured (ZagCam already running or exited during the sample)."
}

New-Item -ItemType Directory -Force -Path (Split-Path -Parent $OutFile) | Out-Null
Set-Content -Path $OutFile -Value $lines -Encoding UTF8
$lines | Write-Output
