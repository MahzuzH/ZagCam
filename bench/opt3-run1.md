# ZagCam benchmark

Generated 2026-11-14 11:27:31, 5s probes, release build.

| Case | fps | RAM MiB | CPU % 1 core | first frame ms | interval ms | latency p50 ms | latency p95 ms | preprocess ms | infer ms | mask ms | blur ms | composite ms |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| off/balanced | 19.3 | 20.7 | 17.1 | 235 | 51.79 | 5.8 | 7.4 |  |  |  |  |  |
| blur/eco | 19.3 | 49.2 | 16.8 | 229 | 51.82 | 4.6 | 12.8 | 0.83 | 1.82 | 0.28 | 1.13 | 0.89 |
| blur/balanced | 19.3 | 54.8 | 56.7 | 245 | 51.95 | 20.5 | 23.7 | 1.31 | 3.38 | 1.12 | 2.7 | 3.73 |
| blur/quality | 19.3 | 55.5 | 62.1 | 243 | 51.91 | 21.8 | 24.2 | 1.32 | 6.66 | 1.17 | 3.21 | 3.67 |
| image/balanced | 19.3 | 55.1 | 45.9 | 242 | 51.93 | 17.6 | 19.5 | 1.42 | 3.38 | 1.13 | 0 | 3.71 |

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
| 73.4 | 0 |
