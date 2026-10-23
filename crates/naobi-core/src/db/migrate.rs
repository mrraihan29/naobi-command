//! Migration runner: file copy first, then each missing step in its own transaction (D-014, ADR-0006).
use super::Error;
use rusqlite::Connection;
use std::path::Path;

/// Brings `conn` up to `migrations.len()`. A fresh file runs everything; an older file is
/// copied to `<name>.bak-v<N-1>` first. A newer file is left alone (expand-only keeps old readers working).
pub fn run(conn: &mut Connection, path: Option<&Path>, migrations: &[&str]) -> Result<(), Error> {
    let have: usize = conn.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))? as usize;
    if have >= migrations.len() {
        return Ok(());
    }
    if have > 0 {
        if let Some(path) = path {
            // Fold the WAL into the main file so the copy is complete.
            conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE)")?;
            let mut name = path.file_name().unwrap_or_default().to_os_string();
            name.push(format!(".bak-v{have}"));
            std::fs::copy(path, path.with_file_name(name))?;
        }
    }
    for (i, sql) in migrations.iter().enumerate().skip(have) {
        let tx = conn.transaction()?;
        tx.execute_batch(sql)?;
        tx.pragma_update(None, "user_version", (i + 1) as i64)?;
        tx.commit()?;
    }
    Ok(())
}
