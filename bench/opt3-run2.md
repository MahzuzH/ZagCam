# ZagCam benchmark

Generated 2026-11-14 11:28:18, 5s probes, release build.

| Case | fps | RAM MiB | CPU % 1 core | first frame ms | interval ms | latency p50 ms | latency p95 ms | preprocess ms | infer ms | mask ms | blur ms | composite ms |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| off/balanced | 19.3 | 20.6 | 17.7 | 225 | 51.79 | 5.8 | 6.7 |  |  |  |  |  |
| blur/eco | 19.3 | 49.2 | 16.8 | 224 | 51.76 | 4.9 | 12.7 | 0.83 | 1.74 | 0.29 | 1.18 | 0.93 |
| blur/balanced | 19.3 | 55.6 | 53 | 247 | 51.94 | 20 | 22.8 | 1.3 | 3.5 | 1.1 | 2.7 | 3.73 |
| blur/quality | 19.2 | 55 | 59.8 | 248 | 51.98 | 21.6 | 24 | 1.37 | 6.57 | 1.14 | 3.11 | 3.68 |
| image/balanced | 19.3 | 55.6 | 45.9 | 242 | 51.92 | 17.5 | 19.5 | 1.23 | 3.43 | 1.07 | 0 | 3.66 |

Latency is decode plus effect for each probe frame; it excludes the camera frame wait.

## Sizes

| Artifact | MiB |
| --- | ---: |
| ZagCam.exe | 34.14 |
| release binaries (exe + DLLs) | 34.31 |
| model | 0.44 |
| compressed package (exe, provider, model, notices) | 11.37 |

## Idle tray

| RAM MiB | CPU % 1 core |
| ---: | ---: |
| 73.4 | 0.5 |
