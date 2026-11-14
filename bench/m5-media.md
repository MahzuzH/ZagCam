# M5 media-background probe

Reference laptop, `ZagCam 0.5.0-beta.1`, release build, 1280x720. Same probe/bench method as M4.
Latency is decode plus effect per frame; it excludes the camera frame wait.

| Case | fps | RAM MiB | CPU % 1 core | first frame ms | latency p50 / p95 |
| --- | ---: | ---: | ---: | ---: | ---: |
| GIF background, mirror + brightness/contrast | 19.6 | 58.4 | 63.9 | 253 | 22.5 / 24.9 ms |
| Video background, mirror | 19.6 | 94.1–96.6 | 82–106 | 249–258 | 18.3–24.1 / 20.6–29.7 ms |
| Still image (M4 reference) | 19.3 | 55.6 | 46–53 | 225–247 | 17.5–20.0 / 19.5–22.8 ms |

Both media paths follow the ~19 fps camera cap and stay under the 100 ms latency budget. Video loops:
a 15 s probe crossed the 13.3 s source clip without an error or stall.

## Notes

- The GIF path caps the decoded cache at 64 MB and rejects larger files before storing frames.
- Video decoding runs on `zagcam-video`; the pipeline copies the latest prepared frame, so decode does
  not sit in the composite hot path. Decoding is demand-driven by processed frames.
- Video uses more RAM than a still image (frame buffers + decoder) but stays under 100 MB. CPU is
  measured separately, as the M5 plan requires for media backgrounds.
- Live preview CPU: with a real virtual-camera consumer and GIF background, the tray used 76.5% of
  one core with the settings window visible and 71.0% with it closed, so preview adds ~5.5%
  (within the 10% gate).
