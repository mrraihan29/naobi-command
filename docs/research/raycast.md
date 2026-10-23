# Raycast for Windows: clean-room behaviour reference

Purpose: behaviour notes for Naobi Command (personal Rust rewrite for Windows 11). Paraphrased from public docs only. No code, assets or long text copied. Retrieved 2026-10-21.

Conventions:
- [W] = Windows-specific or Windows page. [M] = macOS docs used as fallback (label kept). [?] = doc silent or sources conflict.
- Short names for sources are listed in section 13. Every claim carries a source tag.
- Raycast version context: Windows public beta began 2025-11-20 [blog-win]; v2.0 left beta 2026-08-25; latest seen v2.8 on 2026-10-16 [cl-win].

---

## 1. Technology and footprint

### 1.1 Architecture (Raycast 2.0, both platforms)
- Four parts: native host shell, shared web frontend, one Node backend process, Rust core [deep-dive].
- Host shell on Windows: C# on .NET 8 with WPF. On macOS: Swift/AppKit. It creates windows, registers global hotkeys, runs the tray/menu bar, loads the web UI and supervises Node [deep-dive].
- UI: one React + TypeScript codebase for both OSes, rendered in WebView2 (Chromium) on Windows and WKWebView on macOS. Separate entry points per window (launcher, AI Chat, Notes, Settings) [deep-dive].
- Node backend: one long-running process for database access, the extension runtime and other services [deep-dive].
- Rust core: data layer (shared with iOS), cloud sync, and the file indexer, the latter as a separate process [deep-dive].
- Windows detail: acrylic blur with a custom title bar needed close shell/WebView2 coordination; each window gets its own WebView2 environment; they stop Chromium from throttling the hidden/unfocused WebView because Raycast updates in the background [deep-dive].
- Native feel is faked on top of web UI: popovers and tooltips are real native windows that can extend beyond the main window; Settings is its own native window; hover highlights and pointer cursors are deliberately avoided [deep-dive].
- Extensions: React + TypeScript on Node, UI described declaratively and rendered by the host; Node is now bundled [deep-dive]. Extensions are not sandboxed for file or network I/O [dev-security].
- IPC: platform message handlers and stdio, typed clients generated from one interface declaration [deep-dive].
- Raycast calls the Windows app built to feel native, not a port [blog-win]. (Marketing claim; the stack above is a web UI in a native frame.)

### 1.2 Published numbers
- v1 (Mac) about 200-300 MB; v2 about 350-450 MB typical, "not final". Hidden-window breakdown for v2 (macOS figures): WebView 120-200 MB, Node 150-200 MB, Swift shell about 40 MB; empty WebView about 50 MB, bare Node about 12 MB [deep-dive][M]. No Windows-specific memory table was published in the sources found.
- Windows changelog claims: v2.6 host process about 50% less memory, each extension process about 30% less at startup, a leak fixed [cl-2-6]; v2.6 file indexer about 60% less memory while updating its index [cl-2-6]; v2.7 file search peak memory down by up to about 80%, search-time memory down up to about 95%, searches about 20-30% faster [cl-2-7]; v2.8 file search up to 6x faster, lists scroll up to 2x faster, startup memory about 20% lower, extension installs about 4x faster [cl-2-8].
- Independent: a Nov 2025 Japanese comparison estimated Raycast beta at roughly 300-500 MB vs Flow Launcher about 150 MB, explicitly from community talk and not its own measurement [zenn]. I found no independent latency (hotkey-to-paint) benchmark. Treat all as vendor claims or hearsay.
- File indexer: Rust, scans directly rather than using Spotlight, kept current via file system events; on Windows it reads the NTFS Master File Table to index a whole drive "in seconds" [deep-dive]. The launch post says Windows lacks a system index meeting their bar, so they built their own real-time indexer [blog-win].

Takeaway: Raycast spends roughly 0.3-0.5 GB RAM for a web-UI launcher. A Rust-native launcher can credibly beat this; budget and measure idle working set and hotkey-to-first-paint yourself.

---

## 2. Floating window UX

