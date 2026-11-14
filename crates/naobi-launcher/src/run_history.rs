//! The `>` run history (R-024, D-038): command lines the Owner ran, each stored as a cipher blob
//! (DPAPI in production, T-034). At most `KEEP` lines and none older than `MAX_AGE_MS`. Lines are
//! decrypted only while a `>` query is shown; `forget` drops them (zeroized).
use naobi_core::cipher::Cipher;
use naobi_core::db::Db;
use rusqlite::params;
use zeroize::Zeroizing;

pub const KEEP: i64 = 100;
pub const MAX_AGE_MS: i64 = 30 * 24 * 3600 * 1000;

pub struct Line {
    pub id: i64,
    pub line: Zeroizing<String>,
    /// The program path the first token resolved to, when it did.
    pub program: Option<String>,
}

/// A failure carries no detail: the data is the secret.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StoreError;

pub struct RunHistory {
    db: Db,
    cipher: Box<dyn Cipher>,
    cache: Option<Vec<Line>>,
}

impl RunHistory {
    /// Purges (SQL only, nothing is decrypted).
    pub fn open(db: Db, cipher: Box<dyn Cipher>, now_ms: i64) -> Result<Self, StoreError> {
        let mut h = Self {
            db,
            cipher,
            cache: None,
        };
        h.purge(now_ms)?;
        Ok(h)
    }

    /// Newest first (last_run_at DESC, id DESC). Decrypts on the first call and caches; a row that
    /// does not decrypt is skipped.
    pub fn lines(&mut self) -> &[Line] {
        if self.cache.is_none() {
            self.cache = Some(self.load());
        }
        self.cache.as_deref().unwrap_or(&[])
    }

    /// Drops the cache (each line is zeroized on drop).
    pub fn forget(&mut self) {
        self.cache = None;
    }

    /// The same line (exact, case-sensitive) gets last_run_at = now and the new program; otherwise
    /// one row is inserted. Then purge.
    pub fn record(
        &mut self,
        line: &str,
        program: Option<&str>,
        now_ms: i64,
    ) -> Result<(), StoreError> {
        let existing = self
            .lines()
            .iter()
            .find(|l| l.line.as_str() == line)
            .map(|l| l.id);
        let program_enc = match program {
            Some(p) => Some(self.cipher.encrypt(p.as_bytes()).map_err(|_| StoreError)?),
            None => None,
        };
        let result = match existing {
            Some(id) => self.db.transaction(|tx| {
                tx.execute(
                    "UPDATE run_history SET program_enc = ?1, last_run_at = ?2 WHERE id = ?3",
                    params![program_enc, now_ms, id],
                )
                .map(|_| ())
            }),
            None => {
                let line_enc = self
                    .cipher
                    .encrypt(line.as_bytes())
                    .map_err(|_| StoreError)?;
                self.db.transaction(|tx| {
                    tx.execute(
                        "INSERT INTO run_history (line_enc, program_enc, last_run_at) VALUES (?1, ?2, ?3)",
                        params![line_enc, program_enc, now_ms],
                    )
                    .map(|_| ())
                })
            }
        };
        // The cache no longer matches the table; the next `lines` decrypts again.
        self.cache = None;
        result.map_err(|_| StoreError)?;
        self.purge(now_ms)
    }

    pub fn delete(&mut self, id: i64) -> Result<(), StoreError> {
        self.cache = None;
        self.db
            .transaction(|tx| {
                tx.execute("DELETE FROM run_history WHERE id = ?1", params![id])
                    .map(|_| ())
            })
            .map_err(|_| StoreError)
    }

    fn purge(&mut self, now_ms: i64) -> Result<(), StoreError> {
        self.db
            .transaction(|tx| {
                tx.execute(
                    "DELETE FROM run_history WHERE last_run_at < ?1",
                    params![now_ms - MAX_AGE_MS],
                )?;
                tx.execute(
                    "DELETE FROM run_history WHERE id NOT IN \
                     (SELECT id FROM run_history ORDER BY last_run_at DESC, id DESC LIMIT ?1)",
                    params![KEEP],
                )
                .map(|_| ())
            })
            .map_err(|_| StoreError)
    }

    fn load(&self) -> Vec<Line> {
        let Ok(mut stmt) = self.db.conn().prepare(
            "SELECT id, line_enc, program_enc FROM run_history ORDER BY last_run_at DESC, id DESC",
        ) else {
            return Vec::new();
        };
        let Ok(rows) = stmt.query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, Vec<u8>>(1)?,
                r.get::<_, Option<Vec<u8>>>(2)?,
            ))
        }) else {
            return Vec::new();
        };
        rows.flatten()
            .filter_map(|(id, line_enc, program_enc)| {
                let plain = self.cipher.decrypt(&line_enc).ok()?;
                let line = Zeroizing::new(std::str::from_utf8(&plain).ok()?.to_owned());
                let program = program_enc
                    .and_then(|b| self.cipher.decrypt(&b).ok())
                    .and_then(|p| std::str::from_utf8(&p).ok().map(str::to_owned));
                Some(Line { id, line, program })
            })
            .collect()
    }
}
