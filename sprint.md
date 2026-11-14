# ZagCam sprint plan — M4/M5

Living plan for the next milestone. M2 and M3 are done (2026-11-14): the virtual camera, tray app,
settings, image mode, and on-demand lifecycle all work end to end. M4 is optimization and closed
beta: hit the performance/footprint targets, finish the compatibility matrix and soak, and put a
beta build in front of real users. M4 sprints 10–11 are done with two owned exceptions; sprints
12–13 have their tooling/package ready. M5 is the v1.0 feature set, planned below.

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
| Installer size | 34.1 MiB exe; 34.3 MiB release binaries; 11.4 MiB compressed package | ≤ 30 MB installer |
| Idle RAM (tray, no consumer) | 73.4 MiB (exception) | ≤ 15 MB |
| Active RAM (blur) | 49.2 MiB eco, 55.6 MiB balanced | ≤ 100 MB |
| Active CPU (blur) | 17–26% eco, 53–67% balanced, 60–94% quality (1 core) | ≤ 32% of one core (8% of 4-core) |
| Frame rate | 19.3 fps (camera raw cap ~19; exception) | stable 30 fps, never below 24 fps |
| Capture-to-output latency | balanced p50 20.0 ms, p95 22.8 ms | ≤ 100 ms |
| Time to first frame | 224–248 ms | ≤ 1.5 s after a consumer opens the camera |
| Idle CPU | 0–0.5% | ~0% with no consumer |

Final numbers come from `bench/latest.md`. Eco meets the CPU/RAM targets; the frame-rate and idle
RAM misses have written exceptions and owners in the Sprint 11 completion notes and
`bench/README.md`.

## M4 sprints

| Sprint | Status | Deliverable |
| --- | --- | --- |
| 10 — measurement and quality modes | done | repeatable benchmark, budget table, Eco/Balanced/Quality |
| 11 — performance and footprint | done (2 exceptions) | hit CPU/RAM/latency/size targets or document exceptions |
| 12 — compatibility and soak | tooling ready; manual run pending | full app matrix, unplug/recovery, 4-hour soak |
| 13 — closed beta | package built; beta pending | unsigned beta package, install/uninstall check, tester feedback |

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

Completion (2026-11-14):

- `tools/bench.ps1` records fps, RAM, CPU, per-stage ms, first frame, capture-to-output p50/p95,
  artifact sizes, and idle with the settings window closed.
- Two consecutive runs are within noise (`bench/opt-s-run1.md`/`opt-s-run2.md`,
  `bench/opt3-run1.md`/`opt3-run2.md`); `bench/latest.md` is the current run and `bench/README.md`
  holds the budget table.
- Quality modes persist, switch live from the CLI and settings UI, and v1/invalid settings fall
  back to balanced. Clippy and tests pass.

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

Completion (2026-11-14):

- Met: eco CPU 16.8–26.1%, eco RAM 49.2 MiB, package 11.37 MiB, first frame 224–248 ms,
  capture-to-output p95 ≤ 50.3 ms, idle CPU 0–0.5%.
- Exception, frame rate (owner: ZagCam maintainer): the reference camera delivers ~19 fps raw in
  NV12/MJPEG/YUYV, and off mode is also 19.3 fps. Recheck with a 30 fps camera in Sprint 12.
- Exception, idle RAM (owner: ZagCam maintainer): 73.4 MiB versus ≤ 15 MiB. Closing the settings
  window saves under 1 MiB because `eframe`/`glow` owns the cost; lazy-create or a native Win32
  window is scheduled for M5.
- Removed the 33 ms engine sleep that capped the consumer path near 20 fps; excluded the unused
  DirectML DLL from the build/package; kept `opt-level = 3` after the measured tradeoff.
  Details: `bench/README.md`.

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

Build (2026-11-14):

- Matrix and procedure: `compat/matrix.md`, `compat/edge-cases.md`, `compat/known-issues.md`.
- Soak monitor: `tools/soak.ps1` (requires a real consumer; writes Markdown + CSV, checks consumer
  presence, RAM growth, process exit, and new provider-log errors).
