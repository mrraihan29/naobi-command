# User-scoped file index instead of an elevated NTFS MFT service

Status: accepted
Amends: R-013 (Phase 2 own file index)

The Owner wants near-instant file search replacing Windows Search cold-start latency (`COLD_MS = 3000 ms` in a transient `--search-files` child). Whole-drive NTFS MFT + USN Journal scanning (`FSCTL_ENUM_USN_DATA` / `FSCTL_READ_USN_JOURNAL`) requires Administrator privileges (`SeManageVolumePrivilege`) and ~35–100 MB of in-memory path tables, conflicting with Naobi Command's medium-integrity security model (`R-010`, `R-011`) and 40 MB idle working-set cap (`R-006`).

## Decision

- **User-scoped non-admin index.** Naobi Command indexes `%USERPROFILE%` high-value folders (`Desktop`, `Documents`, `Downloads`, `Pictures`, `Videos`, plus common project folders) and any Owner-configured `[files] roots` in `settings.toml`, skipping build/dependency noise (`.git`, `node_modules`, `target`, `.cargo`, `.venv`, `dist`, `build`).
- **Incremental updates & bounded RAM.** Directory changes are watched at medium integrity via `ReadDirectoryChangesW` and persisted to a disk-backed local index (SQLite FTS5 / compact index) so resident working-set overhead remains `< 5 MB`.
- **Windows Search fallback.** Local index results appear instantaneously at the top of the `Files` section; Windows Search (`--search-files`) stays available as a fallback when local results are sparse or explicitly queried (`ws:`).

## Considered options

- **Elevated `LocalSystem` MFT/USN helper service.** Rejected to avoid breaking single-process, zero-elevation simplicity (`R-010`, `R-011`) and NTFS ACL isolation.
- **External `Everything` IPC dependency.** Rejected as a primary dependency so Naobi Command works standalone out of the box.

## Consequences

- System/OS directories outside configured roots (`C:\Windows`, `C:\Program Files`) are not in the instant local index and rely on the Windows Search fallback.
- Keystroke-to-results latency stays within `R-006` (`<= 25 ms`) without spawning `--search-files` on every everyday file lookup.
