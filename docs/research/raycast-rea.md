# Raycast for Windows 2.8.2.0: static reverse-engineering notes

Purpose: a clean-room record of how the installed Raycast for Windows implements the features Naobi Command will rebuild. It checks the doc-based notes in `raycast.md` against the shipped binaries. It contains behaviour and mechanism notes only: no decompiled code, no copied source, no assets.

- Target: `C:\Program Files\WindowsApps\Raycast.Raycast_2.8.2.0_x64__qypenmj9wpt2a` (MSIX, publisher Raycast Technologies Ltd.). Analysed 2026-10-21.
- Method: static analysis first, then one passive memory sample. No Raycast code was executed by the tools, and no network calls were made to Raycast services.
- Nothing was copied out of the package. No scratch copies were needed; the helper scripts live in the session scratchpad.
- Prompt-injection check: no strings in the target were directed at an analyst or agent.

## 0. Evidence legend

| Tag | What | Tool |
|---|---|---|
| E1 | `Raycast.dll` P/Invoke + ModuleRef inventory (817 imports, 28 modules), `ev_bad8824cb0209e0506ef3e703f9472d44d0c01c2cf79c585e4d93f72ba161537` | REA `inspect_managed_native_boundaries` |
| E2 | `indexer.win32-x64-msvc.node` identity, rustc commit inference, `ev_38dbba29c4da59eb0082c1a43ba34386c4558654a407a3bfa4d63bb219c703c0` | REA `inspect_rust_binary` |
| E3 | `data.win32-x64-msvc.node` identity, `ev_c0315ce0a0298eb2c4e87bdefe132a78cd3f9c502c65fdf73c71d2dfdc4665a9` | REA `inspect_rust_binary` |
| E4 | `Raycast.UIAccess.exe` resource tree (RT_MANIFEST at file offset 5030072), `ev_3a88bd7bc3488a518340eccb2c16144bee707cae4ff1fd7b634fe40f75f32230` | REA `inspect_pe_resources` |
| E5 | backend JS graph (index.mjs + workers), summary `ev_f05bdb06…`, parent `ev_ebda455eda36836b9399c2fa280156631a25e28b0d7b142782ad509ddd2520a4` | REA `analyze_javascript_application` |
| E6 | managed member inspection of `Raycast.dll` **failed** (metadata over the 64 MiB budget), `ev_db18c678…`, `ev_43a57d50…` | REA `inspect_managed_members` |
| U1, U2 | Recorded unknowns `unk_aa7b0f1b…` (show latency / webview suspension), `unk_975221b6…` (DB key custody) | REA `record_unknown` |
| [imp] | PE import tables (IAT and delay-load) parsed by a scratch script | Not REA (no REA tool gives PE imports without a Ghidra session) |
| [str] | ASCII/UTF-16 string scan of the binaries, plus grep of the JS/CSS bundles | Not REA. These strings are **observations of literals**, not proof of execution. |
| [rt] | `Get-Process` / `Win32_Process` sample | PowerShell |

Confidence words: **observed** = literally present in the artifact; **inferred** = my reading of names and strings; **unknown** = not settled.

REA coverage limits: Ghidra is available, but I did not open a native Ghidra session (binaries are 5–46 MB, and string and import evidence answered the questions). Managed member decoding failed (E6). JS analysis of `index.mjs` (7.4 MB) ran out of worker heap (E5 partial). The JS matcher notes below come from a direct read of the bundle text.

---

## 1. Process and component map (Q1)

### 1.1 Corrections to `raycast.md`
- The host is **.NET 10.0.12, self-contained** (WPF + WinForms + CsWinRT), not .NET 8. `Raycast.runtimeconfig.json`: tfm `net10.0`. `mscordaccore_amd64_amd64_10.0.1226…` is shipped [str]. Manifest: `asInvoker`, `dpiAwareness PerMonitorV2`, `longPathAware` [str].
- The "Rust core" is **not one process**. Rust ships as six separate artifacts: an in-process host DLL, a UI-access helper exe, a Windows service exe, a content-extractor exe, and several N-API addons loaded into Node.
- The calculator is **Swift**: SoulverCore (`SoulverCoreDynamic.dll`, `swiftCore.dll`, Foundation, ICU, about 80 MB total) behind an N-API addon. A new Rust engine sits behind a feature flag (see §7).

### 1.2 Inventory (observed)

