//! Non-admin, disk-backed File index (ADR-0009, R-013): SQLite + FTS5 over configured local
//! folders, kept current via incremental events. Never touches UNC/network paths and never logs
//! file names or paths (D-020).
use crate::providers::files::LIMIT;
use rusqlite::{params, Connection, OpenFlags};
use std::collections::HashSet;
use std::path::{Component, Path, PathBuf};

const DEFAULT_USER_DIRS: [&str; 12] = [
    "Desktop",
    "Documents",
    "Downloads",
    "Pictures",
    "Videos",
    "Projects",
    "Coding",
    "Code",
    "src",
    "dev",
    "repos",
    "workspace",
];

const SYNC_BATCH: usize = 500;

const SCHEMA_SQL: &str = "
CREATE TABLE IF NOT EXISTS meta (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS files (
    id   INTEGER PRIMARY KEY,
    path TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    gen  INTEGER NOT NULL DEFAULT 0
);
CREATE VIRTUAL TABLE IF NOT EXISTS files_fts USING fts5(
    name,
    content='files',
    content_rowid='id',
    tokenize='unicode61'
);
CREATE TRIGGER IF NOT EXISTS files_ai AFTER INSERT ON files BEGIN
    INSERT INTO files_fts(rowid, name) VALUES (new.id, new.name);
END;
CREATE TRIGGER IF NOT EXISTS files_ad AFTER DELETE ON files BEGIN
    INSERT INTO files_fts(files_fts, rowid, name) VALUES ('delete', old.id, old.name);
END;
CREATE TRIGGER IF NOT EXISTS files_au AFTER UPDATE OF path, name ON files BEGIN
    INSERT INTO files_fts(files_fts, rowid, name) VALUES ('delete', old.id, old.name);
    INSERT INTO files_fts(rowid, name) VALUES (new.id, new.name);
END;
";

/// Incremental filesystem change applied to `FileIndex`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IndexEvent {
    Added(PathBuf),
    Removed(PathBuf),
    Renamed { from: PathBuf, to: PathBuf },
}

/// True for a drive-absolute local path (`X:\...` or `X:/...`); rejects UNC (`\\...`), device
/// (`\\.\...`, `\\?\...`), and relative paths.
pub fn is_safe_local_path(p: &Path) -> bool {
    let s = p.to_string_lossy();
    if s.starts_with("\\\\") || s.starts_with("//") {
        return false;
    }
    let b = s.as_bytes();
    b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && matches!(b[2], b'\\' | b'/')
}

/// Resolves configured `[files] roots` (or default `%USERPROFILE%` folders when empty), keeping
/// only existing local non-UNC directories without duplicates.
pub fn resolve_roots(configured: &[String], user_profile: Option<&Path>) -> Vec<PathBuf> {
    let candidates: Vec<PathBuf> = if configured.is_empty() {
        let Some(home) = user_profile else {
            return Vec::new();
        };
        DEFAULT_USER_DIRS.iter().map(|d| home.join(d)).collect()
    } else {
        configured
            .iter()
            .filter_map(|raw| expand_root(raw.trim(), user_profile))
            .collect()
    };

    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for dir in candidates {
        if !is_safe_local_path(&dir) || !dir.is_dir() {
            continue;
        }
        let key = dir.to_string_lossy().to_ascii_lowercase();
        if seen.insert(key) {
            out.push(dir);
        }
    }
    out
}

fn expand_root(raw: &str, user_profile: Option<&Path>) -> Option<PathBuf> {
    if raw.is_empty() {
        return None;
    }
    if let Some(rest) = raw.strip_prefix("~/").or_else(|| raw.strip_prefix("~\\")) {
        return Some(user_profile?.join(rest));
    }
    if raw == "~" {
        return user_profile.map(Path::to_path_buf);
    }
    if let Some(rest) = raw
        .strip_prefix("%USERPROFILE%\\")
        .or_else(|| raw.strip_prefix("%USERPROFILE%/"))
    {
        return Some(user_profile?.join(rest));
    }
    if raw.eq_ignore_ascii_case("%USERPROFILE%") {
        return user_profile.map(Path::to_path_buf);
    }
    Some(PathBuf::from(raw))
}

/// True when a directory or file segment matches an ignore rule (exact case-insensitive name or
/// `*.ext` suffix).
pub fn is_ignored_segment(name: &str, ignore: &[String]) -> bool {
    for raw in ignore {
        let pat = raw.trim();
        if pat.is_empty() {
            continue;
        }
        if let Some(ext) = pat.strip_prefix("*.") {
            let dot_ext_len = ext.len() + 1;
            if name.len() >= dot_ext_len
                && name.as_bytes()[name.len() - dot_ext_len] == b'.'
                && name[name.len() - ext.len()..].eq_ignore_ascii_case(ext)
            {
                return true;
            }
        } else if name.eq_ignore_ascii_case(pat) {
            return true;
        }
    }
    false
}

