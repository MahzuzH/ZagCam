# ZagCam beta quickstart

Version: see the title bar or run `ZagCam.exe --version`.

## Install

1. Download `ZagCam-<version>.msi` from the GitHub Releases page.
2. Run it and approve the Windows admin prompt.
3. If SmartScreen appears, choose **More info → Run anyway**. The installer is currently unsigned.
4. Launch **ZagCam** from the Start menu. The settings window and tray icon appear.

The MSI installs to `%ProgramFiles%\ZagCam`, registers the virtual camera provider, and bundles the
segmentation model. Portable zip packages can still be installed with:

```powershell
.\ZagCam.exe vcam install
```

## Use

1. In Settings, choose the **physical webcam** under Camera. Do not choose `ZagCam Camera`.
2. Choose Blur or Background, pick a background file if needed, then click **Save**.
3. In Meet, Teams, Discord, Zoom, or OBS select **ZagCam Camera**.
4. The physical webcam starts only while an app reads ZagCam. Quit removes the session camera.

Quality modes control the CPU/quality tradeoff: Eco is the lightest, Balanced is the default,
Quality runs inference every frame. Background accepts PNG/JPG/WebP/BMP, animated GIF, or a looping
MP4/MOV/AVI/WMV/MKV file. Save named presets and enable global hotkeys (Ctrl+Alt+B toggle,
Ctrl+Alt+N next preset, Ctrl+Alt+O off) in Settings. **Start with Windows** is opt-in.

## Updates

Use the tray menu **Check for updates...**. ZagCam verifies the signed update manifest and installer
hash before asking to install. Updates are never installed automatically.

## Uninstall

1. Quit ZagCam from the tray.
2. Open Windows Settings → Apps → Installed apps → **ZagCam** → Uninstall.
3. Settings in `%APPDATA%\ZagCam` are intentionally kept.

For a portable package, run `.\ZagCam.exe vcam uninstall` and delete the package folder.

## Troubleshooting

- Camera missing from an app: restart the app or reselect the camera (some apps cache the list).
- No video: check `ZagCam.exe vcam status` and `ZagCam.exe vcam watch --seconds 3`.
- Background not visible: make sure the app uses `ZagCam Camera`, not the physical webcam.
- Logs: `%ProgramData%\ZagCam\vcam-<account>.log`.
- Send a report with `collect-logs.ps1`; see `FEEDBACK.md`. No video frames are collected.