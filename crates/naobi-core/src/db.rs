//! The single SQLite store (ADR-0006, D-014): schema, pragmas, migrations and the keyed item hash.
//! No encryption here; the clipboard item stores DPAPI blobs. Every statement is parameterized (T-007).
mod key;
mod migrate;
pub mod schema;

pub use key::item_key;
use rusqlite::{params, Connection, OptionalExtension, Transaction};
use std::path::Path;

/// Unused FRECENCY rows are dropped after 180 days (REPORT 4.2). Timestamps are Unix seconds.
const STALE_SECS: i64 = 180 * 24 * 3600;
const SALT_KEY: &str = "hash_salt";

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("stored hash salt is not 16 bytes of hex")]
    BadSalt,
    #[error("meta key is reserved")]
    ReservedKey,
}

pub struct Db {
    conn: Connection,
    salt: [u8; 16],
}

impl Db {
    /// Opens or creates the file. `random16` is called only on first run (BCryptGenRandom in naobi-win32).
    pub fn open(path: &Path, random16: impl FnOnce() -> [u8; 16]) -> Result<Db, Error> {
        Self::open_with_migrations(path, random16, schema::MIGRATIONS)
    }

    /// Same as `open` with an explicit migration list; tests use it to simulate a newer build.
    #[doc(hidden)]
    pub fn open_with_migrations(
        path: &Path,
        random16: impl FnOnce() -> [u8; 16],
        migrations: &[&str],
    ) -> Result<Db, Error> {
        let conn = Connection::open(path)?;
        // auto_vacuum only takes effect before the first table exists; WAL is persistent in the file.
        conn.execute_batch("PRAGMA auto_vacuum = INCREMENTAL; PRAGMA journal_mode = WAL;")?;
        Self::finish(conn, Some(path), random16, migrations)
    }

    /// For tests: in-memory, fixed salt.
    pub fn open_in_memory() -> Result<Db, Error> {
        let conn = Connection::open_in_memory()?;
        Self::finish(conn, None, || [7; 16], schema::MIGRATIONS)
    }

    fn finish(
        mut conn: Connection,
        path: Option<&Path>,
        random16: impl FnOnce() -> [u8; 16],
        migrations: &[&str],
    ) -> Result<Db, Error> {
        conn.execute_batch(
            "PRAGMA synchronous = NORMAL; PRAGMA cache_size = -2048; PRAGMA foreign_keys = ON;",
        )?;
        migrate::run(&mut conn, path, migrations)?;
        let stored: Option<String> = conn
            .query_row("SELECT value FROM meta WHERE key = ?1", [SALT_KEY], |r| {
                r.get(0)
            })
            .optional()?;
        let salt = match stored {
            Some(hex) => decode_hex(&hex).ok_or(Error::BadSalt)?,
            None => {
                let salt = random16();
                let hex: String = salt.iter().map(|b| format!("{b:02x}")).collect();
                conn.execute(
                    "INSERT INTO meta (key, value) VALUES (?1, ?2)",
                    params![SALT_KEY, hex],
                )?;
                salt
            }
        };
        Ok(Db { conn, salt })
    }

    pub fn salt(&self) -> [u8; 16] {
        self.salt
    }

    /// Read access for the other module crates and for pragma checks.
    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    /// Runs `f` in one transaction; commits on `Ok`, rolls back on `Err`.
    pub fn transaction<T>(
        &mut self,
        f: impl FnOnce(&Transaction) -> rusqlite::Result<T>,
    ) -> Result<T, Error> {
        let tx = self.conn.transaction()?;
        let out = f(&tx)?;
        tx.commit()?;
        Ok(out)
    }

    pub fn meta_get(&self, key: &str) -> Result<Option<String>, Error> {
        Ok(self
            .conn
            .query_row("SELECT value FROM meta WHERE key = ?1", [key], |r| r.get(0))
            .optional()?)
    }

    pub fn meta_set(&self, key: &str, value: &str) -> Result<(), Error> {
        if key == SALT_KEY {
            return Err(Error::ReservedKey);
        }
        self.conn.execute(
            "INSERT INTO meta (key, value) VALUES (?1, ?2)
             ON CONFLICT (key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    /// Counts one open of the item behind `key` at Unix time `now`.
    pub fn record_open(&self, key: i64, now: i64) -> Result<(), Error> {
        self.conn.execute(
            "INSERT INTO frecency (item_key, open_count, last_opened_at) VALUES (?1, 1, ?2)
             ON CONFLICT (item_key) DO UPDATE
             SET open_count = open_count + 1, last_opened_at = excluded.last_opened_at",
            params![key, now],
        )?;
        Ok(())
    }

    /// `(item_key, open_count, last_opened_at)` for every row; scoring lives in the launcher.
    pub fn raw_rows(&self) -> Result<Vec<(i64, i64, i64)>, Error> {
        let mut stmt = self
            .conn
            .prepare("SELECT item_key, open_count, last_opened_at FROM frecency")?;
        let rows = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
            .collect::<Result<_, _>>()?;
        Ok(rows)
    }

    /// Deletes rows not opened for 180 days; returns how many.
    pub fn purge_stale_frecency(&self, now: i64) -> Result<usize, Error> {
        Ok(self.conn.execute(
            "DELETE FROM frecency WHERE last_opened_at < ?1",
            [now - STALE_SECS],
        )?)
    }

    pub fn star(&self, key: i64, now: i64) -> Result<(), Error> {
        self.conn.execute(
            "INSERT INTO starred_item (item_key, starred_at) VALUES (?1, ?2)
             ON CONFLICT (item_key) DO UPDATE SET starred_at = excluded.starred_at",
            params![key, now],
        )?;
        Ok(())
    }

    pub fn unstar(&self, key: i64) -> Result<(), Error> {
        self.conn
            .execute("DELETE FROM starred_item WHERE item_key = ?1", [key])?;
        Ok(())
    }

    pub fn starred_keys(&self) -> Result<std::collections::HashSet<i64>, Error> {
        let mut stmt = self.conn.prepare("SELECT item_key FROM starred_item")?;
        let rows = stmt
            .query_map([], |r| r.get(0))?
            .collect::<Result<std::collections::HashSet<i64>, _>>()?;
        Ok(rows)
    }
}

fn decode_hex(s: &str) -> Option<[u8; 16]> {
    if s.len() != 32 || !s.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let mut out = [0u8; 16];
    for (i, b) in out.iter_mut().enumerate() {
        *b = u8::from_str_radix(&s[2 * i..2 * i + 2], 16).ok()?;
    }
    Some(out)
}
