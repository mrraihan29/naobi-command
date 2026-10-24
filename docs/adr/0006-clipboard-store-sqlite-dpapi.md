# Clipboard store: SQLite with per-blob DPAPI encryption

Status: accepted
Ledger: D-014 (implements R-009), D-015 (limits)

R-009 requires clipboard contents encrypted with Windows DPAPI for the current user, stored in SQLite, readable by no one who opens the file. Raycast solves the same problem with page-level database encryption and a key in Credential Manager (`docs/research/raycast-rea.md` §4).

## Decision

- File: `%LOCALAPPDATA%\NaobiCommand\naobi.db` (local, not roaming: DPAPI blobs are only useful on this machine and profile). `journal_mode=WAL`, `synchronous=NORMAL`, `cache_size` capped at 2 MiB, `auto_vacuum=INCREMENTAL`, `PRAGMA user_version` for schema version.
- Every content column is a DPAPI blob made with `CryptProtectData` (current-user scope, `CRYPTPROTECT_UI_FORBIDDEN`, a fixed app entropy string). Plaintext columns are metadata only: kind, timestamps, pinned flag, byte size, source exe name.
- Search over text cannot use SQL on encrypted blobs. When the clipboard view first opens, a worker decrypts previews (first 4 KiB of each text entry) into an in-memory index; the index is dropped 60 s after the view closes. 1,000 entries at 4 KiB is at most 4 MB.
- Images are stored as PNG blobs, converted from the clipboard DIB in safe Rust (the `png` crate) after the 64 MiB raw-size pre-check, so another app's bytes are never parsed by native codecs inside the process. Each PNG is encrypted as one blob and written and read with incremental blob I/O so a 10 MB image is never duplicated in RAM.
- Retention and caps run in the same transaction as the insert.

## Considered options

- SQLCipher or page encryption with a Credential Manager key: stronger against offline tampering, but adds a C crypto build dependency, key custody code, and a second thing to keep in sync with the 20 MB exe budget. R-009 asks for DPAPI by name.
- One encrypted file per entry: cheap retention but no atomic purge and 1,000 small files for no gain.
- Plaintext SQLite on a BitLocker volume: fails the R-009 acceptance (contents must not be readable in the file).

## Consequences

- Paste from history restores the previous clipboard content afterwards (D-019 v5), except when that content carried an exclusion format or came from an ignored app: then it is left unrestored, so an excluded secret is never put back without its exclusion formats. The restore covers only text, image and file list within the D-015 limits; rich text, HTML, spreadsheet ranges and app-private formats come back degraded. The restore write carries the private marker, so neither the paste nor the restore is stored as a new entry.
- DPAPI does not authenticate who wrote a blob and is readable by any process running as the same user; that residual risk is accepted (D-026).
- Opening the history view costs a decrypt pass (measured in Build; 1,000 `CryptUnprotectData` calls is expected in the low milliseconds range).
- The migration rule is expand-contract with a file copy to `naobi.db.bak-v<N>` before any migration (portable exe, so an older exe may be put back).
