//! Schema per REPORT 4.1. Content and preview columns are BLOB (DPAPI ciphertext, R-009).
//! Migrations are append-only and expand-only (D-014): add tables or nullable/defaulted columns, never drop or rename.

pub const MIGRATIONS: &[&str] = &[V1, V2, V3];

/// Calculator history (issue #2): expressions and results are DPAPI blobs; `calculated_at` is Unix ms.
const V3: &str = "
CREATE TABLE calc_history (
    id            INTEGER PRIMARY KEY,
    expr_enc      BLOB NOT NULL,
    result_enc    BLOB NOT NULL,
    calculated_at INTEGER NOT NULL
);
";

/// R-024: the `>` run history. Lines are DPAPI blobs (T-034); `last_run_at` is Unix milliseconds.
const V2: &str = "
CREATE TABLE run_history (
    id          INTEGER PRIMARY KEY,
    line_enc    BLOB NOT NULL,
    program_enc BLOB,
    last_run_at INTEGER NOT NULL
);
";

const V1: &str = "
CREATE TABLE meta (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
CREATE TABLE clip_entry (
    id             INTEGER PRIMARY KEY,
    kind           TEXT NOT NULL CHECK (kind IN ('text', 'image', 'files')),
    created_at     INTEGER NOT NULL,
    last_copied_at INTEGER NOT NULL,
    pinned         INTEGER NOT NULL DEFAULT 0 CHECK (pinned IN (0, 1)),
    source_exe     TEXT,
    byte_size      INTEGER NOT NULL,
    preview_enc    BLOB NOT NULL
);
CREATE TABLE clip_payload (
    entry_id INTEGER NOT NULL REFERENCES clip_entry (id) ON DELETE CASCADE,
    format   TEXT NOT NULL CHECK (format IN ('text', 'png', 'filelist')),
    data_enc BLOB NOT NULL,
    PRIMARY KEY (entry_id, format)
);
CREATE TABLE frecency (
    item_key       INTEGER PRIMARY KEY,
    open_count     INTEGER NOT NULL,
    last_opened_at INTEGER NOT NULL
);
CREATE TABLE color_history (
    id        INTEGER PRIMARY KEY,
    rgba      INTEGER NOT NULL,
    picked_at INTEGER NOT NULL
);
";
