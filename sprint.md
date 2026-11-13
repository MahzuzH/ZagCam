# ZagCam sprint plan

Living plan for the remaining milestones. The roadmap in [README.md](README.md#roadmap) is the
summary; this file tracks the sprint-sized steps, their gates, and known risks.

Conventions for every sprint:

- Windows 11 only for now (Virtual Camera API needs build 22000+; the dev machine is 26200).
- Definition of done: `cargo clippy --workspace --all-targets -- -D warnings` and
  `cargo test --workspace` both green, plus the sprint's manual gate.
- The virtual camera media source is registered in `HKLM` (admin, once). The provider runs
  inside the Windows Camera Frame Server (`Local Service`, session 0).
- Adapted third-party code stays documented in [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md)
  (CamLights, Apache-2.0; Microsoft Windows-Camera sample, MIT).

## M2 — own virtual camera

| Sprint | Status | Deliverable |
| --- | --- | --- |
| 1 — provider skeleton | done | `zagcam-vcam` / `zagcam-vcam-shared` crates; DLL registers; test pattern |
| 2 — camera lifecycle | done | `MFCreateVirtualCamera` wrapper + `zagcam vcam start/stop/status/install/uninstall`; camera enumerates and streams |
| 3 — frame transport | done | seq-lock channel tests + `vcam watch`; user-space `open()` of the `Global\` section proven |
| 4 — provider reads the channel | done | real frames from an external producer reach the camera; live/pattern transitions verified |
| 5 — engine integration | code done | blurred webcam feed into the camera; open camera only while consumed |
| 6 — hardening and uninstall | code done | compatibility matrix, soak, clean uninstall |

### Sprint 4 — provider serves real frames (done)

Result: `zagcam vcam feed` publishes NV12 into the channel; the provider serves those frames and
falls back to the test pattern when the feed stops. Verified with `vcam watch`
(`engine frame: 1280x720, fresh` while feeding, `stale` after) and the provider log
(`source is now live (engine frames)` / `source is now the test pattern`). A short "hold last live
frame" window (250 ms) stops transient read misses from flashing the fallback pattern.

Implemented notes:

- The NV12 fallback pattern moved to `zagcam-vcam-shared` (`fill_test_pattern`), shared by the
  provider and available to other producers.
- `zagcam vcam feed` opens (never creates) the channel, polls `active_request()` for the client's
  size, and writes an animated color-bar pattern while a client reads.
- `read_frame` rejects torn reads and retries briefly; the provider holds the last live frame for
  250 ms before reverting.

### Sprint 5 — engine integration (blurred frames into the camera)

Implemented:

1. `MfvcamSink` in `src/sink.rs` converts the pipeline's RGB output to NV12 and `write_frame`s
   it to the channel; the client's requested size is handled with a reusable resizer.
2. Publishing is demand-driven: `active_request()` gates capture and writes. The engine does not
   open the physical webcam until a client reads, and releases it after 3 s of no demand (FR-7).
3. `--sink mfvcam` is available; `window` and the OBS `vcam` fallback are unchanged.
4. RGB -> NV12 conversion lives in `src/nv12.rs` with BT.709 limited-range coefficients matching
   the provider media type, plus unit tests.
5. The channel is opened with a 1 s retry throttle; a missing provider or a stopped client leaves
   the engine idle without spinning or crashing.

Manual gate (not yet run):

- Chrome/Discord/Teams pick "ZagCam Camera (Windows Virtual Camera)" and show the blurred feed.
- `zagcam vcam watch` reports `engine frame: 1280x720, fresh (...)` while a client reads.
- With no client, the engine does not open the physical camera (idle CPU/RAM near the M1 baseline).

Risks:

- CPU: RGB->NV12 conversion adds to the already multi-core blur path; keep it in the same buffers
  and reuse allocations.
- Frame rate coupling: the provider requests samples at 30 fps; the engine must not block.

### Sprint 6 — hardening and uninstall

Implemented:

1. `zagcam vcam install/uninstall` self-elevate with one UAC prompt via `ShellExecuteExW`.
   The provider DLL is copied to a SHA-256 path under
   `%ProgramFiles%\ZagCam\VirtualCamera\<hash>\zagcam_vcam.dll` and registered from there.
2. Uninstall removes the session camera (best effort), unregisters the installed DLL or deletes
   the CLSID key directly, and removes the Program Files copy. It is idempotent.
3. `tools/vcam/install-dev.ps1` now builds and calls the same `vcam install/uninstall` path.
4. README documents the install/start/feed/uninstall flow.

Manual gate (not yet run):

- Compatibility matrix per PRD §12: Meet (Chrome/Edge/Firefox), Teams (desktop/web), Discord
  (desktop/web), Zoom, OBS; multiple apps at once; app opened before ZagCam; webcam unplugged
  mid-meeting (placeholder frame).
- Stability: 30–60 min soak, no leak; Frame Server restart recovery.
- Decide whether `--sink vcam` (OBS/UnityCapture fallback) stays or is dropped.

- Matrix entries pass; uninstall leaves no CLSID and no camera; soak shows flat memory.

Risks:

- Installer/code signing is M5 work; this sprint only makes register/unregister robust.
- The provider DLL lives inside the OS camera service — keep it minimal (attack surface, PRD §14).

## Later milestones (from PRD)

- **M3** — tray UI, blur/image modes, saved settings, camera opened on demand end to end.
- **M4** — optimization: work toward the PRD CPU targets (capture/decode and the CPU model are the
  current cost; GPU/NPU is the planned escape hatch).
- **M5** — v1.0: video/GIF backgrounds, presets/hotkeys, signed installer, updater.
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
