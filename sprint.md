# ZagCam sprint plan — M4

Living plan for the next milestone. M2 and M3 are done (2026-11-14): the virtual camera, tray app,
settings, image mode, and on-demand lifecycle all work end to end. M4 is optimization and closed
beta: hit the performance/footprint targets, finish the compatibility matrix and soak, and put a
beta build in front of real users.

Conventions for every sprint:

- Windows 11 only for now (Virtual Camera API needs build 22000+; the dev machine is 26200).
- Definition of done: `cargo clippy --workspace --all-targets -- -D warnings` and
  `cargo test --workspace` both green, plus the sprint's manual gate.
- The virtual camera media source is registered in `HKLM` (admin, once). The provider runs
  inside the Windows Camera Frame Server (`Local Service`, session 0).
- Adapted third-party code stays documented in [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md)
  (CamLights, Apache-2.0; Microsoft Windows-Camera sample, MIT).
- Optimize only against measurements from `tools/bench.ps1`, not guesses. Keep the previous
  baseline in the report so regressions are visible.

## Completed milestones

### M2 — own virtual camera (done)

- Provider DLL, shared-memory frame channel, engine integration, install/uninstall.
- Compatibility and soak gates closed; the OBS `--sink vcam` fallback stays as an optional
  development path behind `--features sink-vcam`.
- Normal path: tray app owns a session-lifetime virtual camera; no `vcam start` needed.

### M3 — UI and MVP features (done)

- Versioned `%APPDATA%\ZagCam\settings.json`, atomic save, corrupt-field recovery.
- Image backgrounds composited through the same mask/EMA path as blur.
- Tray app, settings window, single-instance guard, camera-in-use indicator.
- Engine worker owns capture/inference/conversion/channel writes; settings apply live.
- Physical webcam opens only while an app reads the virtual camera and releases after idle.

## M4 baseline (measured, reference laptop, 720p30 release)

| Metric | Current | M4 target |
| --- | --- | --- |
| Installer size | 33.1 MiB exe; 51.0 MiB release binaries + `DirectML.dll` uncompressed | ≤ 30 MB installer |
| Idle RAM (tray, no consumer) | not measured yet | ≤ 15 MB |
| Active RAM (blur) | ~55 MiB | ≤ 100 MB |
| Active CPU (blur) | ~186% of one core | ≤ 32% of one core (8% of 4-core) |
| Frame rate | ~29 fps | stable 30 fps, never below 24 fps |
| Capture-to-output latency | not measured yet | ≤ 100 ms |
| Time to first frame | not measured yet | ≤ 1.5 s after a consumer opens the camera |
| Idle CPU | near 0% | ~0% with no consumer |

The blur CPU number is the hard problem: it is roughly six times the target. M4 must either
reduce it or produce evidence that the target needs revision before beta.

## M4 sprints

| Sprint | Status | Deliverable |
| --- | --- | --- |
| 10 — measurement and quality modes | todo | repeatable benchmark, budget table, Eco/Balanced/Quality |
| 11 — performance and footprint | todo | hit CPU/RAM/latency/size targets or document exceptions |
| 12 — compatibility and soak | todo | full app matrix, unplug/recovery, 4-hour soak |
| 13 — closed beta | todo | unsigned beta package, install/uninstall check, tester feedback |

### Sprint 10 — measurement and quality modes

Work:

1. Add `tools/bench.ps1` that runs release builds and records, for `off`, `blur`, and `image`:
   - fps, working set, private bytes, CPU percent of one core
   - per-stage ms from `--probe` (preprocess, infer, mask, blur, composite)
   - time to first frame and capture-to-channel latency
   - `ZagCam.exe` size, total release binaries, model size, installer size
   - idle tray RAM/CPU with no consumer
2. Extend `--probe` where needed so the numbers above are available without a GUI.
3. Add quality modes to `Settings`, CLI, and the settings window:
   - `eco`: infer every 4th frame, smaller mask/blur work
   - `balanced`: current behavior (infer every 2nd frame)
   - `quality`: infer every frame, higher-quality mask/blur
   - default `balanced`; persist in settings; bump settings version with a v1 fallback
4. Record the baseline report in `README.md` or a `bench/` note; keep it reproducible.
5. Tests: settings round-trip/defaults for quality, invalid quality falls back, `--quality` parsing.

Gate:

- `tools/bench.ps1` produces the same numbers on two consecutive runs (within noise).
- Quality modes switch live from CLI and settings UI without restarting the consumer.
- Baseline report exists and shows current vs target for every M4 budget.
- `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace` pass.

Risks:

- Measurement noise from camera auto-exposure and background CPU load; run on AC power, close
  other camera apps, and report the median of three runs.
- Quality modes multiply the test matrix; keep them as one `Quality` enum, not separate pipelines.

### Sprint 11 — performance and footprint

Work:

1. Profile the blur hot path in release mode; rank costs by wall time and CPU time:
   capture/decode, preprocessing, inference, mask resize/EMA, blur, composite, NV12 conversion,
   shared-channel write.
2. Reduce CPU with the smallest changes that survive the benchmark:
   - avoid duplicate full-frame passes; fuse conversion/composite where practical
   - tune rayon pool size so inference and capture do not oversubscribe the CPU
   - reduce inference cadence or input work under `eco`
   - test `opt-level = 3` vs `"s"` for the hot crates and record the size tradeoff
