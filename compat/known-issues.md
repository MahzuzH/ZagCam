# Known issues and accepted limitations

Updated for the `0.5.0-beta.1` beta package. This list is shared by Sprint 12 and the beta.

| ID | Severity | Status | Summary |
| --- | --- | --- | --- |
| ZC-01 | P2 | accepted | Reference camera delivers ~19 fps raw; the 24 fps floor needs a 30 fps camera. |
| ZC-02 | P2 | accepted, M6 | Idle tray RAM is ~73 MiB versus the 15 MiB target; `eframe` owns the cost. Sprint 16 reopened the exception with a 73.3 MiB measurement. |
| ZC-03 | P3 | expected | Unsigned build triggers SmartScreen/antivirus warnings; signing is M5 work. |
| ZC-04 | P3 | expected | Some apps cache the camera list; reselect ZagCam after installing or starting it later. |
| ZC-05 | P3 | expected | Settings live in `%APPDATA%\ZagCam` and intentionally survive uninstall. |
| ZC-06 | P2 | open; confirm with real apps | A second simultaneous Source Reader fails with `0xC00D3704`; provider logs both starts. Repro: `compat/multi-consumer-smoke.md`. |

P0/P1 means a crash or hang in a call, ZagCam never appearing in the camera list, or frames leaking
to another user. Keep this table current when the matrix or beta finds a new issue.

