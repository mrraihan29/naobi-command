# `unsafe` is allowed only in `naobi-win32`

Status: accepted
Ledger: D-011 (refines D-005)

The toolchain floor is `#![forbid(unsafe_code)]` in every Rust crate unless an ADR allows it. This product cannot avoid FFI: hotkeys, DWM cloaking, window placement, the clipboard listener, DPAPI, the Windows Search index and power requests are all Win32 or COM calls.

## Decision

- Every crate except `naobi-win32` carries `#![forbid(unsafe_code)]`. That includes `naobi-ui` (the framework crate may use unsafe internally as a dependency; our own adapter code does not).
- `naobi-win32` carries `#![deny(unsafe_op_in_unsafe_fn, clippy::undocumented_unsafe_blocks)]`. Every `unsafe` block has a `// SAFETY:` comment (D-005).
- The crate exposes only safe functions and safe types. Module crates define the ports (traits) and `naobi-win32` implements them; callers never see a raw handle except through `raw-window-handle` values.
- The exception is listed in `CONSTRAINTS.md` (EX-001) with this ADR as its reason.

## Consequences

- One place to audit for memory safety (T-026 in the report). A new Win32 call goes there or nowhere.
- Tests of `naobi-win32` are integration tests on a real Windows desktop; the rest of the workspace is testable without Win32.
