# Design System: Naobi Command

<!-- Minimum per docs/STANDARD.md §8.3. Sources: ledger R-007, D-021, D-030; naobi.dev DESIGN.md, DOC-09 (tokens), DOC-10 (motion), DOC-11 (accessibility), read-only. No color, font or shape below is new, except the `tokyo-haze` and `neon-night` Built-in themes the Owner added on 2026-11-20 (D-046): each is a Naobi token adapted to a Windows desktop launcher. Anything marked "derived" or "proposal" is listed in the report's open questions. -->

## How the web system was adapted

| Web system (naobi.dev) | Naobi Command |
|---|---|
| Warm paper canvas, near-black ink | `paper` theme: window `paper-elevated`, raised areas `paper-canvas`, text `ink-*` |
| Dock stone, oka, umi, on-stone | `stone` theme: window `dock-stone`, raised `dock-oka`, deepest well `dock-umi`, text `on-stone` |
| (not in the web system) | `tokyo-haze` and `neon-night` themes: two further dark Built-in themes added by the Owner (D-046), with their own palettes below |
| Oxide accent, rare and small | Selection bar, topmost border, pinned marker only (Accent Discipline, DOC-09 §7) |
| Focus blue `#275D9A` on paper, `#E3BDA9` on stone | Same pair, 2 px ring |
| Inter for interface, JetBrains Mono technical | Same; Newsreader is not used (Scoped Voice Rule: it belongs to the website Home) |
| Hairlines instead of cards, no shadows except the dock | Hairline dividers, no card chrome, no shadow beyond the OS window shadow |
| 100 to 600 ms motion envelope | Launcher budget below: nothing on show or hide, at most 120 to 180 ms elsewhere |
| Theme follows the device unless overridden | The `light` and `dark` Theme slots follow the Windows app mode live (R-007 v2, R-032); either slot can name a Custom theme |

## Tokens

`AppsUseLightTheme` chooses the Theme slot, and the app changes without restart (R-007, R-032). The tables below are the four Built-in themes; the values of `paper` and `stone` come from `DESIGN.md` unless marked derived, and `tokyo-haze` and `neon-night` are the Owner's own palettes. A Custom theme may override them (see Themes).

### Color: `paper` theme (light)

Text/background pairs must meet WCAG 2.2 AA: 4.5:1 for normal text, 3:1 for large text and UI components. Ratios computed with the WCAG relative-luminance formula; the same pairs become the unit test `r007_token_pairs_meet_wcag_aa`.

| Token | Value | Use | Contrast vs background |
|---|---|---|---|
| `surface.window` | `#FAF8F2` (paper-elevated) | Launcher window, panels | base |
| `surface.raised` | `#F3F0E8` (paper-canvas) | Footer, action panel, preview well | base |
| `surface.selected` | `#E8E6E0` (wash-hover `rgb(29 26 22 / .08)` over window) | Selected and hovered row | base |
| `text.primary` | `#171714` (ink-primary) | Input text, row titles | 16.91:1 on window, 15.77 on raised, 14.39 on selected |
| `text.secondary` | `#393732` (ink-secondary) | Selected-row subtitle, notices | 11.19:1 on window, 9.53 on selected |
| `text.muted` | `#6D6A63` (ink-muted) | Subtitles, hints, footer | 5.08:1 on window, 4.74 on raised; **4.32 on selected: fails, do not use there** |
| `accent` | `#914A3B` (accent-oxide) | 2 px selection bar, topmost border, pinned marker | 6.09:1 on window, 5.18 on selected (non-text 3:1 met) |
| `focus` | `#275D9A` (focus) | 2 px focus ring on panel items | 6.34:1 on window, 5.91 on raised, 5.40 on selected |
| `state.danger` | `#A63E32` (DOC-09 provisional) | Armed-command marker, error glyph | 5.89:1 on window, 5.01 on selected; always paired with text |
| `state.success` | `#4E6A51` (DOC-09 provisional) | "Copied" glyph | 5.64:1 on window; always paired with text |
| `line.hair` | `#D9D3C7` (line-hair) | Dividers, window border | 1.40:1, decorative only |
| `line.strong` | `#C7C1B6` (line-stone) | Key hint chip outline | 1.69:1, decorative only (the chip text carries the meaning) |

