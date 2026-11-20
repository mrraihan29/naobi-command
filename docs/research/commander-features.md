# macOS Commander Features & Windows 11 Implementation Research

Purpose: Primary-source technical and product research for Naobi Command (`mrraihan29/naobi-command`) evaluating macOS commander-inspired features (Apple Spotlight, Raycast, Alfred) against Windows 11 APIs and the project's constraints (`R-004`, `R-006`, `R-010`, `R-011`, `R-013`, `R-014`). Paraphrased from official docs, primary specifications, and local repository files; no third-party code or assets copied. Retrieved 2026-11-20.

Conventions:
- `[source-id]` tags map to full URLs or local repository paths in Section 5.
- Claims explicitly labeled **(inference)** or **Option** represent architectural synthesis against Naobi Command's constraints rather than a quote from an external source.
- Scope context: Naobi Command targets Windows 11 `x86_64` in a single medium-integrity Rust + Slint (software renderer) resident process with `<= 40 MB` idle working set (`R-006`), `<= 20 MB` release exe (`R-006`), `p95_est <= 25 ms` keystroke-to-results (`R-006`), and strict network silence in the MVP (`R-010`, enforced by `deny.toml`) `[repo-constraints][repo-report][repo-deny]`.

---

## 1. Currency Conversion

### 1.1 Reference UX: macOS Spotlight, Raycast, and Alfred
- **Apple Spotlight** evaluates inline currency conversions directly in the search field in two forms: a bare currency amount such as `$100` or `100 yen` (which converts to the user's configured/local currency or opens Calculator for target selection) and an explicit conversion phrase such as `300 krone in euros` `[apple-calc][apple-spot]`.
- **Raycast Calculator** evaluates currency queries inline in Root Search (`100 usd in gbp`, `USD1K`, `10K`, `rupiah`, minor units like `cents` or `pence`, plus crypto) `[ray-calc]`.
  - It includes an explicit **Refresh Currencies** command in the Action Panel (`Ctrl+K`) alongside **Swap Units** and **Show History** (where historical currency entries update live) `[ray-calc]`.
  - Operational lesson from Raycast's Windows changelog: currency conversion relies on background-cached rates, and Raycast had to patch stale-rate refresh bugs (`v2.5`) `[repo-raycast]`.
- **Alfred** takes a simpler local-only approach in its built-in Calculator: it strips currency symbols (`$`, `£`, `€`) so pasted monetary figures calculate cleanly without manual cleanup, while live FX conversion is left to workflows rather than built-in core `[alfred-calc]`.

### 1.2 Primary FX Rate Feeds Compared

| Source | Cost & Auth | Coverage (incl. IDR) | Update Cadence | Rate Limits & Caching | License / Terms Constraints |
|---|---|---|---|---|---|
| **ECB Reference Rates** (`eurofxref-daily.xml`) `[ecb-rates][ecb-xml]` | Rp0, no API key | 30 currencies (EUR + 29 including `USD`, `IDR`, `JPY`, `GBP`, `SGD`, `MYR`, `AUD`) `[ecb-xml]` | Once per working day around 16:00 CET (excludes TARGET closing days) `[ecb-rates]` | Static ~2 KB XML file; once-daily fetch is sufficient `[ecb-xml]` | Free to use/reproduce with accurate presentation and source citation to ECB; informational only (discouraged for transactions) `[ecb-legal][ecb-rates]` |
| **Frankfurter** (`api.frankfurter.dev`) `[frank-docs][frank-v1]` | Rp0, no API key; open source (self-hostable) `[frank-docs]` | v2: 219 currencies + 39 assets from 112 official/central bank sources (includes `IDR` via ECB and Bank Indonesia); v1 uses ECB working-day set `[frank-docs][frank-v1][frank-lic]` | Throughout the day across providers; ECB subset daily ~16:00 CET `[frank-docs][frank-v1]` | No hard quota published in docs; supports `?quotes=` filtering to minimize payload and self-hosting via Docker `[frank-docs]` | Code/docs/Docker MIT-licensed; reference data remains subject to upstream central bank terms (e.g., ECB attribution; Bank Indonesia has no published restrictions) `[frank-lic]` |
| **ExchangeRate-API (Open Access)** (`open.er-api.com/v6/latest/USD`) `[erapi-free]` | Rp0, no API key `[erapi-free]` | 161–165 world currencies (JSON keyed by ISO 4217 code) `[erapi-free]` | Once every 24 hours; includes `time_next_update_unix` and `time_eol_unix` deprecation field `[erapi-free]` | Explicitly allows once-hourly or once-daily polling; abusive bursts trigger HTTP 429 for 20 minutes; caching permitted `[erapi-free]` | Requires visible/discreet attribution link (`Rates By Exchange Rate API`); **redistribution prohibited** `[erapi-free]` |
| **FloatRates** (`floatrates.com/daily/usd.json`) `[float-json]` | Rp0, no API key `[float-json]` | Broad global fiat coverage including `IDR` `[float-json]` | Every 12 hours (12:00 AM / 12:00 PM) `[float-json]` | Static daily JSON/XML per base currency; higher frequency requires paid Custom Exchange Rates `[float-json]` | Free daily feeds for general use; lacks the explicit MIT/self-host transparency of Frankfurter `[float-json]` |
| **Yahoo Finance** (unofficial query endpoints) `[yahoo-api][yahoo-guide]` | No free official public FX API | Broad | Intraday | Unofficial endpoints subject to throttling/breakage | YDN / Yahoo API Terms prohibit unauthorized automated scraping/competing redistribution, cap retention at 24h unless permitted, and disallow business productivity tools on non-commercial feeds `[yahoo-api][yahoo-guide]`. **Not recommended.** |

### 1.3 Naobi Command Constraints & Architecture Options
- **MVP boundary (`R-004`, `R-014`)**: Currency conversion is explicitly Phase 2 (`naobi-calc` today supports pure `std`-only arithmetic, `%`, hex/bin, and 8 static unit categories in `crates/naobi-calc/src/units.rs`) `[repo-report][repo-calc]`.
- **Network silence & compile-time bans (`R-010`, `T-016`, `deny.toml`)**: The repository does not merely avoid network calls at runtime; `deny.toml` bans every Rust HTTP client crate (`reqwest`, `hyper`, `ureq`, `isahc`, `curl`, `surf`, `attohttpc`) **and** bans `windows` crate networking features (`Win32_Networking_WinHttp`, `Win32_Networking_WinInet`, `Win32_Networking_WinSock`) `[repo-deny][repo-constraints]`. Any in-process fetcher would also fail `naobi-perf network-silence` (`R-010`) unless gated behind an explicit user action or an amended Phase 2 requirement `[repo-constraints][repo-report]`.
- **Design options for Phase 2** **(inference)**:
  1. **Option A — Zero-Socket Core + Explicit Local Cache Helper (Recommended)**: Keep `naobi-calc` pure and `naobi-app` completely socket-free (`deny.toml` intact). Store an optional FX table in `%LOCALAPPDATA%\NaobiCommand\fx-rates.json` (or SQLite `META`). Refresh rates only on an explicit **Refresh Currency Rates** command (or user-configured Windows Task Scheduler job) invoking PowerShell/OS `curl.exe` as an out-of-process user action—matching the existing `>` command model in `R-010 v2`—fetching ECB's ~2 KB `eurofxref-daily.xml` or Frankfurter's filtered JSON (`?base=USD&quotes=IDR,EUR,GBP,JPY,SGD,...`). `naobi-calc` loads the cached file into memory (~4 KB) and converts offline via EUR/USD triangulation (`amount * rate_to / rate_from`).
  2. **Option B — On-Demand / Once-Daily `WinHttp` in `naobi-win32`**: Amend `R-010` and `deny.toml` in Phase 2 to allow `Win32_Networking_WinHttp` (using Windows OS TLS with zero extra binary bloat) for an explicit opt-in daily or manual `Ctrl+R` refresh to `api.frankfurter.dev` or `www.ecb.europa.eu`. Preserves the `<= 20 MB` exe and `<= 40 MB` RAM budgets, but weakens the strict compile-time guarantee of `T-016`.
  3. **Option C — Pure Browser Fallback**: Keep `naobi-calc` currency-free and pass `53 usd to idr` to a search Quicklink. Zero maintenance, but loses the instant inline answer UX.
- **UX & Grammar Details for `naobi-calc`** **(inference)**:
  - Support both bare local-target queries (`53 usd` -> converts to configurable `default_currency = "IDR"` in `settings.toml`, inspired by Spotlight) and explicit pairs (`53 usd to idr`, `100 eur in jpy`, shorthand `53k usd`), plus symbol aliases (`$`, `€`, `£`, `¥`, `Rp`).
  - Surface the cached rate date in the row subtitle (e.g., `53 USD to IDR · ECB 2026-11-19`) and suppress or warn clearly if no cache file exists yet, avoiding Raycast's silent stale-rate failure mode `[repo-raycast]`.

---

## 2. Near-Instant File Search on Windows 11

### 2.1 Current Naobi Command Baseline (`R-013`)
- `crates/naobi-win32/src/search.rs` queries the Windows Search `SystemIndex` via OLE DB (`Search.CollatorDSO.1`) using `Contains(System.FileName, ...)` or `LIKE` fallback, capped at 30 rows `[repo-search][repo-report]`.
- **Why it is isolated in a child process**: Loading Windows Search OLE DB into a process permanently maps **tens of megabytes** of COM/provider DLLs into the working set `[repo-search][repo-files]`. To protect `R-006` (`<= 40 MB` idle working set), `crates/naobi-app/src/modules/files.rs` spawns a transient `--search-files` child process on demand, gives cold startup `COLD_MS = 3000 ms`, drops any query exceeding `ANSWER_MS = 500 ms` (`T-020`), and terminates the child after `IDLE = 30 s` `[repo-files]`.
- **Structural limitations**: Dependent on the `WSearch` service and user-configured indexing scopes (unindexed folders/drives are invisible), and cold-start queries pay process spawn + COM/OLE DB initialization latency up to several hundred milliseconds to 3 seconds `[repo-files][repo-powertoys][repo-raycast]`.

### 2.2 Technical Comparison of Windows 11 Indexing Mechanisms

| Mechanism | Privilege Requirement (`R-010`/`R-011`) | Initial Scan & Update Latency | Memory Footprint vs `R-006` (`<= 40 MB`) | Scope & Trade-offs |
|---|---|---|---|---|
| **Windows Search OLE DB (`Search.CollatorDSO`)** (Current MVP) `[ms-ole-search][repo-search]` | **Medium integrity** (standard user) `[repo-search]` | Zero app startup scan; warm query tens of ms, cold `--search-files` spawn up to `COLD_MS = 3000 ms` `[repo-files]` | ~0 MB idle in resident process (tens of MB isolated in transient child) `[repo-files]` | Limited to Windows Search scopes; silent gaps on unindexed drives; requires `WSearch` running `[repo-powertoys][repo-files]` |
| **WinRT `Windows.Storage.Search` (`QueryOptions` + `IndexerOption.OnlyUseIndexer`)** `[ms-winrt-search]` | **Medium integrity** `[ms-winrt-search]` | Same underlying `WSearch` index; avoids raw OLE DB unsafe bindings (`REPORT.md` RK-05) `[repo-report]` | Pulls WinRT/storage COM overhead into caller unless also isolated in child process **(inference)** | Cleaner API than OLE DB, but shares `WSearch` scope limits and indexing blind spots `[ms-winrt-search]` |
| **Direct NTFS MFT + USN Journal (`FSCTL_ENUM_USN_DATA`, `FSCTL_READ_USN_JOURNAL`)** `[ms-fsctl-enum][ms-fsctl-read]` | **Requires Administrator** (`CreateFile("\\.\X:")` DASD handle requires admin privileges; volume FSCTLs enforce `SeManageVolumePrivilege`) `[ms-createfile][ms-ifs-priv]` | Fresh Win11 (~250k files): **~5 s**; 1M files: **~1 min** (Everything FAQ). Raycast indexes whole NTFS drives "in seconds" via MFT `[vt-faq][ray-deep]` | Everything uses **~35 MB RAM** for 250k files, **~100 MB** for 1M files (+ ~1 MB for Everything Service); Raycast v2.6–2.8 required 60–95% indexer memory cuts `[vt-faq][vt-svc][repo-raycast]` | **Breaks `R-010`/`R-011` in MVP** (cannot run at medium integrity without a privileged Windows Service or UAC). Also unsupported on SMB/FAT/exFAT (`FSCTL_ENUM_USN_DATA` SMB = No); bypasses folder ACLs unless filtered `[ms-fsctl-enum][vt-svc]` |
| **User-Scoped Walk + `ReadDirectoryChangesW`** `[ms-readdirc][vt-folder]` | **Medium integrity** (standard user, respects NTFS ACLs; uses `FILE_FLAG_BACKUP_SEMANTICS` without needing `SeBackupPrivilege`) `[ms-createfile][ms-readdirc]` | Slower initial directory walk than MFT (seconds for scoped user dirs, minutes for full drive); real-time incremental updates via overlapped I/O or IOCP `[ms-readdirc][vt-folder]` | Scales strictly with indexed file count: ~50,000 filtered user-folder items fits in **~4–8 MB** (or <1 MB idle if queried via SQLite/mmap) **(inference)** | Supported on NTFS, ReFS, and SMB shares (`ERROR_NOTIFY_ENUM_DIR` / 0-byte return requires subtree rescan on buffer overflow); misses system/other-user paths blocked by ACLs `[ms-readdirc]` |

### 2.3 Key Findings for Phase 2 Own File Index (`R-013`)
1. **The Privilege Wall is Absolute**: Microsoft Learn explicitly documents that opening a volume handle (`\\.\C:`) with `CreateFile` to issue `DeviceIoControl` requires **administrative privileges** `[ms-createfile]`, and file-system drivers enforce `SeManageVolumePrivilege` (`SE_MANAGE_VOLUME_NAME`) on volume-level FSCTLs `[ms-ifs-priv]`. Both Everything (via its `Everything Service` running as `LocalSystem` over a named pipe restricted by code signature) `[vt-svc]` and full-drive MFT scanners must cross this boundary. Naobi Command (`R-010`, `R-011`) runs strictly at medium integrity in MVP and is unsigned (`D-028`) `[repo-report]`.
2. **The 40 MB Memory Wall (`R-006`)**: Naobi Command's Slint + accessibility baseline already sits at **34.8 MB working set** (`REPORT.md` RK-19), leaving only **~5.2 MB of headroom** under the 40 MB idle limit `[repo-report]`. An Everything-scale in-memory index of 250k–1M paths (35–100 MB in C) `[vt-faq]` **cannot live inside the resident process**.
3. **Viable Phase 2 Architectures** **(inference)**:
   - **Recommended (Medium-Integrity Scoped Index)**: Walk and watch only high-value user roots (`%USERPROFILE%`, Desktop, Documents, Downloads, Projects) at medium integrity using `ReadDirectoryChangesW` (`FILE_FLAG_OVERLAPPED` + IOCP) with aggressive dev-noise ignore rules (`.git`, `node_modules`, `target`, `.cargo`, `.venv`, `.rayignore`, matching Raycast's ignore strategy `[repo-raycast]`). Persist compact path/name records in SQLite FTS5/trigram or an mmap'd index file on disk so resident RAM stays `< 5 MB` and `keystroke-to-results` stays `< 25 ms` without admin rights.
   - **Alternative (Full-Drive MFT Helper Service or Everything SDK Bridge)**: If whole-drive (`C:\` + secondary NTFS drives) instant search is required in Phase 2, isolate the MFT/USN scanner in an on-demand or separate local Windows Service (`R-010` Phase 2 helper boundary) or optionally query an already-running `Everything` instance via IPC (`vt-faq`), keeping the resident Slint process unprivileged and under 40 MB.

---

## 3. High-Value Small Commander Utilities

Evaluated against Apple Spotlight `[apple-spot][apple-calc]`, Raycast Manual `[ray-calc][ray-notes][ray-snippets][ray-emoji][ray-sys][ray-nav][ray-ph]`, and Alfred Docs `[alfred-feat][alfred-calc][alfred-dict][alfred-sys][alfred-snip]`, filtered through Naobi Command's constraints (`R-004`, `R-006`, `R-010`, `R-011`, `D-012`).

### 3.1 Natural Language Timezones, Dates & Durations
- **Reference UX**: Spotlight supports `time in Paris` and `Japan local time` `[apple-calc]`; Raycast Calculator handles `5pm ldn in sf`, `time in tokyo`, `time diff Paris`, DST-safe arithmetic (`time in brisbane + 4 hours`), `days until 31 Mar`, `145 mins to timespan`, `week 44 2026`, ISO-8601 Zulu timestamps, and Unix epoch deltas (`from 1700000000 to 1700003600`) `[ray-calc]`.
- **Windows 11 Reality & Naobi Fit**: Completely offline, zero Win32 `unsafe` required. Can live inside `naobi-calc` (`std` + a compact MIT/Apache IANA tzdb crate such as `jiff` or compiled static timezone/city alias table, ~150–300 KB exe impact, zero idle RAM). High daily ROI for remote/global work (`3pm est in wib`, epoch <-> ISO-8601).

### 3.2 Developer Micro-Tools (Zero-Bloat Inline Utilities)
- **Reference UX**: Raycast Dynamic Placeholders (`{uuid}`) `[ray-ph]`, PowerToys Value Generator (`#` — cited in `powertoys.md` §2.2), and Alfred offline workflows (Lorem Ipsum, encoders) `[alfred-why][repo-powertoys]`.
- **Windows 11 Reality & Naobi Fit**: Pure Rust string/crypto transforms behind a `CommandSource` or prefix (e.g., `uuid`, `ulid`, `b64 <text>`, `url <text>`, `sha256 <text>`, `json` on clipboard, `wc` word/char count, `lorem`). Zero Win32, zero network, negligible RAM (`< 100 KB` binary delta).

### 3.3 Calculator & Clipboard Improvements
- **Calculator**:
  - *Existing*: `naobi-calc` handles arithmetic, `%`, powers, hex/bin, and 8 static unit categories `[repo-calc]`.
  - *High-value additions*: Inline **color format conversion** (`#ff6363` / `rgb(...)` / `oklch(...)` -> copy in other formats, sharing `naobi-picker`'s existing color math `[repo-report][ray-calc]`); **expression chaining / last-answer variable (`ans`)** (Alfred uses `=` to replace expression with answer `[alfred-calc]`); **calculation history** (Raycast keeps 3 months `[ray-calc]`, easy in existing SQLite); CSS `rem`/`px` units `[ray-calc]`; and **currency symbol stripping** when pasting prices into arithmetic (`$120 + $45`, per Alfred `[alfred-calc]`).
- **Clipboard History**:
  - *Existing*: Text, PNG up to 10 MB, file lists, DPAPI-encrypted SQLite, Proton Pass ignore list, paste & restore (`D-019`) `[repo-report]`.
  - *High-value additions*: **Paste as Plain Text** (`Ctrl+Shift+Enter` or Action Panel — already supported cleanly since `naobi-clipboard` stores plaintext strings `[repo-report][repo-raycast]`), **Sequential Paste Queue** (Raycast v2.8 `[repo-raycast]`), and **type filter pills/chords** (`Ctrl+P` for Text/Images/Files `[repo-raycast]`).

### 3.4 Window & Process Switcher (Same-Integrity)
- **Reference UX**: Raycast **Switch Windows** searches all open windows by app name + title and lets you close/minimize from the Action Panel without focusing `[ray-nav]`; Alfred offers **Quit / Force Quit** `[alfred-sys]`.
- **Windows 11 Reality & Naobi Fit**:
  - `EnumWindows` + `DwmGetWindowAttribute(DWMWA_CLOAKED)` (to filter hidden UWP/background shell windows) + `GetWindowTextW` + `QueryFullProcessImageNameW`. Uses existing `naobi-win32` Win32 primitives; costs ~0 MB idle RAM (enumerated on query or view open).
  - **Force-quit hung process**: `OpenProcess(PROCESS_TERMINATE)` + `TerminateProcess` works at medium integrity for the user's own non-elevated processes; elevated processes return `ERROR_ACCESS_DENIED`, triggering Naobi's standard `R-011` admin-window Notice `[repo-report]`. (Note: Window snapping was removed in `D-039`, so a lightweight window *switcher* + close/minimize/kill-hung fills a real gap without restoring zone-snapping complexity `[repo-report]`.)

### 3.5 System & Hardware Quick Commands
- **Existing in Naobi**: 12 commands in `crates/naobi-launcher/src/providers/system.rs` (`Lock`, `Sleep`, `Hibernate`, `Restart`, `ShutDown`, `SignOut`, `EmptyRecycleBin`, `ToggleDarkMode`, `OpenWindowsSettings`, `OpenSettingsFile`, `PauseHotkeys`, `Quit`) `[repo-sys]`.
- **Windows 11 Implementation Realities for Candidate Additions**:
  - **Sleep Displays**: `SendMessageW(HWND_BROADCAST, WM_SYSCOMMAND, SC_MONITORPOWER, 2)` — trivial, reliable, zero bloat.
  - **Volume Presets / Toggle Mute**: Documented Core Audio COM (`IMMDeviceEnumerator` -> `IAudioEndpointVolume`) — reliable, medium integrity.
  - **Wi-Fi / Bluetooth Toggles**: WinRT `Windows.Devices.Radios.Radio::RequestAccessAsync` + `SetStateAsync` `[ms-radios]`. Caveat: Raycast hit repeated Bluetooth timeouts and required hiding commands when no adapter exists (`v2.8`) `[repo-raycast]`.
  - **Display Brightness**: Laptop internal panels use WMI (`WmiMonitorBrightnessMethods`), whereas external monitors require DDC/CI via `SetMonitorBrightness` (`dxva2.dll`, not supported on all monitors/cables) `[ms-brightness]`.
  - **Night Light**: No clean documented public Win32 API; apps either parse undocumented `CloudStore` registry blobs (which caused repeated bugs in Raycast `[repo-raycast]`) or launch `ms-settings:nightlight` `[ms-settings]`.
  - **Audio Output Device Switcher**: `IMMDeviceEnumerator` is read-only for policy; switching the default output device requires the **undocumented COM interface `IPolicyConfig`** (`AudioSes.dll`) or launching Windows 11's built-in Quick Settings Volume Mixer (`Win+Ctrl+V` / `ms-settings:sound`) `[ms-win11-kb][ms-settings]`.
  - **Network / Local IP Info**: `GetAdaptersAddresses` (local read only, zero sockets opened, `R-010` compliant) **(inference)**.
  - **Windows Settings Deep Links**: `ShellExecuteW` on `ms-settings:*` URIs (`ms-settings:sound`, `ms-settings:network-wifi`, `ms-settings:nightlight`, `ms-settings:apps-volume`) `[ms-settings]` gives reliable one-keystroke navigation without fragile undocumented APIs.

### 3.6 Emoji & Symbol Picker, Dictionary, Scratchpad, and Text Snippets
- **Emoji & Symbol Picker** (`R-004` Phase 2 `[repo-report]`):
  - Windows 11 already includes `Win + .` natively (`ms-win11-kb` notes Unicode Emoji 15 + search).
  - Building an in-launcher searchable Emoji & Symbol view (matching Raycast's name/keyword/category search and `:` trigger `[ray-emoji]`) requires embedding a compact Unicode name + keyword table (~300–500 KB compressed) and pasting via Naobi's existing `D-019` clipboard-paste pipeline. Minimal RAM impact if loaded on view open and dropped on idle.
- **Dictionary / Define**:
  - macOS Spotlight and Alfred rely on Apple's built-in system Dictionary database (`Dictionary.app` / CoreServices) `[apple-spot][alfred-dict]`.
  - Windows 11 has **no equivalent offline system dictionary API** (only WinRT spell-checking). An offline dictionary in Naobi would require bundling a local corpus (e.g., WordNet ~5–15 MB, threatening the `<= 20 MB` exe budget in `R-006`) or a separate data file in `%LOCALAPPDATA%`; otherwise, a browser Quicklink (`dict <word>`) is the zero-bloat choice.
- **Floating Quick Notes / Scratchpad**:
  - Raycast Notes is a separate WebView2 window with rich Markdown (`Pro` feature) `[ray-notes][repo-raycast]`.
  - Under Naobi's `R-029` constraint ("adds no window and no process") `[repo-report]`, a separate always-floating editor window violates the single-window invariant; however, a **pushed Scratchpad view** inside the existing Launcher window backed by a single `%APPDATA%\NaobiCommand\scratchpad.md` file (with `Esc` saving/closing) achieves 90% of the quick-capture value with near-zero RAM/exe cost.
- **Text Snippets / Quick Expand** (`R-004` Phase 2 `[repo-report]`):
  - **Critical constraint conflict**: Both Raycast and Alfred auto-expand typed keywords system-wide (`;;addr` -> address) `[ray-snippets][alfred-snip]`. On Windows 11, global keystroke-stream monitoring requires a **low-level keyboard hook (`WH_KEYBOARD_LL`)**, which Naobi Command explicitly rejected in `D-012` / ADR-0004 `[repo-report]`.
  - **Recommended Phase 2 compromise**: Ship **searchable manual snippets** inside the Launcher (`snip <alias>` or dedicated hotkey, supporting dynamic placeholders `{date}`, `{time}`, `{uuid}`, `{clipboard}` `[ray-ph]`) that paste via Naobi's existing `D-019` `SendInput Ctrl+V` flow—delivering snippet value with **zero keyboard hook** and zero password-field keylogging risk.

---

## 4. Consolidated Phase 2 Prioritization Matrix

| Feature | Inspiration | Windows 11 Feasibility | Constraint Fit (`R-006`, `R-010`, `R-011`, `D-012`) | Recommendation |
|---|---|---|---|---|
| **Timezone / Date / Epoch Math** | Spotlight, Raycast | Pure Rust + compact tzdb | High fit (0 Win32, 0 net, <300 KB exe) | **Phase 2 Priority 1** (extend `naobi-calc`) |
| **Developer Micro-Tools** (UUID, hashes, Base64/URL, JSON) | Raycast placeholders, PowerToys | Pure Rust | High fit (0 Win32, 0 net, <100 KB exe) | **Phase 2 Priority 1** |
| **Calculator & Clipboard Polish** (`ans`, color convert, paste-plain, seq paste) | Spotlight, Raycast, Alfred | Pure Rust + existing `D-019` | High fit (reuses `naobi-picker`/`naobi-clipboard`) | **Phase 2 Priority 1** |
| **Window Switcher + Same-Integrity Kill** | Raycast Switch Windows, Alfred Force Quit | `EnumWindows` + `DWMWA_CLOAKED` + `TerminateProcess` | High fit (`R-011` notice on elevated windows) | **Phase 2 Priority 1** |
| **System Commands (`ms-settings:`, Sleep Display, Volume, Local IP)** | Raycast System Commands, Alfred | Documented Win32 / Core Audio / URIs | High fit (avoid undocumented `IPolicyConfig` & Night Light registry hacks) | **Phase 2 Priority 1** |
| **Currency Conversion** (`53 usd` -> IDR, `53 usd to idr`) | Spotlight, Raycast | ECB XML / Frankfurter JSON | Fits `R-010` if resident app reads cached file updated by explicit external/child action | **Phase 2 Priority 2** (with Option A cache) |
| **Near-Instant File Search** | Everything, Raycast Windows | Scoped `ReadDirectoryChangesW` (non-admin) vs MFT Service (admin) | MFT conflicts with `R-010`/`R-011` & `R-006` 40 MB cap; scoped user-dir watcher fits | **Phase 2 Priority 2** (scoped index or external Everything IPC) |
| **Manual Searchable Snippets** | Raycast Snippets, Alfred Snippets | Launcher view + `D-019` paste | Fits `D-012` (avoids `WH_KEYBOARD_LL` auto-expansion hook) | **Phase 2 Priority 2** |
| **Emoji & Symbol Picker** | Raycast Emoji | Embedded Unicode table vs native `Win + .` | Fits `R-006` (~400 KB) if table freed on idle | **Phase 2 Priority 3** (native `Win + .` covers MVP) |
| **Scratchpad View** | Raycast Notes | Pushed Slint text view + `.md` file | Must stay inside single Launcher window (`R-029`) | **Phase 2 Priority 3** |
| **Offline Dictionary / Define** | Spotlight, Alfred | No Windows OS dictionary API | Poor exe-size fit (`R-006` 20 MB cap) unless external data file | **Defer / Keep as Quicklink** |

---

## 5. Sources

### Repository Baseline
- `[repo-constraints]` `/home/antigravity/mrraihan29/naobi-command/CONSTRAINTS.md`
- `[repo-report]` `/home/antigravity/mrraihan29/naobi-command/docs/design/REPORT.md`
- `[repo-deny]` `/home/antigravity/mrraihan29/naobi-command/deny.toml`
- `[repo-raycast]` `/home/antigravity/mrraihan29/naobi-command/docs/research/raycast.md`
- `[repo-powertoys]` `/home/antigravity/mrraihan29/naobi-command/docs/research/powertoys.md`
- `[repo-search]` `/home/antigravity/mrraihan29/naobi-command/crates/naobi-win32/src/search.rs`
- `[repo-files]` `/home/antigravity/mrraihan29/naobi-command/crates/naobi-app/src/modules/files.rs`
- `[repo-calc]` `/home/antigravity/mrraihan29/naobi-command/crates/naobi-calc/src/lib.rs` (and `units.rs`)
- `[repo-sys]` `/home/antigravity/mrraihan29/naobi-command/crates/naobi-launcher/src/providers/system.rs`

### Apple Spotlight, Raycast, and Alfred Primary Documentation
- `[apple-calc]` https://support.apple.com/guide/mac-help/mchldd6ba066/mac (Get calculations and conversions in Spotlight on Mac)
- `[apple-spot]` https://support.apple.com/guide/mac-help/search-with-spotlight-mchlp1008/mac (Find what you need with Spotlight on Mac)
- `[ray-calc]` https://manual.raycast.com/calculator (Calculator | Raycast Manual)
- `[ray-notes]` https://manual.raycast.com/notes (Notes | Raycast Manual)
- `[ray-snippets]` https://manual.raycast.com/snippets (Snippets | Raycast Manual)
- `[ray-emoji]` https://manual.raycast.com/emoji-symbols (Emoji & Symbols | Raycast Manual)
- `[ray-sys]` https://manual.raycast.com/system-commands (System Commands | Raycast Manual)
- `[ray-nav]` https://manual.raycast.com/navigation (Navigation | Raycast Manual)
- `[ray-ph]` https://manual.raycast.com/dynamic-placeholders (Dynamic Placeholders | Raycast Manual)
- `[ray-deep]` https://www.raycast.com/blog/a-technical-deep-dive-into-the-new-raycast (A Technical Deep Dive Into the New Raycast)
- `[alfred-feat]` https://www.alfredapp.com/help/features/ (Features - Alfred Help and Support)
- `[alfred-why]` https://www.alfredapp.com/help/why-choose-alfred/ (Why Choose Alfred? - Alfred Help and Support)
- `[alfred-calc]` https://www.alfredapp.com/help/features/calculator/ (Calculator - Alfred Help and Support)
- `[alfred-dict]` https://www.alfredapp.com/help/features/dictionary/ (Dictionary - Alfred Help and Support)
- `[alfred-sys]` https://www.alfredapp.com/help/features/system/ (System Commands - Alfred Help and Support)
- `[alfred-snip]` https://www.alfredapp.com/help/features/snippets/ (Snippets and Text Expansion - Alfred Help and Support)

### Foreign Exchange (FX) Rate APIs & Terms
- `[ecb-rates]` https://www.ecb.europa.eu/stats/policy_and_exchange_rates/euro_reference_exchange_rates/html/index.en.html (Euro foreign exchange reference rates)
- `[ecb-xml]` https://www.ecb.europa.eu/stats/eurofxref/eurofxref-daily.xml (ECB daily XML feed)
- `[ecb-legal]` https://www.ecb.europa.eu/services/using-our-site/disclaimer/html/index.en.html (ECB Disclaimer & copyright)
- `[frank-docs]` https://frankfurter.dev/ (Frankfurter v2 documentation)
- `[frank-v1]` https://frankfurter.dev/v1/ (Frankfurter v1 documentation)
- `[frank-lic]` https://frankfurter.dev/license/ (Frankfurter License & Terms)
- `[erapi-free]` https://www.exchangerate-api.com/docs/free (ExchangeRate-API Open Access documentation)
- `[float-json]` https://www.floatrates.com/json-feeds.html (FloatRates Free JSON Currency Exchange Rate Feeds)
- `[yahoo-api]` https://legal.yahoo.com/us/en/yahoo/terms/product-atos/apiforydn/index.html (Yahoo Developer API Terms of Use)
- `[yahoo-guide]` https://legal.yahoo.com/us/en/yahoo/guidelines/ydn/index.html (Yahoo Developer Network Guidelines)

### Windows 11 APIs & Everything (voidtools) Documentation
- `[ms-createfile]` https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-createfilea (CreateFileA — Physical Disks and Volumes / Directories)
- `[ms-fsctl-enum]` https://learn.microsoft.com/en-us/windows/win32/api/winioctl/ni-winioctl-fsctl_enum_usn_data (FSCTL_ENUM_USN_DATA IOCTL)
- `[ms-fsctl-read]` https://learn.microsoft.com/en-us/windows/win32/api/winioctl/ni-winioctl-fsctl_read_usn_journal (FSCTL_READ_USN_JOURNAL IOCTL)
- `[ms-usn-walk]` https://learn.microsoft.com/en-us/windows/win32/fileio/walking-a-buffer-of-change-journal-records (Walking a Buffer of Change Journal Records)
- `[ms-ifs-priv]` https://learn.microsoft.com/en-us/windows-hardware/drivers/ifs/privileges (Managing Privileges in a File System — SeManageVolumePrivilege)
- `[ms-readdirc]` https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-readdirectorychangesw (ReadDirectoryChangesW function)
- `[ms-ole-search]` https://learn.microsoft.com/en-us/previous-versions/windows/desktop/legacy/ff684395(v=vs.85) (Using SQL and AQS Approaches to Query the Index)
- `[ms-winrt-search]` https://learn.microsoft.com/en-us/uwp/api/windows.storage.search.queryoptions?view=winrt-28000 (QueryOptions Class — Windows.Storage.Search)
- `[ms-settings]` https://learn.microsoft.com/en-us/windows/apps/develop/launch/launch-settings (Launch Windows Settings — ms-settings: URI scheme)
- `[ms-radios]` https://learn.microsoft.com/en-us/uwp/api/windows.devices.radios.radio?view=winrt-28000 (Radio Class — Windows.Devices.Radios)
- `[ms-brightness]` https://learn.microsoft.com/en-us/windows/win32/api/highlevelmonitorconfigurationapi/nf-highlevelmonitorconfigurationapi-setmonitorbrightness (SetMonitorBrightness function)
- `[ms-win11-kb]` https://support.microsoft.com/en-us/servicing/os/windows-11/2023/11/november-14-2023-kb5032190-os-builds-22621-2715-and-22631-2715 (Windows 11 KB5032190 — Win+Ctrl+V volume mixer)
- `[vt-faq]` https://www.voidtools.com/faq/ (voidtools Everything FAQ)
- `[vt-svc]` https://www.voidtools.com/support/everything/everything_service/ (Everything Service documentation)
- `[vt-idx]` https://www.voidtools.com/support/everything/indexes/ (Everything Indexes documentation)
- `[vt-folder]` https://www.voidtools.com/support/everything/folder_indexing/ (Everything Folder Indexing documentation)