/// True when any normal component of `rel` matches `ignore`.
pub fn is_ignored_rel_path(rel: &Path, ignore: &[String]) -> bool {
    rel.components().any(|c| match c {
        Component::Normal(seg) => seg.to_str().is_some_and(|s| is_ignored_segment(s, ignore)),
        _ => false,
    })
}

/// True when `path` contains an ignored segment relative to its matching root (or in any segment
/// when no root matches).
pub fn is_ignored_path(path: &Path, roots: &[PathBuf], ignore: &[String]) -> bool {
    for root in roots {
        if let Ok(rel) = path.strip_prefix(root) {
            return is_ignored_rel_path(rel, ignore);
        }
    }
    is_ignored_rel_path(path, ignore)
}

/// Disk-backed SQLite + FTS5 File index with a 1 MiB page-cache cap (`< 5 MB` resident RAM).
pub struct FileIndex {
    conn: Connection,
}

impl FileIndex {
    /// Opens or creates a persistent index at `path`.
    pub fn open(path: &Path) -> Result<Self, rusqlite::Error> {
        let conn = Connection::open(path)?;
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             PRAGMA cache_size = -1024;
             PRAGMA temp_store = MEMORY;
             PRAGMA mmap_size = 0;",
        )?;
        conn.execute_batch(SCHEMA_SQL)?;
        Ok(Self { conn })
    }

    /// Opens an additional read-only connection to an existing WAL index (for concurrent queries).
    pub fn open_reader(path: &Path) -> Result<Self, rusqlite::Error> {
        let flags = OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX;
        let conn = Connection::open_with_flags(path, flags)?;
        conn.execute_batch(
            "PRAGMA cache_size = -1024;
             PRAGMA temp_store = MEMORY;
             PRAGMA mmap_size = 0;
             PRAGMA busy_timeout = 50;",
        )?;
        Ok(Self { conn })
    }

    /// In-memory index for Tier 1 tests.
    pub fn open_in_memory() -> Result<Self, rusqlite::Error> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch(SCHEMA_SQL)?;
        Ok(Self { conn })
    }

    /// Crawls `roots` (skipping `ignore`, symlinks, and non-local paths), upserts entries in
    /// bounded transactions, prunes stale rows, and releases unused SQLite cache pages.
    pub fn sync_roots(
        &mut self,
        roots: &[PathBuf],
        ignore: &[String],
    ) -> Result<usize, rusqlite::Error> {
        let next_gen: i64 =
            self.conn
                .query_row("SELECT COALESCE(MAX(gen), 0) + 1 FROM files", [], |r| {
                    r.get(0)
                })?;
        let mut total = 0usize;
        let mut batch: Vec<(String, String)> = Vec::with_capacity(SYNC_BATCH);
        for root in roots {
            if !is_safe_local_path(root) || !root.is_dir() {
                continue;
            }
            crawl_dir(root, ignore, &mut |path, name| {
                total += 1;
                batch.push((path, name));
                if batch.len() >= SYNC_BATCH {
                    flush_batch(&mut self.conn, &batch, next_gen)?;
                    batch.clear();
                }
                Ok(())
            })?;
        }
        if !batch.is_empty() {
            flush_batch(&mut self.conn, &batch, next_gen)?;
        }
        let tx = self.conn.transaction()?;
        tx.execute("DELETE FROM files WHERE gen != ?1", params![next_gen])?;
        tx.commit()?;
        let _ = self.conn.execute_batch("PRAGMA shrink_memory;");
        Ok(total)
    }

    /// Applies incremental add/remove/rename events in a single transaction.
    pub fn apply_events(
        &mut self,
        events: &[IndexEvent],
        roots: &[PathBuf],
        ignore: &[String],
    ) -> Result<(), rusqlite::Error> {
        if events.is_empty() {
            return Ok(());
        }
        let gen: i64 = self
            .conn
            .query_row("SELECT COALESCE(MAX(gen), 1) FROM files", [], |r| r.get(0))?;
        let tx = self.conn.transaction()?;
        for ev in events {
            match ev {
                IndexEvent::Removed(path) => delete_subtree(&tx, path)?,
                IndexEvent::Added(path) => upsert_path(&tx, path, roots, ignore, gen)?,
                IndexEvent::Renamed { from, to } => {
                    delete_subtree(&tx, from)?;
                    upsert_path(&tx, to, roots, ignore, gen)?;
                }
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Searches indexed file names, returning at most `limit.min(LIMIT)` full paths.
    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<String>, rusqlite::Error> {
        let cap = limit.min(LIMIT);
        let trimmed = query.trim();
        if trimmed.is_empty() || cap == 0 {
            return Ok(Vec::new());
        }

        let mut results = Vec::new();
        let mut seen = HashSet::new();

        if let Some(fts_query) = fts_prefix_query(trimmed) {
            let mut stmt = self.conn.prepare_cached(
                "SELECT f.path
                 FROM files_fts
                 JOIN files f ON f.id = files_fts.rowid
                 WHERE files_fts MATCH ?1
                 ORDER BY bm25(files_fts), length(f.name), f.path
                 LIMIT ?2",
            )?;
            let rows = stmt.query_map(params![fts_query, cap as i64], |r| r.get::<_, String>(0))?;
            for row in rows {
                let path = row?;
                if seen.insert(path.clone()) {
                    results.push(path);
                }
            }
        }

        if results.len() < cap {
            let words: Vec<String> = trimmed
                .split_whitespace()
                .map(str::to_ascii_lowercase)
                .filter(|w| !w.is_empty())
                .collect();
            if let Some(anchor) = words.iter().max_by_key(|w| w.len()) {
                let pattern = format!("%{}%", like_escape(anchor));
                let mut stmt = self.conn.prepare_cached(
                    "SELECT path, name
                     FROM files
                     WHERE name LIKE ?1 ESCAPE '\\'
                     ORDER BY length(name), path
                     LIMIT 200",
                )?;
                let rows = stmt.query_map(params![pattern], |r| {
                    Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
                })?;
                for row in rows {
                    let (path, name) = row?;
                    let lower = name.to_ascii_lowercase();
                    if words.iter().all(|w| lower.contains(w)) && seen.insert(path.clone()) {
                        results.push(path);
                        if results.len() >= cap {
                            break;
                        }
                    }
                }
            }
        }

        Ok(results)
    }
}

fn flush_batch(
    conn: &mut Connection,
    batch: &[(String, String)],
    gen: i64,
) -> Result<(), rusqlite::Error> {
    let tx = conn.transaction()?;
    {
        let mut stmt = tx.prepare_cached(
            "INSERT INTO files (path, name, gen) VALUES (?1, ?2, ?3)
             ON CONFLICT(path) DO UPDATE SET
                 name = excluded.name,
                 gen  = excluded.gen
             WHERE files.name != excluded.name OR files.gen != excluded.gen",
        )?;
        for (path, name) in batch {
            stmt.execute(params![path, name, gen])?;
        }
    }
    tx.commit()?;
    Ok(())
}

fn crawl_dir(
    root: &Path,
    ignore: &[String],
    visit: &mut impl FnMut(String, String) -> Result<(), rusqlite::Error>,
) -> Result<(), rusqlite::Error> {
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(ft) = entry.file_type() else {
                continue;
            };
            // Never traverse symlinks/junctions (avoids cycles and network targets).
            if ft.is_symlink() {
                continue;
            }
            let name_os = entry.file_name();
            let Some(name) = name_os.to_str() else {
                continue;
            };
            if is_ignored_segment(name, ignore) {
                continue;
            }
            let path = entry.path();
            if ft.is_dir() {
                stack.push(path);
            } else if ft.is_file() {
                if let Some(p) = path.to_str() {
                    visit(p.to_owned(), name.to_owned())?;
                }
            }
        }
    }
    Ok(())
}

