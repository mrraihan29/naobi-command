# UI/UX teardown: Command Palette, Raycast and the current Naobi Launcher

Purpose: clean-room input for the Naobi Command UI/UX upgrade. It records layout numbers and interaction behaviour only. No code, markup, icons or text from either product go into Naobi (D-004).

Date: 2026-11-18.

## 0. Sources and limits

| Tag | Source | Status |
|---|---|---|
| [cp] | PowerToys Command Palette UI source, MIT, `microsoft/PowerToys` branch `main`, read through the GitHub API (`src/modules/cmdpal/Microsoft.CmdPal.UI`, last change to `ShellPage.xaml` is `ef43bf8ff5`, 2026-10-19) | Read. Static only: nothing was run. |
| [rc-rea] | `docs/research/raycast-rea.md` section 8: numbers taken from Raycast for Windows 2.8.2.0 when it was installed | Earlier evidence, not re-checked. |
| [rc-doc] | `docs/research/raycast.md`: public manual and changelog notes | Earlier evidence, not re-checked. |
| [rc-log] | `%LOCALAPPDATA%\Raycast\caches\releases-changelog\changelog.json`, Raycast's cached public changelog | Read. Text claims by the vendor, no behaviour observed. |
| [nc] | Naobi: `crates/naobi-ui/ui/app.slint`, `naobi_core::ui`, and frames from the headless UI check (`NAOBI_UI_CHECK_DIR`) at commit `51a080d` | Read and rendered. |

Limits:
- Raycast and the PowerToys binaries are no longer on this machine: the Raycast MSIX is gone, `C:\Program Files\PowerToys` holds only empty folders, and winget has no Raycast package. No shipped binary was analysed this round, so REA was not used. PowerToys does not need it, because its source is public.
- Nothing here was measured at runtime. Motion values are the numbers written in the source, not observed frames.
- Marked **inferred** where the statement is a reading of the code rather than a literal value.

## 1. Launcher layout

| Element | Command Palette [cp] | Raycast Windows [rc-rea] | Naobi today [nc] |
|---|---|---|---|
| Window | set by the app, not in these files | 750 x 475 | 750 x 475 |
| Search area | field min 32, 12 px padding above and below | 66 | 66 |
| Result row | 44 | 40 | 40 |
| Row icon | 20 in a 28 px column, 12 px gap | 22 (16 compact) | 22, 12 px gap |
| Title / subtitle | 14 / 14 on one line, subtitle in secondary ink | body 13 | 13 Inter / 12 JetBrains Mono |
| Section header | **28**, caption size, secondary ink, 8 px above, not focusable | not recorded | **40**, same as a row |
| Row accessory | right-aligned tags, at most 3 then a `+N` badge, 4 px apart, 8 px from the edge | not recorded | hint chip, but not right-aligned (section 4) |
| Bottom bar | min 40, 4 px padding | 42 | 42 |
| Corner radius | control radius from the theme | 8 | 8 window, 2 row |

## 2. Interaction patterns

### Bottom bar [cp]
- Left: Settings on the root page. On any other page: a status badge and the **page title**.
- Right, in order: primary action, secondary action, More. Each is **label first, then key caps**. Labels are capped at 160 px and elide.
- Key caps: 4 px padding, radius 6, 1 px divider border, 10 px text, one cap per key (`Ctrl` and `Enter` are separate caps).

### Action menu [cp]
- Min width 248. Rows pad 12 x 8, with a 32 px icon column (16 px icon), title capped at 200 px, shortcut right-aligned in caption ink, 16 px between them.
- Four row kinds: normal, **critical** (icon, title and shortcut all in the critical color), separator (1 px), and a **back row** naming the current submenu.
- The filter field can sit at the top or the bottom of the menu; the caller picks by where the menu opens. The bottom-bar path and the anchored path pass different locations.
- Keys: Up and Down move while the filter keeps focus, Enter runs, Ctrl+Enter runs the secondary, Esc closes, Alt+Left leaves a submenu, Tab moves focus.
- A hidden assertive live element announces the opening and the selected item to Narrator.