### Color: `stone` theme (dark)

| Token | Value | Use | Contrast vs background |
|---|---|---|---|
| `surface.window` | `#2A2622` (dock-stone) | Launcher window, panels | base |
| `surface.raised` | `#36312C` (dock-oka) | Footer, action panel | base |
| `surface.well` | `#0D0B09` (dock-umi) | Image preview well, color readout | base |
| `surface.selected` | `#3A3632` (derived: `on-stone` at 8% over window) | Selected and hovered row | base |
| `text.primary` | `#F3F0E8` (on-stone) | Input text, row titles | 13.18:1 on window, 11.30 on raised, 10.52 on selected, 17.25 on well |
| `text.secondary` | same as `text.primary` | The stone palette has no separate secondary tier | n/a |
| `text.muted` | `rgb(243 240 232 / .58)` (on-stone-dim) | Subtitles, hints, footer | 5.43:1 on window, 4.94 on raised, 4.70 on selected, 6.15 on well |
| `accent` | `#E3BDA9` (stone-focus, the dark value of the earth token) | Selection bar, topmost border option, pinned marker | 8.66:1 on window |
| `focus` | `#E3BDA9` (stone-focus) | 2 px focus ring | 8.66:1 on window, 7.42 on raised, 6.91 on selected, 11.34 on well |
| `line.hair` | `#4A433A` (stone rim) | Dividers, window border | 1.54:1, decorative only |
| oxide `#914A3B` | not used | Rejected on stone: 2.32:1 on the window, below 3:1 | n/a |
| focus blue `#275D9A`, danger `#A63E32` | not used | Rejected on stone: 2.23:1 and 2.40:1 | n/a |

No danger or success tint is defined for stone. In stone, state is conveyed by text and a glyph in `text.primary` (DOC-11 §31, use of color), so no new color is invented.

### Color: `tokyo-haze` theme (dark, built-in)

Added by the Owner on 2026-11-20 (D-046, D-030 v2); not derived from naobi.dev DESIGN.md. All colors are opaque. `text.muted` and `state.danger` are lighter than the Owner's first sketch (`#5D6B63`, `#C8382E`) because those failed 4.5:1. The ratios are the same pairs as `r007_token_pairs_meet_wcag_aa`, which iterates every built-in theme.

| Token | Value | Use | Contrast vs background |
|---|---|---|---|
| `surface.window` | `#1B221F` | Launcher window, panels | base |
| `surface.raised` | `#2A312D` | Footer, action panel | base |
| `surface.selected` | `#333B37` | Selected and hovered row | base |
| `surface.well` | `#0D0F0E` | Image preview well, color readout | base |
| `text.primary` | `#F0E6B8` | Input text, row titles | 12.91:1 on window, 10.61 on raised, 9.18 on selected, 15.31 on well |
| `text.secondary` | `#D6CDA4` | Notices; also the selected-row subtitle | 10.14:1 on window, 7.21 on selected |
| `text.muted` | `#9DB0A6` | Subtitles, hints, footer | 7.10:1 on window, 5.84 on raised, 8.42 on well |
| `accent` | `#E8A33D` (amber) | Selection bar, topmost border option, pinned marker | 7.52:1 on window, 5.35 on selected |
| `focus` | `#7CC9B3` (teal) | 2 px focus ring | 8.39:1 on window, 6.89 on raised, 5.96 on selected |
| `state.danger` | `#F89A8B` | Armed-command marker, error glyph | 7.71:1 on window, 5.48 on selected; always paired with text |
| `state.success` | `#93BD82` | "Copied" glyph | 7.60:1 on window; always paired with text |
| `line.hair` | `#3B4641` | Dividers, window border | 1.65:1, decorative only |
| `line.strong` | `#505D57` | Key hint chip outline | 2.35:1, decorative only |

Worst margin: `state.danger` on selected, 5.48 against 4.5. Windows-mode family and Duplicate base: `stone`.

### Color: `neon-night` theme (dark, built-in)

Added by the Owner on 2026-11-20 (D-046, D-030 v2); not derived from naobi.dev DESIGN.md. All colors are opaque. The focus red is lightened from `#C8382E` to `#F0634F` so the ring passes 3:1 on the selected row.

