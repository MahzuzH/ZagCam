# Multi-consumer smoke (2026-11-14)

Environment: `ZagCam 0.4.0-beta.1`, Windows 11 26200, one built-in camera.

1. Started the tray app; `--list` showed `1: ZagCam Camera (Windows Virtual Camera) [1] (virtual)`.
2. Started two consumers: `ZagCam.exe --camera 1 --probe 25`.

Result:

- Consumer 1: 29.7 fps, 23.4 MiB, first frame 59 ms, p95 latency 7.5 ms.
- Consumer 2: `capture failed: Could not capture frame: Hardware MFT failed to start streaming
  due to lack of hardware resources. (0xC00D3704)`.
- Tray app stayed alive; the provider log showed both `start 1280x720` calls and no provider
  error.

Status: **pending real-app confirmation**. Two simultaneous Media Foundation Source Readers fail,
but the provider reports both streams started. Repeat with two real apps (Meet + Discord/OBS) before
classifying it. Tracked as ZC-06 in `compat/known-issues.md`.