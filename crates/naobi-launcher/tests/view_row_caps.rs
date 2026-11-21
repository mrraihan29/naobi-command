//! Row caps on an empty query: the root lists EMPTY_QUERY_ROWS; an unranked view lists up to MAX_ROWS.
use naobi_core::command::{CommandSource, Cx, Outcome, Query};
use naobi_core::db::Db;
use naobi_core::ids::{ActionId, ItemId, ViewId};
use naobi_core::item::{Action, Item, ItemKind};
use naobi_core::view_source::{Preview, ViewSource};
use naobi_launcher::frecency::Frecency;
use naobi_launcher::rank::{EMPTY_QUERY_ROWS, MAX_ROWS};
use naobi_launcher::{Clock, Launcher};
use std::sync::Arc;

struct Zero;
impl Clock for Zero {
    fn now_ms(&self) -> i64 {
        1_000_000_000_000
    }
}

struct Many(usize);

impl CommandSource for Many {
    fn items(&self, _q: &Query) -> Arc<[Item]> {
        (0..self.0)
            .map(|i| Item {
                id: ItemId(format!("i{i}")),
                title: format!("Item {i:02}"),
                subtitle: String::new(),
                keywords: Vec::new(),
                alias: None,
                kind: ItemKind::Command,
                primary: ActionId("run".into()),
                secondary: None,
            })
            .collect()
    }
    fn actions(&self, _id: &ItemId) -> Vec<Action> {
        Vec::new()
    }
    fn run(&self, _id: &ItemId, _action: ActionId, _cx: &mut Cx) -> Outcome {
        Outcome::Done
    }
}

struct UnrankedView(Many);

impl CommandSource for UnrankedView {
    fn items(&self, q: &Query) -> Arc<[Item]> {
        self.0.items(q)
    }
    fn actions(&self, id: &ItemId) -> Vec<Action> {
        self.0.actions(id)
    }
    fn run(&self, id: &ItemId, action: ActionId, cx: &mut Cx) -> Outcome {
        self.0.run(id, action, cx)
    }
}

impl ViewSource for UnrankedView {
    fn view_id(&self) -> ViewId {
        ViewId("many".into())
    }
    fn opened_by_hotkey(&self) -> bool {
        true
    }
    fn preview(&self, _id: &ItemId) -> Option<Preview> {
        None
    }
    fn ranked(&self) -> bool {
        false
    }
}

fn rig(n: usize) -> Launcher {
    let db = Db::open_in_memory().unwrap_or_else(|e| panic!("db: {e}"));
    let frecency = Frecency::open(db, 0).unwrap_or_else(|e| panic!("frecency: {e}"));
    Launcher::new(
        vec![Box::new(Many(n))],
        vec![Box::new(UnrankedView(Many(n)))],
        frecency,
        Box::new(Zero),
    )
}

fn row_count(l: &Launcher) -> usize {
    l.screen().sections.iter().map(|s| s.rows.len()).sum()
}

// @R-031
#[test]
fn r031_unranked_view_lists_every_row_on_an_empty_query() {
    let mut l = rig(30);
    assert!(l.show_view(&ViewId("many".into())));
    assert_eq!(row_count(&l), 30);
    let mut l = rig(MAX_ROWS + 20);
    assert!(l.show_view(&ViewId("many".into())));
    assert_eq!(row_count(&l), MAX_ROWS);
}

// @R-012
#[test]
fn r012_root_empty_query_still_caps_at_the_empty_query_rows() {
    let l = rig(30);
    assert_eq!(row_count(&l), EMPTY_QUERY_ROWS);
}
