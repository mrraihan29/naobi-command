# Gate 3 manual checklist

What no harness can judge (REPORT 9.5). The Owner runs it on the finished build; each line has the steps, the
expected result and a box. Anything that does not match opens a bug against the owning item, not against NCMD-25.

Before you start: quit every running Naobi, then `cargo build --release -p naobi-app`, and start
`target\release\naobi-app.exe` yourself (the tier 2 and perf runs use a separate build with the `perf` feature).
The automated half of Gate 3 is `cargo test --workspace`, `cargo test --workspace --no-fail-fast -- --ignored --test-threads=1` (Naobi quit) and
`cargo run --release -p naobi-perf -- all --out target/perf.json`.

## 1. R-007: keyboard only, and Narrator

- [ ] **Launcher.** Alt+Space opens it; type to search; Up/Down moves; Enter runs; Tab opens the action list;
  Esc closes. Nothing needs the mouse.
- [ ] **Every module by keyboard.** From the Launcher, without the mouse: run an app, run a calculation (`2+2`),
  open clipboard history (Alt+Shift+V) and paste an entry, toggle Always on Top
  (Win+Ctrl+T), pick a color (Win+Shift+C, Esc cancels), `awake 1h`, `lock`, a file search.
- [ ] **Visible focus.** In every list and in the action list the focused row is unmistakable in light and dark.
- [ ] **Narrator.** Turn Narrator on (Win+Ctrl+Enter). The search box announces its name; arrowing announces
  each row's title and subtitle; the sections announce their names; a Notice is announced; the Launcher
  announces itself when it opens. Note any control with no name.
- [ ] **Accessibility Insights (FastPass).** Run it against the open Launcher: no failure in the name, role or
  keyboard checks. Run the contrast check on both themes: every text and background pair is at least 4.5:1
  (WCAG 2.2 AA; the token pairs are also checked by the unit tests).
- [ ] **Theme.** Switch Windows between light and dark while Naobi runs; the Launcher follows without a restart.

## 2. R-019: Awake

- [ ] `awake 1h` in the Launcher. In an **elevated** shell run `powercfg /requests`: the Naobi process is listed
  under SYSTEM (and DISPLAY after `awake display on`). `awake off`: it is gone. Timed mode: after the time ends
  it is gone (use `awake 1m` and wait). RK-09: this check needs the elevated shell and cannot be done by the app.
- [ ] **Tray icon in three states.** Off, Indefinite and Timed look different at 16 px and at 24 px (set the
  Windows scaling to 150% to see both), on a light and a dark taskbar. The tooltip names the mode and, for Timed,
  the time left.
- [ ] The `awake 1h` row reads naturally in the Launcher, with its hint.
- [ ] `awake display on` while Awake is Off: note what you see (known gap: no feedback).

## 3. R-011: an elevated window

- [ ] Open Notepad **as administrator**. With it focused press Win+Ctrl+T (Always on
  Top): it shows the admin-window Notice naming the window, and the window does not pin.
- [ ] A normal (not elevated) Notepad pins as usual afterwards.

## 4. R-008: hotkey conflict and Do Not Disturb

- [ ] Enable **PowerToys Run** (Alt+Space) and start Naobi: a Notice names Alt+Space, the banner shows on the next
  Launcher open (or tray Open), and the tray tooltip says so. Disable PowerToys Run, restart Naobi: no Notice.
- [ ] Turn **Do Not Disturb** on and make the conflict again: the balloon may be suppressed, but the banner on tray
  Open and the tooltip still name the hotkey.

## 5. Clipboard

- [ ] **Alt+Shift+V with two keyboard layouts.** Add a second layout (for example US and Indonesian). With each
  active, Alt+Shift+V opens the clipboard history and does not switch the layout (switch the layout hotkey in
  Windows if it clashes, and write down which one you chose).
- [ ] Copy from Proton Pass (desktop app): the entry is not stored; the browser extension copy is stored
  (accepted, D-027).
- [ ] Paste from history into Notepad and into a browser field; the previous clipboard comes back afterwards.

## 6. explorer.exe restart

- [ ] End and restart explorer.exe (Task Manager, Restart): the Naobi tray icon comes back without restarting the
  app, and the hotkeys still work.

## 7. Window management (NCMD-21)

- [ ] Elevated and non-elevated Notepad behave as in section 3.

## 8. Always on Top (NCMD-22)

- [ ] The border color is readable on light and dark windows and does not flicker while the window moves.
- [ ] Clicking through the border reaches the window under it.
- [ ] Unpin removes the border; closing a pinned window removes it too.

## 9. Color picker (NCMD-24)

- [ ] Win+Shift+C on a one-monitor and a two-monitor setup: an overlay covers every monitor; the magnifier follows
  the cursor and shows the exact pixel; click picks, Esc and right click cancel.
- [ ] The overlay and the magnifier look right in light and dark (UI-CHECK; there is no drawn mockup).
- [ ] After a pick the HEX is on the clipboard and the color is in `Color History` (newest first, 20 at most);
  Enter on a history row copies its HEX; the formats RGB, HSL and OKLCH are in the action list.
- [ ] At 150% or 200% scaling the picked color is still the pixel under the cursor.

## 10. File search (NCMD-17)

- [ ] Type part of a file name that you know is indexed: the Files section appears last, with the file name as
  title and the full path as subtitle; Enter opens it; the action list has Open Containing Folder.
- [ ] Stop the **Windows Search** service (`net stop WSearch` in an elevated shell) and type a query: the row
  "File search unavailable" shows and the rest of the Launcher keeps working. Start the service again.

## 11. Icons (NCMD-16)

- [ ] App rows show the app's own icon; file rows show the icon for their extension; a row with no icon shows the
  placeholder and nothing else changes.
- [x] Decided (T-030): icons are shown only for packaged apps and for exe paths on a local fixed drive. A
  shortcut, internet shortcut or library is never asked for its icon, so no icon location on a network is
  followed (`icon_id_is_local`).

## 12. Network silence and the build

- [ ] `cargo run --release -p naobi-perf -- network-silence --minutes 10` ends with `pass: true`
  (`r010_no_socket_during_session`). It samples once a second, so a socket that lives less than a second would be
  missed; `cargo deny check` bans the network crates for that reason.
- [ ] `cargo run --release -p naobi-perf -- all --out target/perf.json` prints PASS for every line. It needs the
  pinned conditions of NCMD-27: the 143 Hz panel mode, AC power, no dogfood Naobi and no PowerToys Run.
- [ ] The exe is at most 20 MB and its manifest is `asInvoker` and per-monitor-v2 (the perf run prints the size).
