//! The last 20 picks, in the `color_history` table (R-017). A color is not personal data (REPORT 4).
use crate::format::Rgb;
use naobi_core::db::Db;
use rusqlite::params;

/// R-017: the 21st pick drops the oldest.
pub const KEEP: i64 = 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pick {
    pub id: i64,
    pub rgb: Rgb,
    pub picked_at: i64,
}

pub struct ColorHistory {
    db: Db,
}

impl ColorHistory {
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    /// Records a pick and drops everything older than the newest 20.
    pub fn push(&mut self, c: Rgb, now_ms: i64) -> Result<(), naobi_core::db::Error> {
        // The column is RGBA; a screen pixel is opaque.
        let rgba = i64::from(u32::from_be_bytes([c.r, c.g, c.b, 0xFF]));
        self.db.transaction(|tx| {
            tx.execute(
                "INSERT INTO color_history (rgba, picked_at) VALUES (?1, ?2)",
                params![rgba, now_ms],
            )?;
            tx.execute(
                "DELETE FROM color_history WHERE id NOT IN \
                 (SELECT id FROM color_history ORDER BY id DESC LIMIT ?1)",
                params![KEEP],
            )?;
            Ok(())
        })
    }

    /// Newest first.
    pub fn list(&self) -> rusqlite::Result<Vec<Pick>> {
        let mut stmt = self.db.conn().prepare(
            "SELECT id, rgba, picked_at FROM color_history ORDER BY picked_at DESC, id DESC",
        )?;
        let rows = stmt.query_map([], |r| {
            let rgba: i64 = r.get(1)?;
            Ok(Pick {
                id: r.get(0)?,
                rgb: {
                    let [r, g, b, _] = (rgba as u32).to_be_bytes();
                    Rgb { r, g, b }
                },
                picked_at: r.get(2)?,
            })
        })?;
        rows.collect()
    }
}
