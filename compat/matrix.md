# Sprint 12 compatibility matrix

Run on the target beta build (`ZagCam.exe --version`) on Windows 11 build 22000+. Record the app
version and the camera used for every row. A row passes only when the app shows live ZagCam video
with blur/image active; mark any exception with a link to its notes.

Status: `pending` rows are manual work for Sprint 12, not silent passes.

| App | Version | OS build | Camera | Result | Notes / log |
| --- | --- | --- | --- | --- | --- |
| Google Meet, Chrome |  |  |  | pending |  |
| Google Meet, Edge |  |  |  | pending |  |
| Google Meet, Firefox |  |  |  | pending |  |
| Microsoft Teams desktop |  |  |  | pending |  |
| Microsoft Teams web |  |  |  | pending |  |
| Discord desktop |  |  |  | pending |  |
| Discord web |  |  |  | pending |  |
| Zoom desktop |  |  |  | pending |  |
| OBS Studio |  |  |  | pending |  |

## Before each run

1. Install the provider once from the package: `ZagCam.exe vcam install` (admin).
2. Start `ZagCam.exe`, pick the source camera, set blur or image mode in Settings, Save.
3. Confirm the Windows Camera app sees live ZagCam video.
4. Clear old provider logs or note the current size.

## Per-row procedure

1. Open the app and select **ZagCam Camera** (not the physical webcam).
2. Start a local self-view or test call.
3. Check: correct orientation, no green/pink frames, blur follows the person, image is stable,
   audio unaffected, camera list shows one ZagCam entry.
4. Note app version, OS build, camera model, and result.
5. On failure, attach `%ProgramData%\ZagCam\vcam-<account>.log` and the engine console output.

## Close-out

- Every row has `pass` or an accepted exception with a reason and owner.
- No open P0/P1 bug. P0/P1 means crash/hang during a call, camera never appears, or frames leak
  between users.