| Token | Value | Use | Contrast vs background |
|---|---|---|---|
| `surface.window` | `#0D0F0E` | Launcher window, panels | base |
| `surface.raised` | `#171A18` | Footer, action panel | base |
| `surface.selected` | `#222623` | Selected and hovered row | base |
| `surface.well` | `#050605` | Image preview well, color readout | base |
| `text.primary` | `#F0E6B8` | Input text, row titles | 15.31:1 on window, 13.96 on raised, 12.21 on selected, 16.16 on well |
| `text.secondary` | `#D6CDA4` | Notices; also the selected-row subtitle | 12.02:1 on window, 9.59 on selected |
| `text.muted` | `#A3A08B` | Subtitles, hints, footer | 7.29:1 on window, 6.65 on raised, 7.69 on well |
| `accent` | `#D9772B` (orange) | Selection bar, topmost border option, pinned marker | 6.07:1 on window, 4.84 on selected |
| `focus` | `#F0634F` (neon red) | 2 px focus ring | 6.05:1 on window, 5.51 on raised, 4.82 on selected |
| `state.danger` | `#FF7B6B` | Armed-command marker, error glyph | 7.60:1 on window, 6.06 on selected; always paired with text |
| `state.success` | `#8FC47A` | "Copied" glyph | 9.48:1 on window; always paired with text |
| `line.hair` | `#2B302D` | Dividers, window border | 1.43:1, decorative only |
| `line.strong` | `#434944` | Key hint chip outline | 2.08:1, decorative only |

Worst margins: `state.danger` on selected, 6.06 against 4.5 (text); `focus` on selected, 4.82 against 3 (UI). Windows-mode family and Duplicate base: `stone`.

### Type scale

Inter (interface) and JetBrains Mono (technical), both embedded (R-007, OFL licenses in `THIRD_PARTY.md` at scaffold). Sizes are logical px at 100%; the renderer multiplies by monitor DPI scale and by `ui.text_scale` (1.0, 1.125 or 1.25). The sizes are DOC-09 `type.100` to `type.500`.

| Token | Size / line height | Weight | Use |
|---|---|---|---|
| `type.input` | 16 / 24 (type.400) | Inter 400 | Search field text and placeholder |
| `type.title` | 13 / 20 (type.200) | Inter 400 | Row title, action label |
| `type.title-match` | 13 / 20 | Inter 600 | The title letters matched by the query (R-025). Weight only: the accent stays rare |
| `type.title-strong` | 13 / 20 | Inter 500 | Section header (letter spacing +0.025 em, as `interface-label`), Calculator result label |
| `type.kind` | 12 / 16 (type.100) | Inter 400 | Right-aligned row kind, time and non-technical subtitle |
| `type.meta` | 12 / 16 (type.100) | JetBrains Mono 400 | Technical row text: folder path, calculator expression, HEX |
| `type.footer` | 12 / 16 (type.100) | Inter 500, +0.025 em | Footer text and key hint chips |
| `type.preview` | 13 / 20 (type.200) | JetBrains Mono 400 | Clipboard text preview |
| `type.result` | 18 / 24 (type.500) | Inter 500 | Calculator result value, empty-state title |
| `type.readout` | 14 / 20 (type.300) | JetBrains Mono 500 | Color picker HEX readout |

The NCMD-1 spike used 11 px for meta and footer; the design raises it to 12 px (`type.100`, the smallest DOC-09 token).

### Spacing scale

DOC-09 4 px root scale. Component numbers from D-021 (Raycast Windows measurements, numbers only).

| Token | Value |
|---|---|
| `space.1` | 4 px |
| `space.2` | 8 px (list padding, row horizontal padding, gap between icon and title) |
| `space.3` | 12 px |
| `space.4` | 16 px (search field horizontal padding) |
| `space.6` | 24 px |
| `size.window` | 750 x 475 logical px |
| `size.header` | 66 px |
| `size.row` | 40 px (Density: compact 36, normal 40, comfortable 44) |
| `size.section` | 28 px section header (D-042) |
| `size.action` | 32 px action row |
| `size.footer` | 42 px |
| `size.icon` | 22 px (16 px in compact lists such as the action panel) |
| `size.border` | 1 px hairline, 2 px accent bar, 2 px focus ring |

