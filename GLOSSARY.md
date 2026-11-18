# Naobi Command

A personal Windows 11 launcher and utility suite for one owner. It replaces Raycast and PowerToys with one resident Rust process. This glossary fixes the words used in the ledger, diagrams, report and code.

## Language

### People and product

**Owner**:
The single Windows 11 user who installs and uses the app; also the only decider.
_Avoid_: user, customer, admin, account

**Module**:
One of the six built-in capabilities: launcher, calculator, clipboard history, color picker, always-on-top, awake. Phase 2 modules do not ship in the MVP.
_Avoid_: plugin, extension, feature, tool

**Launcher**:
The floating window the Owner opens with the launcher hotkey; it hosts the root search and every view pushed on top of it.
_Avoid_: palette, run box, spotlight, popup

**Hotkey**:
A global key chord that starts a Module action from any foreground app.
_Avoid_: shortcut, keybinding (those mean in-window keys)

**Hotkey conflict**:
A Hotkey that Windows refuses to register because another process owns it.
_Avoid_: collision, hijack

### Launcher

**Root search**:
The single text field at the root of the Launcher that searches every kind of Item at once.
_Avoid_: omnibox, command bar

**Item**:
Anything the root search can return: app, file, command, quicklink or calculator result.
_Avoid_: entry (reserved for the clipboard), result (the ranked form of an Item)

**Result**:
An Item as ranked for one query, shown as one row.
_Avoid_: hit, match

**Alias**:
A short exact name the Owner assigns to an Item or a quicklink; an exact alias match always ranks first.
_Avoid_: keyword, prefix, trigger

**Frecency**:
A score combining how often and how recently the Owner opened an Item; it adds a boost of at most 20% of the fuzzy score, so it breaks ties between equal fuzzy matches and never outweighs a clearly better match.
_Avoid_: popularity, usage count, history

**Primary action** / **Secondary action**:
The action Enter / Ctrl+Enter runs on the selected Result.
_Avoid_: default action, alt action

**Action panel**:
The list of every action available for the selected Result, opened with Ctrl+K.
_Avoid_: context menu, more menu

**View**:
A screen pushed on the root search (for example clipboard history). Esc pops it; a view opened directly by its own hotkey is hidden by Esc instead.
_Avoid_: page, mode, screen (screen means a mockup)

**Armed command**:
A system command that has had its first Enter and waits for the second Enter to run.
_Avoid_: pending, confirm dialog

**Quicklink**:
A named URL template with a `{query}` placeholder and an alias, opened in the default browser.
_Avoid_: bookmark, search engine, web search

**System command**:
One of lock, sleep, hibernate, restart, shut down, sign out, empty recycle bin, toggle dark mode, open Windows settings.
_Avoid_: power command, OS action

**Command line**:
The text the Owner types after `>` in the root search; it runs in a new PowerShell console window, or starts a GUI program directly.
_Avoid_: command (a kind of Item), script, shell command

**Run row**:
The first Result of a `>` query; Enter runs its command line.
_Avoid_: execute row, terminal row

**Run history**:
The command lines the Owner ran from `>`, newest first, at most 100 for 30 days; each is an Item, never an entry.
_Avoid_: frecency, recent commands

**Notice**:
A visible message about something that did not work or was refused (hotkey conflict, admin window, file search unavailable, settings parse error). Never silent.
_Avoid_: toast, alert, error popup

### Clipboard history

**Clipboard entry**:
One stored clipboard copy: text, an image up to 10 MB, or a file list.
_Avoid_: clip, snippet, item (item means launcher Item)

**Pinned entry**:
A Clipboard entry the Owner marked to keep; it survives the retention purge.
_Avoid_: favorite, starred, locked

**Retention**:
The rule that purges unpinned entries older than 30 days or beyond 1,000 entries.
_Avoid_: expiry, TTL, cleanup

**Ignored app**:
An app whose copies are never stored (default the Proton Pass desktop app).
_Avoid_: blocklist, denylist

**Exclusion flag**:
A Windows clipboard format (`ExcludeClipboardContentFromMonitorProcessing`, or `CanIncludeInClipboardHistory` set to 0) that tells history tools not to store a copy.
_Avoid_: privacy flag, secret flag

**Paste target**:
The window that was in the foreground when the Owner opened clipboard history; Enter pastes into it.
_Avoid_: previous app, last window

### Window modules

**Work area**:
The part of a monitor not covered by the taskbar.
_Avoid_: screen, desktop, bounds

**Topmost**:
The state of a window kept above all non-topmost windows by always-on-top. Used instead of "pinned" for windows.
_Avoid_: pinned (that word belongs to Pinned entry), sticky, float

**Topmost border**:
The 2 px accent rectangle drawn around a topmost window: oxide in Windows light mode, the pale earth tone in dark mode.
_Avoid_: frame, outline, highlight

**Elevated window**:
A window of a process running at higher integrity than Naobi Command; Windows blocks control of it (UIPI).
_Avoid_: admin app, protected window

**Admin-window notice**:
The Notice shown when a command targets an Elevated window.
_Avoid_: UAC error

### Color picker and awake

**Pick**:
One click of the color picker that copies a color as HEX and records it.
_Avoid_: sample, eyedrop

**Color history**:
The last 20 picked colors, newest first.
_Avoid_: swatches, palette

**Awake mode**:
Off, Indefinite or Timed. Off lets Windows sleep normally; Indefinite and Timed keep the PC from sleeping; Timed ends by itself.
_Avoid_: caffeine, stay-on, keep-alive

**Keep display on**:
The option that makes Awake also keep the monitor from turning off.
_Avoid_: screen on

### Process and storage

**Instance**:
The one running Naobi Command process; a second launch only shows the first Instance's Launcher.
_Avoid_: session, copy

**Pre-warm**:
Creating the Launcher window hidden at startup so showing it creates nothing.
_Avoid_: preload, warm start

**Cloak**:
Hiding the Launcher window with DWM cloaking instead of destroying or minimizing it.
_Avoid_: minimize, close, hide (hide is the user-visible result)

**Settings file**:
The TOML file in `%APPDATA%\NaobiCommand` that holds all Owner settings; it is hot-reloaded and edited by hand in MVP.
_Avoid_: config, preferences, options

**Data store**:
The local SQLite file in `%LOCALAPPDATA%\NaobiCommand` that holds clipboard entries, frecency and color history.
_Avoid_: database server, cache

### Themes

**Theme**:
A named set of colors, corner radii, list density and font families that the app draws with.
_Avoid_: skin, mode, dark mode (dark mode names the system command)

**Built-in theme**:
`paper` (light) or `stone` (dark); read-only, shipped in the app, and always meets WCAG 2.2 AA.
_Avoid_: default skin, system theme

**Custom theme**:
A Theme the Owner keeps as one TOML file in the themes folder; it starts from a Base theme.
_Avoid_: user theme, profile

**Base theme**:
The Built-in theme that supplies every value a Custom theme does not set or sets wrongly.
_Avoid_: parent, template

**Theme slot**:
The `light` or `dark` setting that names the Theme used in that Windows app mode.
_Avoid_: active theme file, variant

**Density**:
The result row height of a Theme: compact (36 px), normal (40 px) or comfortable (44 px).
_Avoid_: spacing mode, size
