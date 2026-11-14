# ZagCam

Lightweight virtual camera that blurs or replaces your webcam background. Pick "ZagCam" as your camera in Discord, Meet, Teams, or anything else that reads a camera from the OS — no per-app setup.

**Status: M2 and M3 done; M4 sprints 10–11 done (with documented exceptions), sprints 12–13 pending.** Capture, background blur, image replacement, quality modes, the tray app, and an own Media Foundation virtual camera are implemented. The engine publishes frames into that camera only while an app is reading it. Next is the compatibility matrix, soak, and closed beta.

## Why

- Small footprint: low RAM, CPU, and installer size compared to NVIDIA Broadcast, XSplit VCam, Camo, or OBS.
- Written in Rust.
- Uses no camera or CPU when no app is using the virtual camera.
- One background setting for every video app.
- Fully local: no video frames leave the machine.

## Features

**MVP (Windows)**

- Capture from a physical webcam, choose the source camera.
- Modes: off (pass-through), background blur (adjustable intensity), image replacement.
- One virtual camera visible to Chrome, Edge, Discord, Teams.
- Tray app with quick toggle and a simple settings window.
- Physical camera opened only while the virtual camera is in use.
- Quality modes (Eco / Balanced / Quality) controlling inference rate and resolution.

**v1.0**

- GIF and looping video backgrounds.
- Presets and global hotkeys.
- Live preview in settings.
- Mirror/flip, basic brightness and contrast.
- Optional start with the OS.
- Digitally signed installer and update mechanism.

**Later:** Linux, macOS, Windows 10 (if demand), GPU/NPU acceleration, higher-quality matting model.

## How it works

```
physical webcam → downscale → segmentation model → upscale + smooth mask
                → composite (blur / image / video) → virtual camera
```

Two processes:

- **Virtual camera provider** — tiny component loaded by the OS. Only receives frames and serves them as a camera.
- **Engine** — captures, runs inference, renders. Sends frames to the provider over shared memory.

The split keeps OS-side code minimal and lets the engine restart without the virtual camera disappearing.

Performance strategy: infer at low resolution, run the model every 2 frames and interpolate, smooth the mask across frames (EMA), downscale-blur-upscale for blur, allocate buffers once and reuse, and open the camera/model only when there is a consumer.

## Performance targets

Measured on the reference laptop with `tools/bench.ps1`; current numbers and the two documented
exceptions are in [`bench/README.md`](bench/README.md).

| Metric | Target |
| --- | --- |
| Installer size (CPU build) | ≤ 30 MB |
| Idle RAM (tray, camera off) | ≤ 15 MB |
| Active RAM (private working set) | ≤ 100 MB |
| Active CPU (4-core laptop, 720p30) | ≤ 8% |
| Capture-to-output latency | ≤ 100 ms |
| Frame rate | stable 30 fps, never below 24 fps |
| Time to first frame | ≤ 1.5 s after an app opens the camera |
| Idle CPU | ~0% with no camera consumer |

## Platform

- **Windows 11** — first target, via Media Foundation Virtual Camera.
- **Linux** — via v4l2loopback.
- **macOS 12.3+** — via CoreMediaIO Camera Extension.
- **Windows 10** — needs a DirectShow filter; deferred until there is demand.

## Works with

Google Meet (Chrome, Edge, Firefox), Microsoft Teams (desktop and web), Discord (desktop and web), plus Zoom and OBS Studio as extra comparisons. Edge cases tested: several apps using the camera at once, apps opened before ZagCam, webcam unplugged mid-meeting.

## Privacy

All processing is local. No account or login required. Telemetry is opt-in and never contains video. A clear indicator shows when the webcam is active. OS-side code is kept minimal to reduce attack surface.

## Tech stack

- **Language:** Rust
- **Capture:** `nokhwa`
- **Inference:** `ort` (ONNX Runtime), MediaPipe Selfie Segmentation (landscape, 256x144) as the starting model
- **Compositing:** CPU with SIMD (`fast_image_resize`) and `rayon`; `wgpu` as an optional GPU path
- **GIF decoding:** `gif` / `image`, frames kept at native GIF resolution
- **Windows API:** `windows` crate, `MFCreateVirtualCamera`
- **UI:** tray app with `eframe`/`egui`, tray menu with `tray-icon`
- **Build:** `opt-level = 3`, LTO, `codegen-units = 1`, `strip = true`

## Roadmap

