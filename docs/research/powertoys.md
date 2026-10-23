# PowerToys behaviour reference (clean-room input for Naobi Command)

Task: 20261021-2301-naobi-command-discover. Role: researcher. Date of research: 2026-10-21.
Scope: launcher (PowerToys Run + Command Palette), FancyZones, Color Picker, Always On Top, Awake, plus a note on clipboard history.
Method: behaviour only. Everything below is paraphrased from official docs and the MIT-licensed source; no code is copied. Where a statement is my own reasoning rather than a cited fact it is marked **(inference)**.

## 0. Citation conventions and provenance

- `repo:<path>` = https://github.com/microsoft/PowerToys/blob/main/<path> (default branch read on 2026-10-21 via raw.githubusercontent.com).
- `learn:<slug>` = https://learn.microsoft.com/en-us/windows/powertoys/<slug>.
- `#NNNN` = https://github.com/microsoft/PowerToys/issues/NNNN (or /pull/NNNN where stated as PR).
- "local" = read-only inspection of `%LOCALAPPDATA%\Microsoft\PowerToys\*` on the owner's machine. Local values are user state plus an older build, so they can differ from source defaults (example: Always On Top frame thickness is 2 locally, 4 in source). Source/Learn win over local when they disagree.
- Local install facts: PowerToys last-run version string `v0.101.2362` (local `last_version_run.json`); Command Palette MSIX `Microsoft.CommandPalette 0.12.12365.0` (Get-AppxPackage). In the local `settings.json`, PowerToys Run is disabled and CmdPal is enabled, so CmdPal is the launcher this owner actually uses.

## 1. Cross-cutting architecture facts (affect every module)

