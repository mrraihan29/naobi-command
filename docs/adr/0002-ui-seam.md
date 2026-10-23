# The UI framework is Slint (software renderer, accessibility on) and stays isolated in one crate

Status: accepted
Ledger: D-034 (the framework, resolves D-002), D-010 (the isolation), D-032 (accessibility criterion)

Slint versus GPUI was put to a running prototype (NCMD-1, two iterations). The Owner chose Slint; the second iteration fixed the renderer and the memory numbers. Every other crate still depends only on plain data types, so a later change touches one crate.

## Decision

- Framework: Slint, release train, with the **software renderer** and the **accessibility** Cargo feature enabled (D-034). Fonts embedded (Inter, JetBrains Mono).
- Isolation: `naobi-core::ui` holds plain data, `ViewModel` (an immutable `Send` snapshot of everything on screen) and `UiEvent` (`TextChanged`, `Key(KeyChord)`, `RowActivated`, `Dismissed`), plus the design tokens as Rust data. Key meaning (Enter, Ctrl+Enter, Ctrl+K, Esc ladder) is decided in the core. `Launcher::handle(UiEvent)` returns `Effect` values, so tests assert effects and view models without any window.
- Only `naobi-ui` depends on Slint. `naobi-app` calls `naobi-ui` directly: a `run` entry point that owns the main-thread loop and a `Clone + Send` `UiHandle` (`post(ViewModel)`, `show`, `hide`, a safe `raw-window-handle` value). There is no `UiShell` trait: with one adapter it would be an abstraction with a single implementation.
- Overlays that must not take focus or must be click-through (color picker overlay, topmost border), the tray menu and Notices are native Win32 owned by `naobi-win32`. They never touch Slint.
- The window is driven by the event loop (`invoke_from_event_loop`); no repeating timers (the prototype's leftover 5 ms timer was the whole 1.09% idle CPU).

## Measured evidence

From `docs/research/ui-prototype.md` and `.ai-workspace/20261021-2301-naobi-command-discover/coder-slint-iter2.md` (i7-10750H, 143 Hz, 125% DPI, not a fully idle machine), untrimmed:

| Measure | Slint software + a11y | Slint femtovg | Slint skia | GPUI |
|---|---|---|---|---|
| Idle working set | 27.3 MB (24.0 without a11y) | 79.9 MB | 159.7 MB | 77.8 MB |
| Private bytes | 8.5 MB | 103.5 MB | 257.6 MB | 103.5 MB |
| Idle CPU (one core) | 0.00% | 0.10% | 0.08% | 4.85% |
| Threads | 5 | 18 | 16 | 34 |
| Tray-ready from process start | 33 to 39 ms | 35 ms | 39 ms | not built |
| Window created and cloaked | 197 ms (249 ms with a11y) | 1179 ms | 1139 ms | 1061 ms |
| Release exe | 10.7 MB | 9.8 MB | 18.2 MB | 5.3 MB |

Accessibility costs 3.3 MB working set with the software renderer (under the 5 MB limit of D-032), so it is on. The prototype tree exposes an Edit and a List; rows are Text elements with no selected state, so Build adds `accessible-role` and `accessible-label` annotations.

## Why the others were rejected

- GPUI: no UI Automation tree and no AccessKit in its lock; a production text input (caret, selection, paste, IME) would need custom or third-party work; idle CPU 4.85% of one core before cycles, unexplained; 34 threads against 18; its latency metric is vsync-bound (about 23 ms p95 on a 60 Hz panel, extrapolated); a 1.7 GiB build tree and 8 minute clean build; pre-1.0 `gpui` 0.2.2 pinned to `windows` 0.61; the painted card was inset about 8 px inside the window. Its advantages (exe 5.3 MiB, glyph fallback, fewer GPU resources) did not outweigh these.
- femtovg (OpenGL): about 80 MB idle working set, 255 MB private after showing, window creation about 1.15 s.
- skia: 160 MB idle working set, 18 MB exe, no render notifier.

## Consequences

- Open risk (RK-15): keystroke-to-results p95 <= 16 ms is not proven with the software renderer. The prototype's screen probe gave an upper bound of 26.7 ms with about 7 ms probe overhead and vsync of 7 ms; the femtovg notifier measured 10.4 ms. The software renderer has no render notifier, so the production `naobi-perf` must instrument its present path, and the first Build slice measures it before other modules are built. If it fails, the Owner decides at that point; there is no automatic renderer switch.
- The software renderer rasterizes on the CPU: no blur, no large shadows, short fades only (DESIGN_SYSTEM.md). The footer glyph U+21B5 is absent from the Inter subset and renders nothing, so key hints use text ("Enter") or the font subset is fixed.
- Idle working set is measured without trimming (R-006 v3). The prototype showed `EmptyWorkingSet` after hide would reach 2 to 5 MB at a next-show cost below probe noise; it is not used (YAGNI) and remains available as a fallback.
- Cost: no framework-native models in module code, and one view-model clone per update, bounded by the 50-row cap.