- 1-minute end-to-end smoke passed with the virtual camera read back as a consumer:
  `compat/soak-smoke.md`.
- Two simultaneous Source Readers reproduce `0xC00D3704`; needs real-app confirmation before
  classifying. Repro: `compat/multi-consumer-smoke.md`, tracked as ZC-06.
- The manual matrix, recovery cases, and 4-hour soak are still pending; this sprint is not done
  until those rows have results.

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

Build (2026-11-14):

- Visible beta version: `0.5.0-beta.1` in the title bar and `ZagCam.exe --version`.
- Package: `tools/package-beta.ps1` produces `dist\ZagCam-<version>.zip` plus a SHA-256 with the
  exe, provider DLL, model/license, notices, quickstart, feedback docs, known issues, and log
  collector.
- Verification: `tools/verify-install.ps1` checks package files and, when elevated, install,
  reinstall/idempotence, uninstall, registration, and settings preservation.
- Feedback: `beta/SETUP_SURVEY.md`, `beta/tester-tracker.csv`,
  `.github/ISSUE_TEMPLATE/beta-report.yml`, and `tools/collect-logs.ps1`.
- Clean-install verification and tester recruitment are still pending; the beta gate is not met
  until the tracker has results.

## M4 exit criteria

M4 is done when:

- The Sprint 11 budget gate is met or every miss has a written exception and an owner.
- The full compatibility matrix and 4-hour soak pass.
- A beta package is available and the beta success thresholds are met.
- No open P0/P1 bugs; clippy and tests are green.

## M5 — v1.0 (planned)

M5 turns the beta into v1.0: media backgrounds, presets/hotkeys, live preview, basic adjustments,
auto-start, and a signed installer with updates. GPU/NPU stays optional and must not block the
release.

Entry conditions:

- M4 exit is met before release work starts: compatibility matrix/soak complete, no open P0/P1, and
  the beta thresholds met. Unfinished M4 items get an explicit owner and deferral.
- ZC-06 (two simultaneous consumers) is confirmed with real apps and fixed or accepted.
- A code-signing certificate and timestamp service are available for Sprint 17; without them the
  signed-installer gate is blocked, not quietly skipped.

M5 budgets (same machine/bench as M4):

| Metric | Target |
| --- | --- |
| Installer | ≤ 30 MB, signed |
| Active RAM | ≤ 100 MB |
| Active CPU | Eco ≤ 32% of one core; GIF/video measured separately |
| Idle RAM | ≤ 15 MB (fix the `eframe` exception) |
| Frame rate | never below the source camera rate; no added stalls |
| Capture-to-output latency | ≤ 100 ms |
| GIF cache | ≤ 64 MB; oversized GIFs resized or rejected with a clear error |
| Live preview | ≤ 10% extra CPU while visible; 0% while hidden |

| Sprint | Status | Deliverable |
| --- | --- | --- |
| 14 — media backgrounds | todo | looping GIF and video, bounded memory, same mask/composite path |
| 15 — presets, hotkeys, auto-start | todo | named presets, global hotkeys, opt-in launch at login |
| 16 — preview, adjustments, idle shell | todo | live preview, mirror/flip, brightness/contrast, ≤15 MB idle |
| 17 — signed installer and updater | todo | signed package, upgrade path, verified update check |
| 18 — hardening and optional GPU | todo | M5 soak/matrix, release candidate, GPU/NPU spike only |

### Sprint 14 — media backgrounds

Work:

1. Extend `BackgroundSource` with `Gif` and `Video` variants; keep static image and blur unchanged.
2. GIF: decode with the `gif`/`image` crates, preserve per-frame delays, loop forever, cap the cache
   at 64 MB, scale/reject oversized files with a clear error, and never download media.
3. Video: pick one decoder that fits the size/license budget after a spike; loop, honor the source
   frame rate, and avoid decoding ahead of the frame clock.
