//! Frecency (GLOSSARY): how often and how recently the Owner opened an item. Rows live in the
//! `frecency` table keyed by a salted xxh3 hash of the item id, never by a path (D-014).
use naobi_core::db::{self, Db};
use naobi_core::ids::ItemId;
use std::collections::{HashMap, HashSet};

/// Opens lose half their weight every 30 days (D-016).
const HALF_LIFE_SECS: f64 = 30.0 * 86_400.0;

pub struct Frecency {
    db: Db,
    /// item key -> (open count, last opened at). Loaded once so a boost costs no SQL per row.
    rows: HashMap<i64, (i64, i64)>,
    /// Keys of Starred items (Home, Issue #23).
    starred: HashSet<i64>,
}

impl Frecency {
    /// Drops rows unused for 180 days (REPORT 4.2), then loads the rest.
    pub fn open(db: Db, now_secs: i64) -> Result<Self, db::Error> {
        db.purge_stale_frecency(now_secs)?;
        let rows = db
            .raw_rows()?
            .into_iter()
            .map(|(key, count, last)| (key, (count, last)))
            .collect();
        let starred = db.starred_keys()?;
        Ok(Self { db, rows, starred })
    }

    pub fn is_starred(&self, id: &ItemId) -> bool {
        self.starred.contains(&self.key(id))
    }

    pub fn toggle_star(&mut self, id: &ItemId, now_secs: i64) -> Result<bool, db::Error> {
        let key = self.key(id);
        if self.starred.contains(&key) {
            self.db.unstar(key)?;
            self.starred.remove(&key);
            Ok(false)
        } else {
            self.db.star(key, now_secs)?;
            self.starred.insert(key);
            Ok(true)
        }
    }

    /// Counts one open of `id` at `now_secs`.
    pub fn record(&mut self, id: &ItemId, now_secs: i64) -> Result<(), db::Error> {
        let key = self.key(id);
        self.db.record_open(key, now_secs)?;
        let row = self.rows.entry(key).or_insert((0, now_secs));
        row.0 += 1;
        row.1 = now_secs;
        Ok(())
    }

    /// Open count weighted by age: every 30 days since the last open halves it.
    pub fn decayed_opens(&self, id: &ItemId, now_secs: i64) -> f64 {
        let Some(&(count, last)) = self.rows.get(&self.key(id)) else {
            return 0.0;
        };
        let age = (now_secs - last).max(0) as f64;
        count as f64 * 0.5f64.powf(age / HALF_LIFE_SECS)
    }

    fn key(&self, id: &ItemId) -> i64 {
        db::item_key(&self.db.salt(), &id.0)
    }
}
