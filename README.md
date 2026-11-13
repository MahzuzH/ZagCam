# ZagCam

Lightweight virtual camera that blurs or replaces your webcam background. Pick "ZagCam" as your camera in Discord, Meet, Teams, or anything else that reads a camera from the OS — no per-app setup.

**Status: M2 code done (manual gates pending); M3 sprints 7–9 code done (manual gates pending).** Capture, background blur, and an own Media Foundation virtual camera are implemented. The engine now publishes blurred frames into that camera (see [Build and usage](#build-and-usage)); compatibility and soak gates remain.

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

**v1.0**

- GIF and looping video backgrounds.
- Presets and global hotkeys.
- Live preview in settings.
- Quality modes (Eco / Balanced / Quality) controlling inference rate and resolution.
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

Design estimates, to be validated by measurement in the prototype.

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
- **Build:** `opt-level = "s"`, LTO, `codegen-units = 1`, `strip = true`

## Roadmap

1. **M0** — Capture prototype: webcam shown in a window via `nokhwa`, baseline CPU/RAM measured. *(done)*
2. **M1** — Background blur: `ort` + MediaPipe, smoothed mask, output to OBS Virtual Camera for validation. *(done)*
3. **M2** — Own virtual camera (Windows 11): provider + engine over shared memory, detected by Chrome, Discord, Teams. *(in progress: engine integration done; compatibility and soak gates remain)*
4. **M3** — UI and MVP features: tray, blur and image modes, saved settings, camera opened on demand. *(sprints 7–9 code done, manual gates pending)*
5. **M4** — Optimization and closed beta: performance tuning, compatibility matrix, stability fixes.
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

### Own virtual camera (M2/M3, code done, manual gates pending)

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

| Mode | fps | Working set | CPU (1 core = 100%) |
| --- | --- | --- | --- |
| off (capture only) | ~29 | ~19 MiB | ~33% |
| blur | ~29 | ~55 MiB | ~186% (~1.9 cores) |

Blur per frame: preprocess ~1.7 ms, inference ~4 ms (every 2nd frame), mask ~1.9 ms, blur ~3.5 ms,
composite ~1.9 ms. Resizing and compositing use `rayon`, so the blur path uses about two cores to
keep 30 fps. The CPU budget in the PRD (~8% of a 4-core laptop) is **not** met yet; capture/decode
and the CPU-only model are the main costs, and reducing them is M4 work.

Media Foundation reports a bogus 1 fps in the media type, so trust the measured fps, not the
printed format line.

## License and name

TBD. "ZagCam" is a working title.