4. Playback clock lives in the pipeline; a paused/hidden consumer does not decode frames.
5. Settings/CLI/settings UI accept GIF/video paths with validation; persist in settings v3 with a
   v2 fallback.
6. Tests: GIF delay/loop math, cache cap, invalid/corrupt media, settings round-trip/migration.

Gate:

- GIF and looping video show in the virtual camera with correct background compositing.
- Cache stays ≤ 64 MB; no per-frame allocation in steady state; no crash on corrupt files.
- Bench: fps follows the camera, latency stays ≤ 100 ms, and active RAM stays ≤ 100 MB.
- Clippy/tests green.

Risks:

- Video decoder licensing/size; choose before writing the pipeline and document it in
  `THIRD_PARTY_NOTICES.md`.
- Long GIFs/high-resolution videos blow the memory budget; reject early instead of degrading.

Completion (2026-11-14):

- `BackgroundKind` detects still/GIF/video; GIF keeps per-frame delays, loops, caps the decoded
  cache at 64 MB, and rejects oversized files. Video uses a Media Foundation source reader on a
  worker thread and loops at EOF.
- CLI/UI accept image, GIF, and video paths; corrupt/missing media falls back to off with an error.
- Release probes: GIF 19.6 fps, 58.4 MiB, p95 24.9 ms; video 19.6 fps, 94.1�96.6 MiB, p95 29.7 ms.
  A 15 s probe crossed the 13.3 s video clip without a stall. Details: `bench/m5-media.md`.
- GIF clock/load, settings detection, and pipeline adjustment tests cover the new logic.
### Sprint 15 — presets, hotkeys, auto-start

Work:

1. Settings v3: named presets (mode, blur, image/media path, adjustments) and the active preset;
   migrate v1/v2 with defaults and keep corrupt-field recovery.
2. Tray menu switches presets in one click; settings UI edits/saves/deletes presets.
3. Global hotkeys via `RegisterHotKey` (toggle effect, next preset, off); resolve conflicts with a
   visible error and no silent failure.
4. Optional launch-at-login: HKCU Run entry only when enabled; remove it on disable/uninstall.
5. Tests: preset migration/round-trip, invalid preset fallback, hotkey parsing/conflict handling,
   auto-start registry value formatting.

Gate:

- Preset switch applies live without restarting the consumer.
- Hotkeys work while a meeting app is focused; conflicts show a clear message.
- Auto-start is opt-in, survives reboot, and leaves no registry entry after disable/uninstall.
- Clippy/tests green.

Risks:

- Global hotkeys may collide with games/apps; keep registration best-effort and visible.
- Presets increase settings schema surface; one migration path and one fallback rule.

Completion (2026-11-14):

- Settings schema v3 stores named presets (mode, blur, media path, quality, adjustments), the
  opt-in hotkey flag, and the opt-in launch-at-login flag; v1/v2 files migrate with defaults.
- Tray menu rebuilds from the saved presets; the settings window saves/applies/deletes presets.
- Fixed global hotkeys: Ctrl+Alt+B toggle, Ctrl+Alt+N next preset, Ctrl+Alt+O off. Registration is
  all-or-nothing; conflicts surface a visible error. Verified end-to-end with a temp settings file:
  Ctrl+Alt+O switched to off and Ctrl+Alt+N applied the next preset.
- Auto-start uses the HKCU Run key and is removed by `vcam uninstall`; registry quoting is tested.
- Manual checks still pending: reboot auto-start, tray preset clicks, and real-meeting hotkeys.
### Sprint 16 — preview, adjustments, idle shell

Work:

1. Live preview in the settings window: show the processed frame through a small shared/engine
   channel, pause when the window is hidden or no consumer is active, and never feed preview frames
   into the virtual camera path.
2. Mirror/flip and brightness/contrast: apply in the composite path, persist in settings/presets,
   and cover both blur and image/media backgrounds.
3. Fix the idle RAM exception: lazy-create the settings window or replace the `eframe` shell with a
   native Win32 window, then re-measure idle RAM against ≤ 15 MB.
