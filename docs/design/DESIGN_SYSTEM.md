# Design System: Naobi Command

<!-- Minimum per docs/STANDARD.md §8.3. Sources: ledger R-007, D-021, D-030; naobi.dev DESIGN.md, DOC-09 (tokens), DOC-10 (motion), DOC-11 (accessibility), read-only. No color, font or shape below is new: each is a Naobi token adapted to a Windows desktop launcher. Anything marked "derived" or "proposal" is listed in the report's open questions. -->

## How the web system was adapted

| Web system (naobi.dev) | Naobi Command |
|---|---|
| Warm paper canvas, near-black ink | `paper` theme: window `paper-elevated`, raised areas `paper-canvas`, text `ink-*` |
| Dock stone, oka, umi, on-stone | `stone` theme: window `dock-stone`, raised `dock-oka`, deepest well `dock-umi`, text `on-stone` |
| Oxide accent, rare and small | Selection bar, topmost border, pinned marker only (Accent Discipline, DOC-09 §7) |
| Focus blue `#275D9A` on paper, `#E3BDA9` on stone | Same pair, 2 px ring |
| Inter for interface, JetBrains Mono technical | Same; Newsreader is not used (Scoped Voice Rule: it belongs to the website Home) |
| Hairlines instead of cards, no shadows except the dock | Hairline dividers, no card chrome, no shadow beyond the OS window shadow |
| 100 to 600 ms motion envelope | Launcher budget below: nothing on show or hide, at most 120 to 180 ms elsewhere |
| Theme follows the device unless overridden | Theme follows the Windows app theme live (R-007); no in-app override in the MVP |

## Tokens

Theme is chosen by `AppsUseLightTheme` and changes without restart (R-007). Values come from `DESIGN.md` unless marked derived.

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

### Type scale

Inter (interface) and JetBrains Mono (technical), both embedded (R-007, OFL licenses in `THIRD_PARTY.md` at scaffold). Sizes are logical px at 100%; the renderer multiplies by monitor DPI scale and by `ui.text_scale` (1.0, 1.125 or 1.25). The sizes are DOC-09 `type.100` to `type.500`.

| Token | Size / line height | Weight | Use |
|---|---|---|---|
| `type.input` | 16 / 24 (type.400) | Inter 400 | Search field text and placeholder |
| `type.title` | 13 / 20 (type.200) | Inter 400 | Row title, action label |
| `type.title-strong` | 13 / 20 | Inter 500 | Section header (letter spacing +0.025 em, as `interface-label`), Calculator result label |
| `type.meta` | 12 / 16 (type.100) | JetBrains Mono 400 | Row subtitle, path, shortcut, byte size |
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
| `size.row` | 40 px |
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

## Component inventory

Surfaces owned by `naobi-win32` (marked native) use OS drawing; the table says what the Naobi tokens still govern.

