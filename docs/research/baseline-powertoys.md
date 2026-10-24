# PowerToys Command Palette baseline (NCMD-6)

Raw data: `docs/research/baseline-powertoys.json` (captured by `naobi-perf baseline-powertoys --capture`, release build, 200 valid runs per latency metric, 0 discarded).

## Conditions
- Machine: OMEN (COMPUTERNAME), display 144 Hz as reported by GDI at capture (pinned 143 Hz range accepted), AC power, release build.
- Hand-written `conditions` block in the JSON (round 2, read-only, PowerToys untouched): Intel i7-10750H, 1920x1080, DPI 120 (125%), PowerToys 0.102.2933.0, Command Palette 0.13.12934.0, AC power, capture date 2026-10-24 UTC.
- Subject: PowerToys 0.102.2933.0 (sparse app), Command Palette 0.13.12934.0 (`Microsoft.CmdPal.UI`), with `PowerToys.exe` and its modules running.
- Chord: Left Ctrl+Space. Window rectangle: `460,240,1000,600` (the palette's rectangle when it opened).
- Probe: the RK-15 screen probe and contrasting backdrop (#FF00FF), with two differences from Naobi's runs. (a) inject_to_t1 hashes a 900x420 px region (x 5-95%, y 20-90% of the window), against Naobi's 400x120 logical px; the list geometry of Command Palette was not verified against a screenshot. (b) hotkey_to_visible ends at the first sample that is not the backdrop colour (it may be a fade or shadow frame), while Naobi's ends when the pixel equals the launcher background (fully shown). inject_to_t1 is measured after each key (letter, then Backspace).
- Background load: all logical CPUs busy 21.0% during the idle window and 47.5% during the latency runs (recorded, not a limit, D-036).
- Owner-stated: the Naobi dogfood instance and PowerToys Run were not running. Only Command Palette was measured.

## Latency (ms)
| Metric | p50 | p95 | Notes |
|---|---|---|---|
| hotkey_to_visible | 35.40 | 37.49 | chord SendInput to the first backdrop change |
| inject_to_t1 | 184.17 | 232.54 (p95_upper) | QPC before SendInput of the key to the probe t1 |
| inject_to_t1 p95_est | | 220.62 | p95_upper minus probe cost |
| inject_to_t1 max | | 442.41 | |
| probe cost (p50 of region hash) | 11.92 | | 1.7 refresh periods (6.94 ms); Naobi's probe cost is 7.0 ms because its region is smaller |

p95_est subtracts the whole probe cost, as in `rk15-result.md`; the true p95 lies between p95_est and p95_upper.

## Idle and install (PowerToys stack, summed over 9 processes)
| Metric | Value | Notes |
|---|---|---|
| idle working set | 884.7 MB | end of a 60 s idle window, palette hidden |
| idle private bytes | 674.1 MB | same processes |
| idle CPU | 1.07 % of one core | same processes, 60 s |
| install size | 1 240.6 MB | sum of `%LOCALAPPDATA%\PowerToys` (1 062.1 MB), SparseApp (0.02 MB), CommandPalette package (178.5 MB) |
| start to tray-ready | n/a | PowerToys is started by the owner, not by the harness |

## Naobi against Command Palette, inject_to_t1
Naobi (RK-15 after tuning, `docs/research/rk15-result.md`): p50 21.80 ms, p95 28.76 ms (p95_est 19.34, p95_upper 26.34).
Command Palette (this capture): p50 184.17 ms, p95 232.54 ms (p95_est 220.62, p95_upper 232.54).

On this probe, on this machine, Naobi's inject_to_t1 p95 (28.76 ms) is lower than Command Palette's (232.54 ms), so the R-006 v5 target 'no worse than Command Palette' is met on the measured metric. This compares the p95 as measured on each side, including probe cost. The two captures were taken in different sessions (background CPU 47.8% for Naobi, 47.5% for Command Palette).

## Caveats
- Command Palette's inject_to_t1 is the time to the first change anywhere in the hashed region after a key. It is not shown to be the time to the correct result list; the 184 ms p50 is not interpreted as true results latency (search debounce, animation and region choice are all untested).
- Region size biases the two sides in opposite directions: the larger region can fire earlier on an unrelated change (Command Palette number too low) and costs about 5 ms more per poll (too high). Neither is close to the 200 ms gap.
- hotkey_to_visible is not comparable with Naobi's 22.2 ms: the end rule differs (first non-backdrop sample vs launcher background fully shown), so the Command Palette figure is a lower bound for 'fully shown'. No parity claim is made on this metric.
- No per-run samples are stored, so the distribution cannot be re-examined after PowerToys is uninstalled.
- PowerToys Run was not running (no PowerLauncher process) and was not measured.
- Start-to-tray-ready is not applicable (see above). Window-ready and exe size are not applicable to an installed PowerToys and were not captured.
- The `naobi-perf all` reader is NCMD-25, not built here.
- The JSON `probe_cost` note still reads "one refresh period"; this file is the correction (1.7 refresh periods).