- Opens on a global hotkey. Default is conflicting in sources: Settings page says a fresh Windows install defaults to the Windows key, with a "Use the Windows Key" toggle [man-settings][W]; the shortcuts page lists Alt+Space [man-keys][W]; a third-party guide also says Alt+Space and mentions conflicts with other launchers [windowsmode]. [?] Likely changed over versions; make it configurable and check conflicts.
- Where it appears: Settings > Launcher > "Show Raycast on" = screen with mouse, screen with active window, or primary screen [man-settings].
- Size: Interface Size (Default, Large, Larger) and Window Mode (Compact or Expanded) [man-settings]. Compact is a stripped-back layout; "Show Favorites in Compact Mode" exists [man-settings]. Compact mode expands the window when a required command argument must be focused [cl-2-6]. Exact pixel size of the window not published [?].
- Return to root: "Pop to Root Search" resets to root immediately or after an inactivity delay up to 180 s [man-settings].
- Esc: clears the search text; if already empty, closes Raycast. Backspace on empty input navigates back [man-search]. Shortcuts page also says Esc goes back and from root closes; Cmd/Shift+Esc pops to root [man-keys]. A Keyboard setting "Escape key behavior" exists [man-settings] (content not documented in what I read).
- Empty search state: shows recently used files (if file search enabled) [man-search]; v2.8 groups commands by extension when the field is empty [cl-2-8]. Recent searches keep their results so repeating is faster [cl-2-6].
- Search history: Up/Down at list edges cycles search history [man-keys].
- Loading states (extension API): `isLoading` shows a loading bar under the search bar; detail pane has its own bar [dev-list]. Empty view with icon/title/description/actions appears when no items or no match, and is suppressed while loading with an empty query [dev-list]. Root Search "Indexing Files..." row appears while the file index builds [cl-2-8].
- Result stability: v2.6 fixed results lagging or going stale while file indexing was busy [cl-2-6]; recent files vanishing during an index update was also a bug [cl-2-6]. Lesson: never block or blank the list on indexer work.
- Visual: acrylic backdrop on Windows [deep-dive]; follows system light/dark [man-settings]; contrast auto-adjusted in v2.6 [cl-2-6].
- Focus quirks fixed over time (useful test list): hotkey no longer shows Raycast over open/save dialogs, Alt-based hotkeys no longer steal focus from text fields, Alt+F4 no longer quits Raycast when hidden, unresponsive after idle/wake [cl-2-7]; sleep/wake detection on laptops [cl-2-0]; recovery if the WebView2 process crashes [cl-2-8].
- Elevated apps: a Windows-only setting "Run Accessibility Helper Elevated" and "Open Folders in Tabs" exist [man-settings]. v2.7 fixed minimize/restore for apps running as administrator [cl-2-7]. Implication: window management on admin windows needs an elevated helper.

---

## 3. Root search / launcher

### 3.1 What it indexes
- Apps (Windows: Start Menu and desktop apps; Mac: also symlinks/aliases and later-mounted drives) with alternate names matching [man-search]. Windows apps page documents Run as Administrator, Run as Different User, Treat as Game, Microsoft Store uninstall behavior, and a "Deep Search" up to 3 folder levels in user-added scopes [man-apps][W]. Installed games (Steam, Epic, Riot, Ubisoft) are indexed and can be shown in File Explorer [blog-win][cl-2-0].
- Commands: built-in, extension, Script Commands, AI Commands [man-search].
- Files and folders only if File Search indexing is on; there is a setting to include files in Root Search (Settings > Launcher > Include Files in Root Search) [man-search][man-file-md].
- Quicklinks, Settings, system settings panes, Windows settings, web URLs, calendar events (with access) [man-search]. Windows: root search covers every setting in the Windows settings catalog and Control Panel tasks [cl-2-0].
- Inline results: calculator expressions and color values with preview [man-search]. Typing a file path offers an "Open in File Explorer" command [man-file-md].
- Quick AI hint can be shown or hidden in Root Search; Tab starts Quick AI from the query [blog-win][cl-2-6][windowsmode]. Bug fixed where Tab both opened a file and started Quick AI [cl-2-0] (key overloading hazard).
- Favorites: pin items in Root Search (Ctrl+F or Ctrl+Shift+F, conflicting pages), reorder with Ctrl+Arrow [man-actionpanel][man-keys].

