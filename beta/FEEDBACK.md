# Beta feedback

Use the beta issue template or send the collected support file. Never send video frames, meeting
recordings, or account credentials; ZagCam collects no telemetry.

## Collect logs

In PowerShell from the package folder:

```powershell
.\collect-logs.ps1
```

The script writes `zagcam-support-<timestamp>.txt` with:

- ZagCam version, Windows build, camera list, `vcam status`
- saved settings (`%APPDATA%\ZagCam\settings.json`)
- provider logs from `%ProgramData%\ZagCam\vcam-*.log`
- recent Windows Application errors that mention ZagCam

Attach that file to the report. Add the app name/version, what you did, what you expected, and what
happened.

## Report checklist

- [ ] ZagCam version (`ZagCam.exe --version`)
- [ ] Windows version/build (`winver`)
- [ ] Camera model (built-in or USB)
- [ ] App and version (Meet, Teams, Discord, Zoom, OBS)
- [ ] Steps to reproduce
- [ ] Support file attached
- [ ] No video/meeting content included

## Severity

- **P0:** crash/hang in a call, ZagCam never appears in the camera list, frames leak between users.
- **P1:** wrong/green frames, camera does not recover after unplug/restart, install/uninstall broken.
- **P2:** performance or quality issue with a workaround.
- **P3:** cosmetic or documentation issue.
