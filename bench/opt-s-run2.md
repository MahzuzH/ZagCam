# ZagCam benchmark

Generated 2026-11-14 10:55:11, 5s probes, release build.

| Case | fps | RAM MiB | CPU % 1 core | first frame ms | interval ms | latency p50 ms | latency p95 ms | preprocess ms | infer ms | mask ms | blur ms | composite ms |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| off/balanced | 19.3 | 20.6 | 23.9 | 228 | 51.84 | 7.5 | 9.5 |  |  |  |  |  |
| blur/eco | 19.3 | 48.7 | 26.1 | 235 | 51.8 | 7 | 17.5 | 1.22 | 2.24 | 0.47 | 1.66 | 1.4 |
| blur/balanced | 19.2 | 55.3 | 81.9 | 259 | 52.09 | 27.1 | 37.3 | 2.35 | 4.25 | 3.03 | 4.09 | 5.95 |
| blur/quality | 19.2 | 55.5 | 94.3 | 260 | 52.15 | 32.4 | 39.7 | 2.48 | 8.9 | 3.07 | 5.4 | 6 |
| image/balanced | 19.2 | 55.1 | 68 | 252 | 52.1 | 22.6 | 30.8 | 2.24 | 4.29 | 3.02 | 0 | 5.91 |

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
