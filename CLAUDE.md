# Naobi Command

Personal Windows 11 launcher and utility suite (launcher, calculator, clipboard history, color picker, always-on-top, awake) with editable themes for one owner, replacing Raycast and PowerToys. Stack: Rust, native UI Slint with the software renderer and accessibility on (D-034) (`docs/adr/0001-stack.md`, `docs/adr/0002-ui-seam.md`).

## Commands
- Install: `rustup toolchain install stable` (host `x86_64-pc-windows-msvc`, VS 2022 Build Tools); then `cargo fetch`
- Format + lint: `cargo fmt --all --check` and `cargo clippy --workspace --all-targets -- -D warnings`
- Types: `cargo check --workspace --all-targets`
- Tests, per-task gate (tier 1): one crate `cargo test -p <crate>`; all `cargo test --workspace`
- Tests, tier 2 (inject keys, write clipboard, move windows; quit the dogfood instance first; before Gate 3): `cargo test --workspace --no-fail-fast -- --ignored --test-threads=1`
- Perf budgets (R-006): `cargo run --release -p naobi-perf -- all --out target/perf.json`; network silence: `... -p naobi-perf -- network-silence --minutes 10`
- Dependencies: `cargo deny check`
- E2E: none; Playwright does not apply (D-006). No hosted CI before Ship: every gate runs locally.
- Ledger check: `uv run C:/Coding/naobi-sdlc/scripts/ledger_check.py docs/ledger.yaml --gate design`

## Architecture in one breath
Cargo workspace, one crate per module: `naobi-core` (kernel and UI data types), `-calc`, `-launcher`, `-clipboard`, `-aot`, `-picker`, `-awake`, `-ui` (framework adapter), `-win32` (the only `unsafe`), `-app` (wiring), `-perf`. Module crates define ports; `naobi-win32` implements them. Nothing outside `naobi-ui` may name Slint.

## Boundaries
- Always: run the commands above on changed files before finishing; put `// @R-xxx` above each test whose name starts `r<nnn>_`; mark tier 2 tests `#[ignore = "tier2: interactive desktop"]`; use the words in `GLOSSARY.md` (pin = clipboard, topmost = window, item = launcher, entry = clipboard).
- Ask first: schema changes, new dependencies (zero-cost OSS only), CI or deploy config, loosening any rule, anything touching `docs/ledger.yaml` requirements.
- Never: commit secrets, suppress a check without a written reason, skip or delete a test, weaken `CONSTRAINTS.md`, add `unsafe` outside `crates/naobi-win32`, use `unwrap`/`expect` outside tests, log clipboard content or query text, open a socket, run elevated, push or deploy without the owner's go.

## Read before coding
- `CONSTRAINTS.md`: quality bar, budgets and Definition of Done
- `GLOSSARY.md`: domain language
- `docs/design/REPORT.md`: design, flows, security, test strategy (seams are the TDD seams)
- `docs/design/DESIGN_SYSTEM.md`: tokens, states, keyboard map
- `docs/ledger.yaml`: requirements (R-) and decisions (D-)
- `docs/research/`: clean-room PowerToys and Raycast behaviour notes (read-only; copy no code or assets)