### Radius

| Token | Value |
|---|---|
| `radius.window` | 8 px (DOC-09 `radius.3`; the OS rounds the window the same way) |
| `radius.row` | 2 px (`radius.1`) selected and hovered row |
| `radius.chip` | 2 px (the seal radius from DESIGN.md) key hint chips |
| `radius.control` | 4 px (`radius.2`) action panel container, preview well |

A Theme may set the window radius to 0, 4 or 8 px (Windows draws it), the row and control radius from 0 to 12 px, and the chip radius from 0 to 8 px.

### Elevation

None in the app. The launcher relies on the OS window shadow. The action panel is separated from the list by a hairline and `surface.raised`, not by a shadow.

### Rendering constraints (software renderer, D-034)

Slint renders on the CPU, so the design avoids anything expensive: no blur or acrylic backdrop, no large soft shadows (already none in the app), no per-frame animation beyond the short fades below, flat fills and 1 px hairlines. Images in the clipboard preview are pre-scaled to a bounded thumbnail. The footer glyph U+21B5 is absent from the embedded Inter subset and renders nothing; key hint chips use words ("Enter", "Ctrl K") unless the subset is extended.

### Motion budgets

Tokens are DOC-10: instant 0, micro 120 ms, quick 180 ms, standard 240 ms, ceiling 600 ms; easing `ease.decelerate` for reveals, `ease.standard` otherwise; no springs.

| Event | Budget |
|---|---|
| Show and hide the Launcher | 0 ms. No animation; the R-006 budget is 50 ms to a visible frame |
| Selection moves, list updates, query typing | 0 ms (same frame) |
| Action panel open | 120 ms opacity, 2 px rise (`distance.micro`); close 0 ms |
| Notice banner reveal | 180 ms opacity; dismiss 0 ms |
| "Copied" confirmation | 120 ms opacity, then the Launcher hides |
| Loading bar (only after an async provider has been pending 150 ms) | Indeterminate sweep, 1.2 s loop, one animation at a time |
| Color picker magnifier | Follows the pointer each frame, no easing |
| Topmost border, tray icon | None |
| While the Launcher is cloaked | No timers, no animation, no frame callbacks |
| Reduced motion (Windows "Animation effects" off, `SPI_GETCLIENTAREAANIMATION`) | Every duration 0; the loading bar becomes the static text "Searching" |

## Themes

A Custom theme is a file in `%APPDATA%\NaobiCommand\themes\<id>.toml` (ADR-0007). Every key is optional; anything left out or invalid comes from `base`.

```toml
name = "Harbor"
base = "stone"              # paper or stone (the other built-in themes cannot be a base)

[colors]                    # "#RRGGBB"; "#RRGGBBAA" only for text colors
window = "#10242B"          # raised, selected, well
text = "#E8F4F2"            # secondary, selected_text, muted
accent = "#7ED7C1"          # focus, danger, success, hair, strong

[shape]
window = 4                  # 0, 4 or 8
row = 6                     # 0 to 12
chip = 6                    # 0 to 8
control = 8                 # 0 to 12

[list]
density = "comfortable"     # compact, normal or comfortable

[fonts]
interface = "Segoe UI"      # an installed family name
mono = "Cascadia Mono"
```

| Rule | Behaviour |
|---|---|
| Surfaces | `window`, `raised`, `selected`, `well`, `hair` and `strong` must be opaque |
| Applying | Enter in the Themes view fills the slot of the current Windows mode. A saved change to an applied file takes effect within 1 s |
| Preview | Moving the selection in the Themes view redraws the Launcher. Leaving the view restores the applied theme |
| Invalid value | The base value is used and a Notice names the file and line (R-033) |
| Low contrast | The theme applies and a Notice names each failing pair with its ratio. The pairs are those of `r007_token_pairs_meet_wcag_aa` |
| Text scale | `ui.text_scale` stays a setting and multiplies the Density row height |
| Reduced motion | Unchanged: a Theme cannot add motion |

## Component inventory

Surfaces owned by `naobi-win32` (marked native) use OS drawing; the table says what the Naobi tokens still govern.

