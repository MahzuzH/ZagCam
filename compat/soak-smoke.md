# ZagCam soak 20261114-121615

> 1-minute end-to-end smoke (`--camera <virtual> --probe` as a real consumer), not the Sprint 12
> 4-hour gate. Use `tools/soak.ps1 -Minutes 240` with the chosen meeting app for the real run.

- Version: ZagCam 0.4.0-beta.1
- Process: 4980 D:\Productive\Project\ZagCam\target\release\ZagCam.exe
- Duration: 1 min, 12 samples
- Consumer: active for the whole run: True
- Process exited during run: False
- Result: PASS

| Metric | First | Last | Peak / delta |
| --- | ---: | ---: | ---: |
| Working set MiB | 23.4 | 23.7 | peak 25, growth 0.3 |
| CPU seconds during run |  |  | 16.9 |
| Provider log | C:\ProgramData\ZagCam\vcam-LOCAL_SERVICE.log |  | new error/panic/fail lines: 0 |

Limit: growth must be <= 25 MiB and the process must not exit.
Samples: D:\Productive\Project\ZagCam\soak\soak-20261114-121615.csv

## New provider log findings

- none