### 3.2 Ranking
- Documented order: exact alias match, alias prefix match, title fuzzy score, subtitle/keyword matches, then frecency [man-search].
- Frecency blends how often and how recently you pick a result for a given query, so frequent picks rise even for shorter queries [man-search].
- "Reset Ranking" action clears learned frecency for one command [man-search].
- Fuzzy strictness: Root Search Sensitivity. Search page says Low/Medium/High; Settings page says Low (more results) or High (fewer, closer) [man-search][man-settings]. [?] Treat as a 2-3 level knob.
- Extension list fuzzy default: matches title and `keywords` [dev-list] (same mental model for command keywords).
- File search got typo tolerance and exact-term boost: v2.3 ranks exact term matches higher, v2.8 handles typos much better [cl-win][cl-2-8].

### 3.3 Aliases and hotkeys
- Alias set via Configure Command from the action panel, or Settings > Shortcuts. Allowed: lowercase letters, digits, spaces (auto-lowercased). Matching is strict, not fuzzy: exact ranks highest, prefix lower; alias + space focuses the first argument field [man-aliases].
- Hotkeys are global and launch a command even when Raycast is closed. Recorder auto-saves after about 1.5 s or on Enter, flags conflicts in red, Backspace clears [man-aliases].
- Hotkey kinds: normal combos; multi-modifier combos; double-tap modifier; single-tap modifier (left/right distinguished); both-sides-held modifier; certain single keys with no modifier [man-aliases][cl-win v0.71].
- Hotkeys record physical keys by default (layout-independent). Built-in Raycast shortcuts use key equivalents [man-aliases].
- Conflicts: the main launcher combo cannot be reused for a command; a combo used by another command is overwritten (previous loses it); some pairs cannot coexist (single Alt tap vs double Alt tap) [man-aliases].
- Windows bugs worth pre-empting: Copilot key toggling Raycast (v2.3), recorded shortcuts turning into modifier-only (v2.6), double-tap keeping the side selection (v2.6) [cl-win][cl-2-6].
- Settings > Shortcuts is a central, filterable view of all hotkeys and aliases [man-settings]. Number hotkeys work with numpad [cl-2-6].

### 3.4 Action Panel (Ctrl+K pattern)
- Open: Ctrl+K (Windows) / Cmd+K (Mac). Enter runs primary action (first in list) without opening it; Ctrl+Enter secondary; Ctrl+Shift+Enter tertiary (Mac uses Opt+Enter) [man-actionpanel][man-keys][W].
- Panel has a "Search for actions..." fuzzy filter, flat list while filtering; Esc closes the panel or backs out of a sub-menu. Some actions open inline views (e.g. hotkey recorder) [man-actionpanel].
- Actions are grouped into sections that vary by item type; extension authors can mark actions destructive [man-actionpanel].
- Standard item actions with consistent chords: Open Ctrl+O, Reveal in File Explorer Ctrl+Shift+O, Open With Ctrl+Alt+O, Quick Look Ctrl+Y, Edit Ctrl+E, Info Ctrl+I, Duplicate Ctrl+D, Pin Ctrl+., Show hidden Ctrl+Shift+., Copy Ctrl+C, secondary copy Ctrl+Shift+C, Paste Ctrl+V, Delete Ctrl+X, Delete all Ctrl+Shift+X [man-keys][W].
- Per-item management chords: Configure Command Ctrl+Shift+, ; Disable Command Ctrl+Shift+D ; Settings Ctrl+, . The manual contradicts itself on a few of these between pages [?]; verify before copying [man-actionpanel][man-keys].
- Forms: Ctrl+Enter submit, Tab/Shift+Tab fields, Esc cancel [man-keys].

### 3.5 Navigation stack
- Extension API: views are pushed onto a stack; Esc automatically pops to the previous view [dev-nav]. Combined with root behavior: Esc pops, empties text, or closes at root [man-search].
- Shift+Esc / Cmd+Esc pops straight to root [man-keys]. Ctrl+W closes window [man-keys].
- List keys: Up/Down; Alt+Up/Down page; Ctrl+Up/Down section; Ctrl+N/P default alternative; optional Vim (Ctrl+J/K/L/H) via Settings > Keyboard > Navigation Bindings [man-keys]. Shift+Tab goes to the parent directory when browsing files [man-search].
- Left/Right move between command argument fields [man-search]. Up to 3 `{argument}` placeholders per quicklink/snippet [man-placeholders].
- Virtual lists support Page Up/Page Down (v2.7) [cl-2-7].

