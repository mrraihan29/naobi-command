# Upgrade opportunities: PowerToys, Raycast, macOS Spotlight, Slint and Win32 (clean-room)

Purpose: a ranked list of upgrades for Naobi Command after the Phase 2 Commander Suite, measured against what PowerToys, Raycast for Windows and macOS Spotlight ship today, plus platform upgrades (Slint, Win32). Clean-room: behaviour is paraphrased from official docs and changelogs; no code, text blocks or assets are copied. Retrieved 2026-11-21. Builds on, and does not repeat, `powertoys.md`, `raycast.md`, `raycast-rea.md`, `ui-ux-teardown.md` and `commander-features.md`.

Conventions:
- `[id]` tags map to URLs or repo paths in section 9. Claims marked **(inference)** are design synthesis against Naobi's constraints, not something a source says.
- Size: **S** about 1 to 3 days, **M** about a week, **L** several weeks (inference, one owner-developer with agents).
- Constraint shorthand: R-006 budgets (idle working set <= 50 MB, hotkey-to-visible p95 <= 50 ms, keystroke p95_est <= 25 ms, exe <= 20 MB) `[repo-constraints]`; R-010 no sockets except a '>' line, URL opens and the ADR-0008 FX fetch `[repo-ledger][repo-adr8]`; R-011 never elevated `[repo-ledger]`; D-012 RegisterHotKey only, no low-level keyboard hook `[repo-ledger]`; R-005 no plugin system, no AI `[repo-ledger]`; R-004 out-of-scope list (OCR, remapper, PowerRename, Image Resizer, Hosts editor, Peek) `[repo-ledger]`; D-008 zero-cost OSS dependencies only.
- Idle-budget pattern: shell, COM, WinRT and OLE DB work runs in a transient child process so the resident process stays small (as `--search-files` does today) `[repo-files]`.

---

## 1. Already have (do not propose again)

Verified from the repo on `feat/phase2-commander-suite`:

| Area | What exists | Evidence |
|---|---|---|
| Launcher | Root search over apps, files, commands, quicklinks, calculator; exact alias then fuzzy plus bounded frecency; action panel; pushed views with back control; Alt+1 to Alt+9; matched-letter highlighting | `[repo-ledger]` R-012, R-025 to R-028; `crates/naobi-launcher/src/{engine,rank,frecency}.rs` |
| Providers | apps, calc, inline color, dev micro-tools (`uuid`, `ulid`, `b64`/`b64d`, `url`/`urld`, `sha256`, `json` format/minify, `wc`), emoji, files, quicklinks (`{query}`, alias), `>` run line (pwsh), scratchpad, snippets, system, window switcher | `crates/naobi-launcher/src/providers/*.rs` |
| System commands | Lock, Sleep, Sleep Displays, Hibernate, Restart, Shut Down, Sign Out, Empty Recycle Bin, Toggle Dark Mode, Toggle Mute, volume presets 0/25/50/75/100, Copy Local IP, Open Windows/Night Light/Sound/Wi-Fi/Bluetooth Settings, Open Settings File, Pause Hotkeys, Quit | `crates/naobi-launcher/src/providers/system.rs` |
| Window switcher | Focus, Minimize, Close, Force Quit | `crates/naobi-launcher/src/providers/windows.rs`, `crates/naobi-win32/src/window_switcher.rs` |
| Calculator | Arithmetic, `ans` chaining, 100-entry DPAPI history, %, powers, hex/bin, px/rem, symbol stripping, 8 unit categories, timezone/date/epoch, cached FX conversion | `[repo-ledger]` R-014; `crates/naobi-calc/src/*.rs` |
| FX fetcher | Hardened WinHttp to the Frankfurter/ECB allowlist, 24h cache | `[repo-adr8]`; `crates/naobi-win32/src/fx.rs` |
| File index | User-scoped, ReadDirectoryChangesW, disk-backed, Windows Search fallback, Open and Open containing folder actions | `[repo-adr9]`; `crates/naobi-win32/src/dirwatch.rs`; `providers/files.rs` |
| Clipboard | Text, images, file lists, pin, search, paste, copy only, **paste as plain text**, **paste queue** (add, next, clear), save entry as snippet, DPAPI store, ignore list | `[repo-ledger]` R-015; `crates/naobi-clipboard/src/{history,paste}.rs` |
| Color picker | Win+Shift+C overlay with magnifier, HEX/RGB/HSL/OKLCH, 20-color history | R-017, D-023 |
| Always on top, Awake | Win+Ctrl+T with border; Awake indefinite/timed/display | R-018, R-019 |
| Themes | paper, stone, tokyo-haze, neon-night; custom TOML themes; light/dark slots following Windows app mode; Themes view with live preview | R-030 to R-033, D-043, D-046 |
| Shell | Tray only, single instance, pre-warmed cloaked window, autostart, sticky Notices, hot-reloaded `settings.toml` | R-022, R-023, D-007, D-009, D-018 |
| Hotkeys | Fixed slots only: `launcher`, `clipboard`, `color_picker`, `topmost`, plus optional `window_switcher`, `emoji`, `snippets`, `scratchpad` | `crates/naobi-core/src/settings.rs` |
| Platform | Slint 1.18.1, software renderer, accessibility on; Rust 1.99.1 | `Cargo.toml`, `rust-toolchain.toml` |

