# ZagCam

Lightweight Windows virtual camera that blurs or replaces your webcam background. Pick "ZagCam"
as your camera in Discord, Meet, Teams, Zoom, or anything else that reads a camera from the OS —
no per-app setup.

> **v1.0.0 — Windows 11 only.** The installer is unsigned, so SmartScreen shows an Unknown
> publisher warning on first run. Linux, macOS, and Windows 10 are not planned.

## Why

- Small footprint: low RAM, CPU, and installer size compared to NVIDIA Broadcast, XSplit VCam,
  Camo, or OBS.
- Written in Rust.
- Uses no camera or CPU when no app is using the virtual camera.
- One background setting for every video app.
- Fully local: no video frames leave the machine.

## Features

- Capture from a physical webcam; choose the source camera.
- Modes: off (pass-through), background blur (adjustable intensity), image, GIF, and looping video
  backgrounds. GIFs are capped at 64 MB.
- Quality modes (Eco / Balanced / Quality) controlling inference rate and resolution.
- Named presets, tray switching, and global hotkeys (Ctrl+Alt+B toggle, Ctrl+Alt+N next preset,
  Ctrl+Alt+O off).
- Live preview, mirror/flip, brightness and contrast in the settings window.
- Optional start with Windows.
- Own Media Foundation virtual camera, visible to Chrome, Edge, Firefox, Discord, Teams, Zoom,
  and OBS.
- Physical camera opened only while the virtual camera is in use, and released right after.
- Per-machine MSI installer with the model and provider bundled.
- Signed update manifest, version check, verified download, and install only after approval.

## Requirements

- Windows 11, build 22000 or later (developed on 26200).
- Administrator rights once, to register the virtual-camera provider.
- No OBS install needed. The OBS virtual camera is an optional development fallback only.

## Install

