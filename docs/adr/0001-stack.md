# Stack: Rust with a native UI

Status: accepted
Ledger: D-001 (also D-003 toolchain, D-005 code layout)

The whole product is Rust: one process, no webview, no managed runtime. The rule in `docs/STANDARD.md` §6.1 admits Rust for a measured need in memory and performance, and this project has one.

## Measurable reasons

| Evidence | Number | Source |
|---|---|---|
| Budget to meet | idle working set <= 40 MB total, cold start <= 300 ms, hotkey-to-visible p95 <= 50 ms, exe <= 20 MB | R-006 |
| Raycast (WPF host + WebView2 + Node), idle, installed on the owner's PC | about 991 MB working set summed over 8 processes (686 MB private) | `docs/research/raycast-rea.md` §9 |
| PowerToys Command Palette (WinUI 3, .NET), resident | 137 MB working set, 180 MB private | `docs/research/powertoys.md` §8.2 |
| Raycast's own Rust UI-access helper (hooks, clipboard, window ops) | 16 MB working set | `docs/research/raycast-rea.md` §9 |
| PowerToys Awake, C# vs Rust (maintainer harness) | 14.3 MB vs 2.0 MB private, 130 ms vs 16.5 ms start | `docs/research/powertoys.md` §6.3 |
| Tauri/WebView2 | +60 to 100 MB working set | D-001 |

A single Rust process with native rendering is the only option in the table that can credibly sit under 40 MB. Rust also gives direct, safe-by-default access to the Win32 surface the product lives on (`RegisterHotKey`, DWM, `SetWindowPos`, clipboard listener, DPAPI) through the `windows` crate.

## Decisions

- Language: Rust stable, host `x86_64-pc-windows-msvc` only (R-003, D-003). No other language in the repo except Slint markup (D-034).
- UI: Slint with the software renderer and accessibility on (D-034, ADR-0002); measured 27.3 MB idle working set. Every other crate is written so the choice does not leak (ADR-0002).
- `unsafe`: not forbidden workspace-wide. It is allowed in exactly one crate, `naobi-win32` (ADR-0003).
- Workspace: one crate per module plus `naobi-core`, `naobi-win32`, `naobi-ui`, `naobi-app` and `naobi-perf` (D-005).
- Dependencies: free and open source only (D-008). Planned, to be confirmed by `cargo deny` at scaffold: `windows`, `rusqlite` (bundled SQLite), `png`, `xxhash-rust`, `nucleo-matcher`, `serde` + `toml`, `thiserror`, `anyhow`, `tracing`. No async runtime: std threads and channels are enough for six background workers.

## Rejected

- Tauri / Electron / any webview: breaks the 40 MB budget (D-001).
- .NET (WPF, WinUI 3): the measured Command Palette baseline is 137 MB working set before any feature work.
- Rust UI frameworks other than Slint and GPUI: not evaluated; D-002 names the two candidates.
