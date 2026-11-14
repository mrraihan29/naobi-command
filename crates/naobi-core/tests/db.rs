//! Data store tests (D-014, R-009 part). Real SQLite files in the temp dir; no mocks.
#![allow(clippy::unwrap_used)] // helpers outside #[test] fns; clippy.toml only exempts test fns themselves
use naobi_core::db::{item_key, schema, Db, Error};
use rusqlite::{types::Value, Connection};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

static N: AtomicU32 = AtomicU32::new(0);

/// Fresh empty directory, removed on drop.
struct Dir(PathBuf);
impl Dir {
    fn new() -> Dir {
        let p = std::env::temp_dir().join(format!(
            "naobi-db-test-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst)
        ));
        std::fs::create_dir_all(&p).unwrap();
        Dir(p)
    }
    fn db(&self) -> PathBuf {
        self.0.join("naobi.db")
    }
}
impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn pragma(c: &Connection, name: &str) -> String {
    match c
        .pragma_query_value(None, name, |r| r.get::<_, Value>(0))
        .unwrap()
    {
        Value::Integer(i) => i.to_string(),
        Value::Text(t) => t,
        other => format!("{other:?}"),
    }
}

// @R-009
#[test]
fn d014_wal_pragmas_and_schema() {
    let d = Dir::new();
    let db = Db::open(&d.db(), || [1; 16]).unwrap();
    let c = db.conn();
    assert_eq!(pragma(c, "journal_mode"), "wal");
    assert_eq!(pragma(c, "synchronous"), "1"); // NORMAL
    assert_eq!(pragma(c, "cache_size"), "-2048"); // 2 MiB
    assert_eq!(pragma(c, "auto_vacuum"), "2"); // INCREMENTAL
    assert_eq!(pragma(c, "user_version"), "2");
    let names: Vec<String> = c
        .prepare("SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(
        names,
        [
            "clip_entry",
            "clip_payload",
            "color_history",
            "frecency",
            "meta",
            "run_history"
        ]
    );
}

// @R-009
#[test]
fn d014_schema_v2_adds_run_history_and_backs_up_v1() {
    let d = Dir::new();
    {
        let db = Db::open_with_migrations(&d.db(), || [1; 16], &schema::MIGRATIONS[..1]).unwrap();
        assert_eq!(pragma(db.conn(), "user_version"), "1");
        db.record_open(42, 1000).unwrap();
    }
    let db = Db::open(&d.db(), || [9; 16]).unwrap();
    assert_eq!(pragma(db.conn(), "user_version"), "2");
    let bak = d.0.join("naobi.db.bak-v1");
    assert!(bak.exists());
    assert_eq!(
        pragma(&Connection::open(&bak).unwrap(), "user_version"),
        "1"
    );
    assert_eq!(db.raw_rows().unwrap(), vec![(42, 1, 1000)]);
    db.conn()
        .execute(
            "INSERT INTO run_history (line_enc, last_run_at) VALUES (x'00', 1)",
            [],
        )
        .unwrap();
    drop(db);
    // A V1-only reader still opens the V2 file.
    let old = Db::open_with_migrations(&d.db(), || [9; 16], &schema::MIGRATIONS[..1]).unwrap();
    old.record_open(42, 2000).unwrap();
    assert_eq!(old.raw_rows().unwrap(), vec![(42, 2, 2000)]);
}

#[test]
fn d014_migration_copies_backup_and_is_expand_contract() {
    let d = Dir::new();
    {
        let db = Db::open_with_migrations(&d.db(), || [1; 16], &schema::MIGRATIONS[..1]).unwrap();
        db.record_open(42, 1000).unwrap();
    }
    // Test-only second migration: adds a nullable column, nothing else.
    let v2 = [
        schema::MIGRATIONS[0],
        "ALTER TABLE frecency ADD COLUMN note TEXT",
    ];
    let db = Db::open_with_migrations(&d.db(), || [9; 16], &v2).unwrap();
    assert_eq!(pragma(db.conn(), "user_version"), "2");
    assert_eq!(db.salt(), [1; 16]); // existing salt survives
    let bak = d.0.join("naobi.db.bak-v1");
    assert!(bak.exists());
    assert_eq!(
        pragma(&Connection::open(&bak).unwrap(), "user_version"),
        "1"
    );
    drop(db);
    // An older (v1) reader still opens the v2 file and uses its own v1 statements.
    let old = Db::open(&d.db(), || [9; 16]).unwrap();
    old.record_open(42, 2000).unwrap();
    assert_eq!(old.raw_rows().unwrap(), vec![(42, 2, 2000)]);
}