3. Reduce memory:
   - reuse every per-frame buffer; no allocation in the steady-state path
   - keep one decoded/resized background image; keep the provider frame buffer bounded
   - measure idle tray RAM; if `eframe` alone breaks the 15 MB target, either lazy-create the
     settings window or switch it to a native Win32 window, per the Sprint 8 risk
4. Reduce installer size:
   - find out why `DirectML.dll` is linked even though the CPU path is the only supported EP;
     disable the unused EP if possible (17.7 MiB candidate saving)
   - measure a compressed installer/zip; the 30 MB target is for the installer, not the exe
5. Keep the image path on the same mask/EMA and `blend_rgb` path; no quality regression.

Gate:

- `eco` meets active CPU ≤ 32% of one core and active RAM ≤ 100 MB at 720p30.
- `balanced` and `quality` are measured and documented; neither drops below 24 fps.
- Idle RAM ≤ 15 MB and idle CPU ~0% with no consumer, or a written exception with a plan.
- First frame ≤ 1.5 s; capture-to-output latency ≤ 100 ms.
- Installer ≤ 30 MB compressed, or a written size plan with the remaining MB accounted for.
- All sprint 10 benchmarks re-run and recorded; clippy/tests pass.

Risks:

- The CPU-only MediaPipe model may not reach 8% of a 4-core laptop. If `eco` cannot reach it,
  stop and revise the target with data instead of hiding the miss; GPU/DirectML is M5+ work.
- Removing the DirectML dependency may change ONNX Runtime packaging; verify model inference on
  a clean machine, not just the dev machine.
- A native Win32 settings window is a larger change than trimming `eframe`; only take it if the
  idle RAM measurement proves it is necessary.

### Sprint 12 — compatibility and soak

Work:

1. Run the full compatibility matrix:
   - Google Meet in Chrome, Edge, Firefox
   - Microsoft Teams desktop and web
   - Discord desktop and web
   - Zoom and OBS Studio as comparisons
2. Run the edge cases:
   - several apps reading the virtual camera at once
   - app opened before ZagCam starts
   - webcam unplugged and replugged mid-meeting
   - Frame Server restart while ZagCam is running
   - no source camera at startup, then plug one in
3. Run a 4-hour soak with a real consumer: memory must stay flat, no crash, camera recovers
   after provider/Frame Server restart.
4. Fix P0/P1 bugs found; log known limitations with repro steps.
5. Keep provider logs (`%ProgramData%\ZagCam\vcam-<account>.log`) and engine stderr in the test
   notes for each failure.

Gate:

- Every matrix entry passes or has a documented, accepted exception.
- 4-hour soak passes with flat memory and no crash.
- Unplug/replug and Frame Server restart recover without restarting ZagCam.
- No open P0/P1 issues; clippy/tests pass.

Risks:

- Third-party apps update and change camera handling; record app versions in the matrix.
- Some apps cache the camera list; “opened before ZagCam” may require reselecting the camera.
  Document that as expected behavior unless a fix is cheap.
- Hardware availability: test with at least one USB webcam and one built-in camera if possible.

### Sprint 13 — closed beta

Work:

1. Build a beta package (zip or unsigned installer) containing:
   - `ZagCam.exe`, provider DLL, ONNX model, licenses/notices, quickstart
   - install/uninstall instructions and the admin requirement for the provider
   - a visible beta version string
2. Verify clean install, upgrade, and uninstall:
   - provider registers/unregisters
   - settings survive uninstall
   - no stale session camera after Quit or uninstall
3. Add a lightweight feedback path: issue template, exact steps to collect logs, and a short
   setup survey. No video frames, no account, no telemetry unless explicitly opted in.
4. Recruit 5–10 testers; track setup success, meeting use, crashes, and the top friction points.
5. Triage feedback into M5 work; fix only beta blockers before the next milestone.

Gate:

- ≥ 80% of testers complete install and use ZagCam in a meeting without help.
- Crash-free session rate ≥ 99% across beta sessions.
- Install/uninstall/upgrade leave no stale camera and keep settings intact.
- Beta blockers fixed or explicitly deferred; clippy/tests pass.

Risks:

- Unsigned builds trigger SmartScreen and antivirus false positives; signing is M5 work.
  Document the “More info → Run anyway” step for testers.
- Beta users may run old Windows builds or unusual cameras; capture build number and camera model.
- Feedback volume can stall the sprint; timebox triage and keep a written “known issues” list.

## M4 exit criteria

M4 is done when:

- The Sprint 11 budget gate is met or every miss has a written exception and an owner.
- The full compatibility matrix and 4-hour soak pass.
- A beta package is available and the beta success thresholds are met.
- No open P0/P1 bugs; clippy and tests are green.

## Later milestones (from PRD)

- **M5** — v1.0: GIF/video backgrounds, live preview, presets/hotkeys, mirror/brightness,
  auto-start, signed installer, updater, GPU/NPU as an optional path.
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
- Do not re-enable the `rfd`/`tray-icon` `common-controls-v6` features without an application
  manifest: `TaskDialogIndirect` is missing from the comctl32 v5 shim and the exe fails to load
  with `STATUS_ENTRYPOINT_NOT_FOUND`.
