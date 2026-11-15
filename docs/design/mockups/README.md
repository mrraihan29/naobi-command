# Mockups

Status: the direction is approved by the Owner (D-031). `launcher-mockups.html` covers screens 1 to 5 below in paper and stone (root, results with calculator, action panel, clipboard history, window management). Screens 6 to 8 (color picker overlay, awake and tray menu, empty/error/loading states) follow the same pattern and are specified per ticket in Plan; the state lists below are the input for those tickets. Each screen is drawn in both themes, `paper` and `stone`, at 100% scale, 750 x 475 logical px unless noted. Tokens: `docs/design/DESIGN_SYSTEM.md`. Do not add colors, fonts or shapes that are not in it.

| # | Screen | Required states |
|---|---|---|
| 1 | Launcher root, empty query | Default (10 most frecent items, section headers, footer with Enter and Ctrl K chips); selected row (accent bar + wash); hover row |
| 2 | Results with a Calculator row | `2^10 + 15%` (value 1177.6); `5 km to mi`; mixed list (app, command, quicklink `gh react`, file); exact-alias match on top; fuzzy highlight of matched letters; Files section appended late; selected row in paper (subtitle steps up to secondary) |
| 3 | Action panel | Open over a Result with the filter field focused (2 px focus ring); a row selected; filtered to zero ("No actions"); a disabled action with its reason |
| 4 | Clipboard history with image preview | Text entry selected with text preview; image entry selected with image in the preview well; file-list entry; pinned entry marker; search active; empty ("Nothing copied yet"); image over 10 MB attempted (Notice "Image over 10 MB not stored") |
| 5 | Window management command list | Query `half` or `snap`: Left Half, Right Half, Top Half, Bottom Half, thirds, quarters, Center, Maximize, Almost Maximize, Next Display, Restore with their hotkeys as chips; Notice "Cannot move <window name>: it runs as administrator" |
| 6 | Color picker overlay | Magnifier ring at cursor with HEX readout; near a screen edge (readout flips side); second monitor; after click ("Copied #914A3B" confirmation); Color history list with a row's action panel (Copy as HEX, RGB, HSL, OKLCH) and the empty state |
| 7 | Awake and tray | Tray icon in three states (Off, Indefinite, Timed) at 16 and 24 px in light and dark taskbar; tray menu (Open, Pause hotkeys, Awake status, Settings, Quit) with Awake state line and Hotkeys paused state; launcher showing `awake 1h` result row |
| 8 | Empty, error and loading states | No results ("No results for <query>" with the quicklink fallback hint); "File search unavailable" row; loading bar with a pending file query; Notice banner for Ctrl+Space conflict ("Ctrl+Space is used by another app"); Settings parse error banner with line number; Armed command row ("Restart: press Enter again to confirm"); reduced-motion loading ("Searching") |

Also include one frame at 150% DPI with `ui.text_scale` 1.25 to show the window growth rule, and one frame of the stone theme with the keyboard focus ring on the action panel filter.