#[test]
fn d014_item_key_is_stable_xxh3() {
    // Independent vector: XXH3_64bits("") with seed 0 is 0x2D06800538D394C2 (xxHash reference).
    assert_eq!(item_key(&[0; 16], "") as u64, 0x2D06_8005_38D3_94C2);
    let salt = [1, 2, 3, 4, 5, 6, 7, 8, 9, 9, 9, 9, 9, 9, 9, 9];
    let id = "C:\\Apps\\Notepad.exe";
    let a = item_key(&salt, id);
    assert_eq!(a, item_key(&salt, id));
    assert_ne!(a, item_key(&[2; 16], id));
    // Only the first 8 salt bytes seed the hash.
    assert_eq!(
        a,
        item_key(&[1, 2, 3, 4, 5, 6, 7, 8, 0, 0, 0, 0, 0, 0, 0, 0], id)
    );
    // Pinned from the first run so the algorithm and byte order cannot drift.
    assert_eq!(a, GOLDEN);
}
const GOLDEN: i64 = -1483481640253124595;

#[test]
fn d014_salt_created_once() {
    let d = Dir::new();
    let first = Db::open(&d.db(), || [5; 16]).unwrap().salt();
    assert_eq!(first, [5; 16]);
    let again = Db::open(&d.db(), || panic!("random must not be called again")).unwrap();
    assert_eq!(again.salt(), [5; 16]);
}

#[test]
fn d014_meta_set_rejects_reserved_salt_key() {
    let d = Dir::new();
    let db = Db::open(&d.db(), || [5; 16]).unwrap();
    assert!(matches!(
        db.meta_set("hash_salt", &"00".repeat(16)),
        Err(Error::ReservedKey)
    ));
    drop(db);
    let again = Db::open(&d.db(), || panic!("random must not be called again")).unwrap();
    assert_eq!(again.salt(), [5; 16]);
}

#[test]
fn d014_corrupt_salt_is_bad_salt_not_panic() {
    let d = Dir::new();
    drop(Db::open(&d.db(), || [5; 16]).unwrap());
    // 30 chars; 32 bytes non-ASCII; 32 bytes with '+' (from_str_radix accepts it); 32 bytes non-hex.
    let bad = [
        "00".repeat(15),
        "\u{e9}".repeat(16),
        format!("+1{}", "00".repeat(15)),
        "zz".repeat(16),
    ];
    for v in bad {
        let c = Connection::open(d.db()).unwrap();
        c.execute("UPDATE meta SET value = ?1 WHERE key = 'hash_salt'", [&v])
            .unwrap();
        drop(c);
        let r = Db::open(&d.db(), || [9; 16]);
        assert!(matches!(r, Err(Error::BadSalt)), "salt {v:?}");
    }
}

#[test]
fn d014_sql_metacharacters_roundtrip() {
    let db = Db::open_in_memory().unwrap();
    let evil = "'; DROP TABLE META;--😀";
    db.meta_set(evil, evil).unwrap();
    assert_eq!(db.meta_get(evil).unwrap().as_deref(), Some(evil));
    assert!(db.meta_get("hash_salt").unwrap().is_some()); // table survived
}

#[test]
fn d014_stale_frecency_purged() {
    let db = Db::open_in_memory().unwrap();
    let now = 1_000_000_000;
    let day = 86_400;
    db.record_open(1, now - 181 * day).unwrap();
    db.record_open(2, now - 179 * day).unwrap();
    assert_eq!(db.purge_stale_frecency(now).unwrap(), 1);
    let rows = db.raw_rows().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].0, 2);
}

// @R-009
#[test]
fn r009_schema_has_no_plaintext_content_column() {
    let db = Db::open_in_memory().unwrap();
    let c = db.conn();
    let tables: Vec<String> = c
        .prepare("SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%'")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    for t in tables {
        let cols: Vec<(String, String)> = c
            .prepare("SELECT name, type FROM pragma_table_info(?1)")
            .unwrap()
            .query_map([&t], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        for (name, ty) in cols {
            let contentish = ["data", "preview", "content", "text", "body"]
                .iter()
                .any(|w| name.contains(w));
            if contentish {
                assert_eq!(ty, "BLOB", "{t}.{name} must be a blob");
                assert!(
                    name.ends_with("_enc"),
                    "{t}.{name} must be marked encrypted"
                );
            }
        }
    }
}