4. Keep preview off the hot path: no extra full-frame copies when preview is hidden.
5. Tests: adjustment math, preset persistence, preview lifecycle, idle measurement with the
   settings window closed.

Gate:

- Preview is ≤ 10% extra CPU while visible and 0% while hidden; no virtual-camera regression.
- Mirror/brightness/contrast persist across restart and apply live.
- Idle RAM ≤ 15 MB with no consumer, or the exception is reopened with new evidence.
- Clippy/tests green.

Risks:

- Preview can double memory/CPU if it shares no buffers; reuse the pipeline output or send a
  downscaled copy.
- A native window is a larger change; only take it if lazy `eframe` cannot reach the RAM target.

Completion (2026-11-14):

- Live preview: the engine publishes a downscaled 320 px frame only while the settings window is
  visible; the UI shows it as an egui texture. Preview frames never enter the virtual-camera path.
- Mirror/flip and brightness/contrast are stored in settings/presets, applied after compositing,
  and tested (pixel math + mirror order). CLI flags support headless checks.
- The idle RAM exception is **not fixed**: with the settings window closed the tray still uses
  73.3 MiB and ~0.6% CPU. `eframe` owns the cost; lazy-create/native shell is deferred to M6.
  This is the reopened exception allowed by the Sprint 16 gate.
- Preview CPU with a real consumer: 76.5% one core visible versus 71.0% hidden (GIF background), so
  preview adds ~5.5% and stays within the 10% gate.
- Manual gate still pending: visual preview check and idle RAM acceptance.
### Sprint 17 — signed installer and updater

Work:

1. Build a real installer (MSIX or WiX) that registers the provider, installs the model/media
   assets, creates shortcuts, and writes an uninstall entry.
2. Sign the exe, provider DLL, and installer; timestamp every signature. Verify on a clean machine.
3. Upgrade path: install over the previous version, preserve settings, replace the provider copy,
   and leave no stale session camera.
4. Update check: signed manifest/feed, version compare, download to a temp path, verify signature,
   install only after approval, and roll back on failure.
5. Uninstall removes the provider, shortcuts, auto-start entry, and installed files; settings stay.
6. Tests: manifest parsing/version compare, signature-failure rejection, upgrade/uninstall scripted
   verification, no update without a valid signature.

Gate:

- Installer is signed and verifies on a clean Windows 11 machine.
- Install, upgrade, and uninstall preserve settings and leave no stale camera/registry entries.
- Update check rejects tampered packages and never runs without a valid signature.
- Installer ≤ 30 MB; clippy/tests green.

Risks:

- The certificate/timestamp service may not be available; Sprint 17 is blocked until it is.
- SmartScreen/antivirus reputation takes time even with a signature; document the first-run path.
- Updater attack surface; fail closed and do not auto-install.

### Sprint 18 — hardening and optional GPU

Work:

1. Re-run the full compatibility matrix and a 4-hour soak with GIF/video, presets, preview, and
   auto-start enabled; keep memory flat and no crashes.
2. Run the upgrade/uninstall matrix again with the signed installer.
3. GPU/NPU spike only: measure DirectML/WebGPU as an optional path; do not make it a release gate.
4. Fix only M5 release blockers; defer the rest to M6 with owners.
5. Update README, changelog, known issues, and the v1.0 release notes.

Gate:

- Full matrix/soak pass, no open P0/P1, signed package installs and updates.
- Every budget either met or has a written exception and owner.
- Clippy/tests green; release candidate reproducible from a clean checkout.

Risks:

- GPU/NPU will pull in packaging/driver complexity; keep it opt-in and out of the v1.0 critical path.
- Release hardening can stall on third-party app updates; record versions and rerun failing rows.

## M5 exit criteria

M5 is done when:

- GIF/video backgrounds, presets/hotkeys, live preview, adjustments, and auto-start work end to end.
- The signed installer/updater passes clean install, upgrade, uninstall, and signature rejection.
- The M5 budgets are met or every miss has a written exception and owner.
- Full compatibility/soak is green and no P0/P1 issues are open.

## Later milestones (from PRD)

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