| Component | default | hover | focus | disabled | loading | error | empty |
|---|---|---|---|---|---|---|---|
| Launcher window | `surface.window`, 1 px `line.hair` border, `radius.window`, 750 x 475 | n/a, not interactive | n/a, focus lives in children | n/a, never disabled | n/a | n/a | n/a |
| View marker | In a pushed View: 28 px chip with a back glyph and the view name left of the field. In a View opened by its own hotkey: the name only. `type.footer`, 1 px `line.strong` | Pointer cursor stays default; `surface.selected` on the back chip | Not in the tab order: Esc and Backspace are the keyboard path; exposed as a button named "Back" | n/a | n/a | n/a | n/a |
| Search field | Placeholder "Search apps and commands, or type > to run one..." in `text.muted`, text in `text.primary`, caret, no box, bottom hairline | n/a, always focused while open | Always has keyboard focus when the Launcher is shown; ring not drawn on the field, caret and the open window are the indicator | n/a | Loading bar under the field when an async provider is pending over 150 ms | Parse error of the Settings file shows as a Notice, not in the field | Placeholder shown |
| Loading bar | n/a, absent | n/a | n/a | n/a | 2 px `accent` sweep under the header; static "Searching" under reduced motion | n/a | n/a |
| Result row | 22 px icon, title `type.title` with matched letters in `type.title-match`, subtitle only when it adds information (`type.meta` for a path or expression), kind in `type.kind` at the right edge; while Alt is held the first nine visible rows show a number chip there | `surface.selected` | `surface.selected` + 2 px `accent` bar at the left edge; paper theme subtitle steps to `text.secondary` | "Unavailable" rows dim only the icon and title to 0.38 opacity; the reason text is information and stays in `text.muted` (5.08:1 on paper, 5.43:1 on stone), so it meets contrast; not selectable for the primary action | n/a, rows never show loading (late file rows are appended in their own section) | n/a, errors are Notices | n/a |
| Section header | `size.section` tall, `type.title-strong` in `text.muted`, aligned to the bottom. The root list stays flat; only sections with a real meaning, such as Files and Web, get a header | n/a | n/a, not focusable | n/a | n/a | n/a | n/a |
| Calculator row | Result in `type.result`, expression and unit in `type.meta`; Enter label "Copy" | `surface.selected` | As Result row | n/a | n/a | No row is shown for invalid input or division by zero (D-017) | n/a |
| Key cap | `type.footer`, 1 px `line.strong`, `radius.chip`, 20 px tall; one cap per key ("Ctrl" and "K" are two caps, 3 px apart) | n/a, not interactive | n/a | n/a | n/a | n/a | n/a |
| Footer bar | `surface.raised`, the current view name left ("Launcher", "Clipboard", "Themes"), then right-aligned actions as label followed by key caps, 42 px | n/a | n/a | An action the selected Result does not have is left out | n/a | n/a | Shows only the view name |
| Action panel | `surface.raised`, `radius.control`, 340 px wide, anchored bottom right above the footer: the selected Item's title in `type.footer`, then grouped actions divided by hairlines, then the filter field | Row `surface.selected` | The filter keeps keyboard focus with a 2 px `focus` ring; Up and Down move the selected action | Unavailable actions listed dimmed with reason | n/a | n/a | "No actions" row |
| Action row | `size.action` tall, 16 px glyph, `type.title`, shortcut key caps right. A destructive action sits in the last group in `state.danger` (paper) or `text.primary` (stone), always with its label | `surface.selected` | `surface.selected` + 2 px `accent` bar | 0.38 opacity | n/a | n/a | n/a |
| Notice banner | Row above results, `surface.raised`, glyph + `type.title` message naming its subject, 1 px `line.strong` bottom; sticky until seen (shown again on the next Launcher show, also via tray Open); dismiss with Esc (first Esc dismisses it) | n/a | Announced when shown; not focusable | n/a | n/a | Red `state.danger` glyph in paper, `text.primary` glyph in stone, text always present | n/a |
| Armed row (R-020) | Selected row whose title reads "Restart: press Enter again to confirm" and shows a `state.danger` bar in paper or `text.primary` bar in stone | n/a | The armed row keeps selection; any other key or selection change disarms; 5 s timeout | n/a | n/a | n/a | n/a |
| Clipboard entry row | Kind glyph, first line of text (or "Image 1920 x 1080", or "3 files") in `type.title`, relative time in `type.meta`, pinned marker (`accent`) | `surface.selected` | As Result row | n/a | n/a | n/a | n/a |
| Preview pane | `surface.well`, `radius.control`, text in `type.preview`, image fitted and centered, then a facts block under a hairline: label in `text.muted`, value in `text.primary`, `type.kind` | n/a | n/a | n/a | n/a (decryption is synchronous) | "Cannot read this entry" text, no facts | "Nothing selected" |
| Clipboard list | 300 px list, preview pane in the rest. Facts: Type, Length or Size or Files, Copied | n/a | As Result row | n/a | Rows appear as the index builds | n/a | Empty state: "Nothing copied yet" |
| Themes list | 300 px list: palette glyph, theme name, and at the right the slot it fills ("Light", "Dark") or Built-in or Custom | n/a | As Result row; the selected theme previews at once | n/a | n/a | An unreadable file is listed dimmed with its reason | Always holds the four Built-in themes: paper, stone, tokyo-haze, neon-night |
| Theme pane | Preview pane with all 14 color swatches and their token names, then facts: Base, Density, Fonts, Contrast | n/a | n/a | n/a | n/a | Problems are Notices | n/a |
| Color history row | 22 px swatch (1 px `line.strong`), HEX in `type.meta`, time | `surface.selected` | As Result row | n/a | n/a | n/a | Empty state: "No colors picked yet" |
| Empty state | `type.result` title, `type.title` helper line in `text.muted`, centered in the list area | n/a | n/a | n/a | n/a | n/a | This is the empty state |
| Color picker overlay (native) | Frozen screen snapshot per monitor, magnifier ring around the cursor with a 1 px `line.hair` outline, HEX readout in `type.readout` on `surface.well` | n/a | Esc cancels; the overlay has focus | n/a | n/a | n/a | n/a |
| Topmost border (native) | 2 px rectangle around the pinned window in the theme accent of Windows: oxide `#914A3B` in light mode, `#E3BDA9` in dark mode (R-018 v2; 6.09:1 on the paper window, 8.66:1 on the stone window); setting `aot.border_color` defaults to `auto` | n/a | n/a | Hidden when disabled in settings | n/a | n/a | n/a |
| Tray icon (native) | Three icon states: Off, Indefinite, Timed (Awake mode); tooltip names the mode and remaining time | n/a, OS tooltip | OS focus | n/a | n/a | Hotkeys paused or an active Notice such as a hotkey conflict shows a distinct overlay glyph and is named in the tooltip | n/a |
| Tray menu (native) | Open, Pause hotkeys, Awake status, Settings, Quit; OS-themed, OS-focus, mnemonic keys | OS | OS | Disabled items grayed by the OS | n/a | n/a | n/a |
| Balloon notification (native) | OS notification with the Notice message | n/a | n/a | n/a | n/a | Title "Naobi Command" and the message that names the subject | n/a |

