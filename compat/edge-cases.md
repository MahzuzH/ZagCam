# Sprint 12 edge cases

Run these after the matrix smoke test, on the beta build. Record `pass`, `fail`, or `expected`
with the provider log snippet. Status starts as `pending`; do not treat an untested case as a pass.

| Case | Steps | Expected | Status |
| --- | --- | --- | --- |
| Several apps read at once | Open Meet plus Discord/OBS and select ZagCam in both. | Both get frames; no crash; CPU stays bounded; one source camera is opened. | pending; two Source Readers fail with 0xC00D3704, see `multi-consumer-smoke.md` |
| App opened before ZagCam | Open the app and select ZagCam, then start ZagCam. | App may need the camera reselected because it cached the list; video starts after selection. | pending |
| Webcam unplugged mid-call | Pull the USB source camera for 5 s, then reconnect. | Engine retries, source returns, virtual camera recovers without restarting ZagCam or the consumer. | pending |
| Frame Server restart | Stop `FrameServer` / `FrameServerMonitor`, then let Windows restart it. | Provider restarts, engine reconnects, frames return without restarting ZagCam. | pending |
| No camera at startup | Start ZagCam with no source camera, then plug one in. | App stays alive and shows the retry message; the camera appears after plug-in and Save/reselect. | pending |
| Settings after uninstall | Change settings, quit, uninstall provider, reinstall, start app. | Camera and mode settings survive; session camera is not stale. | pending |

## Recovery checks

For every failure, capture:

- `ZagCam.exe vcam status`
- `ZagCam.exe vcam watch --seconds 3`
- `%ProgramData%\ZagCam\vcam-<account>.log`
- engine stderr and the app version/OS build

## Known expected behaviour

- Some consumers cache the camera list; “opened before ZagCam” may require reselecting the camera.
- The virtual camera has session lifetime: it disappears when the tray app quits. That is intended.