### 3.6 Fallback commands
- Shown when Root Search has no match. Customizable since Windows v0.40 (2025-12-20): manage which commands appear and their order; available pool includes all Quicklinks and Script Commands that take one argument [altto-fallback][cl-win].
- Manage via the "Manage Fallback Commands" command or the gear in the no-results header [altto-fallback]. Settings > Launcher has a Fallback Commands entry [man-settings]. Script Command fallbacks receive the search query as their first argument [cl-2-6]. Quick AI can be set as fallback [man-new-v2].
- Official fallback manual page returned 404 for the URL I tried; details come from the AlternativeTo write-up of the changelog [?].

---

## 4. File search

- Two layers, per docs: Raycast's own index for names/paths, and the OS index only for optional "Content Search". Windows content search relies on Windows Search with file types set to "Index Properties and File Contents"; the toggle is greyed out if the folders are not in the Windows index [man-file-md][W].
- The name index is Raycast's own, built because Windows lacks a suitable system index [blog-win]; Rust process, MFT-based on NTFS for fast full-drive scans, FS-event updates [deep-dive]. File Search page does not itself mention MFT [?].
- Scope: home folder by default (plus /Applications on Mac); add Search Scopes for other folders; on first run Raycast asks for the home folder and warns if you grant only subfolders [man-file][M for /Applications].
- Ignore rules: hidden files excluded by default (toggle "Include Hidden Files"); honors .gitignore, .ignore, git exclude and its own `.rayignore` (nearest-file semantics, "Exclude from Index" action) when "Use Ignore Files" is on; auto-filters node_modules, *.tmp, system caches; skips caches/sessions of dev tools (e.g. .npm, .cargo/registry, .venv, .cargo/global-cache); global comma-separated ignore globs; auto-exclusion list for folders that apps rewrite too often [man-file][cl-2-7].
- Volumes: "Index Network & Removable Drives" (v2.5 added the setting to disable network/removable/virtual indexing) and "Keep Files from Removed Volumes" [man-file][cl-win].
- Resource controls: Stop Indexing from the toast; Index Files forces rescan; Reset to Defaults reindexes; optional low-disk-space block; changes made while Raycast was closed are picked up at launch (fixed in v2.8) [man-file][cl-2-8].
- UI: empty query shows Recently Used; folder view sort by Date Modified/Created/Name (remembered); details panel shows metadata (image dimensions since v2.7); Quick Look; Open in Terminal for folders; Save as Quicklink; Share; Toggle Hidden Files; Send to AI [man-file][cl-2-7].
- Index is local and never synced to Raycast servers [man-file].
- Search syntax: no query operators documented besides `text:` for OCR text; Windows applicability unclear [man-file-md][?].

---

## 5. Calculator

Source: manual calculator page (cross-platform, uses Ctrl/Alt chords for Windows) unless noted.
- Units: `10ft in m`; CSS units `2rem in px`, base size configurable (default 16); `2 inches in px at 72 ppi` [man-calc].
- Currency: `100 usd in gbp`; `K` shorthand; crypto mentioned in intro without detail [man-calc]. Network: the manual says nothing about offline behavior [?]. Evidence for network use: rates update in the background, and a "Refresh Currencies" action exists only on currency results (Ctrl+R in extension page) [calc-ext][cl-0-48]; a stale-rate bug was fixed in v2.5 [cl-win]. Offline fallback behavior is undocumented.
- Percent: `52% of 900`, `20% off 80` [man-calc].
- Math words and functions: `square root of 625`, `2 power 10`, trig/hyperbolic functions [man-calc].
- Dates and durations: `monday in 3 weeks`, `days until 31 Mar`, `August 5 + 5`, `3:45pm + 5`, ISO-8601 timestamps, `145 mins to timespan`, `workhours in 2023`, `55h in workdays` [man-calc].
- Time zones: `5pm ldn in sf`, `time in tokyo`, `time diff Paris`; cities, airports and landmarks resolve; adding time to a city clock stays in that zone across DST [man-calc]. Keywords `now`, `today`, `tomorrow`, `yesterday`, `time` show results alone; v2.3 restored results for these words and suppressed results for plain bare numbers [man-calc][cl-win].
- Colors: hex (3/4/8), rgb/rgba, hsl/hsla, hwb, lab, lch, oklab, oklch; named colors not converted; Enter on a color opens "Copy Color as..." [man-calc].
- Actions: Enter copies answer; Ctrl+Enter pastes into the active app; Ctrl+Alt+C copies unformatted; Ctrl+Alt+Shift+C copies question and answer; swap-units action [man-calc][cl-0-48].
- History: browse, search, pin; entries kept 3 months, pinned forever; currency and time conversions update live; results saved even when window hidden (v2.0 fix) [cl-0-48][cl-2-0].
- Settings (Settings > Extensions > Calculator): syntax highlighting, color conversion, REM/EM base, decimal separator from system locale [man-calc].
- Not documented: variables, user functions [?].

