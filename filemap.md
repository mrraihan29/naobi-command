# File map (planned, v0.2, Gate 2 applied)

One row per planned crate or top-level file, with the sub-item that creates it. Written in Plan; coders update their rows when they commit. Tickets are Plane NCMD keys (epic NCMD-3).

## Repository root

| File | Purpose | Item |
|---|---|---|
| `Cargo.toml` | Virtual workspace, `members = ["crates/*"]`, workspace lints, pinned dependency versions, release profile | NCMD-4 |
| `rust-toolchain.toml`, `clippy.toml`, `deny.toml` | Toolchain floor, unwrap allowance in tests, cargo-deny policy (target x86_64-pc-windows-msvc only, advisories, licenses with GPL-3.0-only only for the Slint crates, bans) | NCMD-4 |
| `.cargo/config.toml` | `+crt-static`, `/DEPENDENTLOADFLAG:0x800` | NCMD-4 |
| `CLAUDE.md`, `CONSTRAINTS.md`, `GLOSSARY.md` | Project rules, quality bar, domain words | design |
| `THIRD_PARTY.md` | Fonts (OFL) and crate licences | NCMD-25 |
| `assets/tray/` | Tray icon states (Awake off, indefinite, timed; paused and notice overlays; light and dark taskbar) | NCMD-12 |
| `docs/ledger.yaml` | Requirements and decisions (D-035 records Gate 2; R-024 and D-038 record the `>` run command, NCMD-30 to 36), `links.tickets` per R- | design, Plan |
| `docs/design/`, `docs/adr/`, `docs/diagrams/` | Design report, design system, mockups, ADRs, diagrams | design; NCMD-7 edits text |
| `docs/research/` | Clean-room notes; `rk15-result.md` (NCMD-27), `baseline-powertoys.json` (NCMD-6) | NCMD-27, 6 |
| `docs/verify/` | `gate3-manual-checklist.md` (NCMD-25), `usage-log.md` (NCMD-26) | NCMD-25, 26 |

## Crates (`crates/`)

| Crate | Purpose | Unsafe | Item |
|---|---|---|---|
| `naobi-core` | Kernel: ids, `Item`, `Action`, `Effect`, `Notice`, `Cipher` (`cipher.rs`, moved from naobi-clipboard, NCMD-31), schema V2 `run_history`, `AppEvent` (with `Timer`), `CommandSource`, `ViewSource`, `Settings`, quicklink URL builder, chord parser, paths, `Db` and migrations, UI data types and tokens, `ThemeSource` | no | NCMD-4 (anchor), 5 (`ui.rs`), 8, 9, 11 (`tokens.rs`), 13 (`theme.rs`) |
| `naobi-win32` | Run adapter for `>` command lines (`run.rs`: program lookup through PATH and App Paths, PE subsystem read, console and direct start; tests in `tests/run.rs`; NCMD-32). Every Win32, COM and WinRT adapter: process metrics, platform thread (`platform.rs`: NCMD-5 creates, NCMD-12 sole later writer; hidden top-level tool window so broadcasts arrive; forwards all P1 messages, `post_to_p1`), DWM, tray, token, key injection and probe (feature `perf`), hotkeys, instance, DPAPI, clipboard, window ops, topmost, power, picker (own thread), search, apps, shell exec, paste, notices, dir watcher | yes (EX-001) | NCMD-4 and every later item adds one file per adapter |
| `naobi-perf` | Perf harness binary: percentile, verdicts, idle, exe-size (NCMD-4); hotkey-to-visible, keystroke-to-results, cold-start (NCMD-27); baseline-powertoys (NCMD-6); network-silence, all (NCMD-25) | no | NCMD-4, 27, 6, 25 |
| `naobi-ui` | Slint adapter (software renderer, accessibility on); the only crate that names Slint; `UiHandle::window()` returns the HWND as `isize` | no | NCMD-5, 11 |
| `naobi-app` | Composition root: platform routing, controller, effects, modules registry (NCMD-15, blocks all module wiring), notices, settings runtime, index worker, logging, perf fixture and readiness lines (feature `perf`); tier 2 flow tests | no | NCMD-5, 27, 12, 13, 15 and module wiring files |
| `naobi-calc` | Expression parser, percent semantics, unit conversion | no | NCMD-10 |
| `naobi-launcher` | Root search engine, ranking, frecency, view stack, Esc ladder, action panel, armed commands, providers (apps, files, system, quicklinks, calculator, run: `providers/run.rs`), run history (`run_history.rs`, DPAPI, 100 lines and 30 days; R-024) | no | NCMD-14, 16, 17, 18, 33 |
| `naobi-clipboard` | History rules, filters, retention, store, text index, paste and restore, view, commands | no | NCMD-19, 20 |
| `naobi-winmgr` | Snap geometry, restore rectangles, commands | no | NCMD-21 |
| `naobi-aot` | Topmost set, border state and geometry, commands | no | NCMD-22 |
| `naobi-awake` | Awake mode state machine, grammar, commands | no | NCMD-23 |
| `naobi-picker` | Color math, color history, view, commands | no | NCMD-24 |

## Registry files (one appended line per item)

`crates/naobi-core/src/lib.rs`, `crates/naobi-win32/src/lib.rs`, `crates/naobi-launcher/src/providers/mod.rs`, `crates/naobi-app/src/modules/mod.rs`, `crates/naobi-perf/src/main.rs`, `Cargo.lock`, and the dependency and `windows` feature lines of each crate `Cargo.toml`.

## Tests

Tier 1 in each crate's `tests/` and unit modules (the `>` run flow: `crates/naobi-launcher/tests/run_command.rs`, NCMD-33). Tier 2 (`#[ignore = "tier2: interactive desktop"]`): `crates/naobi-app/tests/flow_*.rs` (including `flow_run.rs`, NCMD-34) and `crates/naobi-win32/tests/*.rs`. Tier 3 is `naobi-perf`.
