//! Persistent calculator history (issue #2): expression and result are stored as cipher blobs
//! (DPAPI in production, T-034). Keeps at most `KEEP` newest entries.
use naobi_core::cipher::Cipher;
use naobi_core::db::Db;
use rusqlite::{params, OptionalExtension};
use zeroize::Zeroizing;

pub const KEEP: i64 = 100;

pub struct CalcEntry {
    pub id: i64,
    pub expr: Zeroizing<String>,
    pub result: Zeroizing<String>,
    pub calculated_at: i64,
}

/// A failure carries no detail: the data is the secret.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StoreError;

pub struct CalcHistory {
    db: Db,
    cipher: Box<dyn Cipher>,
}

impl CalcHistory {
    pub fn open(db: Db, cipher: Box<dyn Cipher>) -> Self {
        Self { db, cipher }
    }

    /// Records `(expr, result)` at `now_ms`, updating `calculated_at` if the exact pair already
    /// exists, and prunes older entries beyond `KEEP`.
    pub fn record(&mut self, expr: &str, result: &str, now_ms: i64) -> Result<(), StoreError> {
        let existing = self
            .list()
            .into_iter()
            .find(|e| e.expr.as_str() == expr && e.result.as_str() == result)
            .map(|e| e.id);
        match existing {
            Some(id) => self.db.transaction(|tx| {
                tx.execute(
                    "UPDATE calc_history SET calculated_at = ?1 WHERE id = ?2",
                    params![now_ms, id],
                )
                .map(|_| ())
            }),
            None => {
                let expr_enc = self
                    .cipher
                    .encrypt(expr.as_bytes())
                    .map_err(|_| StoreError)?;
                let result_enc = self
                    .cipher
                    .encrypt(result.as_bytes())
                    .map_err(|_| StoreError)?;
                self.db.transaction(|tx| {
                    tx.execute(
                        "INSERT INTO calc_history (expr_enc, result_enc, calculated_at) VALUES (?1, ?2, ?3)",
                        params![expr_enc, result_enc, now_ms],
                    )?;
                    tx.execute(
                        "DELETE FROM calc_history WHERE id NOT IN \
                         (SELECT id FROM calc_history ORDER BY calculated_at DESC, id DESC LIMIT ?1)",
                        params![KEEP],
                    )
                    .map(|_| ())
                })
            }
        }
        .map_err(|_| StoreError)
    }

    /// Newest first (`calculated_at DESC, id DESC`). Unreadable rows are skipped.
    pub fn list(&self) -> Vec<CalcEntry> {
        let Ok(mut stmt) = self.db.conn().prepare(
            "SELECT id, expr_enc, result_enc, calculated_at \
             FROM calc_history ORDER BY calculated_at DESC, id DESC",
        ) else {
            return Vec::new();
        };
        let Ok(rows) = stmt.query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, Vec<u8>>(1)?,
                r.get::<_, Vec<u8>>(2)?,
                r.get::<_, i64>(3)?,
            ))
        }) else {
            return Vec::new();
        };
        rows.flatten()
            .filter_map(|(id, expr_enc, result_enc, calculated_at)| {
                self.decrypt_entry(id, &expr_enc, &result_enc, calculated_at)
            })
            .collect()
    }

    pub fn get(&self, id: i64) -> Option<CalcEntry> {
        let row: (Vec<u8>, Vec<u8>, i64) = self
            .db
            .conn()
            .query_row(
                "SELECT expr_enc, result_enc, calculated_at FROM calc_history WHERE id = ?1",
                params![id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()
            .ok()??;
        self.decrypt_entry(id, &row.0, &row.1, row.2)
    }

    /// Most recent numeric result for `ans` chaining. Only decrypts until the first valid number.
    pub fn latest_ans(&self) -> Option<f64> {
        let mut stmt = self
            .db
            .conn()
            .prepare("SELECT result_enc FROM calc_history ORDER BY calculated_at DESC, id DESC")
            .ok()?;
        let rows = stmt.query_map([], |r| r.get::<_, Vec<u8>>(0)).ok()?;
        for blob in rows.flatten() {
            let Ok(plain) = self.cipher.decrypt(&blob) else {
                continue;
            };
            let Ok(text) = std::str::from_utf8(&plain) else {
                continue;
            };
            if let Some(v) = naobi_calc::parse_ans(text) {
                return Some(v);
            }
        }
        None
    }

    pub fn delete(&mut self, id: i64) -> Result<(), StoreError> {
        self.db
            .transaction(|tx| {
                tx.execute("DELETE FROM calc_history WHERE id = ?1", params![id])
                    .map(|_| ())
            })
            .map_err(|_| StoreError)
    }

    fn decrypt_entry(
        &self,
        id: i64,
        expr_enc: &[u8],
        result_enc: &[u8],
        calculated_at: i64,
    ) -> Option<CalcEntry> {
        let expr_bytes = self.cipher.decrypt(expr_enc).ok()?;
        let res_bytes = self.cipher.decrypt(result_enc).ok()?;
        let expr = Zeroizing::new(std::str::from_utf8(&expr_bytes).ok()?.to_owned());
        let result = Zeroizing::new(std::str::from_utf8(&res_bytes).ok()?.to_owned());
        Some(CalcEntry {
            id,
            expr,
            result,
            calculated_at,
        })
    }
}
