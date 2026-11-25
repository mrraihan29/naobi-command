# Constraints

Last reviewed: 2026-11-25 (Post-Phase 2 Commander Upgrades, Spec #16) · Stack: Rust stable, `x86_64-pc-windows-msvc`, native UI Slint 1.19.1 (edition 1.19), software renderer, accessibility on (D-034) (docs/adr/0001-stack.md)
Read before writing code. Never weaken this file to make a change pass.

## Floor (always enforced)

- No new suppressions (`@ts-ignore`, `eslint-disable`, `# noqa`, `# type: ignore`, `#[allow(...)]`) without a reason in the commit
- No unimplemented stubs or empty catch blocks
- No skipped or deleted tests without a reason
- No secrets in source
- This file is never weakened to make a change pass
- Rust: `#![forbid(unsafe_code)]` in every crate unless an ADR allows it (one exception, EX-001 below)

## Enforced with numbers

| Dimension | Rule | Checked by | Runs at |
|---|---|---|---|
| Format + lint (Rust) | Zero warnings | `cargo fmt --all --check` and `cargo clippy --workspace --all-targets -- -D warnings` | task end (local) |
| Types (Rust) | Compiles | `cargo check --workspace --all-targets` | every edit |
| Tier 1 tests (D-025) | All pass; no Win32, no desktop; tier 2 tests are `#[ignore]`d so they do not run here | `cargo test --workspace` (nextest optional after Gate 1) | task end (local) |
| Tier 2 tests (D-025) | All pass on an interactive desktop with the dogfood instance quit (they inject keys, write the clipboard and move windows) | `cargo test --workspace --no-fail-fast -- --ignored --test-threads=1` | before Gate 3 and on demand; not part of the per-task gate |
| No unwrap/expect outside tests | Zero (D-005) | workspace lints `clippy::unwrap_used` and `clippy::expect_used` set to deny, run by the clippy row above | task end (local) |
| Unsafe | Only in `naobi-win32`; every block has `// SAFETY:` (D-011) | `forbid(unsafe_code)` in all other crates, `clippy::undocumented_unsafe_blocks` denied, run by the clippy row above | every edit |
| Performance (R-006) | Every budget in "Project numbers" reports PASS | `cargo run --release -p naobi-perf -- all --out target/perf.json` (reads `docs/research/baseline-powertoys.json`, no PowerToys needed) | before Gate 3, after any hot-path change |
| Network silence (R-010) | No socket opened in a 10-minute session without an explicit URL action (hardened `WinHttp` FX refresh in `naobi-win32` allowed at most once per 24h or on explicit refresh per ADR-0008) | `cargo run --release -p naobi-perf -- network-silence --minutes 10` | Gate 3 |
| Accessibility | All Naobi token text pairs (every built-in theme: `paper`, `stone`, `tokyo-haze`, `neon-night`, `washi`, `sakura-dawn`, `matcha`, `dune`, `midnight-ink`, `kyoto-dusk`, `deep-sea`) meet WCAG 2.2 AA; a Custom theme below AA applies with a Notice naming the pair (R-033); manual checklist in DESIGN_SYSTEM.md passes | test `r007_token_pairs_meet_wcag_aa` in `cargo test --workspace`; manual keyboard-only pass and Accessibility Insights for Windows | task end (test); Gate 3 (manual) |
| Dependencies (Rust) | No advisories, licenses allowed, banned crates absent | `cargo deny check` (cargo-deny) | task end (local) |
| Secrets | None in source or history | `gitleaks detect --redact` | every edit, before commit (local) |
| Static analysis | No high findings | `semgrep scan --config p/default --error` | before commit (local) |
| Ledger | Gate passes; from Build every active R- has a ticket and a test | `uv run C:/Coding/naobi-sdlc/scripts/ledger_check.py docs/ledger.yaml --gate design` (add `--require tickets,tests` from Build) | before commit (local) |

There is no hosted CI before Ship (D-025): every gate above is a local command, and a CI mirror of these commands is deferred to `naobi-ship`.

E2E (Playwright) and axe do not apply: the app is native (D-006). Their place is taken by the tier 2 integration tests (run before Gate 3) and the manual accessibility checklist.

## Conventions checked in review (not by a tool)

- Libraries use `thiserror`; the binary uses `anyhow` (D-005).
- Use the words in `GLOSSARY.md` in identifiers, logs, UI copy and docs.
- No clipboard content, query text or file names in logs (D-020).
- Dependency direction: module crates depend on `naobi-core` only (plus `naobi-launcher` on `naobi-calc`), never on `naobi-ui`, `naobi-win32` or each other; `naobi-win32` depends on module crates only to implement their ports; `naobi-app` is the only crate that sees everything. `serde_json` is scoped to local parsing/metrics in `naobi-launcher` (Windows Terminal `settings.json`), `naobi-app` (`perf_report`), and `naobi-perf` only (zero network sockets).

## Project numbers

| Budget | Value | Ledger id |
|---|---|---|
| Hotkey to visible | p95 <= 50 ms over 200 runs | R-006 |
| Keystroke to results | p95_est <= 25 ms over a 10,000-item index (R-006 v5) | R-006 |
| Idle working set | <= 50 MB total, measured without working-set trimming (Phase 1 spike: 27.3 MB; Phase 2 Commander Suite: ~47.5 MB working set, ~16.5 MB private) | R-006 v5 |
| Idle CPU | < 0.5% averaged over 60 s | R-006 |
| Process start to tray-ready (tray icon shown, hotkeys registered) | <= 300 ms | R-006 |
| Pre-warmed launcher window ready | <= 1.5 s after process start; a hotkey pressed earlier is served as soon as the window is ready | R-006 |
| Release exe size | <= 20 MB | R-006 |
| PowerToys baseline | Same metrics recorded for the installed PowerToys, captured before it is uninstalled and committed as `docs/research/baseline-powertoys.json` | R-006, R-002 |
| Clipboard retention | <= 30 days and <= 1,000 entries; pinned exempt (<= 100) | R-009, R-015, D-015 |
| Clipboard image | <= 10 MB stored PNG; raw bitmap > 64 MiB rejected | R-015, D-015 |
| Clipboard text | <= 1 MiB per entry | D-015 |
| Clipboard store | <= 500 MB total | D-015 |
| Color history | 20 colors | R-017 |
| Theme file | <= 64 KiB, id `[a-z0-9-]{1,40}`, at most 100 themes read | D-043 |
| Theme values | Window radius 0, 4 or 8; row and control radius 0 to 12; chip radius 0 to 8; font family <= 64 characters | R-030, D-043 |
| Visible rows | <= 50 | D-016, D-021 |
| File results | <= 30, async; local disk-backed File index resident overhead < 5 MB (ADR-0009) | D-016, ADR-0009 |
| Calculation history | <= 100 entries, DPAPI-encrypted | R-014 |
| FX rate response & cache | <= 256 KiB, 5 s timeout, 24h TTL, HTTPS allowlist only | ADR-0008 |
| Snippets file | <= 256 KiB (`snippets.toml`), zero `WH_KEYBOARD_LL` hooks | ADR-0004 |
| Money and time | Rp0 (free and open-source dependencies only); no hard deadline | D-008 |

## Measured, not yet enforced (ratchets)

| Metric | Today | Direction |
|---|---|---|
| Mutation score on the diff (cargo-mutants) | not yet measured | must not fall |
| Release exe size | 10.7 MB (NCMD-1 spike, software renderer with accessibility) | must not rise without a reason |
| Idle working set | 27.3 MB, 8.5 MB private (NCMD-1 spike, no modules yet, untrimmed); ~47.5 MB, 16.5 MB private (Phase 2 suite) | must not rise without a reason; limit 50 MB |

## Exceptions

| ID | Rule | Path | Reason | Owner | Expires |
|---|---|---|---|---|---|
| EX-001 | `#![forbid(unsafe_code)]` | `crates/naobi-win32/**` | All Win32, COM and DPAPI calls live here; every block carries `// SAFETY:` (docs/adr/0003-unsafe-in-win32-crate-only.md, D-011) | Raihan | never |
| EX-002 | `#![forbid(unsafe_code)]` becomes `unsafe_code = "deny"` | `crates/naobi-ui/**` | Slint-generated code carries `#[allow(unsafe_code)]`, which forbid rejects (E0453). Hand-written sources are scanned for `unsafe` by test `naobi_ui_hand_written_sources_have_no_unsafe` (docs/adr/0003-unsafe-in-win32-crate-only.md) | Raihan (delegated to the orchestrator) | never |

## Definition of Done (standing)

- Acceptance criteria met and verified at runtime, not only compiled or typechecked.
- New behaviour has tests that fail without the change; existing tests pass; no regressions.
- Docs and ADRs updated where the change affects them.
- A rollback path exists for risky changes (migrations are expand-contract).
- The owner has reviewed before merge or deploy.