1. **M0** — Capture prototype: webcam shown in a window via `nokhwa`, baseline CPU/RAM measured. *(done)*
2. **M1** — Background blur: `ort` + MediaPipe, smoothed mask, output to OBS Virtual Camera for validation. *(done)*
3. **M2** — Own virtual camera (Windows 11): provider + engine over shared memory, detected by Chrome, Discord, Teams. *(done)*
4. **M3** — UI and MVP features: tray, blur and image modes, saved settings, camera opened on demand. *(done)*
5. **M4** — Optimization and closed beta: performance tuning, compatibility matrix, stability fixes. *(sprints 10–11 done; 12–13 pending)*
6. **M5** — v1.0 (Windows): video backgrounds, presets and hotkeys, signed installer, update mechanism.
7. **M6** — Linux, then **M7** — macOS.

## Build and usage

Normal use: `cargo run --release --` with no arguments opens the tray app and owns the ZagCam
virtual camera for as long as it runs. The CLI below stays for development and diagnostics.

```sh
cargo run --release -- --list                  # list cameras
cargo run --release --                         # tray app (normal use)
cargo run --release -- --sink window           # preview the first camera (Esc quits)
cargo run --release -- --sink window --mode blur   # preview with background blur
cargo run --release -- --mode blur --blur 80   # stronger blur (0..100)
cargo run --release -- --probe 5 --mode blur   # headless, print fps / RAM / CPU and per-stage ms
cargo run --release -- --sink mfvcam --mode blur   # feed ZagCam's own camera
cargo run --release -- --sink vcam --features sink-vcam   # feed the OBS virtual camera
```

Prefer `--release`: debug builds are an order of magnitude slower.

### Model

`models/selfie_segmentation_landscape.onnx` (Apache-2.0) is committed, so no download step is
needed. It expects a 256x144 RGB input scaled to 0..1 and outputs a person mask; see
[models/LICENSE](models/LICENSE).

### Own virtual camera (M2/M3, done)

ZagCam has its own Media Foundation virtual camera (no OBS needed). Install the provider once,
then launch the tray app:

```sh
cargo build -p zagcam-vcam --release
cargo run --release -- vcam install       # UAC once; copies to %ProgramFiles%\ZagCam\VirtualCamera
cargo run --release --                    # normal use: tray app creates the camera and feeds it
cargo run --release -- vcam status        # registered DLL + whether a source is live
cargo run --release -- vcam watch         # show the shared frame channel state
cargo run --release -- --list             # virtual cameras are marked "(virtual)"
cargo run --release -- vcam uninstall     # UAC once; removes the CLSID and installed copy
```

The development path still works: `vcam start` creates a camera with the built-in test pattern,
and `--sink mfvcam` publishes engine frames into it. The tray app instead owns the camera for
its lifetime, so no `vcam start` is needed. The camera is session-scoped and disappears on Quit.
The engine opens the physical webcam only while an app is reading the virtual camera, and
releases it after the client stops. Without an engine, the provider shows its test pattern.

`cargo run --release -- --camera N --probe 5` can read the virtual camera back as an input,
which is a quick end-to-end check. `zagcam vcam feed` still publishes test frames for
provider-only checks. Provider diagnostics go to `%ProgramData%\ZagCam\vcam-<account>.log`.

### OBS virtual camera (fallback)

`--sink vcam` needs a virtual-camera filter registered with Windows. The `virtualcam` crate
(Apache-2.0, `virtualcam 0.1.1`) uses the **OBS Virtual Camera** filter, so install OBS Studio
once (OBS itself does not need to run). Without it the app prints a clear error and the build
feature `sink-vcam` is off by default.

### Baselines (reference laptop, 1280x720 NV12, release)

Latest `tools/bench.ps1` run; see [`bench/latest.md`](bench/latest.md) and
[`bench/README.md`](bench/README.md) for targets and exceptions.

| Mode | fps | Working set | CPU (1 core = 100%) | Latency p50 / p95 |
| --- | --- | --- | --- | --- |
| off | 19.3 | 20.6 MiB | 18% | 5.8 / 6.7 ms |
| blur, eco | 19.3 | 49.2 MiB | 17% | 4.9 / 12.7 ms |
| blur, balanced | 19.3 | 55.6 MiB | 53% | 20.0 / 22.8 ms |
| blur, quality | 19.2 | 55.0 MiB | 60% | 21.6 / 24.0 ms |
| image, balanced | 19.3 | 55.6 MiB | 46% | 17.5 / 19.5 ms |

First frame is 224–248 ms and the compressed package is 11.37 MiB. Eco meets the active CPU and
RAM budgets; balanced and quality trade CPU for quality and are documented. The reference camera
delivers ~19 fps raw, so the 30 fps floor is an exception until Sprint 12 tests a 30 fps camera.
Idle tray RAM is 73.4 MiB against the 15 MiB target; `eframe` owns that cost, and the lazy/native
settings window is scheduled for M5.

Media Foundation reports a bogus 1 fps in the media type, so trust the measured fps, not the
printed format line.

## License and name

TBD. "ZagCam" is a working title.