| Component | default | hover | focus | disabled | loading | error | empty |
|---|---|---|---|---|---|---|---|
| Launcher window | `surface.window`, 1 px `line.hair` border, `radius.window`, 750 x 475 | n/a, not interactive | n/a, focus lives in children | n/a, never disabled | n/a | n/a | n/a |
| Search field | Placeholder "Search apps and commands..." in `text.muted`, text in `text.primary`, caret, no box, bottom hairline | n/a, always focused while open | Always has keyboard focus when the Launcher is shown; ring not drawn on the field, caret and the open window are the indicator | n/a | Loading bar under the field when an async provider is pending over 150 ms | Parse error of the Settings file shows as a Notice, not in the field | Placeholder shown |
| Loading bar | n/a, absent | n/a | n/a | n/a | 2 px `accent` sweep under the header; static "Searching" under reduced motion | n/a | n/a |
| Result row | 22 px icon, title `type.title`, subtitle `type.meta` in `text.muted`, key hint on the right when relevant | `surface.selected` | `surface.selected` + 2 px `accent` bar at the left edge; paper theme subtitle steps to `text.secondary` | "Unavailable" rows dim only the icon and title to 0.38 opacity; the reason text is information and stays in `text.muted` (5.08:1 on paper, 5.43:1 on stone), so it meets contrast; not selectable for the primary action | n/a, rows never show loading (late file rows are appended in their own section) | n/a, errors are Notices | n/a |
| Section header | `type.title-strong` in `text.muted`, 8 px above the first row | n/a | n/a, not focusable | n/a | n/a | n/a | n/a |
| Calculator row | Result in `type.result`, expression and unit in `type.meta`; Enter label "Copy" | `surface.selected` | As Result row | n/a | n/a | No row is shown for invalid input or division by zero (D-017) | n/a |
| Key hint chip | `type.footer`, 1 px `line.strong`, `radius.chip`, 4 px padding; text such as "Enter", "Ctrl K" | n/a, not interactive | n/a | n/a | n/a | n/a | n/a |
| Footer bar | `surface.raised`, "Naobi Command" left, primary action and "Actions Ctrl K" chips right, 42 px | n/a | n/a | Action labels dim to 0.38 when the selected Result has no secondary action | n/a | n/a | Shows only "Naobi Command" |
| Action panel | `surface.raised`, `radius.control`, anchored bottom right above the footer, filter field on top, 16 px icons | Row `surface.selected` | Arrow keys move; selected row as Result row; 2 px `focus` ring on the filter field | Unavailable actions listed dimmed with reason | n/a | n/a | "No actions" row |
| Action row | `type.title`, shortcut chip right | `surface.selected` | `surface.selected` + 2 px `accent` bar | 0.38 opacity | n/a | n/a | n/a |
| Notice banner | Row above results, `surface.raised`, glyph + `type.title` message naming its subject, 1 px `line.strong` bottom; sticky until seen (shown again on the next Launcher show, also via tray Open); dismiss with Esc (first Esc dismisses it) | n/a | Announced when shown; not focusable | n/a | n/a | Red `state.danger` glyph in paper, `text.primary` glyph in stone, text always present | n/a |
| Armed row (R-020) | Selected row whose title reads "Restart: press Enter again to confirm" and shows a `state.danger` bar in paper or `text.primary` bar in stone | n/a | The armed row keeps selection; any other key or selection change disarms; 5 s timeout | n/a | n/a | n/a | n/a |
| Clipboard entry row | Kind glyph, first line of text (or "Image 1920 x 1080", or "3 files") in `type.title`, relative time in `type.meta`, pinned marker (`accent`) | `surface.selected` | As Result row | n/a | n/a | n/a | n/a |
| Preview pane | `surface.raised` (paper) or `surface.well` (stone), `radius.control`, text in `type.preview`, image fitted and centered | n/a | n/a | n/a | "Decrypting" text for over 150 ms | "Cannot read this entry" text | "Nothing selected" |
| Clipboard list | List left, preview right (proposal: 300 px list, remainder preview) | n/a | As Result row | n/a | Rows appear as the index builds | n/a | Empty state: "Nothing copied yet" |
| Color history row | 22 px swatch (1 px `line.strong`), HEX in `type.meta`, time | `surface.selected` | As Result row | n/a | n/a | n/a | Empty state: "No colors picked yet" |
| Empty state | `type.result` title, `type.title` helper line in `text.muted`, centered in the list area | n/a | n/a | n/a | n/a | n/a | This is the empty state |
| Color picker overlay (native) | Frozen screen snapshot per monitor, magnifier ring around the cursor with a 1 px `line.hair` outline, HEX readout in `type.readout` on `surface.well` | n/a | Esc cancels; the overlay has focus | n/a | n/a | n/a | n/a |
| Topmost border (native) | 2 px rectangle around the pinned window in the theme accent of Windows: oxide `#914A3B` in light mode, `#E3BDA9` in dark mode (R-018 v2; 6.09:1 on the paper window, 8.66:1 on the stone window); setting `aot.border_color` defaults to `auto` | n/a | n/a | Hidden when disabled in settings | n/a | n/a | n/a |
| Tray icon (native) | Three icon states: Off, Indefinite, Timed (Awake mode); tooltip names the mode and remaining time | n/a, OS tooltip | OS focus | n/a | n/a | Hotkeys paused or an active Notice such as a hotkey conflict shows a distinct overlay glyph and is named in the tooltip | n/a |
| Tray menu (native) | Open, Pause hotkeys, Awake status, Settings, Quit; OS-themed, OS-focus, mnemonic keys | OS | OS | Disabled items grayed by the OS | n/a | n/a | n/a |
| Balloon notification (native) | OS notification with the Notice message | n/a | n/a | n/a | n/a | Title "Naobi Command" and the message that names the subject | n/a |

## Key screens and mockups

Direction approved by the Owner (D-031): `docs/design/mockups/launcher-mockups.html` (root, results with calculator, action panel, clipboard history, window management; paper and stone). The remaining screens follow the same pattern and are specified per ticket in Plan. The required states per screen are in `docs/design/mockups/README.md`.

| Screen | Mockup path | Approval (D- id) |
|---|---|---|
| Launcher root, empty query | `docs/design/mockups/launcher-mockups.html` | D-031 |
| Results with a Calculator row | `docs/design/mockups/launcher-mockups.html` | D-031 |
| Action panel open | `docs/design/mockups/launcher-mockups.html` | D-031 |
| Clipboard history with image preview | `docs/design/mockups/launcher-mockups.html` | D-031 |
| Window management command list | `docs/design/mockups/launcher-mockups.html` | D-031 |
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
| Backspace on an empty query | Pop the view (proposal, from Raycast) |
| Up, Down, PageUp, PageDown | Move selection |
| Ctrl+. | Pin or unpin the clipboard entry (proposal, from Raycast) |
| Ctrl+X | Delete the clipboard entry when the field has no text selection (proposal) |
| Alt+Space | Open the Launcher (R-008) |
| Alt+Shift+V | Open clipboard history (R-015) |

Focus model: while the Launcher is open, keyboard focus stays in the search field and the list selection is a virtual cursor exposed to assistive technology as the active descendant. In the action panel, focus moves to its filter field and Esc returns it. When the Launcher hides because of Esc or Enter, focus returns to the Paste target or the previously focused window (unless the action launched a process); when it hides because focus moved to another window, it only cloaks and that window keeps the keyboard.

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