### Search field and navigation [cp]
- Esc has four settings; the default clears the text first, then goes back.
- **Backspace on an empty field goes back once per press.** A flag set while Backspace is held stops a held key from deleting the text and then also leaving the page.
- A back button appears left of the field on sub-pages, faded in with 187 ms and 333 ms values.
- Progress bar sits directly under the search row, fading with 333 ms.

### Direct access [cp]
- Alt+1 to Alt+9 acts on the Nth interactive item; a setting picks run or select, and Alt+Shift+digit selects.
- Number cues are drawn over the rows only while access-key mode is active, in one lazily created overlay, so normal list rendering pays nothing.

### Details and feedback [cp]
- Details pane: a column beside the list with 16 px margin, optional hero image, title, body, then key/value metadata, tags, links and separators.
- Empty state: 48 px icon, semibold title, secondary subtitle, centered.
- Toast: a small surface at the bottom center, at most 560 wide, 16 px icon, message, and at most one command button.
- List containers have an **empty transition collection**: rows do not animate when results change.

### Raycast
- One keyboard grammar: Enter primary, Ctrl+Enter secondary, Ctrl+K all actions, Esc back or close [rc-doc].
- Hover highlights and pointer cursors are deliberately avoided to feel native [rc-doc].
- Action Panel search ranks complete-word matches higher; root search groups commands when the field is empty; recent searches keep their results [rc-log].
- In-window toasts and HUDs can be dismissed and can show pending and progress states [rc-log].
- Details panel carries file metadata such as image dimensions [rc-doc].

## 3. What both products agree on

1. Results update with no list animation; speed is the feel.
2. The bottom bar states what Enter will do, and names the current place.
3. The action menu has real structure: icons, separators, a distinct destructive style.
4. Section headers are clearly smaller than rows.
5. A secondary pane carries metadata, not only a preview.

## 4. Gaps in the current Naobi Launcher [nc]

| # | Gap | Evidence |
|---|---|---|
| G1 | The row layout does not right-align its trailing parts. The engine sends no hint or pinned flag today, so this shows only in the UI-check fixture, but a right-aligned kind label needs the same fix | `ResultRow` uses `alignment: start` (`app.slint:138`); `engine.rs` `to_row`; `stone-results` frame |
| G2 | Footer chips float at uneven positions, and the label comes after the chip | Footer layouts (`app.slint:569-616`); every frame |
| G3 | The footer always says "Naobi Command"; no view shows where the Owner is | `app.slint:574`; `Screen` has no view title |
| G4 | Section headers are as tall as rows. In the running app only the Files section has a header; the root list is otherwise flat | `HeaderItem` 40 px (`app.slint:79`); `engine.rs` `screen` |
| G5 | Matched letters are not highlighted although mockup screen 2 requires it | No match data in `RowView`; `docs/design/mockups/README.md` screen 2 |
| G6 | `kind` exists but is only read by assistive technology; rows of different kinds look the same | `model.rs` `accessible_label` |
| G7 | Every subtitle is monospace, and a long path loses its end to the ellipsis | `app.slint:193-200` |
| G8 | The action panel has no icons, groups or destructive style | `ActionView` has only label, shortcut, reason |
| G9 | The action panel filter is at the top, far from the footer the eye just left | `app.slint:663-688` |
| G10 | The clipboard preview has no metadata, and an image fills a large near-black block | `PreviewPane`; `stone-clipboard-image` frame |
| G11 | **Inferred:** holding Backspace to clear the query in a sub-view also leaves the view, because a repeat on the now-empty field reaches the core | `app.slint:417` does not tell repeats apart; `engine.rs:455` pops a pushed view on Backspace with an empty query |
| G12 | No direct key for the Nth result | Keyboard map in `DESIGN_SYSTEM.md` |

## 5. Constraints any upgrade must respect

- R-006: keystroke to results p95 <= 25 ms, hotkey to visible p95 <= 50 ms, idle working set <= 40 MB.
- D-034: Slint software renderer, so no blur or acrylic and no large soft shadows.
- R-007: Naobi tokens, paper and stone, Inter and JetBrains Mono, WCAG 2.2 AA, every capability by keyboard.
- `DESIGN_SYSTEM.md` accent discipline: the accent stays rare and small.
- The list scroll math assumes every line is one height (`app.slint:407`), so a shorter section header needs that logic changed with it.