fn delete_subtree(tx: &rusqlite::Transaction<'_>, path: &Path) -> Result<(), rusqlite::Error> {
    let Some(p) = path.to_str() else {
        return Ok(());
    };
    let trimmed = p.trim_end_matches(['\\', '/']);
    let lo = format!("{trimmed}\\");
    // ']' is ASCII 0x5D, immediately after '\\' (0x5C), bounding all descendants in B-tree order.
    let hi = format!("{trimmed}]");
    tx.execute(
        "DELETE FROM files WHERE path = ?1 OR (path >= ?2 AND path < ?3)",
        params![trimmed, lo, hi],
    )?;
    Ok(())
}

fn upsert_path(
    tx: &rusqlite::Transaction<'_>,
    path: &Path,
    roots: &[PathBuf],
    ignore: &[String],
    gen: i64,
) -> Result<(), rusqlite::Error> {
    if !is_safe_local_path(path) || is_ignored_path(path, roots, ignore) {
        return Ok(());
    }
    let Ok(meta) = std::fs::symlink_metadata(path) else {
        return Ok(());
    };
    if meta.file_type().is_symlink() {
        return Ok(());
    }
    let mut stmt = tx.prepare_cached(
        "INSERT INTO files (path, name, gen) VALUES (?1, ?2, ?3)
         ON CONFLICT(path) DO UPDATE SET
             name = excluded.name,
             gen  = excluded.gen
         WHERE files.name != excluded.name OR files.gen != excluded.gen",
    )?;
    if meta.is_dir() {
        crawl_dir(path, ignore, &mut |p, name| {
            stmt.execute(params![p, name, gen])?;
            Ok(())
        })?;
    } else if meta.is_file() {
        if let (Some(p), Some(name)) = (path.to_str(), path.file_name().and_then(|s| s.to_str())) {
            stmt.execute(params![p, name, gen])?;
        }
    }
    Ok(())
}

fn fts_prefix_query(query: &str) -> Option<String> {
    let tokens: Vec<String> = query
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
        .map(|t| format!("\"{}\"*", t.replace('"', "")))
        .collect();
    (!tokens.is_empty()).then(|| tokens.join(" AND "))
}

fn like_escape(term: &str) -> String {
    let mut out = String::with_capacity(term.len());
    for c in term.chars() {
        if matches!(c, '\\' | '%' | '_') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}
