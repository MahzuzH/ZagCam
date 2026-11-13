# ZagCam

Lightweight virtual camera that blurs or replaces your webcam background. Pick "ZagCam" as your camera in Discord, Meet, Teams, or anything else that reads a camera from the OS — no per-app setup.

**Status: M0 done.** Webcam capture prototype with baseline measurement (see [Build and usage](#build-and-usage)). Blur and virtual camera still to come.

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
- **Inference:** `ort` (ONNX Runtime), MediaPipe Selfie Segmentation as the starting model
- **Compositing:** CPU with SIMD (`fast_image_resize`); `wgpu` as an optional GPU path
- **GIF decoding:** `gif` / `image`, frames kept at native GIF resolution
- **Windows API:** `windows` crate, `MFCreateVirtualCamera`
- **UI:** tray app with `egui` or `iced`
- **Build:** `opt-level = "s"`, LTO, `codegen-units = 1`, `panic = "abort"`, `strip = true`

## Roadmap

1. **M0** — Capture prototype: webcam shown in a window via `nokhwa`, baseline CPU/RAM measured. *(done)*
2. **M1** — Background blur: `ort` + MediaPipe, smoothed mask, output to OBS Virtual Camera for validation.
3. **M2** — Own virtual camera (Windows 11): provider + engine over shared memory, detected by Chrome, Discord, Teams.
4. **M3** — UI and MVP features: tray, blur and image modes, saved settings, camera opened on demand.
5. **M4** — Optimization and closed beta: performance tuning, compatibility matrix, stability fixes.
6. **M5** — v1.0 (Windows): video backgrounds, presets and hotkeys, signed installer, update mechanism.
7. **M6** — Linux, then **M7** — macOS.

## Build and usage

M0 is a capture prototype: it opens a webcam, decodes frames, and shows them in a window. No blur or virtual camera yet.

```sh
cargo run --release -- --list          # list cameras
cargo run --release --                 # preview the first camera (Esc quits)
cargo run --release -- --camera 1      # preview camera 1
cargo run --release -- --probe 5       # capture 5 s headless and print fps / RAM / CPU
```

Prefer `--release`: the debug build is several times slower because of MJPEG/NV12 decoding.

Baseline measured on the reference laptop (1280x720 NV12, release build): ~29 fps, ~17 MiB working set, ~25-35% of one CPU core for capture and decode only. Media Foundation reports a bogus 1 fps in the media type, so trust the measured fps, not the printed format line.

## License and name

TBD. "ZagCam" is a working title.
