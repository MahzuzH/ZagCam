# ZagCam sprint plan — M3

Living plan for the next milestone. M2 delivered the provider, shared-memory transport, engine
integration, and install/uninstall code; its manual compatibility/soak gates are still the release
blocker. M3 makes ZagCam usable: tray app, blur/image modes, saved settings, and the camera opened
only while an app is reading it.

Conventions for every sprint:

- Windows 11 only for now (Virtual Camera API needs build 22000+; the dev machine is 26200).
- Definition of done: `cargo clippy --workspace --all-targets -- -D warnings` and
  `cargo test --workspace` both green, plus the sprint's manual gate.
- The virtual camera media source is registered in `HKLM` (admin, once). The provider runs
  inside the Windows Camera Frame Server (`Local Service`, session 0).
- Adapted third-party code stays documented in [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md)
  (CamLights, Apache-2.0; Microsoft Windows-Camera sample, MIT).

## M2 carry-over (release gate, not new work)

Before an M3 build is called shippable, finish the M2 manual gates:

- Compatibility matrix: Meet (Chrome/Edge/Firefox), Teams (desktop/web), Discord (desktop/web),
  Zoom, OBS; multiple apps at once; app opened before ZagCam; webcam unplugged mid-meeting.
- 30–60 min soak with flat memory; Frame Server restart recovery.
- Decide whether the OBS `--sink vcam` fallback stays or is dropped.

These can run in parallel with M3. M3 assumes the `--sink mfvcam` path is stable.

## M3 sprints

| Sprint | Status | Deliverable |
| --- | --- | --- |
| 7 — settings and image mode | done (manual gate pending) | persisted config + image backgrounds in the engine |
| 8 — tray app and settings window | done (manual gate pending) | tray icon, quick toggle, simple settings UI |
| 9 — app lifecycle | todo | one app owns the camera; engine stays demand-driven end to end |

### Sprint 7 — settings and image mode

Work:

1. Add `src/settings.rs` with a versioned `Settings` struct:
   - source camera name (stable) plus index fallback
   - mode: `off`, `blur`, `image`
   - blur intensity (0..100)
   - background image path
2. Load/save `%APPDATA%\ZagCam\settings.json`:
   - missing file: defaults
   - corrupt or invalid fields: keep valid fields, use defaults for the rest, log once
   - atomic save: write temp file, then rename
   - leave settings untouched on uninstall
3. Add image-background support to the engine:
   - add `image` mode to `Mode` and the CLI
   - decode PNG/JPEG once with the `image` crate
   - resize the background once to the capture size, then composite with the existing mask in
     `Pipeline`
   - missing/corrupt image: reject the switch with a clear message; do not crash or silently
     change the selected mode
4. Keep `blend_rgb` as the single compositing path so image and blur share mask/EMA behavior.
5. Tests: settings round-trip, defaults, corrupt JSON, background resize/composite, missing image.

Gate:

- CLI can switch `--mode image --background PATH` and the camera shows the replacement.
- Settings survive a restart and reload the same source/mode/blur/image.
- Missing image gives an error and leaves the previous valid mode active.

Risks:

- Image decode memory: decode once, keep one resized frame; reject absurd dimensions.
- Background composition cost: reuse buffers; do not decode or resize per frame.

### Sprint 8 — tray app and settings window

Work:

1. Choose the UI stack: `eframe` for the settings window and `tray-icon` for the tray menu.
   If tray integration fights the winit loop, fall back to native `Shell_NotifyIconW`; do not
   pull in a full framework.
2. `zagcam` with no arguments opens the tray app instead of the preview window.
   Preserve the existing CLI paths (`--list`, `--probe`, `vcam ...`) for development.
3. Tray menu:
   - Enable/disable effect (quick toggle, remembers the last mode)
   - Off / Blur / Image
   - Open settings
   - Quit
4. Settings window:
   - camera source picker from `capture::list()`
   - mode picker
   - blur slider
   - background image picker
   - Save applies live; Cancel reverts
5. Show a clear “camera in use” indicator and a single-instance guard so two ZagCam processes
   cannot fight over the session camera.
6. No live preview, GIF/video, hotkeys, or quality presets; those are later milestones.

Gate:

- Tray icon appears; menu toggles blur on/off while an app is reading the camera.
- Changing blur intensity or image applies without restarting the app.
- Settings window opens and closes without blocking the engine.
- Quit removes the session camera; relaunch restores the saved settings.

Risks:

- UI dependency size: measure the release binary; if `eframe` blows the installer budget,
  replace it with a native Win32 window before adding more UI. Measured 2026-11-14:
  `ZagCam.exe` 33.1 MiB, release binaries + `DirectML.dll` 51.0 MiB uncompressed; installer
  compression still untested.
- UI and engine on one thread would freeze; keep capture/inference on the engine thread.

### Sprint 9 — app lifecycle and on-demand camera

Work:

1. The tray app owns the session virtual camera for its lifetime. It appears to apps while
   ZagCam runs and disappears on Quit.
2. Move engine ownership into the app:
   - main thread owns the camera/COM lifetime and tray/UI
   - worker thread owns capture, inference, conversion, and channel writes
   - settings changes go through a small control channel/`Arc<Mutex<_>>`
3. Apply settings live:
   - mode/blur/background: next frame
   - source camera change: release the current session, reopen on next consumer
4. Keep the Sprint 5 demand rule: the physical webcam opens only when `active_request()` is
   present, and closes after the short idle grace period.
5. Handle app edge cases:
   - app opened before ZagCam: camera appears when ZagCam starts
   - camera unplugged: provider shows placeholder/test pattern, engine retries and recovers
   - no source camera at startup: stay alive and retry instead of exiting
6. End-to-end path: install once, launch ZagCam, pick “ZagCam Camera” in Meet/Discord/Teams.
   No separate `vcam start` needed for normal use.
7. Tests: settings apply while running, source loss/recovery, single-instance guards, no camera
   open with no consumer.

Gate:

- Launch ZagCam → tray icon; open Discord/Meet/Teams → choose ZagCam → blurred or image
  background appears.
- With no app reading, the physical webcam is released and CPU is near idle.
- Changing mode/source in the settings window takes effect live.
- Quit removes the camera; relaunch restores settings and works again.
- `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace` pass.

Risks:

- COM/Media Foundation lifetime: keep `VirtualCamera` on the thread that keeps the process alive.
- Thread shutdown: tray Quit must stop the worker, drop the camera, and not deadlock on the
  channel.
- Camera reopen churn when switching sources; use one release/reopen, not per frame.

## Later milestones (from PRD)

- **M4** — optimization and closed beta: work toward CPU/RAM targets, quality modes, finish the
  compatibility matrix and soak if not already done.
- **M5** — v1.0: GIF/video backgrounds, live preview, presets/hotkeys, mirror/brightness,
  auto-start, signed installer, updater.
- **M6** — Linux (`v4l2loopback`).
- **M7** — macOS (CoreMediaIO camera extension).

## Known environment issues

- **rust-analyzer lock:** VS Code's rust-analyzer holds proc-macro DLLs in `target\debug`, which
  blocks `cargo clean` and relinking (`LNK1104 ... document_features-*.dll`). Close VS Code or stop
  the `rust-analyzer` / `rust-analyzer-proc-macro-srv` processes before a clean or a rebuild that
  must rewrite those files.
- `panic = "abort"` is **not** set in the release profile: the provider must unwind into the COM
  method's `Result` instead of aborting the Frame Server service.
- Media Foundation reports a bogus `1 fps` in the raw media type; trust measured frames, not the
  printed format line.