---

## 6. Clipboard history

- Types: text, images, files, links, emails, colors; keeps every original format so "Paste as..." can pick plain/rich/RTF/HTML; items copied together form one entry [man-clip]. Mac v1 page listed text, images, colors, links [clip-core][M].
- Retention: 1 day, 1 week, 1 month, 3 months free; 6 months, 1 year, unlimited need Pro and must be chosen manually [man-clip]. Bulk delete by window (5/15/30 min, 1 h, 24 h) or all [man-clip].
- Pin: Ctrl+. on Windows (pinned items survive cleanup) [man-clip][clip-core].
- Ignore: Settings > Clipboard History > Disabled Applications; defaults include password tools (Keychain on Mac); add more with + [man-clip]. Separately, passwords copied from password managers and other "transient" clipboard data are ignored by default, per the Mac product page [clip-core][M]. Mechanism details (clipboard format flags) not documented [?].
- Encryption claim (Mac pages): content encrypted on local disk, never leaves the computer [clip-core][M]. Developer docs: local-first, local encrypted database; secure data in macOS Keychain; Windows credential storage not described [dev-security][M]. The current manual page says nothing on encryption [man-clip]. Cloud Sync does not sync clipboard history (per user report, not an official quote) [?].
- Limits: current manual states no item or file-size limit. A snippet from a search result cited a 32,768-character text cap and the legacy Mac v1 page cited 256 entries; neither confirmed on the current page [?][M].
- Actions: Enter pastes into the active app (default Primary Action setting; alternative: copy to clipboard); Ctrl+Enter does the other; Paste as Plain Text, Paste as PNG; Paste Sequentially with Reset Paste Sequence (v2.8 fixed repeated entries) [man-clip][cl-2-8]. Save as File / Save as Snippet exist [cl-win].
- Search and filters: search entries by name; Ctrl+P filters by Text, Images, Files, Links, Emails, Colors; OCR-based search via AI "Ask Clipboard" [man-clip].
- Entry point: dedicated alias/hotkey recommended; opened as an ordinary command in the launcher, not a separate app [man-clip].

---

## 7. Window management

- Size/state: Maximize, Maximize Height/Width, Toggle Fullscreen, Reasonable Size (60% of screen up to 1025x900), Restore (previous size and position) [man-wm].
- Halves (L/R/T/B), thirds (first, first-two, center, last-two, last), fourths, quarters, sixths [man-wm].
- Position: Center (keeps size), Move Up/Down/Left/Right to edge; Move/Resize to exact values (x,y; width,height) added in v2.2 [man-wm][cl-win].
- Displays: Previous/Next Display; Previous/Next Space (Windows: virtual desktops with Open/Close/Rename/Move to Desktop 1-9); Minimize on This Display; per-display Show Desktop (v2.3); focus-cycling commands for app windows [man-wm][cl-win].
- Settings: gap between windows and screen edge; Left/Right Half can cycle sizes or hop screens. Windows only: Auto Close Empty Desktops. Mac only: Stage Manager, presets [man-wm][M].
- Custom commands: size, anchor and offsets in points or percent (offsets may be negative, sizes not); gaps not applied to custom commands; Duplicate a built-in into a fixed-layout custom command [man-wm].
- Layouts: up to 8 windows per display; each entry may launch a Quicklink, file, URL or app with args; leftover windows can be ignored, minimized or closed (close goes via the app so save prompts still appear); "Save Current Layout" and "Create Layout From Current Windows" (v2.2) capture the current arrangement [man-wm][cl-win].
- Deeplinks trigger custom commands with parameters; layouts by name only [man-wm].
- Windows snapping: adjacent tiled windows share a draggable divider that resizes both; snapped windows keep layout on switch (v2.7) [man-wm][cl-2-7].
- Switch Windows: lists all windows, search by app name or title; actions without focusing (minimize, close, fullscreen); Mac needs Accessibility, Windows runs an accessibility helper [man-nav][man-settings].
- Known edge cases from fixes: Chrome profile windows without a desktop shortcut, windows pinned to all desktops ("not on a valid desktop" error), crash on Windows before 24H2, next/previous desktop and display, switching must actually focus the target [cl-2-8][cl-0-48].

