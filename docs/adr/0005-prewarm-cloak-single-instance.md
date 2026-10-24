# Launcher window: pre-warmed, cloaked, never destroyed; one instance

Status: accepted
Ledger: D-013 (mechanism for R-023, supports R-006)

The hotkey-to-visible budget is 50 ms at p95. Process start, font load, index load and window creation cannot happen inside it. Both reference launchers keep one long-lived, already-created window: PowerToys Run toggles visibility, Command Palette cloaks with `DWMWA_CLOAK` and re-asserts topmost on show, Raycast parks a hidden window on the target monitor (`docs/research/powertoys.md` §2.4, `docs/research/raycast-rea.md` §3).

## Decision

- At startup the platform thread creates a hidden top-level tool window (never shown; a message-only window does not receive the broadcast messages) and registers the hotkeys; the UI shell creates the Launcher window hidden; the tray icon appears. That is "tray-ready": tray icon shown and hotkeys registered within 300 ms of process start (R-006 v2). The pre-created Launcher window must be ready within 1.5 s of process start; a hotkey pressed before that is queued and served as soon as the window is ready. Fonts are embedded; the app catalog (the `shell:AppsFolder` scan can take hundreds of milliseconds cold) and frecency load on the index worker afterwards, and the Launcher shows an "Indexing apps" row until the catalog is ready.
- Hiding is `DwmSetWindowAttribute(DWMWA_CLOAK, TRUE)`. For a hide driven by Esc or Enter it also returns the foreground to the window recorded when the Launcher opened (`SetForegroundWindow(previous)`), unless the action launched a process, in which case the new process takes the foreground. Cloaking alone does not deactivate a window, so without the focus return an invisible Launcher would keep the keyboard. For a hide caused by focus loss (the Owner clicked another window) it only cloaks and never calls `SetForegroundWindow`: the window the Owner just chose already has the keyboard, and pulling focus back would take it away or make a taskbar button flash. Showing is: pick the monitor under the cursor, move the window there at that monitor's DPI while cloaked, uncloak, then the foreground trick (an empty mouse `SendInput`, then `SetForegroundWindow`). The window is never destroyed, so its HWND is constant (R-023 acceptance).
- Style `WS_EX_TOOLWINDOW` keeps it out of the taskbar and Alt+Tab (R-022 acceptance).
- Paste from clipboard history uses the same hide: write the entry with the private marker, cloak, return focus to the paste target, verify, `SendInput` Ctrl+V; about 300 ms later the previous clipboard content is restored (D-019 v5; text, image and file list within the D-015 limits, other formats come back degraded), unless another copy happened in between or the snapshot carried an exclusion format or came from an ignored app. Such a snapshot is not restored, so a password-manager copy is never written back without its exclusion formats.
- Single instance: named mutex `Local\NaobiCommand.Instance`. A second process finds the first's hidden tool window (`WS_EX_TOOLWINDOW`, never shown, the platform thread's window) by class name, posts one payload-free message (`WM_APP_SHOW`), and exits. Spoofing that message can only show the Launcher.
- No animation or timer runs while cloaked.

## Considered options

- Create and destroy per show: window creation plus first paint will not fit in 50 ms; also changes the HWND.
- Park off-screen: the window stays composited and focusable by mistake, and the thumbnail shows in some Alt+Tab replacements.
- `SW_HIDE`: DWM drops the surface, so the next show pays a resurface cost.

## Consequences

- The swap chain stays resident (about 1.4 MB per buffer at 750 x 475 x 4 bytes); counted in the memory allocation.
- A cloaked window may still be visible to UI Automation; check with Accessibility Insights, and with a screen reader (D-032; accessibility is on under D-034) (risk in the report).
- Cloaking works with Slint: in the NCMD-1 spike 40 of 40 hotkey presses toggled the DWM cloak, and the software-renderer window is created and cloaked in about 200 ms (D-034). The hotkey thread starts before any UI work, so tray-ready is 33 to 35 ms (software renderer; 39 ms is skia).
- The 300 ms clipboard restore (D-019 v5) starts when `SendInput` returns, so a paste target that handles `WM_PASTE` slowly may paste the restored content. Accepted limitation, characterised by a tier 2 case in the clipboard paste item (a helper window delays `WM_PASTE` by 500 ms).