## Key screens and mockups

The approved visual reference is `docs/design/mockups/ui-upgrade.html` (D-044). It replaces `docs/design/mockups/launcher-mockups.html` (D-031). The required states per screen are in `docs/design/mockups/README.md`.

| Screen | Mockup path | Approval (D- id) |
|---|---|---|
| Launcher root, empty query | `docs/design/mockups/ui-upgrade.html` screen 1 | D-044 |
| Results with matched letters and Files | `ui-upgrade.html` screen 2 | D-044 |
| Action panel open | `ui-upgrade.html` screen 3 | D-044 |
| Clipboard history with facts | `ui-upgrade.html` screen 4 | D-044 |
| Themes view and theme pane | `ui-upgrade.html` screen 5 | D-044 |
| Custom theme preview with a contrast Notice | `ui-upgrade.html` screen 6 | D-044 |
| Number cues while Alt is held | `ui-upgrade.html` screen 7 | D-044 |
| Compact Density | `ui-upgrade.html` screen 8 | D-044 |
| Color picker overlay | follows the same pattern; specified per ticket in Plan | D-031 (direction) |
| Awake and tray menu | follows the same pattern; specified per ticket in Plan | D-031 (direction) |
| Empty, error and loading states | follows the same pattern; specified per ticket in Plan | D-031 (direction) |