**Already in progress (not proposed here):** web search in Vivaldi from the Launcher (being built separately for the Owner).

**Removed by decision:** window snapping and zones (D-039) `[repo-ledger]`.

---

## 2. Ranked recommendations (all sources folded in)

Value is judged for one owner who uses the launcher all day and wants to beat Spotlight on Windows.

| Rank | Upgrade | Inspired by | Size | Constraint risk | Value |
|---|---|---|---|---|---|
| 1 | Quick keys: an alias and optional global hotkey for any item or command, with arguments typed after the alias (§3.1) | Spotlight quick keys `[apple-actions]`, Raycast aliases and hotkeys `[repo-raycast]` | M | Low (RegisterHotKey, D-012) | Very high |
| 2 | Slint 1.19 upgrade (§6.1) | Slint changelog `[slint-cl]` | S to M | Low; re-run `naobi-perf all` | High (idle wake fix, accessibility leak fix, color emoji, TextEdit undo) |
| 3 | Browse modes and kind filters: Ctrl+1 to Ctrl+4 scopes, `/pdf`, `kind:` (§3.2) | Spotlight `[apple-keys][apple-narrow]` | S | None | High |
| 4 | Scheduled theme switch (fixed hours or sunrise/sunset from configured coordinates) driving the existing light/dark slots (§4.1) | PowerToys Light Switch `[pt-lightswitch]`, Raycast Auto Dark Mode v2.10 `[ray-cl]` | S | None if coordinates are typed, no location API | High |
| 5 | Same-app window cycling and Restart Frontmost App (§4.2) | PowerToys Window Hopper `[pt-hopper]`, Raycast v2.9 `[ray-cl]` | S | Low | High |
| 6 | Clipboard transforms: paste as .txt/.png file, HTML to Markdown, CSV/XML to JSON (§4.3) | PowerToys Advanced Paste non-AI actions `[pt-advpaste]` | S to M | Low (no AI, all local) | Medium-high |
| 7 | Home on empty query: pinned items then recents (§4.4) | CmdPal Home and quick-access shelf `[pt-cmdpal]`, Spotlight suggestions `[apple-tahoe]` | S | Low | Medium-high |
| 8 | Calculator: color contrast ratio and color mixing; tip, VAT, bill split, fractions, week number, time left (§4.5) | Raycast v2.12 and v2.13 `[ray-cl]` | S | None (offline parts only) | Medium |
| 9 | File row actions: Reveal and select in Explorer, Who is locking this file, Run as administrator (§4.6) | Spotlight Cmd-R `[apple-keys]`, File Locksmith `[pt-locksmith]`, CmdPal Ctrl+Shift+Enter `[pt-cmdpal]` | S to M | Runas launches a UAC prompt for the target only; Naobi stays unelevated | Medium |
| 10 | Full Windows Settings catalog (`$` style) and Windows Terminal profiles as items (§4.7) | CmdPal `[pt-cmdpal]` | S | None | Medium |

Then medium (§5) and big bets (§7), with conflicts in §8.

---

## 3. macOS Spotlight ideas

Benchmark: macOS 27 Spotlight as documented by Apple. Spotlight's inline calculations and conversions are already covered in `commander-features.md` §1 and §3.1 and are not repeated.

