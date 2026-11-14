# ZagCam beta quickstart

Version: see the title bar or run `ZagCam.exe --version`.

## Install

1. Unzip the package to a folder you can keep (for example `%LOCALAPPDATA%\ZagCamBeta`).
2. Open PowerShell in that folder and register the virtual camera once:
   ```powershell
   .\ZagCam.exe vcam install
   ```
   Approve the Windows admin prompt. The provider DLL is copied to
   `%ProgramFiles%\ZagCam\VirtualCamera` and registered for all users.
3. Start `.\ZagCam.exe`. The settings window and a tray icon appear.

If SmartScreen appears, choose **More info → Run anyway** (the beta is unsigned; signing is M5
work).

## Use

1. Pick the source camera and choose Blur or Background in Settings. Save.
2. In Meet, Teams, Discord, Zoom, or OBS select **ZagCam Camera**.
3. The physical webcam starts only while an app reads ZagCam; Quit removes the session camera.

Quality modes control the CPU/quality tradeoff: Eco is the lightest, Balanced is the default,
Quality runs inference every frame. Background accepts PNG/JPG/WebP/BMP, animated GIF, or a looping
MP4/MOV/AVI/WMV/MKV file. Save named presets and enable global hotkeys (Ctrl+Alt+B toggle,
Ctrl+Alt+N next preset, Ctrl+Alt+O off) in Settings. **Start with Windows** is opt-in.

## Uninstall

1. Quit ZagCam from the tray.
2. In the package folder run:
   ```powershell
   .\ZagCam.exe vcam uninstall
   ```
3. Delete the package folder. Settings in `%APPDATA%\ZagCam` are intentionally kept.

## Troubleshooting

- Camera missing from an app: restart the app or reselect the camera (some apps cache the list).
- No video: check `.\ZagCam.exe vcam status` and `.\ZagCam.exe vcam watch --seconds 3`.
- Logs: `%ProgramData%\ZagCam\vcam-<account>.log`.
- Send a report with `collect-logs.ps1`; see `FEEDBACK.md`. No video frames are collected.