1. Download `ZagCam-1.0.0.msi` from the [GitHub Releases page](https://github.com/MahzuzH/ZagCam/releases).
2. Run it and approve the Windows admin prompt.
3. If SmartScreen appears, choose **More info → Run anyway**. The installer is unsigned.
4. Launch **ZagCam** from the Start menu. The settings window and tray icon appear.
5. In Settings, choose the **physical webcam** under Camera — not `ZagCam Camera`. Choose Blur or
   Background, pick a background file if needed, then click **Save**.
6. In Meet, Teams, Discord, Zoom, or OBS, select **ZagCam Camera**.

The MSI installs to `%ProgramFiles%\ZagCam`, registers the virtual-camera provider, and bundles the
segmentation model. Settings live in `%APPDATA%\ZagCam` and intentionally survive uninstall.

## Updates

Use the tray menu **Check for updates...**. ZagCam verifies the signed update manifest and the
installer SHA-256 before asking to install. Updates are never installed automatically. The MSI is
unsigned, so Windows may show **Unknown publisher** and SmartScreen may require **More info → Run
anyway**.

## How it works

```
physical webcam → downscale → segmentation model → upscale + smooth mask
                → composite (blur / image / video) → virtual camera
```

Two processes:

- **Virtual camera provider** — a tiny component loaded by the OS. It only receives frames and
  serves them as a camera.
- **Engine** — captures, runs inference, and renders. It sends frames to the provider over shared
  memory.

The split keeps OS-side code minimal and lets the engine restart without the virtual camera
disappearing. Performance strategy: infer at low resolution, run the model every other frame and
interpolate, smooth the mask across frames (EMA), downscale-blur-upscale for blur, allocate buffers
once and reuse, and open the camera/model only when there is a consumer.

## Performance

Measured on the reference laptop with `tools/bench.ps1` (720p, release build). Full numbers,
exceptions, and reproducibility evidence are in [`bench/README.md`](bench/README.md).

| Metric | Target | Measured |
| --- | --- | --- |
| Installer/package | ≤ 30 MB | 11.37 MiB compressed |
| Idle RAM (tray, camera off) | ≤ 15 MB | 73.4 MiB — **exception** |
| Active RAM (private working set) | ≤ 100 MB | 49.2–96.6 MiB |
| Active CPU, eco | ≤ 32% of one core | 16.8–26.1% |
| Capture-to-output latency | ≤ 100 ms | p95 ≤ 50.3 ms |
| Frame rate | 30 fps, never below 24 | 19.3 fps — **exception** |
| Time to first frame | ≤ 1.5 s after an app opens the camera | 224–258 ms |
| Idle CPU | ~0% with no camera consumer | 0–0.5% |

Baselines (reference laptop, 1280x720 NV12, release):

| Mode | fps | Working set | CPU (1 core = 100%) | Latency p50 / p95 |
| --- | --- | --- | --- | --- |
| off | 19.3 | 20.6 MiB | 18% | 5.8 / 6.7 ms |
| blur, eco | 19.3 | 49.2 MiB | 17% | 4.9 / 12.7 ms |
| blur, balanced | 19.3 | 55.6 MiB | 53% | 20.0 / 22.8 ms |
| blur, quality | 19.2 | 55.0 MiB | 60% | 21.6 / 24.0 ms |
| image, balanced | 19.3 | 55.6 MiB | 46% | 17.5 / 19.5 ms |

Documented exceptions:

- **Frame rate.** The reference camera delivers ~19 fps raw; off mode is also 19.3 fps, so the
  pipeline is not the cap. A 30 fps source is needed to validate the floor.
- **Idle RAM.** `eframe`/`glow` keeps ~73 MiB even with the settings window closed. A lighter tray
  shell is deferred.
- **Signing.** The MSI and exe are unsigned; SmartScreen warnings are expected. Update manifests
  are signed and verified.

Media Foundation reports a bogus 1 fps in the media type, so trust the measured fps, not the
printed format line.

## Works with

Google Meet (Chrome, Edge, Firefox), Microsoft Teams (desktop and web), Discord (desktop and web),
plus Zoom and OBS Studio. Tested edge cases: several apps using the camera at once, an app opened
before ZagCam, and the webcam unplugged mid-meeting. Known issues and accepted limitations live in
[`compat/known-issues.md`](compat/known-issues.md).

## Privacy

All processing is local. No account or login is required. Telemetry is opt-in and never contains
video. A clear indicator shows when the webcam is active, and OS-side code is kept minimal to
reduce attack surface.

## Tech stack

- **Language:** Rust
- **Capture:** `nokhwa`
- **Inference:** `ort` (ONNX Runtime), MediaPipe Selfie Segmentation (landscape, 256x144) as the
  starting model
- **Compositing:** CPU with SIMD (`fast_image_resize`) and `rayon`
- **Media backgrounds:** `image` for stills/GIF; Media Foundation `IMFSourceReader` for looping
  video
- **Windows API:** `windows`/`windows-sys`, `MFCreateVirtualCamera`, `RegisterHotKey`, HKCU Run key
- **UI:** tray app with `eframe`/`egui`, tray menu with `tray-icon`
- **Build:** `opt-level = 3`, LTO, `codegen-units = 1`, `strip = true`

## Scope

Windows 11 only, via the Media Foundation Virtual Camera API. Not planned: Linux, macOS,
Windows 10, and GPU/NPU acceleration. The project is feature-complete for the Windows 11 v1.0
release; remaining work is quality and polish, tracked in [`compat/known-issues.md`](compat/known-issues.md).

## Troubleshooting

- Camera missing from an app: restart the app or reselect the camera. Some apps cache the camera
  list.
- No video: check `ZagCam.exe vcam status` and `ZagCam.exe vcam watch --seconds 3`.
- Background not visible: make sure the app uses **ZagCam Camera**, not the physical webcam.
- Logs: `%ProgramData%\ZagCam\vcam-<account>.log`.
- Send a report with `collect-logs.ps1`; see [`beta/FEEDBACK.md`](beta/FEEDBACK.md). No video
  frames are collected.

## Development

Build requirements: Rust stable, the WiX v7 CLI (`winget install WiXToolset.WiXCLI`), and the
Visual Studio C++ build tools.

```sh
cargo build --release
cargo build --release -p zagcam-vcam
cargo run --release -- --list                      # list cameras
cargo run --release --                             # tray app (normal use)
cargo run --release -- --sink window               # preview the first camera (Esc quits)
cargo run --release -- --sink window --mode blur   # preview with background blur
cargo run --release -- --probe 5 --mode blur       # headless: fps / RAM / CPU and per-stage ms
```

Prefer `--release`: debug builds are an order of magnitude slower. When launching
`target\release\ZagCam.exe` directly, run it from the repository root or copy `models` next to the
exe; the MSI packages the model correctly.

Checks before a release:

```sh
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

### Virtual camera (development)

ZagCam has its own Media Foundation virtual camera, so no OBS is needed:

```sh
cargo build -p zagcam-vcam --release
cargo run --release -- vcam install       # UAC once; copies to %ProgramFiles%\ZagCam
cargo run --release --                    # tray app creates the camera and feeds it
cargo run --release -- vcam status        # registered DLL + whether a source is live
cargo run --release -- vcam watch         # show the shared frame channel state
cargo run --release -- vcam uninstall     # UAC once; removes the CLSID and installed copy
```

The tray app owns the camera for its lifetime, so `vcam start` is not needed. The camera is
session-scoped and disappears on quit. Without an engine, the provider shows its test pattern.
`--sink mfvcam` publishes engine frames into the camera; `vcam feed` publishes test frames for
provider-only checks. Provider diagnostics go to `%ProgramData%\ZagCam\vcam-<account>.log`.

### Packaging and updates

```sh
powershell -ExecutionPolicy Bypass -File tools\build-installer.ps1
powershell -ExecutionPolicy Bypass -File tools\release.ps1
```

`build-installer.ps1` produces `dist\ZagCam-<version>.msi` (per-machine, unsigned). The MSI bundles
the exe, provider DLL, model, notices, quickstart, and known issues. `release.ps1` produces the
GitHub release assets in `dist\release\v<version>\`: `ZagCam-<version>.msi`, `latest.json`,
`latest.json.sig`, and `SHA256SUMS`. Publish as a normal release (not a pre-release) so the
`/releases/latest/download/` feed resolves.

The manifest is signed with the Ed25519 key in `.secrets\zagcam-update.key`; the app rejects a
missing or invalid signature. Generate a key once with:

```sh
cargo run -p zagcam-sign-manifest --release -- keygen .secrets\zagcam-update.key
```

Back up the private key and keep it out of git. The public key lives in `src/update.rs`. Update
checks use the latest GitHub release:

```sh
.\target\release\ZagCam.exe update check
.\target\release\ZagCam.exe update check --feed .\dist\release\v1.0.0\latest.json
```

Verification helpers: `tools\verify-installer.ps1` and `tools\verify-update.ps1`.

### Benchmarks and soak

```sh
powershell -ExecutionPolicy Bypass -File tools\bench.ps1
powershell -ExecutionPolicy Bypass -File tools\soak.ps1 -Minutes 240
```

Results and budget exceptions live in [`bench/`](bench/).

### Model

`models/selfie_segmentation_landscape.onnx` (Apache-2.0) is committed, so no download step is
needed. It expects a 256x144 RGB input scaled to 0..1 and outputs a person mask; see
[models/LICENSE](models/LICENSE).

### OBS virtual camera (fallback)

`--sink vcam` needs a virtual-camera filter registered with Windows. The `virtualcam` crate
(Apache-2.0, `virtualcam 0.1.1`) uses the **OBS Virtual Camera** filter, so install OBS Studio
once (OBS itself does not need to run). Without it the app prints a clear error, and the build
feature `sink-vcam` is off by default.

## License

MIT. See [LICENSE](LICENSE). Third-party notices are in
[THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