---

## 8. Quicklinks

- A quicklink is a named URL, deeplink or file path with optional Open With app; default opens in the default browser [man-ql].
- Query placeholder syntax: `{argument name="query"}`, e.g. in a search URL; bare `{argument}` is also valid, up to 3 arguments, optional `default="..."` and `options="a, b"` choices [man-ql][man-placeholders]. The docs use "argument" rather than a literal `{query}` token.
- Dynamic placeholders usable in the link: `{clipboard}` (with offset), `{selection}`, `{date}`/`{time}`/`{datetime}`/`{day}` with offset, format and locale, `{calculator}`, `{browser-tab}`, `{uuid}`, `{snippet name=...}`, `{cursor}`. Modifiers chained with `|`: uppercase, lowercase, trim, percent-encode, json-stringify, raw [man-placeholders].
- Hotkey invocation can pass selected text as the argument for single-argument links [man-ql]. v2.8 fixed an "Invalid URL" error when `{selection}` could not resolve [cl-2-8].
- Root Search flow: type alias or name, then space moves focus into the argument field; focus is released after running (v2.7 fix) [man-aliases][cl-2-7].
- Import/export JSON (`name`, `link` required; `iconName`, `openWith` optional); duplicates (same title and content) skipped with a count [man-ql]. Library of hundreds of prebuilt links [man-ql].
- Windows: native paths like `C:\Projects\my-app`; Ctrl chords for save/action panel/tag filter [man-ql][W]. Favicons preserved when Open With is a browser (v2.7) [cl-2-7].

---

## 9. System commands

- Manual lists one set for both platforms with "a few platform-specific extras", without labelling which is which; one-shot, no confirmation screen except where noted [man-sys].
- Power/session: Lock Screen, Sleep, Hibernate, Sleep Displays, Restart, Shut Down, Log Out, Show Screen Saver, Toggle Energy Saver [man-sys][cl-2-7].
- Audio: Play/Pause, next/previous, Toggle Mute, Volume Up/Down, Set Volume presets, Volume Mixer, per-app volume, Toggle Microphone Mute, Toggle Camera Mute [man-sys][cl-2-7].
- Display: Show Desktop, Toggle System Appearance (light/dark), Toggle HDR, Toggle Night Light (Stage Manager is Mac-only) [man-sys][M for Stage Manager].
- Files: Open Trash/Recycle Bin, Empty Trash (warning option; default not stated), Eject All Disks, Toggle Hidden Files [man-sys].
- Apps: Quit All Apps / Except Frontmost (warning on by default, exclusion list), Hide/Unhide, Dismiss Notifications [man-sys].
- Connectivity: Wi-Fi, Toggle Wi-Fi, Bluetooth, Toggle Bluetooth, Toggle Airplane Mode. Windows v2.7 added WiFi, Toggle WiFi, Bluetooth, Toggle Energy Saver, Toggle Camera Mute; v2.4 added Toggle Airplane Mode and several toggles confirm new state; v2.8 hides Wi-Fi/Bluetooth commands when no adapter exists [cl-win][cl-2-8].
- Reliability notes: Night Light toggle needed repeated fixes; Bluetooth toggle timeouts; Wi-Fi commands need the WLAN AutoConfig service [cl-2-0][cl-2-7][cl-2-8].

---

## 10. Settings window structure

Opened with Ctrl+, from the launcher or by typing "Settings"; it is its own native window [man-settings][deep-dive]. Tabs [man-settings]:
- Account: plan, sign in.
- General: Open at Login, tray toggle, Raycast hotkey, Use the Windows Key (W), Appearance (theme, Interface Size, Window Mode, favorites in compact).
- Launcher: Show Raycast on, Pop to Root Search, Root Search Sensitivity, Search History, Customize Search (Files, Contacts), Fallback Commands.
- Shortcuts: all hotkeys and aliases, filter by category.
- Keyboard: Hyper Key, Escape behavior, auto-switch input source, Navigation Bindings (Emacs or Vim), page navigation keys.
- Advanced: Favicon provider, Open Folders in Tabs (W), Run Accessibility Helper Elevated (W), export/import settings, Connection (proxy, certificates).
- Organizations, About, AI, Applications (Auto Quit, search scopes, per-app alias/hotkey), Extensions (per-extension preferences and per-command toggles).
- Windows grouping: settings grouped by Windows' own categories and filtered by device capabilities; opening a setting from elsewhere scrolls to and highlights it (v2.0, v2.1, v2.7) [cl-win]. Settings has its own search covering extensions, commands and settings [man-new-v2][M].
- Proxy: honors HTTP_PROXY / HTTPS_PROXY / NO_PROXY since v2.8 [cl-2-8].
- Cloud sync, import/export are separate manual topics [man-home].