1. PowerToys has a runner process plus per-module DLLs. Some modules run their own helper process (Always On Top, Awake, Color Picker UI, Run's PowerLauncher), FancyZones runs inside the runner process (learn:fancyzones, section "Applications that need FancyZones to run elevated").
2. Hotkeys are handled in two different ways in the code base:
   - Always On Top registers its own hotkeys with `RegisterHotKey` on a hidden tool window (repo:src/modules/alwaysontop/AlwaysOnTop/AlwaysOnTop.cpp, `RegisterHotkey`).
   - Run tries `RegisterHotKey` first and falls back to a low-level keyboard hook only if registration fails (repo:src/modules/launcher/PowerLauncher/ViewModel/MainViewModel.cs, `SetHotkey`, log text "Registering global shortcut failed. Will use low-level keyboard hook instead").
   - Run also has an optional "centralized keyboard hook" mode (learn:run, setting "Use centralized keyboard hook"; repo:src/modules/launcher/Microsoft.Launcher/dllmain.cpp, `use_centralized_keyboard_hook`).
3. Foreground-lock workaround used in two places: send one empty mouse `SendInput` event first so the calling thread counts as last-input owner, then activate (Run: repo:src/modules/launcher/PowerLauncher/MainWindow.xaml.cs `BringProcessToForeground`, issue #4270 referenced in the comment; FancyZones: repo:src/modules/fancyzones/FancyZonesLib/WindowUtils.cpp, foreground helper). A Rust launcher needs the same trick or `AllowSetForegroundWindow` / `AttachThreadInput` **(inference: alternatives)**.
4. Excluded-apps lists are shared logic in `repo:src/common/utils/excluded_apps.h`: entries are matched against the process path by substring that must touch the file name (so `Notepad` matches `Notepad.exe` and `Notepad++.exe`), and also as a case-folded substring of the window title (first 255 chars).
5. Game-mode detection is `SHQueryUserNotificationState` returning "running D3D full screen" (repo:src/common/utils/game_mode.h). Run's full-screen guard uses the same API plus a foreground-window-covers-monitor check (repo:src/modules/launcher/PowerLauncher/Helper/WindowsInteropHelper.cs `IsWindowFullscreen`).
6. UIPI/elevation: a non-elevated PowerToys cannot control elevated windows. Documented per module below. A Rust implementation has the same constraint unless it runs elevated or uses a UIAccess manifest **(inference)**.
7. PowerToys settings live as one JSON file per module under `%LOCALAPPDATA%\Microsoft\PowerToys\<Module>\settings.json` with a `{ "properties": {...}, "name": ..., "version": ... }` envelope (local files; Always On Top, Awake, Color Picker, FancyZones, Run all follow it). Hotkeys are stored as `{win, ctrl, alt, shift, code}` with a Win32 virtual-key `code`.

---

## 2. Launcher: PowerToys Run (legacy) and Command Palette (CmdPal, current)

### 2.1 Which one is the reference

- Learn marks PowerToys Run as being succeeded by Command Palette ("PowerToys Run is getting an upgrade to v2", learn:run). The CmdPal README calls CmdPal "the next iteration of PowerToys Run" (repo:src/modules/cmdpal/README.md).
- Defaults: Run = `Alt+Space` (learn:run). CmdPal = `Win+Alt+Space` (learn:command-palette/overview; repo:src/modules/cmdpal/README.md).
- Run is a WPF/.NET app (repo:doc/devdocs/modules/launcher/architecture.md; `UseWPF` and self-contained in repo:src/modules/launcher/PowerLauncher/PowerLauncher.csproj). CmdPal is a packaged WinUI 3 app on Windows App SDK, self-contained, with `PublishAot` enabled in the project (repo:src/modules/cmdpal/Microsoft.CmdPal.UI/Microsoft.CmdPal.UI.csproj).
- Naobi Command should treat Run as the behaviour spec for ranking/plugins (small, documented in source) and CmdPal as the spec for show/hide mechanics and modern UX options.

### 2.2 User-facing behaviour (Run)

Source: learn:run.

- Alt+Space toggles. Esc hides. Typing searches all "global" plugins; a per-plugin direct activation keyword restricts to one plugin.
- Result shortcuts: Ctrl+Shift+Enter run as admin, Ctrl+Shift+U run as different user, Ctrl+Shift+E open containing folder, Ctrl+C copy path, Tab cycles results and context buttons.
- Default activation keywords (learn:run table): Calculator `=`, History `!!`, Windows Search `?`, OneNote `o:`, PowerToys `@`, Program `.`, Registry `:`, Service `!`, Shell `>`, Time and date `)`, Unit converter `%%`, URI handler `//`, Value generator `#`, VS Code workspaces `{`, Web search `??`, Windows settings `$`, Windows Terminal `_`, Window Walker `<`.
- Learn warns that activation keywords can collide with global queries (path characters, math operators, `(`).
- Program plugin accepts arguments after `--` or after a token starting with `-`, `--` or `/` (learn:run, "Program parameters").

Run general options and defaults. Documented names in learn:run; default values below come from the local `PowerToysRunSettings.json` (shape of the settings, values may be user-tuned) cross-checked with source where noted.

| Option | Default | Note |
|---|---|---|
| Hotkey | Alt+Space | learn:run |
| UseCentralizedKeyboardHook | false | fixes focus when triggered from an elevated window (learn:run) |
| IgnoreHotkeysOnFullscreen | false | learn:run: hotkey ignored in F11 full-screen when enabled |
| SearchQueryResultsWithDelay (input smoothing) | true | local |
| SearchInputDelay / SearchInputDelayFast | 150 ms / 0 ms | "background execution plugins" / "immediate plugins" (learn:run); local |
| MaxResultsToShow | 4 locally (user value; Learn says "number of results shown before scrolling") | local |
| SearchClickedItemWeight | 5 | "Selected item weight" (learn:run); local |
| SearchWaitForSlowResults | false | learn:run; local |
| HideWhenDeactivated | true | local; source: `OnDeactivated` hides (repo:.../MainWindow.xaml.cs) |
| ClearInputOnLaunch | false | local |
| LastQueryMode | Selected (re-open selects previous query text); alternatives Empty, Preserved | repo:src/modules/launcher/PowerLauncher/ViewModel/MainViewModel.cs `OnHotkey` |
| StartupPosition | 0 (primary / cursor / focused-window choice) | learn:run "Preferred monitor position" |
| TabSelectsContextButtons | true | local |

### 2.3 User-facing behaviour (CmdPal)

Sources: learn:command-palette/overview, learn:command-palette/settings.

- Win+Alt+Space opens; compact mode (small box that expands) is off by default.
- Built-in commands in the tree (repo:src/modules/cmdpal/ext/): Actions, Apps, Bookmark, Calc, ClipboardHistory, Indexer (files), PerformanceMonitor, PowerToys, Registry, RemoteDesktop, Shell, System, TimeDate, WebSearch, WindowsServices, WindowsSettings, WindowsTerminal, WindowWalker, WinGet. No unit-converter extension exists in that directory listing (checked 2026-10-21); unit conversion is only a Run plugin (section 2.7).
- Prefixes mentioned in Learn: `>` shell, `??` web search, `=` calculator, `$` Windows settings, `file ` for file search.
- Settings worth copying (learn:command-palette/settings): activation shortcut; "use low-level keyboard hook" (for responsiveness, may cause compatibility issues); ignore shortcut in fullscreen; ignore shortcut when Windows heuristically reports busy/fullscreen; press the shortcut 3 times within 2 seconds to override the suppression; return home after N seconds; select or keep previous search text; preferred monitor (cursor, primary, focused window, in place, last position); Escape behaviour choices; backspace-on-empty goes back; single-click activation; tray icon; Alt+F4 quits (off by default); disable animations.

### 2.4 Show latency and process model

Findings (details and numbers in section 8):

- Run keeps one long-lived `PowerToys.PowerLauncher.exe` process whose WPF main window is created at startup and then only toggled between visible and collapsed. The hotkey handler flips `MainWindowVisibility`; it does not construct a window (repo:src/modules/launcher/PowerLauncher/ViewModel/MainViewModel.cs `OnHotkey`, `ToggleWox`; repo:.../App.xaml.cs `OnStartup` builds MainWindow, initialises plugins in `PluginManager.InitializePlugins`, measures `LauncherBootEvent`).
- Run's telemetry distinguishes a cold boot time (`LauncherBootEvent`, `BootTimeMs`) from a warm hotkey-to-visible time (`LauncherWarmStateHotkeyEvent`, `HotkeyToVisibleTimeMs`) (repo:.../App.xaml.cs, repo:.../MainWindow.xaml.cs `OnVisibilityChanged`). I found no published numbers for those telemetry fields.
- CmdPal also keeps the window alive: on startup the window is created then "cloaked" to avoid the first-show animation; hiding means cloak (DWM cloaking) and leave the window technically shown with `SW_SHOWNA`, not destroy or `SW_HIDE` when cloak succeeds; showing means cloak, `ShowWindow`, uncloak, then `SetForegroundWindow` + `SetActiveWindow`, and re-assert `HWND_TOPMOST` (repo:src/modules/cmdpal/Microsoft.CmdPal.UI/MainWindow.xaml.cs, `HideWindow` and the show path around the `Cloak()`/`Uncloak()` calls).
- CmdPal single instance uses Windows App SDK `AppInstance` key redirection so a second launch forwards activation to the first (repo:src/modules/cmdpal/Microsoft.CmdPal.UI/Program.cs `DecideRedirection`).
- CmdPal subclasses its window procedure with `SetWindowLongPtr(GWL_WNDPROC)` using a delegate named for hotkey handling (MainWindow.xaml.cs ~206); the precise messages handled were not traced.

### 2.5 Search, ranking and fuzzy matching (Run) - the spec to reimplement

All from repo:src/modules/launcher.

1. Query parsing (`PowerLauncher/Plugin/QueryBuilder.cs`): if the text starts with a non-global plugin's activation keyword (longest keyword wins) only that plugin runs; otherwise every enabled global plugin runs.
2. Execution (`ViewModel/MainViewModel.cs`): each keystroke waits a small fixed debounce (constant 20 ms), then queries plugins as separate tasks. "Non-delayed" plugins run in parallel by default (`PTRunNonDelayedSearchInParallel`). Plugins that implement `IDelayedExecutionPlugin` (Windows Search indexer is the documented one) run afterwards, gated by the input-delay settings. A new keystroke cancels the previous query via cancellation tokens. Results from a stale query are dropped by comparing the originating query text.
3. Sorting (`Wox.Plugin/Result.cs` `GetSortOrderScore`, `PowerLauncher/ViewModel/ResultsViewModel.cs` `Sort`): sort key = plugin `WeightBoost` + result `Score` + (`SelectedCount` x multiplier), multiplier default 5. `SelectedCount` comes from a persisted "user selected record" (title/subtitle/plugin keyed, with count and last-selected time), stored in `PowerToys Run\Settings\UserSelectedRecord.json` (local file confirms shape: `SelectedCount`, `LastSelected`, `Title`, `SubTitle`, `PluginID`, `Search`, `Score`). Results can opt out via `DisableUsageBasedScoring`. History-plugin duplicates of a live result are removed.
4. Fuzzy matcher (`Wox.Infrastructure/StringMatcher.cs`), paraphrased:
   - Query is lower-cased and split on spaces. Each sub-token must be found as an in-order character subsequence in the target; matching prefers a start-of-word alignment by backtracking to verify that preceding characters also match.
   - It returns matched character indices for highlighting plus a score, and tries from several start offsets keeping the best.
   - Score rises with query length and match compactness, falls with distance of the first match from the start of its word and with spread of the match. Match length is weighted twice as strongly as position. Extra points when the target is only a few characters longer than the query (+20 if under 5 extra chars, +10 if under 10), when every query token is a contiguous substring (up to ~4 chars at 10 each, then 5 each), and +10 for case-insensitive exact match.
   - A "search precision" filter drops weak results; enum values Regular = 50, Low = 20, None = 0 (the threshold is applied to the final score).
5. Program scoring (`Plugins/Microsoft.Plugin.Program/Programs/Win32Program.cs`): the score is the max over fuzzy scores of display name, localized name, description (halved), executable name, localized executable name and the shortcut-resolved executable name; +5 when the query has no program arguments; results whose path matches but name does not are filtered.
6. Persistence and boosts: a plugin can set `WeightBoost` (default 0 in all local plugin entries).

CmdPal change (fuzzy matcher): release notes for 0.95 say CmdPal got a new fuzzy matcher and smarter fallbacks (https://github.com/microsoft/PowerToys/releases/tag/v0.95.0). PR #44090 (precomputed fuzzy matcher) says it precomputes normalized strings, allocates effectively nothing during matching, strips diacritics, drops the same-case bonus when the query is all lower-case, allows skipping word separators (so "Power Point" matches "PowerPoint"), and claims up to 100x over the old matcher and ~2-5x over the improved toolkit matcher (https://github.com/microsoft/PowerToys/pull/44090). **Minimum worth copying:** normalized-lowercase precompute, subsequence match with word-start bias, usage-count boost.

### 2.6 App indexing sources

Run Program plugin (repo:doc/devdocs/modules/launcher/plugins/program.md; repo:src/modules/launcher/Plugins/Microsoft.Plugin.Program):

- Packaged apps: `PackageManager` lists packages for the current user; icons resolved from the manifest assets by scale and theme; install/uninstall events (`PackageCatalog`) keep the list live.
- Win32: user Desktop, Public Desktop, Start Menu, Common Start Menu, some registry-sourced entries, and folders on `PATH`. Win32 has no install event, so `FileSystemWatcher`s watch those folders and update the catalog (`Storage/Win32ProgramFileSystemWatchers.cs`).
- Defaults (`ProgramPluginSettings.cs`): Start Menu, Desktop, Registry and PATH sources are all on; indexed suffixes `bat, appref-ms, exe, lnk, url`; the "run command" suffix list additionally has `cpl, msc`; a minimum score threshold field of 0.75 exists in settings.
- Shortcut handling: `.lnk` parsed through the shell link COM interface (`Wox.Infrastructure/ShellLinkHelper.cs`) to get target path and arguments; `.url` files are read as text for `URL=` and `IconFile=`; PWAs (Chrome/Edge proxies) are identified by path.
- De-duplication: same name + executable + full path are treated as one entry.
- Launch: `ShellExecute` style start through `ProcessStartInfo` with `UseShellExecute` and verb `runAs` / `runAsUser` for the admin and other-user actions (Win32Program.cs). Packaged apps are activated via `IApplicationActivationManager` (`Programs/ApplicationActivationManager.cs`).
- CmdPal Apps extension re-uses the same design with a singleton `AppCache`, the same Start Menu/Desktop folders via `Environment.SpecialFolder`, `PackageManager.FindPackagesForCurrentUser`, package-catalog events and file watchers (repo:src/modules/cmdpal/ext/Microsoft.CmdPal.Ext.Apps/AppCache.cs, Programs/Win32Program.cs, Programs/UWP.cs, Storage/PackageRepository.cs), plus a per-item cache of fuzzy-normalized title/subtitle (AppListItem.cs).

### 2.7 Calculator and unit conversion

- Calculator (Run plugin): parses with the third-party Mages expression engine (repo:doc/devdocs/modules/launcher/plugins/calculator.md; repo:.../Microsoft.PowerToys.Run.Plugin.Calculator/CalculateEngine.cs). Pre-processing: a regex gate that only accepts calculator-looking input; implicit multiplication like `2(3+4)` and `(1+2)(3+4)` by inserting `*`; `log(` rewritten to base-10 and `ln(` to natural log; constant `e` added; a regex pre-check rejects division by a literal zero (while allowing `0.5`, `0x..`, `0b..`, `0o..`); array results are rejected. Output is converted to decimal and rounded to 10 digits with away-from-zero midpoint rounding. Number parsing and output follow the system culture unless "use English format" options are set (defaults false). Operators and functions listed in learn:run "Calculator plugin" (+ - * / % ^, ceil/floor/round/exp/max/min/abs/log/ln/sqrt/pow/factorial/sign/rand/randi/pi and trig/hyperbolic families).
- CmdPal Calc extension adds base conversion (hex/bin/etc. - file `Helper/BaseConverter.cs`), persistent history with save/delete/clear, `logn(x, base)` and `root(x, n)` (learn:command-palette/overview; release notes v0.99.0). It keeps the same engine lineage (`CalculateEngine.cs`, `CalculateHelper.cs`, `NumberTranslator.cs`).
- Unit conversion is NOT part of the calculator. It is a separate Run plugin "Unit Converter", activation `%%`, `to` and `in` interchangeable, 13 categories (acceleration, angle, area, duration, energy, information, length, mass, power, pressure, speed, temperature, volume), built on the UnitsNet library (repo:doc/devdocs/modules/launcher/plugins/community.unitconverter.md; learn:run). Locally it is not global (needs `%%`). Input interpretation (`InputInterpreter.cs`) normalises aliases (`kph`, `kmph`, `cph` -> `km/h`, `cm/h`; `metre` -> `meter`) and parses feet-and-inches input. Names/abbreviations are not localised (learn:run).
- Rust path **(inference)**: no need for a general expression engine port; a small precedence parser plus a units table covers the documented surface. Currency is not supported by either plugin.

### 2.8 File search provider

- Run "Windows Search" plugin (activation `?`, global by default) queries the Windows Search index, not the file system (learn:run; repo:src/modules/launcher/Plugins/Microsoft.Plugin.Indexer).
- Mechanism: COM `CSearchManager` -> catalog `SystemIndex` -> `ISearchQueryHelper` builds SQL, executed through OLE DB; selected columns are item URL, file name, attributes; scope restricted to `file:`; hidden files excluded unless enabled; max results default 30; user text without wildcards uses the indexed `Contains(System.FileName, ...)` form (faster), with `%`/`_` it falls back to `LIKE` (SearchHelper/WindowsSearchAPI.cs). Excluded patterns add `NOT` clauses on the item URL. It implements the delayed-execution interface so keystrokes do not wait on it (Main.cs).
- It warns when the index does not cover all drives (`DriveDetection/IndexerDriveDetection.cs`, learn:run "Windows Search settings").
- Folder plugin adds path navigation with `>` (search inside folder) and `*` (mask) (learn:run).
- CmdPal's Indexer extension also uses OLE DB rowsets and reads catalog status (repo:src/modules/cmdpal/ext/Microsoft.CmdPal.Ext.Indexer/Indexer/*).
- Limitation: results depend on the user's index scope; non-indexed drives are invisible. A faster alternative (NTFS MFT/USN scan, like Everything) is a design option **(inference)**, not what PowerToys does.

### 2.9 System commands and web search

- System commands (repo:src/modules/launcher/Plugins/Microsoft.PowerToys.Run.Plugin.System/Components/Commands.cs): Shutdown = `shutdown /s /hybrid /t 0`; Restart = `shutdown /g /t 0`; Sign out = `ExitWindowsEx` logoff; Lock = `LockWorkStation`; Sleep and Hibernate = `SetSuspendState`; UEFI firmware reboot = `shutdown /r /fw /t 0` elevated and shown only when booted in UEFI mode; Recycle Bin open/empty; IP and MAC address lookups from network interfaces (`NetworkConnectionProperties.cs`). Options (local defaults): ConfirmSystemCommands false, LocalizeSystemCommands true, SeparateResultEmptyRB false, ShowSuccessOnEmptyRB false.
- Web search (`??`): opens the default browser's search page for the query, resolves the default browser through a shared helper (`Wox.Plugin.Common` `DefaultBrowserInfo`); option `NotGlobalIfUri` hides the web result when the input looks like a URL (repo:src/modules/launcher/Plugins/Community.PowerToys.Run.Plugin.WebSearch/Main.cs). URI handler `//` opens URLs and `mailto:`/`ms-settings:` (learn:run).
- Other plugins worth classifying for MVP: Window Walker (`<`, needs enumeration of top-level windows and a kill action with caveats listed in learn:run), Shell (`>`), Time and Date (custom format strings, learn:run), Value Generator (GUID v1/v3/v4/v5/v7, MD5/SHA*, base64, URL escape).

### 2.10 Edge cases and known problems (launcher)

- Elevated foreground window: Run may not receive focus unless the centralized keyboard hook option is used (learn:run).
- Alt+Space collides with the system window menu shortcut; users still report it as a hijack (#13860, open).
- Full-screen/game: Run option `IgnoreHotkeysOnFullscreen`; CmdPal has heuristics-based suppression with a triple-press override (learn:command-palette/settings).
- Multi-monitor/DPI: Run positions via WPF DIPs relative to the chosen monitor work area (MainWindow.xaml.cs `Left/Top` computed from work area, top at one quarter of remaining height); CmdPal uses explicit DPI-aware move/resize and a display-area lookup (`MoveAndResizeDpiAware`, `WindowPositionHelper.GetDpiForDisplay`).
- Freezes after resume from sleep: a blurred empty rectangle for seconds (#32806, open).
- Plugin/runtime crashes: WPF backdrop handle error at startup (#43010, closed), UWP launch null reference (#47184, closed).
- Memory complaints: see section 8.

### 2.11 Minimal reimplementation vs droppable

Need (MVP): resident hidden window; global hotkey via `RegisterHotKey` (fallback LL hook optional); debounce + cancellation; plugin trait with sync and "slow" tiers; subsequence fuzzy matcher with highlight indices; usage-count boost persisted to disk; app index from Start Menu + Desktop + packaged apps (+ optional PATH) with file-watcher/catalog events; Windows Search index file provider; calculator; system commands; web search; run-as-admin action.
Can drop: Pinyin, themes/backdrops, thumbnails, OneNote/VS Code/Registry/Service/Value-generator plugins, telemetry, plugin hot-loading, localisation of system commands, WinGet, Dock, extension gallery.

---

## 3. FancyZones

### 3.1 User-facing behaviour and default hotkeys

Source: learn:fancyzones unless stated.

- Drag a window, hold Shift (default) to show zones, release over a zone to snap. Setting "Hold Shift key to activate zones while dragging" toggles auto-snap vs manual mode. A non-primary mouse button can toggle zone activation. If both are off, zones appear immediately when dragging starts.
- Snap to several zones: hover near the shared edge of two adjacent zones (activates both; window takes the bounding rectangle), or hold Ctrl while dragging, or (setting) middle mouse for multi-zone spanning.
- Keyboard: with "Override Windows Snap" enabled, Win+Left/Right (zone index mode) or Win+arrows (relative-position mode) move windows between zones; Win+Ctrl+Alt+arrows extend to multiple zones in relative mode. Win+PgUp/PgDn cycles windows stacked in the same zone. Editor opens with Win+Shift+` per Learn (the local settings file has Win+` for the editor, a user/older value).
- Quick layout switch: Win+Ctrl+Alt+[0-9] applies a custom layout that has a number assigned; also usable during a drag.
- Rotation mode: hold Alt+X (default) and press Left/Right to rotate windows across monitors (opt-in setting).
- Editor: layouts are templates (local `layout-templates.json`: blank, focus, columns, rows, grid, priority-grid; spacing 16 and sensitivity radius 20 in the local sample) or custom Grid/Canvas layouts. Grid is relative (percent-based and resizes with the screen); Canvas zones are free rectangles and may overlap. Space around zones and "distance to highlight adjacent zones" are per-layout. Defaults per monitor orientation (horizontal/vertical).
- CLI exists (`FancyZonesCLI.exe`, commands such as `get-layouts`, `set-layout`, `set-hotkey`; learn:fancyzones).

### 3.2 Settings and defaults

Defaults from the local `FancyZones\settings.json` (property names are the config keys):

| Key | Local value | Meaning |
|---|---|---|
| fancyzones_shiftDrag | true | Shift activates |
| fancyzones_mouseSwitch | true | non-primary mouse toggles |
| fancyzones_mouseMiddleClickSpanningMultipleZones | false | middle-click span |
| fancyzones_overrideSnapHotkeys | false | Win+arrow override |
| fancyzones_moveWindowsBasedOnPosition | false | index vs relative |
| fancyzones_moveWindowAcrossMonitors | false | cross-monitor arrow moves |
| fancyzones_overlappingZonesAlgorithm | 0 | algorithm index for overlapping zones (Learn: "When multiple zones overlap"; names of the options not verified) |
| fancyzones_displayOrWorkAreaChange_moveWindows | true | keep windows in zones on resolution change |
| fancyzones_zoneSetChange_moveWindows | false | re-snap on layout change |
| fancyzones_appLastZone_moveWindows | false | new windows go to last zone |
| fancyzones_openWindowOnActiveMonitor | false | |
| fancyzones_restoreSize | false | restore size on unsnap |
| fancyzones_quickLayoutSwitch / flashZonesOnQuickSwitch | true / true | |
| fancyzones_show_on_all_monitors | false | draw zones on all monitors |
| fancyzones_span_zones_across_monitors | false | treat all monitors as one |
| fancyzones_makeDraggedWindowTransparent | false | |
| fancyzones_allowPopupWindowSnap / allowChildWindowSnap | false / false | |
| fancyzones_disableRoundCornersOnSnap | false | Windows 11 only |
| fancyzones_highlight_opacity | 50 | percent |
| zone colors | highlight #0078D7, border #FFFFFF, zone #F5FCFF, number #000000, systemTheme true | |
| fancyzones_windowSwitching, next/prev tab hotkeys | true, Win+PgDn / Win+PgUp (vk 34/33) | |
| fancyzones_excluded_apps | empty | exclusion list (shared matching logic, section 1) |

Persistence files in `FancyZones\` (local): `custom-layouts.json` (grid layout: uuid, name, `rows`, `columns`, `rows-percentage` and `columns-percentage` in units where 10000 = 100%, a `cell-child-map` matrix that assigns grid cells to zone ids, `spacing`, `sensitivity-radius`), `applied-layouts.json` (per monitor + virtual-desktop assignment, keyed by monitor name, instance id, serial number, monitor number, virtual desktop GUID), `editor-parameters.json` (monitor geometry, DPI and work area passed to the editor), `app-zone-history.json`, `layout-hotkeys.json`, `layout-templates.json`, `last-used-virtual-desktop.json`.

### 3.3 APIs and evidence

All in repo:src/modules/fancyzones.

| Concern | API / mechanism | Evidence |
|---|---|---|
| Detect drag start/end | `SetWinEventHook` out-of-context, skip own process: `EVENT_SYSTEM_MOVESIZESTART`, `EVENT_SYSTEM_MOVESIZEEND`, `EVENT_OBJECT_NAMECHANGE`, `EVENT_OBJECT_UNCLOAKED`, `EVENT_OBJECT_SHOW`, `EVENT_OBJECT_CREATE`, `EVENT_OBJECT_DESTROY`; plus `EVENT_OBJECT_LOCATIONCHANGE` handled | FancyZones/FancyZonesApp.cpp `InitHooks`; FancyZonesLib/FancyZones.cpp (switch near line 287) |
| Shift / Win / Ctrl / Alt state during drag | `WH_KEYBOARD_LL` hook plus `GetAsyncKeyState` | FancyZonesApp.cpp; FancyZones.cpp (modifier reads near line 692) |
| Non-primary mouse buttons | `WH_MOUSE_LL` hook | FancyZonesLib/MouseButtonsHook.cpp |
| Win+arrow override, rotation mode | handled in the LL keyboard hook callback (swallows keys) | FancyZones.cpp (arrow handling around lines 704-735, rotation around 817) |
| Layout hotkeys | `RegisterHotKey` on a hidden tool window | FancyZones.cpp (`RegisterHotKey` ~1578) |
| Overlay | one layered tool-window overlay per work area; Direct2D `HwndRenderTarget`; dedicated render thread | FancyZonesLib/WorkArea.cpp (`CreateWindowExW` `WS_EX_TOOLWINDOW`), ZonesOverlay.cpp (render thread, D2D factory) |
| Move/size window into zone | `SetWindowPlacement` with `WPF_ASYNCWINDOWPLACEMENT`, called twice (second call lets Windows fix scaling), waits up to 5 x 100 ms while minimised, remembers maximise state | WindowUtils.cpp `SizeWindowToRect` (comments reference issues #1685, #365) |
| Visible bounds vs window rect | `DwmGetWindowAttribute(DWMWA_EXTENDED_FRAME_BOUNDS)` | WindowUtils.cpp ~376 |
| Rounded corners toggle | `DWMWA_WINDOW_CORNER_PREFERENCE` | WindowUtils.cpp ~410-452 |
| Monitor info / change | `EnumDisplayMonitors`, `GetMonitorInfoW`, `GetDpiForMonitor(MDT_EFFECTIVE_DPI)`, `WM_DISPLAYCHANGE`, `EnumDisplayDevicesW(EDD_GET_DEVICE_INTERFACE_NAME)` for stable monitor instance id | WindowUtils.cpp; FancyZones.cpp ~931; MonitorUtils.cpp ~242 |
| Virtual desktops | reads `HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\VirtualDesktops` (CurrentVirtualDesktop, VirtualDesktopIDs) | FancyZonesLib/VirtualDesktop.cpp |
| Window filter | style checks (visible, not minimised, not tool window, root window, popup rules, owner rules, excluded list, current virtual desktop) | FancyZonesWindowProcessing.cpp `DefineWindowType` |
| DPI of target windows | DPI-aware conversion helpers for non-DPI-aware windows | WindowUtils.cpp ~293-343 (`DPIAware::Convert`/`InverseConvert`) |
| Zone hit-testing | per-layout sensitivity radius (template default 20 px), overlap resolution by zone-centre distance threshold (constant 75) | FancyZonesLib/Layout.cpp |
| Dragged window transparency | `WS_EX_LAYERED` set during drag (optional) | WindowMouseSnap.cpp ~216 |

Window-filter rules in words (FancyZonesWindowProcessing.cpp): reject minimised; require `WS_VISIBLE`; reject `WS_EX_TOOLWINDOW`; reject child windows; popup windows are allowed only when they have a thick frame and a caption or min/max box (excludes menus, start menu, notifications); reject owned windows unless "child windows" setting is on; apply exclusion list; require current virtual desktop. Windows launched by PowerToys Workspaces are skipped for automatic snapping.

### 3.4 Edge cases

- Elevated windows: FancyZones must run elevated to snap them; named examples HWiNFO64, TeamSpeak 3 (learn:fancyzones "Applications that need FancyZones to run elevated").
- Mixed-DPI multi-monitor: non-DPI-aware apps can end a few pixels off a screen edge; "span zones across monitors" requires all monitors at the same DPI scale and gets unexpected effects with rotated monitors (learn:fancyzones, setting table; WindowUtils.cpp checks DPI of all monitors).
- Apps that never raise `EVENT_SYSTEM_MOVESIZESTART` cannot be zoned: VMware Workstation Player (#666), Application Guard (#616); remote-desktop style apps (Citrix, RemoteApp, Hyper-V connection, Parsec) and some games/launchers are listed as non-zonable (learn:fancyzones table).
- Screen-capture tools conflict (ShareX #7291, Screenpresso #4478): exclusion-list workaround.
- Remote driver quirks: TeamViewer display drivers can prevent per-monitor layouts (#6945).
- Game/fullscreen: no module-specific fullscreen guard found in FancyZones source; popups and tool windows are filtered by style instead **(inference from DefineWindowType)**.
- Display change: windows re-snapped when resolution/work-area changes (setting default true).

### 3.5 Performance notes

- Closed reports of high idle CPU: #12244 (about 25 percent CPU on two external monitors, v0.41.4, 21 comments, closed) and #22517 (closed). Learn says "Show zones on all monitors" can have a performance impact.
- Costs structurally come from two global hooks (keyboard LL, mouse LL) that run on every key/click while the module is enabled, plus per-monitor D2D overlay windows and a render thread (sources above). LL hook callbacks must return within the system hook timeout or input stutters **(inference from Win32 semantics)**.

### 3.6 Minimal reimplementation vs droppable

Need: WinEvent hook for move/size start/end; modifier-key polling during drag; one overlay window per monitor (layered, click-through, topmost); grid layouts stored as percentages; zone hit-test with edge-proximity multi-zone; `SetWindowPlacement`-style snap with DWM frame-bounds compensation; per-monitor + per-virtual-desktop layout assignment keyed by stable monitor id; excluded apps list; style-based window filtering; restore-on-unsnap.
Can drop at first: canvas layouts, layout editor GUI (start with JSON/TOML + a minimal grid editor), quick-switch hotkeys, Win+arrow override (LL keyboard hook), rotation mode, window cycling, CLI, transparent drag, zone history across app restarts.

---

## 4. Color Picker

### 4.1 Behaviour and defaults

Source: learn:color-picker.

- Hotkey default Win+Shift+C (local settings confirm; also hard-coded fallback in repo:src/modules/colorPicker/ColorPicker/dllmain.cpp).
- Activation behaviour: "Pick a color first" (default locally, `activationaction = 1`) or "Open editor". Mouse actions during picking (defaults): left click = pick and open editor, middle click = pick and close, right click = close without copying. Scroll up zooms in (up to 5 levels per `ZoomWindowHelper` constants: base 50x50 px region, max level 4, max window 432 DIP), scroll down zooms out. Zooming freezes the captured image, which is how you pick non-hover states (learn tip).
- Default copy format HEX (`copiedcolorrepresentation`). Visible formats locally: HEX, RGB, HSL on; HSV, CMYK, HSB, HSI, HWB, NCol, CIELAB, CIEXYZ, VEC4, Decimal, HEX Int off.
- Format strings are token templates (local `visiblecolorformats`): HEX default `%Rex%Grx%Blx`, RGB `rgb(%Re, %Gr, %Bl)`, HSL `hsl(%Hu, %Sl%, %Ll%)`. Tokens (learn:color-picker): %Re %Gr %Bl %Al %Cy %Ma %Ye %Bk %Hu %Si %Sl %Sb %Br %In %Hn %Ll %Lc %Va %Wh %Bn %Ca %Cb %Xv %Yv %Zv %Dv %Na; channel formatters b (byte), h/H (1-digit hex), x/X (2-digit hex), f/F (float with/without leading zero). Custom formats allowed; the default new-format example uses a quoted template string.
- History: stored as `"A|R|G|B"` strings in `ColorPicker\colorHistory.json` (local), limit 20 (`colorhistorylimit`), right-click remove, export grouped by color or format. Optional color name (`showcolorname` default false). Optional cursor change (`changecursor`: local false; source default true).
- Editor: fine-tune HSV/RGB/HEX, pick similar colors, history list.

### 4.2 APIs and evidence

All in repo:src/modules/colorPicker unless stated.

- Process model: the module DLL launches `WinUI3Apps\PowerToys.ColorPickerUI.exe` with `ShellExecuteExW`, and the hotkey is registered by the runner/module which then signals a named event the UI waits on (ColorPicker/dllmain.cpp; ColorPickerUI/Helpers/NativeEventWaiter.cs). The UI is WinUI 3 with Win2D for the magnifier (ZoomWindowHelper.cs comments).
- Pixel read: GDI. A 1x1 bitmap + `Graphics.CopyFromScreen` at the cursor position, `GetCursorPos` for the position (doc: repo:doc/devdocs/modules/colorpicker.md; ColorPickerUI/Mouse/MouseInfoProvider.cs). Failure handling exists for a missing desktop DC (non-interactive session).
- Magnifier: a 50x50 px capture around the cursor is drawn by Win2D at integer zoom factors; the magnifier window is excluded from capture (`SetWindowDisplayAffinity` with `WDA_EXCLUDEFROMCAPTURE`) during the grab and re-included afterwards (Helpers/ZoomWindowHelper.cs, Helpers/WindowCaptureExclusionHelper.cs).
- Click interception: a `WH_MOUSE_LL` hook swallows left/right/middle/wheel events while active so the underlying app does not get the click (Mouse/MouseHook.cs).
- Cursor: optional system cursor replacement through `SystemParametersInfo` / restore with `SPI_SETCURSORS` (Mouse/CursorManager.cs).
- Keyboard: a `WH_KEYBOARD_LL` hook exists (NativeMethods.cs, Keyboard/*); its exact key handling (probably Esc to close) was not traced **(inference)**.
- Clipboard: written through `Helpers/ClipboardHelper.cs` (implementation not traced).
- Color math: RGB to HSL/HSV/HSB/HSI/HWB/CMYK/CIELAB/CIEXYZ/NCol/decimal in `Helpers/ColorHelper.cs`; token substitution in `Helpers/ColorRepresentationHelper.cs` and a shared `ColorFormatHelper`.

### 4.3 Edge cases

- Does not draw above Start menu or Action Center, but picking still works; fails to activate when the focused app is elevated unless PowerToys is elevated; no HDR/WCG support (learn:color-picker "Limitations"). HDR reads return brightened/white values (#29421, open).
- `CopyFromScreen` returns the composed desktop pixel in the capture DC; DPI-virtualized cursor coordinates can mismatch on mixed-DPI setups unless the process is per-monitor DPI aware **(inference; PowerToys handles DPI in ZoomWindowHelper/MonitorResolutionHelper.cs but I did not trace the exact conversion)**.
- Anti-cheat/games: no special handling found.

### 4.4 Performance

- Memory complaint: Color Picker UI reported near 1 GB together with Run and PowerAccent (#33499, v0.81.1, open). WPF-on-ARM64 allocation issue (#35547, open) covers Color Picker, Run and FancyZones editor.
- Idle: the UI process is resident waiting on an event; there is no timer polling documented. During picking, capture and zoom are driven by mouse movement and wheel events with a throttled-action helper (Helpers/ThrottledActionInvoker.cs, CoalescedAction.cs).

### 4.5 Minimal reimplementation vs droppable

Need: global hotkey; fullscreen transparent topmost overlay or LL mouse hook to intercept clicks; `GetCursorPos` + screen pixel read (GDI `GetPixel`/`BitBlt` of 1x1 and a small zoom region; DXGI Desktop Duplication is an alternative **(inference)**); magnifier window excluded from capture; HEX/RGB/HSL conversion; token-based custom formats; clipboard write; 20-item history in JSON.
Can drop: editor window with harmonies, CIELAB/XYZ/NCol/HWB formats, color names, export, cursor replacement.

---

## 5. Always On Top

### 5.1 Behaviour and defaults

Source: learn:always-on-top; repo:src/modules/alwaysontop/AlwaysOnTop/Settings.h.

- Hotkey Win+Ctrl+T toggles the foreground window. Source also adds opacity hotkeys: Win+Ctrl+Plus and Win+Ctrl+Minus, step 10, floor 20, ceiling 100 (Settings.h).
- Settings and source defaults: show border true, thickness 4 px, opacity 100, color #00ADEF (RGB 0,173,239) or system accent color (`frameAccentColor` true), rounded corners true, sound true, "show in system menu" false, "block in game mode" true, excluded apps empty. Local file has thickness 2, color #0099cc (user/older values).
- Unpin by hotkey again or by closing the window. Border follows the pinned window. Pin state per window is remembered with a window property.
- Sound: PlaySound of Windows media files for on/off and opacity changes (repo:.../Sound.h).
- Excluded apps: partial-name match, `.exe` for exact (learn:always-on-top; shared logic section 1).

### 5.2 APIs and evidence (repo:src/modules/alwaysontop/AlwaysOnTop)

- Pin/unpin: `SetWindowPos(HWND_TOPMOST / HWND_NOTOPMOST, SWP_NOMOVE | SWP_NOSIZE)`; topmost state read from `WS_EX_TOPMOST`; pinned flag stored with `SetProp` named `AlwaysOnTop_Pinned` (AlwaysOnTop.cpp ~619-650).
- Re-pin guard: on `EVENT_OBJECT_FOCUS`, any tracked window that lost topmost is pinned again (comment cites issue #19168).
- Hooks: `SetWinEventHook` with `WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS` for `EVENT_OBJECT_LOCATIONCHANGE`, `EVENT_SYSTEM_MINIMIZESTART/END`, `EVENT_SYSTEM_MOVESIZEEND`, `EVENT_SYSTEM_FOREGROUND`, `EVENT_OBJECT_DESTROY`, `EVENT_OBJECT_FOCUS`; extra hooks for menu popup start/end and `EVENT_OBJECT_INVOKED` for the title-bar system-menu item (AlwaysOnTop.cpp ~470-520, 662-840). A visibility check also catches windows closed without a destroy event.
- Border: a separate layered, topmost tool window per pinned window, created with `WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW`, color-keyed transparency, z-ordered directly above the tracked window using `SetWindowPos` with the tracked HWND as insert-after, excluded from Peek with `DWMWA_EXCLUDED_FROM_PEEK`; geometry from `DwmGetWindowAttribute(DWMWA_EXTENDED_FRAME_BOUNDS)`; drawn with Direct2D (`ID2D1HwndRenderTarget`, rounded rect geometry); a 100 ms `SetTimer` refresh also repositions it (WindowBorder.cpp, FrameDrawer.cpp, constant `REFRESH_BORDER_INTERVAL = 100`).
- Virtual desktops: the border is only shown for windows on the current desktop (`VirtualDesktopUtils::IsWindowOnCurrentDesktop`).
- System menu entry: `GetSystemMenu` + insert menu item before Close (AlwaysOnTop.cpp ~544-571).
- Transparency: temporarily adds `WS_EX_LAYERED` and `SetLayeredWindowAttributes(LWA_ALPHA)`, stores each window's original layered state/alpha/colorkey so unpin restores it exactly (AlwaysOnTop.cpp ~880-1020).
- Game guard: `SHQueryUserNotificationState` == D3D fullscreen (section 1).

### 5.3 Edge cases

- Elevated target: when PowerToys is not elevated and the new foreground window is elevated, AOT shows a notification asking for elevation (AlwaysOnTop.cpp ~812). `SetWindowPos` on a higher-integrity window fails under UIPI.
- Other topmost windows are not defeated: pinned window is above all non-topmost windows only; Citrix (#15680), RDP (#16631), VMware (#18834) can sit above it (learn:always-on-top table).
- Fullscreen exclusive apps and games: guarded by game mode; Alt-Tab and fullscreen video can hide topmost windows **(inference)**.
- DPI: border thickness is multiplied by a per-window scaling factor (WindowBorder.cpp ~212, `ScalingUtils.cpp`).

### 5.4 Performance

- No GitHub issue found with a high-CPU complaint for AOT (searches "Always on top high CPU" returned none on 2026-10-21). Cost drivers by design: one extra layered window plus D2D render target per pinned window, 100 ms timer per border, and six out-of-context WinEvent hooks.

### 5.5 Minimal reimplementation vs droppable

Need: `RegisterHotKey`; `SetWindowPos` topmost/non-topmost toggle; per-window pinned set; WinEvent hooks for destroy, minimise and focus (re-pin); excluded apps; elevated-window message. 
Can drop: border (or implement it as a plain layered window without D2D), rounded corners, accent color, sounds, opacity hotkeys, system-menu item, virtual-desktop checks.

---

## 6. Awake

### 6.1 Behaviour and defaults

Source: learn:awake; repo:doc/devdocs/modules/awake.md.

- No hotkey. Controlled from the tray menu, PowerToys Settings or `PowerToys.Awake.exe` CLI.
- Modes (settings value `mode`): 0 passive (use normal power plan), 1 indefinite, 2 timed (interval), 3 expirable (until date/time). Local file has mode 1, hours 0, minutes 0, `keepDisplayOn` false.
- "Keep screen on" (`keepDisplayOn`, default false): without it the display may turn off while the system stays awake.
- Tray: icon per mode, tooltip shows mode/remaining time; "keep awake temporarily" menu entries come from `customTrayTimes` (name -> seconds).
- CLI flags: `--use-pt-config`, `--display-on`, `--time-limit` (seconds), `--expire-at` (time or full date), `--pid`, `--use-parent-pid`; no args = indefinite (learn:awake). `--display-on` default was changed from true to false in PR #44795.
- Timed mode: changing hours/minutes while active resets the timer (learn:awake). Minutes are rounded up so the timer cannot end early (Manager.cs `SetTimedKeepAwake`).
- Lock screen: Learn says Awake does not function while the lock screen is displayed. Maintainers state that from 0.102 Awake stops holding the display on while the session is locked (comment on #48965, PR #49388) - in the current source the state calculator drops the display flag when the screen is locked (Awake/Core/AwakeStateCalculator.cs).

### 6.2 API: SetThreadExecutionState vs PowerRequest

Facts:
- Awake uses only `SetThreadExecutionState` from a dedicated, non-background thread, with `ES_CONTINUOUS | ES_SYSTEM_REQUIRED`, adding `ES_DISPLAY_REQUIRED` only when keep-screen-on is requested and the screen is not locked. Requests are queued to that thread; `ES_CONTINUOUS` alone clears the request. If the call fails Awake logs, reverts to passive mode and updates the tray icon (repo:src/modules/awake/Awake/Core/Manager.cs; Core/AwakeStateCalculator.cs; Core/Native/Bridge.cs; PR #44795 text).
- Why a dedicated thread: with `ES_CONTINUOUS` the state belongs to the calling thread and ends when that thread exits **(inference from the documented behaviour of the API; consistent with Learn text "spawns background threads")**.
- Re-application events fixed in PR #44795: system resume (`WM_POWERBROADCAST` with `PBT_APMRESUMEAUTOMATIC` / `PBT_APMRESUMESUSPEND`) and power-source change (`PBT_APMPOWERSTATUSCHANGE`) now re-issue the call. Session lock/unlock is tracked through `SystemEvents.SessionSwitch` (Manager.cs `OnSessionSwitch`).
- `PowerCreateRequest`/`PowerSetRequest` are not used anywhere in the repo; a 2026-06 issue argues they are needed for Modern Standby (S0) with display off (#48965). Maintainers closed it, asserting the change would not alter behaviour and asking for a `powercfg /requests` comparison; on battery, Modern Standby terminates system/execution requests five minutes after the sleep timeout regardless of API (comment by maintainer quoting the PowerSetRequest remarks). Treat the S0-with-display-off case as unresolved and test it on the owner's laptop **(open question)**.
- Display-required side effect: holds block idle maintenance such as SSD TRIM via Task Scheduler idle detection (documented in #44134, PR #44795).

Tray/CLI infrastructure: tray icon via Win32 `Shell_NotifyIcon` message window; single instance by named mutex; `--pid` binds lifetime to another process; `--use-pt-config` watches `settings.json` for changes (Program.cs, Core/TrayHelper.cs).

### 6.3 Performance data (measured by maintainers/contributors)

- Reported in PR #51181 ("Rewrite Awake in Rust", closed unmerged 2026-10-12, same harness, Release x64): startup 130 ms (C#) vs 16.5 ms (Rust); private memory 14.3 MB vs 2.0 MB; handles 379 vs 174; threads 15 vs 6; CPU cycles during a 10 s timed countdown 114.9 M vs 5.6 M. https://github.com/microsoft/PowerToys/pull/51181
- Reported in PR #51288 (C# NativeAOT prototype, open, "historical measurements"): managed C# 209.4 ms startup, 12.8 MiB private, 43.4 M countdown cycles per 10 s; NativeAOT 64.9 ms, 9.3 MiB, 4.4 M. https://github.com/microsoft/PowerToys/pull/51288
- Complaint history: timed mode burned CPU (#12242, up to 40 percent, v0.41.4, closed; #12385 closed). Current timed mode ticks once per second (Rx interval in Manager.cs `SetTimedKeepAwake`) to update the tray tooltip; I did not establish that this caused the old CPU reports.

### 6.4 Edge cases

- Does not work with the lock screen or after sign-out (Learn); fast user switching changes the session, so the thread's request ends with the process.
- Modern Standby laptops on battery: requests are terminated after a timeout (see above).
- Corporate policy can disable the tool (GPO check in Program.cs).
- Settings file contract: the CLI `--use-pt-config` and the Settings UI share `Awake\settings.json`; `expirationDateTime` is an ISO-8601 string with offset.

### 6.5 Minimal reimplementation vs droppable

Need: one dedicated thread holding `SetThreadExecutionState(ES_CONTINUOUS | ES_SYSTEM_REQUIRED [| ES_DISPLAY_REQUIRED])`; modes indefinite/timed/until; re-apply on resume and AC/DC change; clear state on exit; tray icon with mode and remaining time; keep-display toggle; drop display flag on lock. A Rust version needs no timer thread per second: use one timer to expiry **(inference, consistent with the Rust PR numbers above)**.
Can drop: custom tray times, `--pid` binding, GPO, telemetry, PowerRequest unless Modern Standby testing proves it necessary.

---

## 7. Clipboard history (not a PowerToys module)

- PowerToys Run has no clipboard-history plugin (plugin list in learn:run). Windows provides the feature on Win+V. Command Palette ships a Clipboard History extension that is only a viewer over the Windows feature: it checks `Clipboard.IsHistoryEnabled`, reads `Clipboard.GetHistoryItemsAsync`, subscribes to `Clipboard.HistoryChanged`, and pastes by setting content (repo:src/modules/cmdpal/ext/Microsoft.CmdPal.Ext.ClipboardHistory/Pages/ClipboardHistoryListPage.cs, Helpers/ClipboardHelper.cs). It does not store its own history.
- Windows exclusion formats (https://learn.microsoft.com/en-us/windows/win32/dataxchg/clipboard-formats, section "Cloud Clipboard and Clipboard History Formats"). All three are registered clipboard formats obtained with `RegisterClipboardFormat` using these exact names:
  - `ExcludeClipboardContentFromMonitorProcessing`: putting any data in this format keeps all formats of that clipboard item out of clipboard history and cloud sync.
  - `CanIncludeInClipboardHistory`: serialized DWORD 0 blocks the item from history; DWORD 1 explicitly requests inclusion; has no effect on cloud sync.
  - `CanUploadToCloudClipboard`: serialized DWORD 0 blocks cross-device sync; DWORD 1 explicitly requests sync; does not affect local history.
- Implication for Naobi Command **(inference)**: when the app itself writes sensitive text (a picked color is not sensitive, but a future password/secret feature would be), set `ExcludeClipboardContentFromMonitorProcessing` or the two DWORD formats; when it implements its own history it should honour the same formats from other apps (password managers set them) and skip such items.

---

## 8. Launcher latency and memory

### 8.1 How show latency is achieved

1. Resident process, window pre-created. Run: window built at startup, shown/hidden by visibility only (section 2.4). CmdPal: window created at startup and cloaked, hide = cloak while keeping the window shown, show = uncloak + foreground (MainWindow.xaml.cs). Net effect: hotkey press does not pay process start, JIT, XAML load or plugin init.
2. Plugins initialised in parallel at boot (Run `PluginManager` uses `Parallel.ForEach`, PluginManager.cs ~150) and app catalog cached in memory with filesystem/package watchers (section 2.6). Queries never walk the disk except through the Windows Search index.
3. Query pipeline is cancelable with a 20 ms debounce and tiered plugins (section 2.5), and CmdPal moved to in-place list updates and zero-allocation precomputed fuzzy matching (PR #45764 "CmdPal: Lightning-fast mode"; PR #44090).
4. Ahead-of-time compilation in CmdPal: `PublishAot` in the project; 0.93 release notes claim about 15 percent lower startup memory, 40 percent lower load time, 70 percent lower built-in extension load time, 55 percent smaller install (https://github.com/microsoft/PowerToys/releases/tag/v0.93.0).
5. Optional LL hook or `RegisterHotKey`: Run registers the hotkey natively and falls back to hooks; CmdPal exposes a low-level-hook option "to improve responsiveness".

### 8.2 Numbers found (all cited; none are Run hotkey-to-visible numbers)

| Number | Subject | Source | Quality |
|---|---|---|---|
| ~15% less startup memory, ~40% faster load, ~70% faster built-in extension load, ~55% smaller install | CmdPal 0.92 to 0.93 with full AOT | release notes v0.93.0 (https://github.com/microsoft/PowerToys/releases/tag/v0.93.0) | primary, relative only |
| Install 121 MB to 53.6 MB; memory spike 192.88 MB to 160.56 MB; window loading 1457.40 ms to 909.8 ms; built-in extension loading 846.8 ms to 246.8 ms | CmdPal 0.92 to 0.93 | Neowin article quoted in a web-search result snippet (https://www.neowin.net/news/microsoft-releases-powertoys-093-with-a-faster-command-palette-redesigned-dashboard--more/); direct fetch returned HTTP 403, so unverified secondary | secondary, unverified |
| Up to 100x faster matching vs old matcher; ~2-5x vs improved toolkit matcher; near-zero allocations | CmdPal fuzzy matcher | PR #44090 description | primary, author's claim, benchmark setup not given |
| One local sample: `Microsoft.CmdPal.UI` working set 137.1 MB, private 180.4 MB, 46 threads; `Microsoft.CmdPal.Ext.PowerToys` 23.8 MB WS / 11.1 MB private, 6 threads | CmdPal 0.12.12365 on the owner's PC, state unknown (resident, not necessarily idle) | local `Get-Process` on 2026-10-21 | single measurement |
| ~12 MB (x64) vs ~800 MB (ARM64) | `PowerToys.PowerLauncher.exe` | reporter statement in #35547 (open) | user-reported |
| almost 1 GB each | Run, PowerAccent, ColorPickerUI | screenshot-described report #33499 (open, v0.81.1) | user-reported |
| 32 GB | CmdPal | #48975 (closed, v0.100.1) | user-reported, extreme outlier |
| Awake C# vs Rust vs NativeAOT | see section 6.3 | PRs #51181, #51288 | primary, own harness |

I found no published cold-start or hotkey-to-visible latency for PowerToys Run, and no independent benchmark comparing Run and CmdPal (WebSearch 2026-10-21; Run telemetry fields exist but results are not public).

### 8.3 Process model consequences for Naobi Command

- One resident process hosting hotkey, hidden window and index gives best latency; every extra process (CmdPal runs at least its PowerToys extension as a separate process; `Microsoft.CmdPal.Ext.PowerToys` was seen locally) costs working set. **(inference)** A single Rust process with a native or lightweight GPU UI avoids the 100-200 MB WinUI/.NET baseline seen locally.
- Pre-warm checklist observed in PowerToys: create window early; keep DWM surface alive (cloak instead of destroy); have caches loaded before first keystroke; no first-use JIT (AOT).

---

## 9. Consolidated design implications

1. Resident-and-cloaked window is the proven latency design in both launchers. Copy it: create hidden at login, cloak (`DWMWA_CLOAK`) rather than destroy, uncloak + empty-input foreground trick on hotkey.
2. Awake needs only `SetThreadExecutionState` from a dedicated thread; correctness work is event handling (resume, AC/DC change, lock/unlock) rather than API choice. Modern Standby with display off is unverified and worth a one-hour test with `powercfg /requests`.
3. Always On Top = `SetWindowPos` plus a WinEvent re-pin guard; the border is the expensive part (extra window per pin, 100 ms timer). Make it optional.
4. FancyZones' hot path is WinEvent move/size hooks plus modifier polling plus an overlay; Win+arrow override requires a low-level keyboard hook, which is the part most likely to cause latency and compatibility problems. Keep it out of MVP.
5. Elevation/UIPI is a cross-module limitation (FancyZones snapping, Color Picker hotkey, Always On Top pinning, Run focus). Decide early: run elevated via a scheduled task, ship a UIAccess-signed build, or accept the limitation and show the same elevation notice.
6. Calculator, unit conversion and system commands are small and well specified; unit conversion is a separate provider from calculator in both products, so it can be added later without touching the calculator.
7. Clipboard: do not build history in MVP; if built, honour the three Windows exclusion formats.

## 10. Open questions for the owner

1. Elevation policy: is it acceptable that Naobi Command cannot snap/pin/pick over elevated windows, or should it ship elevated or UIAccess signed?
2. Launcher target: replicate CmdPal's UX (compact box, pages, dock) or Run's flat list with prefixes? Run is far simpler to implement and specify.
3. File search: rely on the Windows Search index (same as PowerToys) or implement an NTFS USN/MFT index for non-indexed drives?
4. Awake: does the owner's laptop use Modern Standby, and must display-off-but-awake work on battery? Microsoft documentation says requests are terminated on DC regardless of API.
5. Hotkey conflicts: Alt+Space (Run) collides with the window menu; Win+Alt+Space (CmdPal) is the current default. Which default should Naobi Command use?
6. Unit conversion: is a bundled small table enough, or should it match UnitsNet's 13 categories?
