# Global hotkeys use `RegisterHotKey` only; no low-level keyboard hook in the MVP

Status: accepted
Ledger: D-012 (supports R-008, R-022)

Both reference products register with `RegisterHotKey` first and fall back to a `WH_KEYBOARD_LL` hook when Windows refuses a chord or the binding is a modifier-only tap (`docs/research/powertoys.md` §2, `docs/research/raycast-rea.md` §2). The hook path is where their complexity and risk live.

## Decision

- All hotkeys (launcher, clipboard, snap commands, color picker, topmost) are registered with `RegisterHotKey` on one message-only window owned by the platform thread. `WM_HOTKEY` is forwarded as an `AppEvent`.
- A refused registration is never worked around. The Owner sees a Notice that names the hotkey (R-008 acceptance) and the module stays reachable from the launcher.
- "Pause hotkeys" (R-022) is `UnregisterHotKey` for every id, and re-register on resume. No hook to disable.
- No low-level hook, no Raw Input probe, no priority boost.

## Considered options

- Hybrid like Raycast: needs a dedicated high-priority hook thread, a Raw Input liveness probe because Windows silently drops slow hooks, pinned working-set pages, and an injected-key tag. That is extra idle cost against R-006 and extra code for no ledger requirement.
- Hook only (PowerToys optional mode): same costs plus input latency risk for every key on the system.

## Consequences

- Modifier-only taps (a lone Win key press, double-tap Ctrl) are not available. None of the ledger hotkeys needs them.
- A chord owned by another process (PowerToys Run or Command Palette on Alt+Space) cannot be taken over; R-008 already says to turn them off during migration.
- Revisit in phase 2 together with the drag-to-zone overlay, which needs the hook anyway.