---

## 11. Windows vs macOS differences (documented)

| Area | Windows | macOS | Source |
|---|---|---|---|
| Host shell | C# .NET 8 WPF | Swift AppKit | [deep-dive] |
| Web view | WebView2 (Chromium) | WKWebView | [deep-dive] |
| Default hotkey | Windows key or Alt+Space ([?] sources disagree) | Opt+Space; "Replace Spotlight" | [man-settings][man-keys] |
| File index | Rust indexer, NTFS MFT | Rust indexer over FS events; Spotlight for content | [deep-dive][man-file-md] |
| Content search | Windows Search index (manual setup) | Spotlight | [man-file-md] |
| Spaces | Virtual desktops, Auto Close Empty Desktops | Spaces, Stage Manager | [man-wm] |
| Elevation | Run as Admin / Different User, elevated accessibility helper | n/a | [man-apps][man-settings] |
| Permissions | Accessibility helper, no TCC prompts | Accessibility, Full Disk, Calendar prompts | [man-nav][man-file-md] |
| AI fallbacks Ask File Search / Ask Finder | Mac only | yes | [man-file-md] |
| Pro features | Not all Pro features on Windows at beta launch; Notes and AI Chat arrived later | n/a | [blog-win][cl-win] |
| Extensions | Hundreds worked at beta; API is cross-platform | thousands | [blog-win] |
| Windows-only | Games listing, Windows settings/Control Panel search, Night Light, virtual desktop commands, divider snapping | | [cl-2-0][man-wm] |

Shortcut mapping: Cmd becomes Ctrl; Option becomes Alt; Ctrl+Shift+Enter replaces Opt+Enter for the tertiary action [man-keys].

---

## 12. UX patterns

### Worth adopting
1. One keyboard grammar everywhere: Enter = primary, Ctrl+Enter = secondary, Ctrl+K = all actions, Esc = back/clear/close. Primary action is the first item in the panel [man-actionpanel][man-keys].
2. Esc ladder: clear query, then pop view, then close. Backspace on empty input goes back [man-search].
3. Consistent per-item chords (open, reveal, copy, pin, delete) so muscle memory transfers across result types [man-keys].
4. Alias-first ranking with strict (non-fuzzy) alias matching and a space-to-argument handoff; power users reach a command in 2-3 keys [man-aliases].
5. Frecency keyed by query, with per-item Reset Ranking [man-search].
6. Never blank or lag the list while indexing; show a thin indicator instead [cl-2-6][cl-2-8].
7. Fallback commands for zero results, user-ordered, passing the query as argument [altto-fallback][cl-2-6].
8. Pop to Root after inactivity with a timer, so reopening is predictable [man-settings].
9. Empty-query state with recents and grouped commands; remembered recent searches with cached results [man-search][cl-2-6][cl-2-8].
10. Configurable hotkeys with conflict warning, physical-key recording, double-tap modifier support [man-aliases].
11. Inline calculator/unit/time zone answer in the root list with copy/paste actions [man-calc].
12. Clipboard defaults: ignore password managers, per-app disable list, filter by type, pin, paste-as-plain [man-clip].
13. Ignore-file support (.gitignore plus a tool-specific file) and built-in noise exclusions for file search [man-file].
14. Clear toggles for unreliable system actions: hide commands when hardware is absent, confirm new state [cl-2-4 via cl-win][cl-2-8].
15. Window layouts capture current state ("save what I see") [man-wm][cl-win].
16. Export/import settings [man-settings].

