# RK-15 result (NCMD-27)

Verdict: **PASS against R-006 v5 / D-037** (owner chose A + C after the 6b miss against v4). The bar is judged on `p95_est` (owner's choice, D-037): 19.3 ms <= 25 ms.
The true on-screen p95 lies **between 19.3 ms (`p95_est`) and 26.3 ms (`p95_upper`, the conservative bound)**, because `p95_est` subtracts the whole
probe cost (7.0 ms, one refresh period) as if all of it were measurement overhead. The conservative bound is 1.3 ms above the 25 ms bar. The v4 bar (p95_upper <= 16 ms) was missed
(6b, p95_est 19.3 ms), which is why the owner changed the budget. Parity part of R-006 v5 (inject_to_t1 no worse than PowerToys Command
Palette, same method): pending, measured by NCMD-6. Slint software renderer stays. Harness verdict logic updated to the v5 bar.

## Conditions (D-036: background CPU is recorded, not a limit)
Machine OMEN, display 144 Hz as reported by GDI (the 143 Hz panel mode, vsync period 6.94 ms), AC power, release build of
naobi-perf and of naobi-app with feature `perf`, dogfood instance and PowerToys Run not running, no cargo/rustc running.
Mean background CPU (all logical CPUs, excluding harness and exe): 47.8% (keystroke), 55.2% (hotkey), from Defender,
Windows Update and indexing. Raw JSON: `target/perf.json` (copy `target/ncmd27/perf-final.json`; before tuning: `target/ncmd27/before-tuning.json`).

## Keystroke to results, 10,000 items, 200 valid runs, 0 discarded
| | wndproc_to_t1 p50 | p95 = p95_upper | max | p95_est | probe cost | probe p95-p50 | inject_to_t1 p50 / p95 |
|---|---|---|---|---|---|---|---|
| before tuning | 19.55 | 26.76 | 33.74 | 19.41 | 7.36 | 6.64 | 22.02 / 29.47 |
| after tuning (final) | 19.82 | 26.34 | 31.23 | 19.34 | 7.00 | 1.58 | 21.80 / 28.76 |

inject_to_wndproc: p50 2.7 ms, p95 4.3 ms. Fuzzy search of 10,000 items takes 0.6 ms (release), so the time is not in matching.
Probe cost is one refresh period (6.94 ms) for any region size: BitBlt and GetPixel from the screen DC wait for the next composed frame.
Tuning pass (row model updated in place, no list rebuild, naobi-ui `apply`) changed p95_upper by -0.4 ms, i.e. nothing; it was reverted.
Probe stability stayed above the 1 ms limit of policy 6a in both runs, and `p95_est` was above 16 ms anyway (the v4 bar).
Against v5: `p95_est` 19.3 ms is a lower-biased estimate (it subtracts a full probe period); the true on-screen p95 lies between 19.3 ms and 26.3 ms.
Known limitation, follow-up for NCMD-6: the capture delay of the screen probe is not bounded independently (DwmGetCompositionTimingInfo was not
added); NCMD-6 compares Naobi and Command Palette `inject_to_t1` measured the same way, where this bias cancels.

## Other R-006 results (perf build)
| Budget | Value | Limit | |
|---|---|---|---|
| hotkey_to_visible p95 (inject_to_t1, 200 runs, 0 discarded; p50 21.0, max 28.0; pixel probe cost 6.96 ms) | 22.2 ms | 50 ms | PASS |
| idle working set (private bytes 7.7 MB) | 25.5 MB | 40 MB | PASS |
| idle CPU, 60 s | 0.0% | 0.5% | PASS |
| process start to tray-ready (p95 of 10) | 66.7 ms | 300 ms | PASS |
| process start to window ready (p95 of 10) | 260.4 ms | 1500 ms | PASS |
| exe size (perf build) | 10.9 MB | 20 MB | PASS |