| Artifact | Kind | Role (evidence) |
|---|---|---|
| `Raycast.exe` + `Raycast.dll` (7.7 MB) | .NET 10 WPF apphost | Windows, tray, WebView2 hosting, settings, IPC hub. Namespaces include `Raycast.WebView.MainWindow`, `Raycast.InvisibleMainWindow`, `Raycast.Handlers.*` (Clipboard, Hotkey, WindowManagement, VirtualDesktops, Wifi, …) [str] |
| `native.dll` (7.3 MB) | Rust cdylib, **UniFFI** | P/Invoked by the host as module `native`. UniFFI namespaces: `native`, `raycast_host`, `shortcuts`, `display`, `window_management` (E1). Crates: `native-dll`, `clipboard`, `pipe-rpc`, `ui-access-rpc`, `rust/raycast-host` (logger, runtime, sentry), `window-management`, `confetti` (D3D11) [str] |
| `Raycast.UIAccess.exe` (5 MB) | Rust exe, manifest **`uiAccess="true"`** (E4) | Global hotkeys, LL keyboard hook, hyper key, snippet expansion, input injection, window arrange/foreground, clipboard reads. Crates: `ui-access-helper`, `shortcuts` (low_level_hook, hook_liveness, hyperkey, keystroke_buffer, snippets), `window-management` (arrange_window, foreground_tracker), `common::uia` [str] |
| `Raycast.SystemService.exe` (4.5 MB) | Rust Windows service (`windows-service` crate) | Name `raycast-system-service-Production`, "Raycast System Service (Production)". Spawns the NTFS scanner and launches the elevated UI-access helper. CLI verbs: `install/initialize/uninstall/start/stop/status/scan/usn-read` [str][imp] |
| `Raycast.StartupLauncher.exe`, `Raycast.TraceRecorder.exe` (ETW: `StartTraceW`, `EnableTraceEx2`), `Raycast.CrashHelper.exe` (.NET) | helpers | Login launch, perf tracing, crash dumps [imp][str] |
| `backend\node.exe` (87 MB) + `index.mjs` (7.4 MB ESM bundle) | Node backend | One long-lived process. It hosts extensions, settings, frecency, AI and so on; workers: `indexer-worker.mjs`, `calculator-worker.mjs` (`worker_threads`) (E5) |
| `backend\data.win32-x64-msvc.node` (46 MB) | Rust N-API | Data layer: rusqlite with an **encrypted SQLite** layer, sync agent, repositories (Clipboard, Frecency, Emoji, Notes, …) (E3)[str] |
| `backend\indexer.win32-x64-msvc.node` (18 MB) | Rust N-API | File indexer `indexer/core` (actors, `minidex` segmented index, `fst`, `rkyv`, `memmap2`), `fs-utils` (change_journal, volume_watcher), `ntfs-scanner-client`, `semantic-index` (content extraction, Windows OCR, PDF render), `cpu-budget` (E2)[str] |
| `backend\Raycast Content Indexer.exe` (7.7 MB) | Rust exe | Out-of-process content extraction (docx/zip/image/rtf/pdf, OCR), CPU-budgeted [str] |
| `backend\fs-utils…node`, `process-utils…node` | Rust N-API | Volume info, image thumbnails, process helpers [imp] |
| `backend\calculator-rs…node` | Rust N-API (`rust_decimal`, `jiff`, `icu_calendar`) | In-house calculator behind flag `rust-calculator` [str] |
| `backend\Calculator.node` + Swift runtime + `SoulverCore_SoulverCore.resources` | Swift N-API | Default calculator engine (SoulverCore) [imp][str] |
| `backend\resources\ripgrep\rg.exe` | tool | Used for text search (exact use not traced) |
| `frontend\` (1363 files) | Vite React bundles | One HTML entry per window: `main-window.html`, `settings-window.html`, `ai-chat-window.html`, `notes-window.html`, `welcome-window.html`, … [str] |
| `api\` | `@raycast/api` + react-reconciler | Extension runtime and templates |

Other libraries shipped next to the host: `e_sqlite3.dll` (Microsoft.Data.Sqlite), `H.NotifyIcon` (tray), `Wpf.Ui` (Fluent controls), `NAudio` (dictation), `UglyToad.PdfPig`, `zxing` (QR), `Microsoft.Recognizers.Text.*`, Sentry, Serilog.

### 1.3 How the parts talk (observed unless marked)

```
WebView2 page (React) --hostObjects.ipcRequest.postMessage-->  Raycast.exe (.NET)
       ^--------------- PostWebMessageAsJson / 'message' event ----'   |
Raycast.exe --P/Invoke (UniFFI)--> native.dll --+--stdio--> node.exe (index.mjs)
                                                 |             |-- worker_threads -> indexer.node / Calculator.node
                                                 |             |-- N-API -> data.node (encrypted SQLite)
                                                 |             '-- spawn -> Raycast Content Indexer.exe
                                                 '--named pipe \\.\pipe\RaycastUIAccessRPC (+ ...RPCLog)--> Raycast.UIAccess.exe