### Patterns to avoid
1. Overloading Tab (opens file and starts AI at once was a shipped bug) [cl-2-0].
2. Heavy runtime for a launcher: three runtimes (WPF, WebView2, Node) cost hundreds of MB and extra recovery logic for a crashed web view [deep-dive][cl-2-8].
3. Doc and UI drift on hotkeys: default launcher key and several action chords are inconsistent across Raycast's own pages [man-settings][man-keys][man-actionpanel].
4. Sharing the Windows key or Alt+Space default with other tools (Copilot key, PowerToys Run) without a conflict check [cl-win][windowsmode].
5. Depending on Windows Search for anything critical; content search needs manual OS index setup and silently disables itself [man-file-md].
6. Alt-based global hotkeys that steal focus from text fields or appear over file dialogs [cl-2-7].
7. Stale or lagging results while the indexer works [cl-2-6].
8. Unclear destructive-command confirmation: Restart/Shut Down confirmation behavior is undocumented, Empty Recycle Bin default unstated [man-sys].
9. Silent failures for hardware-dependent toggles (Wi-Fi service stopped, no Bluetooth adapter) [cl-2-8].
10. Using modifier-only hotkeys without a recorder that tracks left/right and release order; this was a repeated bug source [cl-2-6].
11. Opaque free/Pro split for core features (clipboard retention beyond 3 months is Pro) [man-clip]; not applicable to a personal tool, noted only for contrast.

---

## 13. Open questions (docs silent)

- Exact launcher window size and position on Windows, and the real default hotkey in the current build.
- Whether currency conversion works offline from cached rates.
- Clipboard: Windows storage encryption, size and count limits, and how "transient" data is detected.
- Windows file search: how name search interacts with Windows Search (content only per docs) and exact MFT/USN journal strategy beyond the engineering blog.
- Which system commands prompt before Restart / Shut Down / Empty Recycle Bin.
- Independent latency measurements for hotkey-to-paint and cold start.

---

## 14. Sources

- [blog-win] https://raycast.com/blog/raycast-for-windows (2025-11-20)
- [deep-dive] https://www.raycast.com/blog/a-technical-deep-dive-into-the-new-raycast
- [cl-win] https://www.raycast.com/changelog/windows (index of v0.71 to v2.8)
- [cl-2-0] https://www.raycast.com/changelog/windows/2-0
- [cl-2-6] https://www.raycast.com/changelog/windows/2-6
- [cl-2-7] https://www.raycast.com/changelog/windows/2-7
- [cl-2-8] https://www.raycast.com/changelog/windows/2-8
- [cl-0-48] https://www.raycast.com/changelog/windows/0-48
- [man-home] https://manual.raycast.com/windows
- [man-search] https://manual.raycast.com/search-bar
- [man-actionpanel] https://manual.raycast.com/action-panel
- [man-nav] https://manual.raycast.com/navigation (covers Switch Windows only)
- [man-aliases] https://manual.raycast.com/command-aliases-and-hotkeys
- [man-keys] https://manual.raycast.com/keyboard-shortcuts
- [man-settings] https://manual.raycast.com/settings
- [man-file] / [man-file-md] https://manual.raycast.com/file-search and https://manual.raycast.com/file-search.md
- [man-calc] https://manual.raycast.com/calculator
- [man-clip] https://manual.raycast.com/clipboard-history
- [man-wm] https://manual.raycast.com/window-management
- [man-ql] https://manual.raycast.com/quicklinks
- [man-placeholders] https://manual.raycast.com/dynamic-placeholders
- [man-sys] https://manual.raycast.com/system-commands
- [man-apps] https://manual.raycast.com/applications
- [man-new-v2] https://manual.raycast.com/new-in-v2 (Mac-focused)
- [dev-list] https://developers.raycast.com/api-reference/user-interface/list
- [dev-nav] https://developers.raycast.com/api-reference/user-interface/navigation
- [dev-security] https://developers.raycast.com/information/security (macOS-centric)
- [clip-core] https://raycast.com/core-features/clipboard-history (Mac)
- [calc-ext] https://www.raycast.com/extensions/calculator
- [altto-fallback] https://alternativeto.net/news/2025/12/raycast-adds-customizable-fallback-commands-and-new-system-control-options
- [windowsmode] https://www.windowsmode.com/raycast-for-windows/amp/
- [zenn] https://zenn.dev/yahat/articles/c928bf7a5d1fff
- [altto-launch] https://alternativeto.net/news/2025/11/raycast-for-windows-launches-public-beta-with-native-search-and-extension-support/
- Not retrievable: https://manual.raycast.com/fallback-commands (404), Raycast Windows Beta Manual on Notion (empty), Troubleshooting page (sections empty).
