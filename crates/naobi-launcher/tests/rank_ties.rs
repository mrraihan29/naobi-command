//! Ranking ties through the public ranker.
use naobi_core::ids::{ActionId, ItemId};
use naobi_core::item::{Item, ItemKind};
use naobi_launcher::rank::{Ranker, Source};

fn item(id: &str, title: &str) -> Item {
    Item {
        id: ItemId(id.into()),
        title: title.into(),
        subtitle: String::new(),
        keywords: Vec::new(),
        alias: None,
        kind: ItemKind::Command,
        primary: ActionId("run".into()),
        secondary: None,
    }
}

// @R-012
#[test]
fn r012_equal_scores_and_opens_put_the_shorter_title_first() {
    let pool = [
        item("clear", "Clear Clipboard History"),
        item("open", "Clipboard History"),
    ];
    let hits = Ranker::default().rank(
        "clipboard",
        pool.iter().map(|i| (Source::Root(0), i)),
        |_| 0.0,
        10,
    );
    let ids: Vec<&str> = hits.iter().map(|h| h.item.id.0.as_str()).collect();
    assert_eq!(ids, ["open", "clear"]);
}