indexer.node --pipe \\.\pipe\raycast-system-service-Production (control)--> Raycast.SystemService.exe (LocalSystem)
indexer.node <--pipe \\.\pipe\raycast-ntfs-scanner-Production (data)-- SystemService "scan" child
```
- Page → host: the frontend calls `window.chrome.webview.hostObjects.ipcRequest.postMessage(...)`. This is a COM host object (`AddHostObjectToScript`). Host → page: `PostWebMessageAsJson`, received as a `message` event [str]. The typed IPC classes are generated (`IPCTypes*`, `*SerializeHandler`, System.Text.Json source-gen) [str].
- Host → backend: a Rust `Coordinator` in `native.dll` (`uniffi_raycast_host_fn_method_coordinator_{start,send,notify,stop,boost_execution_speed_for,note_power_transition,note_windows_open}`). Callbacks: `on_request`, `on_notification`, `on_backend_stall`, `on_backend_log` (E1). One string says the backend is a spawned Node child reached over stdio; another says closing stdin signals graceful shutdown [str].
- Host → UI-access helper: the `pipe-rpc` crate over `\\.\pipe\RaycastUIAccessRPC`, with log forwarding on `…RPCLog` [str]. The helper exits when the parent Raycast process dies ("Parent Raycast process has exited unexpectedly") [str].
- The manifest declares `windows.fullTrustProcess` for `Raycast.UIAccess.exe /uiaccess`, plus capabilities `uiaccess`, `allowElevation`, `runFullTrust`, `packageManagement`, `unvirtualizedResources` (AppxManifest.xml).

---

## 2. Global hotkey (Q2)

- **Where:** the hotkeys do not live in the WPF host. The host's 817 P/Invokes include `SetWindowsHookExW` but **no `RegisterHotKey`** (E1). `Raycast.UIAccess.exe` imports `RegisterHotKey`, `UnregisterHotKey`, `SetWindowsHookExW`, `CallNextHookEx`, `RegisterRawInputDevices`, `GetRawInputData`, `SendInput`, `GetAsyncKeyState` and `MapVirtualKeyExW` [imp]. The host drives it through `uiaccess_register_hotkeys`, `unregister_hotkey(s)`, `validate_hotkey`, `set_hotkeys_suppressed`, `start/stop_recording_mode` and `set_hyperkey` (E1). Matches arrive back through `nativeevents_on_binding_triggered` (E1).
- **Hybrid dispatch (observed log templates):** each binding is first tried with `RegisterHotKey`. A binding is "demoted to hook dispatch" (the WH_KEYBOARD_LL hook) in four cases:
  - `RegisterHotKey` refuses the combination because another app or the OS owns it;
  - the id falls outside the RegisterHotKey range;
  - the binding is push-to-talk and needs the key-release event;
  - hotkeys are currently suppressed.
- Hotkey kinds in the `shortcuts` crate: `SingleTap`, `DoubleTap`, `BothSidedSingleTap`, `LayoutDependent` (virtual-key) and `LayoutIndependent` (scan-code), with left/right modifiers distinguished [str].
- **Default launcher hotkey on Windows is a single tap of the Windows key.** The frontend default settings object gives `globalHotkey` = SingleKey, Modifier `Meta`, locality Global [str: `register-query-client-*.js`]. That cannot be done with `RegisterHotKey`, so it relies on the LL hook. This settles the conflict noted in `raycast.md` §2: the default is the Win key; Alt+Space is not the default.
- **Alt+Space / conflicts:** observed facts:
  - when a combination is taken (for example by PowerToys Run or Command Palette), `RegisterHotKey` fails and Raycast falls back to the LL hook;
  - the settings layer refuses to give a command the same chord as the global hotkey (backend error string);
  - the validator returns a `HotkeyValidity`/`ValidationVerdict`;
  - logs warn that "`RegisterHotKey` refused the combination: another application (or the OS) owns it" [str].
  - Inferred: because LL hooks see keys before system hotkey dispatch, the hook path can effectively "win" a contested chord.
- **Hook robustness (observed):**
  - Raw Input is registered as a liveness probe (`hook_liveness.rs`). If raw keyboard events arrive with no hook callback (Windows silently unhooks slow LL hooks), the hook is reinstalled.
  - The hook's hot code pages are pinned in RAM by raising the working-set floor (`SetProcessWorkingSetSizeEx` plus a lock of the executable region), logged as "Pinned N KiB of hook-critical code".
  - The process sets a high priority class (`SetPriorityClass`).
  - Injection uses `SendInput` and `NtUserInjectKeyboardInput` (available to a uiAccess process).
  - A tag value (`set_keyboard_extra_info`, plus AutoHotkey extra-info constants) lets it recognise its own injected keys and AutoHotkey's [str][imp].
- The host caches the activation hotkey so that registration happens immediately at startup, before settings load: "next startup will register the default until settings load" [str].

## 3. Show latency / pre-warm (Q3)

Observed in `Raycast.dll` identifiers and log templates [str] (bodies were not decompiled, E6):
- **Pre-created, hidden, parked window.** Relevant names: `InvisibleMainWindow`, `ParkHiddenWindowOnTargetMonitor`, `PrepareWebViewForTargetMonitor`, `PositionWindowOnTargetMonitor`, `ResizeForTargetMonitor`, `ResolveMouseMonitor` / `ResolveActiveWindowMonitor` / `ResolvePresentationTargetMonitor`, `ScheduleMainWindowGeometryFixup` (`dpi-geometry-fixup`), and the log line "Off-thread hidden parking did not complete; will retry".
  - Inferred: the hidden window is moved to the target monitor (and its DPI) before it is shown, so showing it causes no DPI relayout.
- **Background throttling disabled.** The WebView2 browser arguments include `--disable-background-timer-throttling` with `--renderer-process-limit=2` (an alternate set uses `--single-process`). One renderer was running at idle [rt].
- **Selective suspension.** Each webview has a `SuspendWhenHidden` flag and `TrySuspendAsync`/Resume ("Failed to suspend hidden webview", "Webview … declined suspension; retrying while hidden").
  - Inferred: secondary windows are suspended when hidden and the main window is not. U1 is open for this.
- **Warm-up after sleep and at startup:** `PrewarmWebViewAfterWake`, `PostWakeMemoryPrewarm` ("post-wake webview warm-up"), `DefaultKeepWarm` / `keepWarmFor`, `RegisterSuspendResumeNotification`, and a "startup power boost". The latter is the Coordinator's `boost_execution_speed_for`, which toggles process power throttling (EcoQoS) off for a period ("Execution speed boost enabled ({Reason})").
- **Single instance:** a second launch forwards its deeplink and exits ("Detected existing Raycast instance during …") [str]. `InterRaycastPipe` [str].
- Measured latency: **unknown** (U1). I did not trigger the hotkey.

## 4. Clipboard history (Q4)

- **Monitoring:**
  - Both `native.dll` and the UI-access helper import `AddClipboardFormatListener`, `RemoveClipboardFormatListener`, `GetClipboardSequenceNumber`, `GetClipboardOwner`, `EnumClipboardFormats` and `GetClipboardFormatNameW` [imp]. The listener lives in `clipboard\src\change_listener.rs` on a hidden message window (`common::message_window`) [str].
  - WinRT `Windows.ApplicationModel.DataTransfer.Clipboard` is **not** used for monitoring. No such literal was found; Win32 only.
  - Debounce and retry: some owner processes get an "extended debounce". If another process holds the clipboard open, capture is retried with a delay up to a maximum, and the holder is logged [str].
- **Privacy flags honoured (observed literals and log text):**
  - `ExcludeClipboardContentFromMonitorProcessing`: an item carrying this flag is skipped ("Clipboard content has ExcludeClipboardContentFromMonitorProcessing flag").
  - `CanIncludeInClipboardHistory` is read ("Failed to read CanIncludeInClipboardHistory value").
  - `CanUploadToCloudClipboard` is registered in the helper [str].
  - A skip flag for its own writes (`skip_from_history`) and an app-private format `application/raycast-clipboard-metadata` mark Raycast-originated content.
  - File lists that point into TEMP are skipped [str].
  - The per-app "Disabled Applications" list and organisation retention are settings (`OrganizationClipboardHistoryDisabledApplications`, `…RetentionDays`) [str].
- **Data model:**
  - Each entry has items, and each item has several **representations** keyed by MIME type: `text/plain`, `text/html`, `text/rtf`, `text/uri-list`, `image/png`, `image/jpeg`. CF_* names are mapped [str].
  - SQLite schema in `data.node` (observed DDL):
    - `clipboard.entries`: id, title, created/updated_at, workspace_id, source_name/host/url, copy_count, pinned;
    - `clipboard.items`;
    - `clipboard.representations`: item_id, mime_type, content_type, content TEXT;
    - category and tag router tables, filters, workspaces.
  - Retention runs as SQL deletes by `updated_at` that spare pinned rows [str].
- **Images:** GDI bitmaps and DIBs are converted to PNG via WIC (`gdi_to_bmp`, `bmp_to_png_unsafe` messages). Large or file-backed content is moved into a clipboard cache directory, and the row refers to it ("Could not move file-backed content to clipboard cache directory"). OCR runs on images for search [str].
- **Storage and encryption (observed on disk):** `%LOCALAPPDATA%\Raycast\clipboard.db` and every other `*.db` (main, frecency, settings_v2, ai, …) have **no `SQLite format 3` header**; the first 16 bytes look random. So they are page-encrypted, in WAL mode. `data.node` contains a `sqlite-encryption` crate and messages for keying, rekeying, validating a key, and encrypting an existing plaintext DB in place [str].
  - The host stores secrets in **Windows Credential Manager** (`CredReadW/CredWriteW`) with a **DPAPI mirror** (`CryptProtectData`) as a fallback and self-heal path. There is a `DatabaseEncryptionKeyData` type and a `%LOCALAPPDATA%\Raycast\creds\` folder (E1)[str].
  - Inferred: the DB key is held there and passed to the backend. U2 is open for this.
- Paste: snapshot the clipboard, set the entry, inject Ctrl+V into the last foreground window (`paste_in_foreground_app`, `SendInput`), then restore unless the clipboard changed meanwhile ("skipped deferred clipboard restore because clipboard changed") [str].

## 5. File search indexer (Q5)

- **MFT and USN journal: yes, through a privileged service.**
  - `Raycast.SystemService.exe` (a LocalSystem service) has a `scan` mode, the "NTFS scanner data-pipe server". It enumerates the MFT when no snapshot exists ("No persisted snapshot for drive, enumerating MFT") and persists a zstd snapshot `NtfsScanner\snapshot.bin.zst`. It also has `usn-read` (reading USN records: CREATE/DELETE/RENAME/CLOSE/ATTRIB), which handles NTFS only and exits if the journal was reset [str][imp: `DeviceIoControl`].
  - `indexer.node` asks the service to launch the scanner over the control pipe and reads results from `\\.\pipe\raycast-ntfs-scanner-Production` (crate `ntfs-scanner-client`). Settings flag `useNtfsScanner`, state `NtfsScannerState` [str].
  - The service checks pipe callers (`GetNamedPipeClientProcessId`; "failed to resolve control-pipe caller") [imp][str].
- **Incremental updates:**
  - Per-volume USN cursors (`journal_id`, `usn`) are stored in `change-journal.json`. On start, the indexer catches up from the journal instead of crawling; it re-crawls if the journal was recreated or the backlog is too large.
  - Live changes come from `ReadDirectoryChangesW` watchers (`volume_watcher/windows`). If the watcher drops events they are replayed "from history". A poll driver covers roots that are too slow to watch.
  - Non-NTFS or slow roots fall back to `walkdir`/`FindFirstFileExW` crawling with `walkThreads` and `maxWalkDepth` [str][imp].
- **Index format:**
  - A custom **`minidex` segmented index**: `fst` for name search (checksum-verified, "FST verification failed"), `rkyv` zero-copy records and `memmap2`, a WAL `journal.wal`, a lock file `.minidex.lock` and background **compaction** ("file index compaction", which needs free disk space and runs under a CPU budget).
  - Storage is `%LOCALAPPDATA%\Raycast\index\db`, which on this machine held only an empty `journal.wal`. Inferred: file search is not enabled or not built here.
  - There is a separate `semantic-index` SQLite DB with FTS5 (migrations V1–V10: contentless FTS, zstd content, …) for document content and OCR.
  - Windows Search is used only for "native content search" (`native_search/windows`) [str].
- **Exclusions (observed rule literals):**
  - the ripgrep `ignore` crate honouring `.gitignore`, `.ignore`, `.rayignore` and `.git/info/exclude`;
  - `CACHEDIR.TAG` directories skipped as self-declared caches;
  - hidden files filtered;
  - built-in negative globs for `$RECYCLE.BIN`, `System Volume Information`, `$Windows.~BT`/`~WS`, `Config.Msi`, VCS dirs (`.git .hg .svn .jj .bzr .sl`), build caches (`.cache`, `.dart_tool`, `.angular/cache`, `*.tsbuildinfo`, …), editor and shell history files, `node_modules`, `*.tmp/*.temp`, and AI-tool caches (`.claude/*`, `.aider*`);
  - `\\wsl$` and `\\wsl.localhost` are special-cased;
  - settings: `disallowedPaths`, `excludePatterns`, `includeHiddenFiles`, `indexContentNonLocalVolumes` [str].
- Resource governance: `cpu-budget` crate, `WalkPace`, a pause on low disk or low power, "system woke, resuming indexing", `SetThreadPriority`, and `GetDiskFreeSpaceExW` checks [str][imp].

## 6. Window management (Q6)

- **APIs:**
  - Host P/Invokes: `SetWindowPos`, `Get/SetWindowPlacement`, `IsWindowArranged`, `DwmGetWindowAttribute`/`DwmSetWindowAttribute`, `GetMonitorInfoW`, `MonitorFromWindow/Rect/Point`, `EnumDisplayMonitors`, `GetDpiForMonitor`, `GetDpiForWindow`, `GetSystemMetricsForDpi`, `QueryDisplayConfig`/`DisplayConfigGetDeviceInfo`, `SetWinEventHook`, `RegisterShellHookWindow`, `SetWindowCompositionAttribute` (E1).
  - CsWin32 constants present: `DWMWA_EXTENDED_FRAME_BOUNDS`, `DWMWA_CLOAKED`, `DWMWA_SYSTEMBACKDROP_TYPE`, `DWMWA_WINDOW_CORNER_PREFERENCE`, `SPI_GETWORKAREA` [str]. Presence of a constant does not prove its use.
  - Inferred: frame bounds and work areas follow the standard pattern (subtract the invisible DWM border), because `native.dll` documents a per-display "Work area, in the same coordinate space as `bounds`" [str].
- **Who moves windows:** the UI-access helper (`window-management::arrange_window`, `set_window_pos`, `maximize/minimize/restore`, `toggle_window_topmost`, `force_activate_window`) [str].
  - Arrange respects the user's Snap setting: "window snapping is disabled in Settings; leaving … unarranged". It skips non-resizable windows and windows on other virtual desktops [str].
  - Virtual desktops use undocumented shell COM (`get/switch/move_window_to_virtual_desktop`, wallpaper per desktop) in `native.dll` (E1).
  - Foreground tracking: `foreground_tracker` keeps the last non-Raycast foreground window. Raycast's own windows carry the `RaycastExcludeFg` property so they never enter that cache [str].
- **Elevated windows:**
  - The helper normally runs with `uiAccess=true`. That bypasses UIPI, so it can focus, move and type into elevated windows without admin rights (E4).
  - Optional setting `use_elevated_ui_access_helper`: "launch the UI Access helper elevated through the Raycast System Service". This falls back to a direct launch if the service is unavailable or the user is a standard account. The service uses `CreateProcessAsUserW`/`DuplicateTokenEx` [str][imp].
- DPI: the host is PerMonitorV2 and computes `GetCombinedScaleForMonitor`, `HasMultiMonitorDpiVariation` and `ApplyPhysicalSizeForCurrentMonitor` [str].

## 7. Calculator and root-search ranking (Q7)

- **Calculator:** the default engine is **SoulverCore** (Swift, licensed library) loaded as `Calculator.node` inside a Node `worker_threads` worker (`calculator-worker.mjs`).
  - The worker caches one calculator per (locale, rem base, work hours, datePicker) configuration and feeds it currency rates. It reports missing rates back so the backend can fetch them; missing rates mean offline returns stale or no rates.
  - A second, in-house **Rust calculator** (`calculator-rs`: rust_decimal, jiff, icu_calendar; covers units, currency, time zones, flights, travel time, weather and finance phrases) is selected by the feature flag `rust-calculator`. It is shadow-compared against Soulver ("Shadow comparison crashed" log) [str].
  - Takeaway: Raycast is migrating off a third-party engine to its own Rust parser.
- **Root-search fuzzy matcher (observed in the `backend/index.mjs` text):** a subsequence matcher with a two-row DP score. Behaviour:
  - Text is lower-cased. Word starts are positions after a separator or a lower→upper camelCase hump. Separators: space, tab, newline, `- . / ( ) [ ]`.
  - Per-character score: matching text index 0 = 4, a word start = 3, any other position = 2, a separator in the query matching a separator in the text = 1. Each gap costs 1 (consecutive matches cost nothing). Unmatched separators in the query are counted as "skipped" rather than failing.
  - Exact equality is a special top result. For queries longer than 2 characters, a quick subsequence pre-check rejects non-matches before the DP runs.
  - **Sensitivity** (`rootSearchSensitivity`, default `high`) is a threshold on the score: low = any match; medium = score ≥ 1.5·(len−skipped−2)+4; high = score > 2·(len−skipped).
  - A separate whole-word check (query found with word boundaries on both sides) and alias **prefix/exact** matching sit beside the fuzzy score. Sort ties use `Intl.Collator(numeric, base)`.
- **Frecency (observed DDL in `data.node`):**
  - `frecency.root_search(item_id, frecency_date, search_terms, opened_at)` plus an FTS table `frecency.root_FTS` over `search_terms`.
  - Ranking reads rows ordered by `frecency_date DESC`. That is a single "virtual date" column which each open pushes forward, so recency and frequency fold into one sortable value.
  - Query-specific frecency comes from the FTS match on the stored search terms. Each command can opt out (`omitFromFrecencyTracking`) [str].
  - The exact advance formula for `frecency_date` sits in Rust and was not recovered (low impact).

## 8. Launcher layout metrics (Q8, numbers only)

From `frontend\*.css` and the default settings [str]. Every length is multiplied by `--spx` = 1px × zoom, where zoom comes from the `uiZoom` setting (default 1):

| Metric | Windows value |
|---|---|
| Main window width | 750 |
| Expanded window height | 475 |
| Header (search bar) height | 66 (macOS 64) |
| Footer (action bar) height | 42 |
| List row height (virtualizer estimate) | 40 |
| List icon size | 22 in root list, 16 in compact lists |
| Font sizes used | 11, 13 (body), 16, 18 |
| Spacing scale | 4, 6, 8, 12, 16, 24; root padding 8; list item padding-x 8 |
| Window corner radius | 8px on Windows (macOS 20/26) |
| UI font | `Segoe UI Variable, Segoe UI, InterVariable, sans-serif`; mono `JetBrainsMono` |
| Defaults | windowMode `expanded`, presentation `mouse-screen`, popToRoot 90 s, Esc `pop-back-or-close` |

## 9. Runtime sample (passive)

Raycast was **already running** when I started (started 23:15:44, sampled 23:28:22, so about 13 min idle). I did not launch it, open its UI, log in or change settings, and I left it running because I did not start it. Onboarding state was not inspected. Sizes in MB [rt]:

| Process | Working set | Private | Peak WS |
|---|---|---|---|
| Raycast.exe (.NET host) | 324.8 | 211.1 | 341.4 |
| node.exe (backend) | 175.3 | 171.6 | 200.4 |
| msedgewebview2 browser | 136.9 | 44.7 | 152.0 |
| msedgewebview2 renderer (1) | 133.8 | 89.2 | 212.7 |
| msedgewebview2 gpu-process | 124.1 | 141.8 | 202.3 |
| msedgewebview2 utility ×2 + crashpad | 72.3 | 20.9 | — |
| Raycast.UIAccess.exe | 16.1 | 5.3 | 16.6 |
| conhost (node child) | 7.5 | 1.2 | — |
| **Total** | **≈991 (sum, includes shared pages)** | **≈686** | |

The SystemService and Content Indexer processes were not running, and no Raycast service is installed. Baseline: no PowerToys runner was running. The only PowerToys processes were Command Palette: `Microsoft.CmdPal.UI` (WS 133.3 / private 177.7 / peak 308.5) and `Microsoft.CmdPal.Ext.PowerToys` (23.2 / 11.3).

Note that the Rust UI-access helper (hooks, clipboard and window ops) costs only about 16 MB working set. The whole native "systems" layer fits the Naobi budget; the cost is in .NET, WebView2 and Node.

---

## 10. Unknowns that matter

1. **Show latency and main-webview suspension.** Not measured; still open (U1). Needs a hotkey-triggered ETW/DWM timing run.
2. **DB key custody.** Credential Manager + DPAPI mirror is inferred, not traced (U2). The managed metadata was too large for REA's member decoder (E6).
3. **Exact Win-key-tap handling.** How Raycast keeps the Start menu from opening after a lone Win tap (dummy-key injection is likely given `NtUserInjectKeyboardInput`) was not confirmed.
4. **`minidex` on-disk layout** beyond fst + rkyv + WAL + segments, and the incremental merge policy.
5. **`frecency_date` advance formula.**
6. **Window-arrange geometry details**, such as gap handling and whether DWM extended frame bounds are actually subtracted. Needs a Ghidra pass on `arrange_window.rs` code in `Raycast.UIAccess.exe`.

---

## 11. Implications for Naobi Command

Budget reminder: hotkey-to-visible p95 ≤ 50 ms, idle RAM ≤ 40 MB.

1. **Copy the split, not the stack.** Raycast's native layer (UI-access helper ≈ 16 MB WS, Rust) does hooks, clipboard and window ops well. Its RAM goes to .NET (≈325 MB WS), WebView2 (≈470 MB WS across 6 processes) and Node (≈175 MB). Naobi should be a single Rust process with native rendering (no WebView, no Node, no .NET). Idle ≤ 40 MB is realistic only that way.
2. **Hotkeys: use the hybrid scheme.**
   - Register with `RegisterHotKey` first. Fall back to a `WH_KEYBOARD_LL` hook only for chords that are refused (Alt+Space already owned by PowerToys or CmdPal), modifier-only taps (Win tap, double-tap) and press/release semantics.
   - Run the hook on a dedicated high-priority thread that does no allocation or I/O in the callback.
   - Add a Raw Input liveness probe that re-installs the hook if Windows drops it, and lock the hook's code pages in the working set.
   - Tag injected input via `dwExtraInfo` so the hook ignores its own keys.
   - Default to a chord that is free; detect and report conflicts with a visible message instead of silently stealing.
3. **Elevated windows:** a `uiAccess=true` helper requires a signed binary installed under Program Files (or MSIX). For a personal tool, first ship without it: document that admin windows cannot be controlled unless Naobi itself is run elevated. Add an optional elevated helper later, launched through Task Scheduler "run with highest privileges" rather than a SYSTEM service.
4. **Show latency:**
   - Pre-create the launcher window at startup and keep it hidden: move it off-screen, or cloak it with `DWMWA_CLOAK` so DWM keeps its surface.
   - Park it on the monitor where it will appear, at that monitor's DPI, so showing it is only a cloak/uncloak or `ShowWindow` + `SetForegroundWindow`.
   - Never suspend or throttle the launcher's render path.
   - Opt the process out of EcoQoS briefly at show time (`SetProcessInformation(ProcessPowerThrottling)`), and handle `RegisterSuspendResumeNotification` to re-warm after sleep.
   - Measure with ETW from WM_HOTKEY / hook callback to first DWM present.
5. **Clipboard:**
   - Use `AddClipboardFormatListener` on a message-only window. Read once per `GetClipboardSequenceNumber` change, with debounce and a bounded retry when another process holds the clipboard.
   - Must honour `ExcludeClipboardContentFromMonitorProcessing`, `CanIncludeInClipboardHistory` (value 0) and `CanUploadToCloudClipboard`, plus a per-app deny list (resolved via `GetClipboardOwner` → PID → exe).
   - Mark own writes with a private registered format to avoid loops.
   - Store entry → items → representations (MIME-keyed). Keep text inline and images as PNG files on disk (convert DIB via WIC), referenced by path.
   - Encrypt the DB: SQLCipher-style page encryption, or simpler, encrypt content blobs with a key wrapped by DPAPI (`CryptProtectData`, CurrentUser). Skip Credential Manager unless sync is needed.
6. **File search:**
   - Reading the MFT and USN journal needs admin. Raycast pays for a SYSTEM service to do it. For Naobi, start unprivileged: crawl user scopes with `FindFirstFileExW(FIND_FIRST_EX_LARGE_FETCH)` on a worker pool, keep it fresh with `ReadDirectoryChangesW`, and use the USN journal only when available (FSCTL_QUERY_USN_JOURNAL works for admin; keep a stored cursor so restarts catch up instead of re-crawl).
   - Index: an `fst` map of normalized names → ids plus a compact rkyv/mmap record table; append-only delta + periodic merge mirrors `minidex`.
   - Reuse the `ignore` crate semantics (`.gitignore`, `.ignore`, own `.naobiignore`, `CACHEDIR.TAG`) and Raycast's noise list (`$RECYCLE.BIN`, `System Volume Information`, VCS dirs, `node_modules`, build caches, temp).
   - Never block the UI on the indexer; show a progress row.
7. **Ranking:** implement the same family of matcher, a subsequence DP with bonuses for first character / word start / camelCase and a gap penalty. The `nucleo` or `frizbee` crates give this with SIMD.
   - Expose a 3-level sensitivity as a score threshold relative to query length.
   - Aliases are exact/prefix only and rank above fuzzy.
   - Frecency can be one "virtual timestamp" per item, advanced on each open (sortable single column), plus per-query association (query → item counts). Store it in SQLite.
8. **Calculator:** do not depend on Soulver (Swift runtime ≈ 80 MB of DLLs). Raycast itself is moving to a Rust engine (`rust_decimal` + `jiff`). Naobi can start with an expression parser over `rust_decimal`, plus a units table and cached currency rates. Calculate in the UI thread only if under 1 ms; otherwise use a worker with a cancel token.
9. **Layout target (for parity feel):** window 750 wide, ~475 tall expanded, 66 px search header, 40 px rows, 42 px footer, 22 px icons, 13 px body text, Segoe UI Variable, 8 px corner radius, all scaled by monitor DPI × user zoom. Default position: monitor under the mouse.
10. **Avoid Raycast's process sprawl** (host, helper, service, Node, 6 WebView2 processes, content indexer). Naobi: one process, with an optional elevated helper later. Each extra process costs ~5–15 MB minimum and adds IPC latency on the show path.
