# Builds and installs the ZagCam virtual camera media source (zagcam_vcam.dll).
#
#   .\tools\vcam\install-dev.ps1                 # build + install (one UAC prompt)
#   .\tools\vcam\install-dev.ps1 -Configuration debug
#   .\tools\vcam\install-dev.ps1 -Uninstall      # uninstall (one UAC prompt)
#
# The copy under %ProgramFiles%\ZagCam\VirtualCamera is content-hashed, then registered with COM.

param(
    [ValidateSet("debug", "release")]
    [string]$Configuration = "release",
    [switch]$Uninstall
)

$ErrorActionPreference = "Stop"
$repo = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
$profileFlags = if ($Configuration -eq "release") { @("--release") } else { @() }

Push-Location $repo
try {
    if (-not $Uninstall) {
        $build = @("build", "-p", "zagcam-vcam") + $profileFlags
        & cargo @build
        if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }
    }

    $command = if ($Uninstall) { "uninstall" } else { "install" }
    $run = @("run") + $profileFlags + @("--", "vcam", $command)
    & cargo @run
    exit $LASTEXITCODE
} finally {
    Pop-Location
}
