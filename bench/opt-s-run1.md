# ZagCam benchmark

Generated 2026-11-14 10:54:16, 5s probes, release build.

| Case | fps | RAM MiB | CPU % 1 core | first frame ms | interval ms | latency p50 ms | latency p95 ms | preprocess ms | infer ms | mask ms | blur ms | composite ms |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| off/balanced | 19.3 | 20.6 | 23.3 | 231 | 51.85 | 7.2 | 9.3 |  |  |  |  |  |
| blur/eco | 19.3 | 48.5 | 24.6 | 229 | 51.81 | 6.9 | 17.4 | 1.23 | 2.21 | 0.48 | 1.61 | 1.32 |
| blur/balanced | 19.2 | 54.9 | 85.6 | 262 | 52.13 | 26.9 | 37.8 | 2.31 | 4.27 | 3.11 | 4.32 | 5.84 |
| blur/quality | 19.2 | 55.1 | 89.6 | 259 | 52.11 | 31.8 | 38.9 | 2.38 | 8.32 | 2.98 | 5.33 | 5.66 |
| image/balanced | 19.2 | 55.6 | 69.3 | 258 | 52.08 | 23.1 | 31.6 | 2.26 | 4.38 | 3.05 | 0 | 5.65 |

Latency is decode plus effect for each probe frame; it excludes the camera frame wait.

## Sizes

| Artifact | MiB |
| --- | ---: |
| ZagCam.exe | 33.14 |
| release binaries (exe + DLLs) | 33.3 |
| model | 0.44 |
| compressed package (exe, provider, model, notices) | 10.74 |

## Idle tray

| RAM MiB | CPU % 1 core |
| ---: | ---: |
| 72.8 | 0 |