### 3.1 Quick keys and actions with inline arguments
- **What:** Spotlight runs actions directly from search; an action that needs input asks for it inline and Return runs it; Esc backs out `[apple-actions]`. A quick key is a short string bound to an action; text typed after it becomes the argument (the guide's example binds a two-letter key to a call action and appends a contact name). Spotlight assigns quick keys automatically after an action is run once, and the user can set or edit their own `[apple-actions]`. Apple's launch material describes hundreds of actions, including third-party ones through App Intents `[apple-tahoe]`.
- **Naobi gap:** aliases exist only on quicklinks; hotkeys exist only for fixed module slots (`settings.rs`). No alias on an app, file, snippet, system command or Awake preset.
- **Fit:** `naobi-launcher` (alias table consulted in the exact-alias tier that R-012/D-016 already define), `naobi-core::settings` (`[aliases]` and `[hotkeys]` keyed by item id), `naobi-app` (register extra RegisterHotKey chords). Arguments after an alias reuse the quicklink `{query}` path. Auto-suggesting a quick key after first use is a Notice or action-panel entry, not a silent write (inference). Size **M**.
- **Constraint risk:** low. Hotkeys stay RegisterHotKey (D-012); conflicts already raise a Notice (D-029). Store aliases in `settings.toml`, not in the DPAPI store, because they are not content. Persisting item ids is fine; persisting typed query text is not (D-014, D-020).
- **Value:** very high. This is the single feature that most makes a launcher faster than Spotlight for a single owner.

### 3.2 Browse modes and kind filters
- **What:** Cmd-1 to Cmd-4 limit results to apps, files, actions or clipboard items `[apple-keys][apple-narrow]`. `/` followed by a type (for example `/PDF`) or a `kind:` keyword filters results; attribute keywords like `name:` or `author:` filter by properties; category buttons appear as you type `[apple-narrow]`. Apple's Tahoe announcement adds browse views that list apps, files and clipboard without a query `[apple-tahoe]`.
- **Fit:** `naobi-launcher::engine` gains a scope state; Ctrl+1 to Ctrl+4 (Alt+digits are taken by R-028) map to Apps, Files, Commands, Clipboard. `kind:` and `/ext` map to extension filters in the File index query (`naobi-launcher::file_index`). Size **S**.
- **Constraint risk:** none; tightens results so it helps the 25 ms keystroke budget.
- **Value:** high.

### 3.3 Search inside an app (Tab scoping)
- **What:** typing an app name then Tab searches within that app `[apple-keys][apple-narrow]`.
- **Windows mapping:** an app's Recent and Frequent Jump List items can be read through `IApplicationDocumentLists` by AppUserModelID; pinned items cannot be read `[ms-appdoclists]`.
- **Fit:** new provider in `naobi-launcher` with a port implemented in `naobi-win32`; COM work in a transient child per the idle pattern. Size **M**.
- **Constraint risk:** COM shell objects add working set, so the child-process pattern is required (inference, matching `[repo-files]`). Recent-document names are file names: never log them (D-020).
- **Value:** medium-high (for example `code<Tab>` to reopen a recent VS Code folder).

### 3.4 Quick Look style preview, reveal path, reveal in Finder
- **What:** Space previews a result in Quick Look; holding Cmd shows a result's location; Cmd-R locates the file in Finder `[apple-keys]`.
- **Fit:** reveal-and-select is `SHOpenFolderAndSelectItems` (COM must be initialised) `[ms-shopenfolder]`; today Naobi opens the parent folder without selecting (`providers/files.rs` action `folder`). Hold-Ctrl-to-show-full-path is a UI change in `naobi-ui`. Size **S**. A file content preview pane is **M** and collides with "Peek remains out of scope" in R-004, so it needs an Owner re-scope first.
- **Value:** reveal-and-select medium; preview medium (owner decision).

### 3.5 Search the focused app's menu commands
- **What:** Tahoe makes the current app's menu bar items searchable and runnable from Spotlight `[apple-tahoe]`.
- **Windows mapping:** UI Automation exposes menu items, which can support the ExpandCollapse pattern (the parent menu cannot) `[ms-uia-expand]`; a client walks the previously focused window's tree and invokes items (inference).
- **Fit:** big bet, see §7.2.

### 3.6 Query history and suggestions
- **What:** Up Arrow steps back through earlier searches `[apple-keys]`; Spotlight learns routines and suggests actions `[apple-tahoe]`.
- **Fit:** Up Arrow on an empty field recalls the last N queries; suggestions map to the Home list in §4.4. Size **S**.
- **Constraint risk:** query text is sensitive by policy (D-020 bans it from logs; D-014 stores frecency as a hash). Keep history in memory for the session only, or DPAPI-encrypt it in `naobi.db` (inference). Ask the Owner.
- **Value:** medium.

### 3.7 Clipboard history in the launcher
- Spotlight's Cmd-4 clipboard view with search, copy and Clear History `[apple-clipboard]`: Naobi already exceeds this (pin, paste, queue, plain-text paste). No action.

### 3.8 Ask Siri / Use Model
- macOS 27 puts an assistant and an LLM action in Spotlight `[apple-actions][apple-27]`. Conflicts with R-005 (no AI) and R-010 (network); see §8.

---

## 4. Quick wins (detail for ranks 4 to 10)

### 4.1 Scheduled theme switch
- **What:** PowerToys Light Switch switches Windows light/dark by fixed hours, sunset to sunrise from location or typed coordinates with a minute offset, or following Night Light; it can target system, apps or both, and has a toggle hotkey `[pt-lightswitch]`. Raycast v2.10 added a sunrise/sunset dark-mode command `[ray-cl]`.
- **Fit:** Naobi already has Toggle Dark Mode and theme slots that follow the Windows app mode (R-032). Add `[theme.schedule]` (mode, times, lat/long, offset) in `naobi-core::settings`, a pure sunrise/sunset function in `naobi-core` (inference: standard solar-position math, no dependency), and one one-shot timer per transition in `naobi-app`. Size **S**.
- **Constraint risk:** use a single timer armed for the next transition, not a polling timer (RK-13 idle CPU). Skip the Windows location API; typed coordinates avoid a permission prompt (inference).
- **Value:** high; also re-themes Naobi itself through R-032.

### 4.2 Same-app window cycling and Restart Frontmost App
- **What:** Window Hopper cycles only the focused app's windows with Alt+backtick and Shift+Alt+backtick, shows previews, switches on Alt release, and does not open when the app has fewer than two windows `[pt-hopper]`. Raycast v2.9 added a command that quits and reopens the frontmost app `[ray-cl]`.
- **Fit:** `naobi-win32::window_switcher` already enumerates windows with their processes. A RegisterHotKey chord steps to the next window of the same process on each press, with no preview overlay (inference; detecting Alt release would need a hook or polling, which D-012 rules out). Restart Frontmost: close or terminate the previous-foreground process, then relaunch its image path through `shell_exec`. Size **S** each.
- **Constraint risk:** elevated targets fail under UIPI; the R-011 Notice already covers it.
- **Value:** high.

### 4.3 Clipboard transforms
- **What:** Advanced Paste's local actions: plain text, HTML to Markdown, CSV/XML/INI to JSON, paste text as a .txt file, image as .png (and .jpg in preview), HTML as .html, image to text with Windows OCR, audio/video transcode; AI actions are optional `[pt-advpaste]`.
- **Fit:** action-panel entries on a clipboard entry in `naobi-clipboard`: "Paste as file" writes a temp file then puts a file list on the clipboard and pastes through the existing D-019 flow; "Copy as JSON" reuses the dev provider's JSON code. Size **S** (file paste), **M** (HTML to Markdown, which needs a parser or new dependency; Owner approval).
- **Skip parts:** OCR (R-004 out of scope), transcode (low value, WinRT media), AI (R-005).
- **Value:** medium-high.

### 4.4 Home: pinned and recent items on empty query
- **What:** CmdPal's Home shows pinned commands and optional recents; a quick-access shelf shows pins under a compact box, runnable with Alt+1 to Alt+9 `[pt-cmdpal]`. Spotlight suggests routine actions `[apple-tahoe]`.
- **Fit:** `naobi-launcher::engine` empty-query screen; "Pin to Home" action stored as item ids in `settings.toml` or `naobi.db`; recents from the existing frecency table. Size **S**.
- **Value:** medium-high; pairs with Alt+digits already shipped.

### 4.5 Calculator additions
- **What:** Raycast v2.13: convert and mix colors by name or RGB and check contrast between two colors. v2.12: tip, tax, VAT and bill splitting, fractions, rounding, week numbers, time left, more units and constants; also weather, flights and places `[ray-cl]`.
- **Fit:** `naobi-calc` plus `naobi-core::color`. WCAG contrast math already exists for test `r007_token_pairs_meet_wcag_aa` `[repo-constraints]`, so `contrast #fff #914a3b` is nearly free (inference). Size **S** per item.
- **Skip parts:** weather, flights, places need network (R-010).
- **Value:** medium (contrast high for a theme author).

### 4.6 File row actions
- **Reveal and select:** `SHOpenFolderAndSelectItems` `[ms-shopenfolder]`. **S**.
- **Who is locking this file:** File Locksmith lists processes holding a file and offers End task; it sees only processes it can access unless restarted as admin `[pt-locksmith]`. Windows Restart Manager returns the apps and services using registered resources via `RmGetList` after a session is started and resources registered; registering a directory returns access denied `[ms-rmgetlist]`. Fit: pushed view in `naobi-launcher`, port in `naobi-win32`; reuse Force Quit. **M**. Medium-integrity only (R-011): processes of other users or elevated ones are missing, so say so in the view (inference from `[pt-locksmith]`).
- **Run as administrator:** CmdPal runs supported results elevated with Ctrl+Shift+Enter `[pt-cmdpal]`; `ShellExecuteW` verb `runas` launches a program elevated behind a UAC consent prompt `[ms-shellexecute]`. Naobi itself stays unelevated, matching R-010's "helper elevated on demand" wording `[repo-ledger]`. **S**. Owner should confirm this reading of R-011.

### 4.7 Settings catalog and Terminal profiles
- **What:** CmdPal searches Windows Settings pages after `$`, including Windows Update pages, and opens Windows Terminal profiles `[pt-cmdpal]`. Settings pages open by `ms-settings:` URIs `[ms-settings]`. Terminal's `settings.json` lives under the package `LocalState` folder (stable, preview) or `%LOCALAPPDATA%\Microsoft\Windows Terminal` (unpackaged) `[ms-wt-install]`.
- **Fit:** a static `ms-settings:` table in `naobi-launcher::providers::system` (Naobi has five today); a Terminal-profile provider that reads `settings.json` on view open and launches `wt.exe -p <name>`. Size **S** each.
- **Constraint risk:** reading `settings.json` needs a JSON parser in a non-perf crate; `serde_json` is approved only for `naobi-perf` (D-035), so this asks for a dependency decision (inference).
- **Value:** medium.

---

## 5. Medium

| Upgrade | What (source) | Fit and size | Constraint risk | Value |
|---|---|---|---|---|
| User environment variables view | PowerToys manages environment variables with profiles `[pt-index]`. User variables can be changed and broadcast with `WM_SETTINGCHANGE` "Environment"; system variables live under HKLM `[ms-envvars]` | Pushed view; `naobi-win32` registry port. **M** | User scope only; HKLM needs elevation (R-011) | Medium for a developer |
| Screen ruler | Bounds, spacing, horizontal and vertical measures with edge detection and tolerance, copy on click, extra units including DIP, Win+Ctrl+Shift+M `[pt-ruler]` | Reuse the picker's per-monitor GDI snapshot and overlay (D-023) in `naobi-picker` or a sibling crate. **M** | Overlay only while active; no idle cost if torn down like the picker (inference) | Medium (design/dev work) |
| Crop-and-lock thumbnail | Live thumbnail, reparent and frozen screenshot modes of a window region `[pt-crop]`. `DwmRegisterThumbnail` needs a destination window owned by the caller; cropping is done with `DwmUpdateThumbnailProperties` `[ms-dwmthumb]` | New topmost window per crop in `naobi-aot`. Thumbnail mode only (reparent has documented compatibility issues `[pt-crop]`). **M** | Adds windows while in use; R-029's "no window" applies to the UI/UX upgrade, so confirm with the Owner | Medium |
| Monitor brightness (DDC/CI) | Power Display controls external monitors over DDC/CI (brightness, contrast, volume, input, rotation, color temperature, power) with profiles; Microsoft warns that probing some monitors' capability strings can trigger a Windows crash, and ships an exclusion list `[pt-powerdisplay]`. `SetMonitorBrightness` is the documented high-level call `[ms-brightness]` | `brightness 70` command in `naobi-launcher`, port in `naobi-win32`, work in a transient child. Brightness only. **M** | BSOD risk on capability probe: only call brightness get/set, never parse capabilities (inference); tier 2 test needs a real monitor | Medium |
| Radio and power toggles | Raycast v2.7 added Toggle Wi-Fi, connect to saved networks, Bluetooth connect/pair, Toggle Energy Saver, Toggle Camera Mute; v2.4 Toggle Airplane Mode `[ray-cl]`. Wi-Fi/Bluetooth radio state is `Windows.Devices.Radios` (in `commander-features.md` §3.5) | WinRT in a transient child. **M** | Raycast needed fixes when no adapter exists (`commander-features.md`); Energy Saver and camera-mute APIs not verified (§10) | Medium |
| Focus session | Raycast Focus blocks chosen apps (closes them) and websites (redirects to a block page) for a timed session up to 24 h, with snooze and categories; works on Windows with the panel in the taskbar `[ray-focus]` | Apps only: an Awake-like timed state in a new small crate; close matching windows on activation via window events (inference). **M** | Website blocking needs a hosts edit (elevation) or a browser extension: skip it | Low-medium for one owner |
| `naobi:` deep links | CmdPal can copy a command link to start a command from a script `[pt-cmdpal]`. A per-user URI scheme is a registry key with `URL Protocol` and a `shell\open\command`; Microsoft warns that handlers must treat the URI as hostile input and confirm dangerous actions `[ms-urischeme]` | HKCU registration at autostart setup; second instance forwards to the first (R-023). **S to M** | New attack surface: allowlist command ids, never run `>` lines or paste from a link (inference) | Low-medium |

---

## 6. Platform and tooling upgrades

### 6.1 Slint 1.18.1 to 1.19.0
Slint 1.19.0 was released 2026-11-16 `[slint-rel]`. Items relevant to Naobi, from the changelog `[slint-cl]`:
- **Idle and power:** a winit fix stops the event loop waking at the refresh rate while a window is minimized or hidden, and a Windows fix stops continuous redraws while another window animates. Naobi's launcher lives hidden and cloaked (D-013), so this can lower idle CPU and wakeups. Measure with `naobi-perf idle`.
- **Accessibility:** a memory leak while an accessibility client is active is fixed; text positions reported to assistive tech and focus in virtualized lists are fixed. Naobi ships with accessibility on (D-034).
- **Software renderer:** color emoji, drop shadows, text stroke, gradients on `Path`, `Clip` following `border-radius`, faster gradients and images. Color emoji matter for the emoji picker; check how it renders today on the Owner's PC (§10).
- **Widgets/language:** `TextEdit` `undo()`/`redo()` and `has-selection` on text inputs (scratchpad uses a `TextInput` today, `app.slint`); `ListView.ensure-row-visible()`, `Flickable.scroll-to()` and `ensure-visible()` on every element (the list is a `ScrollView` since NCMD-44, RK-17); `StyledText` can be selectable; animations respect reduced-motion on Windows.
- **Startup:** system fonts are discovered in the background (helps the 1.5 s pre-warm and 300 ms tray-ready budgets).
- **Breaking or noisy:** experimental features are now enabled only by name (value `1` errors); editions are opt-in and the compiler reports an info message without one; MSRV is 1.95 (Naobi pins 1.99.1). The repo sets no experimental flag (searched `naobi-ui` and workspace manifests).
- 1.18.0 (already on 1.18.x) gave `KeyEvent.repeat`, text input content and selection for accessibility, and the dead-key fix; nothing to adopt there.
- **Fit:** bump `slint` and `slint-build` in the workspace `Cargo.toml`, select edition 1.19, re-run fmt, clippy, tier 1, tier 2 and `naobi-perf all`. Size **S** (bump) to **M** (adopt undo, ensure-visible). Risk: low; a new dependency version is still an "Ask first" item per CLAUDE.md.
- **Value:** high.

### 6.2 Win32 APIs that proposals rely on (all documented, medium integrity)
`SHOpenFolderAndSelectItems` `[ms-shopenfolder]`, Restart Manager `RmGetList` `[ms-rmgetlist]`, `DwmRegisterThumbnail` `[ms-dwmthumb]`, `ShellExecuteW` `runas` `[ms-shellexecute]`, `IApplicationDocumentLists` `[ms-appdoclists]`, UI Automation `[ms-uia-expand]`, user environment variables plus `WM_SETTINGCHANGE` `[ms-envvars]`, `SetMonitorBrightness` `[ms-brightness]`, per-user URI scheme `[ms-urischeme]`. All live in `naobi-win32` behind ports; COM and shell calls follow the child-process pattern.

---

## 7. Big bets

### 7.1 Workspaces (launch a set of apps into saved positions)
- **What:** PowerToys Workspaces captures the desktop, saves apps with arguments and positions, launches them with a status dialog, can move existing windows, offers a desktop shortcut and a preview CLI; apps cannot be told to launch at a position, so they are moved after launch, and elevated apps cannot be repositioned (known issue) `[pt-workspaces]`.
- **Fit:** new crate `naobi-workspaces` (data and ports), `naobi-win32` for launch and `SetWindowPos`, items in the launcher (`ws dev`). **L**.
- **Constraint risk:** reintroduces window positioning that D-039 removed for snapping; Owner decision. Elevated apps hit UIPI (R-011).
- **Value:** medium-high if the Owner has fixed daily layouts.

### 7.2 Menu command search for the focused app
- **What and mapping:** see §3.5. A child process walks the previous foreground window's UI Automation tree for MenuItem elements, returns titles and paths, and invokes the chosen item (inference from `[ms-uia-expand]`).
- **Fit:** `naobi-win32` UIA client in a transient child; launcher provider. **L**.
- **Constraint risk:** UIA client memory and latency (child process mandatory); ribbon and custom-drawn apps expose little; elevated windows are blocked (R-011).
- **Value:** high where it works (Office, Explorer, classic Win32), uneven elsewhere.

### 7.3 Script commands
- **What:** Raycast indexes scripts from chosen folders, reads title, mode, icon and arguments from header comments, and on Windows runs any interpreter on PATH; a script with a text first argument can be a fallback command `[ray-scripts]`.
- **Fit:** a scripts folder under `%APPDATA%\NaobiCommand\scripts`, header parsing in `naobi-launcher`, execution through the existing `>` run path (D-038). **M to L**.
- **Constraint risk:** R-005 forbids a third-party plugin system. Owner-written scripts run as the Owner's own process (like `>` lines, R-010), so this may fit, but it needs an explicit Owner ruling.
- **Value:** high for a developer owner; it covers many long-tail asks without new Rust code.

---

## 8. Skip: conflicts with constraints

| Feature | Source | Conflict |
|---|---|---|
| Hyper Key (Caps Lock or a modifier as Ctrl+Alt on Windows) | `[ray-hyper]` | Needs a low-level keyboard hook (D-012) |
| Quick Accent, Keyboard Manager tap-alone remaps | `[pt-index][pt-rel]` | Keyboard hook (D-012); remapper is out of scope (R-004) |
| Grab And Move, Mouse utilities (Find My Mouse, Highlighter, Crosshairs, Mouse Button Lock, CursorHide) | `[pt-index][pt-rel]` | Global mouse hooks or always-on overlays; idle CPU and the same reasoning as D-012 (inference) |
| Mouse Without Borders, Cloud Sync, Raycast MCP server, weather, flights, places, Translate | `[pt-index][ray-cl][ray-manual]` | Sockets (R-010) |
| AI: Advanced Paste with AI, Siri/Use Model, Dictate to AI, AI Chat | `[pt-advpaste][apple-actions][ray-cl]` | R-005 (no AI) and R-010 |
| Hosts File Editor, Services start/stop, system environment variables, Registry editing | `[pt-index][pt-cmdpal][ms-envvars]` | Elevation (R-011); Hosts editor is out of scope (R-004) |
| Text Extractor (OCR), Advanced Paste image to text, Raycast screenshot text search | `[pt-index][pt-advpaste][ray-screens]` | OCR is out of scope (R-004); reopen only if the Owner re-scopes |
| Peek, PowerRename, Image Resizer | `[pt-index]` | Out of scope (R-004) |
| FancyZones and window snapping | `[pt-index]` | Removed by D-039 |
| CmdPal Dock, Power Display flyout, Shortcut Guide side pane | `[pt-cmdpal][pt-powerdisplay][pt-rel]` | Persistent extra UI surfaces; the launcher-only model (R-029 spirit, inference) |
| New+, File Explorer add-ons, File Locksmith context menu | `[pt-index][pt-locksmith]` | Shell extensions load into Explorer; Naobi has no installer (D-007). The launcher action in §4.6 covers Locksmith |
| ZoomIt | `[pt-index]` | Large (zoom, draw, record) with low value for a launcher suite |
| Command Not Found | `[pt-index]` | PowerShell module plus WinGet; outside the app |

---

## 9. Sources

### Repository
- `[repo-constraints]` `CONSTRAINTS.md`
- `[repo-ledger]` `docs/ledger.yaml` (R-004, R-005, R-006, R-010, R-011, R-012, R-014, R-015, R-028, R-029, R-032, D-012, D-014, D-020, D-029, D-035, D-038, D-039)
- `[repo-adr8]` `docs/adr/0008-winhttp-fx-rates.md`
- `[repo-adr9]` `docs/adr/0009-user-scoped-file-index.md`
- `[repo-files]` `crates/naobi-app/src/modules/files.rs` (transient child pattern)
- `[repo-raycast]` `docs/research/raycast.md` §3.3 (aliases and hotkeys)

### Microsoft PowerToys (learn.microsoft.com and GitHub)
- `[pt-index]` https://learn.microsoft.com/en-us/windows/powertoys/ (utility list, updated 2026-10-21)
- `[pt-rel]` https://github.com/microsoft/PowerToys/releases (v0.100 to v0.103 previews)
- `[pt-cmdpal]` https://learn.microsoft.com/en-us/windows/powertoys/command-palette/overview
- `[pt-lightswitch]` https://learn.microsoft.com/en-us/windows/powertoys/light-switch
- `[pt-hopper]` https://learn.microsoft.com/en-us/windows/powertoys/window-hopper
- `[pt-powerdisplay]` https://learn.microsoft.com/en-us/windows/powertoys/power-display
- `[pt-advpaste]` https://learn.microsoft.com/en-us/windows/powertoys/advanced-paste
- `[pt-locksmith]` https://learn.microsoft.com/en-us/windows/powertoys/file-locksmith
- `[pt-crop]` https://learn.microsoft.com/en-us/windows/powertoys/crop-and-lock
- `[pt-workspaces]` https://learn.microsoft.com/en-us/windows/powertoys/workspaces
- `[pt-ruler]` https://learn.microsoft.com/en-us/windows/powertoys/screen-ruler

### Raycast
- `[ray-cl]` https://www.raycast.com/changelog/windows (v2.4 to v2.13, 2026-09-14 to 2026-11-18)
- `[ray-manual]` https://manual.raycast.com/windows (feature index)
- `[ray-scripts]` https://manual.raycast.com/script-commands
- `[ray-focus]` https://manual.raycast.com/focus
- `[ray-hyper]` https://manual.raycast.com/hyper-key
- `[ray-screens]` https://manual.raycast.com/screenshots

### Apple (macOS Spotlight)
- `[apple-actions]` https://support.apple.com/guide/mac-help/take-actions-and-shortcuts-in-spotlight-mchl4953dfeb/mac (macOS 27)
- `[apple-keys]` https://support.apple.com/guide/mac-help/spotlight-keyboard-shortcuts-mh26783/mac (macOS 27)
- `[apple-narrow]` https://support.apple.com/guide/mac-help/narrow-search-results-in-spotlight-mchl4d69efd3/mac (macOS 27)
- `[apple-clipboard]` https://support.apple.com/guide/mac-help/search-your-clipboard-history-mchl40d5b86b/mac
- `[apple-tahoe]` https://www.apple.com/newsroom/2025/06/macos-tahoe-26-makes-the-mac-more-capable-productive-and-intelligent-than-ever/
- `[apple-27]` https://support.apple.com/guide/mac-help/whats-new-in-macos-27-apd07d671600/mac

### Slint
- `[slint-rel]` https://github.com/slint-ui/slint/releases
- `[slint-cl]` https://github.com/slint-ui/slint/blob/master/CHANGELOG.md (1.18.0, 1.18.1, 1.19.0)

### Microsoft Win32 and Windows docs
- `[ms-shopenfolder]` https://learn.microsoft.com/en-us/windows/win32/api/shlobj_core/nf-shlobj_core-shopenfolderandselectitems
- `[ms-rmgetlist]` https://learn.microsoft.com/en-us/windows/win32/api/restartmanager/nf-restartmanager-rmgetlist
- `[ms-dwmthumb]` https://learn.microsoft.com/en-us/windows/win32/api/dwmapi/nf-dwmapi-dwmregisterthumbnail
- `[ms-shellexecute]` https://learn.microsoft.com/en-us/windows/win32/api/shellapi/nf-shellapi-shellexecutew
- `[ms-appdoclists]` https://learn.microsoft.com/en-us/windows/win32/api/shobjidl_core/nn-shobjidl_core-iapplicationdocumentlists
- `[ms-uia-expand]` https://learn.microsoft.com/en-us/windows/win32/winauto/uiauto-implementingexpandcollapse
- `[ms-envvars]` https://learn.microsoft.com/en-us/windows/win32/procthread/environment-variables
- `[ms-urischeme]` https://learn.microsoft.com/en-us/previous-versions/windows/internet-explorer/ie-developer/platform-apis/aa767914(v=vs.85) (archived)
- `[ms-wt-install]` https://learn.microsoft.com/en-us/windows/terminal/install
- `[ms-settings]` https://learn.microsoft.com/en-us/windows/apps/develop/launch/launch-settings
- `[ms-brightness]` https://learn.microsoft.com/en-us/windows/win32/api/highlevelmonitorconfigurationapi/nf-highlevelmonitorconfigurationapi-setmonitorbrightness

---

## 10. Not verified

- Documented APIs for toggling Windows Energy Saver and camera mute (Raycast v2.7 ships both `[ray-cl]`); not looked up.
- Whether the emoji picker renders color emoji today with Slint 1.18.1's software renderer; 1.19 adds color emoji `[slint-cl]`, but the current look was not checked on screen.
- The idle working-set and CPU effect of Slint 1.19 on Naobi; only `naobi-perf idle` can show it.
- Raycast's Windows Script Commands header modes and refresh options; the manual page points to a separate repo for the full syntax `[ray-scripts]`.
- Whether Window Hopper's release-Alt behaviour can be reproduced without a hook; §4.2 assumes it cannot and proposes a per-press step instead.
- Spotlight's documented date filters: the macOS 27 narrowing page does not cover them `[apple-narrow]`.
- The `ms-settings:` page list itself was not re-read; it is cited from `commander-features.md`.
