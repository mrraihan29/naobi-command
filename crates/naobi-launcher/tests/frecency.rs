//! Frecency store over SQLite `:memory:`: decay per the 30-day half-life, and the 180-day purge.
use naobi_core::db::{item_key, Db};
use naobi_core::ids::ItemId;
use naobi_launcher::frecency::Frecency;

const DAY: i64 = 86_400;
const T0: i64 = 1_700_000_000;

// @R-012
#[test]
fn r012_boost_halves_every_30_days() {
    let mut f = Frecency::open(Db::open_in_memory().unwrap(), T0).unwrap();
    let id = ItemId("app:notepad".into());
    f.record(&id, T0).unwrap();
    f.record(&id, T0).unwrap();

    assert!((f.decayed_opens(&id, T0) - 2.0).abs() < 1e-9);
    assert!((f.decayed_opens(&id, T0 + 30 * DAY) - 1.0).abs() < 1e-9);
    assert!((f.decayed_opens(&id, T0 + 60 * DAY) - 0.5).abs() < 1e-9);
    assert_eq!(f.decayed_opens(&ItemId("app:other".into()), T0), 0.0);
}

// @R-012
#[test]
fn r012_rows_unused_for_180_days_are_purged_on_open() {
    let db = Db::open_in_memory().unwrap();
    let old = item_key(&db.salt(), "app:old");
    let recent = item_key(&db.salt(), "app:recent");
    db.record_open(old, T0).unwrap();
    db.record_open(recent, T0 + 170 * DAY).unwrap();

    let f = Frecency::open(db, T0 + 181 * DAY).unwrap();
    assert_eq!(
        f.decayed_opens(&ItemId("app:old".into()), T0 + 181 * DAY),
        0.0
    );
    assert!(f.decayed_opens(&ItemId("app:recent".into()), T0 + 181 * DAY) > 0.0);
}