## Keyboard map

Everything in the MVP is reachable with the keyboard (R-007). Key meaning is decided in `naobi-core`, identical in any UI framework. Rows marked proposal are not in the ledger and are confirmed with the mockups.

| Key | Meaning |
|---|---|
| Enter | Primary action of the selected row |
| Ctrl+Enter | Secondary action (clipboard: copy only) |
| Ctrl+K | Open or close the action panel |
| Esc | Close the action panel or dismiss a Notice, else clear the query, else pop the view, else hide (R-012). A view opened directly by its own hotkey (clipboard history via Alt+Shift+V) hides instead of popping to the root |
| Backspace on an empty query | A new press pops the view. A held Backspace never does (R-028) |
| Up, Down, PageUp, PageDown | Move selection |
| Alt+1 to Alt+9 | Run the first to ninth visible result; number cues show while Alt is held (R-028) |
| Enter in the Themes view | Use the selected theme for the current Windows mode (R-031) |
| Ctrl+. | Pin or unpin the clipboard entry (from Raycast) |
| Ctrl+X | Delete the clipboard entry when the query is empty (with text in the field it stays a cut; use Ctrl+K for Delete) |
| `>` at the start of the root search | Run mode: the run row and the run history; Enter runs, Ctrl+K offers Delete from Run History (R-024) |
| Ctrl+Space | Open the Launcher (R-008) |
| Alt+Shift+V | Open clipboard history (R-015) |

Focus model: while the Launcher is open, keyboard focus stays in the search field and the list selection is a virtual cursor exposed to assistive technology as the active descendant. In the action panel, focus moves to its filter field and Esc returns it. When the Launcher hides because of Esc or Enter, focus returns to the Paste target or the previously focused window (unless the action launched a process); when it hides because focus moved to another window, it only cloaks and that window keeps the keyboard. A paste from clipboard history restores the previous clipboard about 300 ms later for text, image and file list within the D-015 limits; rich text, HTML, spreadsheet ranges and app-private formats come back degraded (D-019 v5).

## Accessibility

- Target: WCAG 2.2 AA (R-007). Playwright and axe do not apply to a native app (D-006).
- Checked automatically: the token table above as a unit test (`r007_token_pairs_meet_wcag_aa`), run in `cargo test`.
- Checked by hand at Gate evidence time: a keyboard-only pass of every MVP capability (R-007 acceptance) and Accessibility Insights for Windows (free) for contrast. A Narrator walk-through is done when accessibility support is switched on (D-032).
- Weighted criterion, not a requirement (D-032; R-007 does not mention screen readers). Slint's accessibility feature costs 3.3 MB working set with the software renderer (under the 5 MB limit), so it is on (D-034). The adapter exposes: search field as an edit control named "Search"; result list as a list with list items whose accessible name is title + subtitle + kind; selected state; a polite live region announcing the result count and Notices; the armed command announced as "Press Enter again to confirm".
- Visible focus: a 2 px ring in `focus`, drawn inside the element bounds so the window edge never clips it. The selected row is identified by the 2 px accent bar and the wash together, and by the selected state in UIA, so color is not the only cue.
- Target size: every pointer target is at least 24 x 24 px (rows are 40 px tall, panel rows 32 px). The stricter 44 px target of DOC-11 is not adopted: this is a keyboard-first desktop launcher with no touch use. Key hint chips are not interactive.
- Use of color: danger and success states always have a text label and a glyph; grayscale remains coherent (DOC-11 §31, §32).
- Reduced motion: follows Windows "Animation effects"; every duration becomes 0 (motion table above).
- Text size: `ui.text_scale` 1.0, 1.125 or 1.25 (from the DESIGN.md Text size setting) multiplied with the monitor DPI scale. The window grows with the scale up to 90% of the work-area height, then rows scroll.
- Windows high contrast: best effort in the MVP (map to system colors if the adapter supports it); recorded as a risk because R-007 only fixes contrast of the tokens.
- Manual checks automation cannot do: keyboard-only flow, visible focus not obscured, target size, reduced motion, and, when accessibility support is switched on (D-032), screen-reader names for controls and a cloaked window not being read.
