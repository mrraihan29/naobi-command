//! The icon cache and which keys may be asked for.
use naobi_core::ids::{ActionId, ItemId};
use naobi_core::item::{Item, ItemKind};
use naobi_core::ui::{Icon, UiEvent};
use naobi_launcher::icons::{askable, IconCache, CAPACITY};

fn icon(n: u8) -> Icon {
    Icon {
        w: 1,
        h: 1,
        rgba: vec![n, 0, 0, 255],
    }
}

// @R-012
#[test]
fn d014_icon_cache_capped_at_100() {
    let mut c = IconCache::default();
    for i in 0..150u32 {
        c.insert(format!("a:app{i}"), icon(1));
    }
    assert_eq!(c.len(), CAPACITY);
    assert!(c.get("a:app0").is_none(), "the oldest went");
    assert!(c.get("a:app49").is_none());
    assert!(c.get("a:app50").is_some());
    assert!(c.get("a:app149").is_some());
}

// @R-012
#[test]
fn r012_the_cache_drops_the_least_recently_used_icon() {
    let mut c = IconCache::default();
    for i in 0..CAPACITY {
        c.insert(format!("a:{i}"), icon(1));
    }
    // Reading the oldest makes it the newest: the next one in line goes instead.
    assert!(c.get("a:0").is_some());
    c.insert("a:new".into(), icon(2));
    assert!(c.get("a:0").is_some());
    assert!(c.get("a:1").is_none());
    // Replacing a key keeps one entry and the new picture.
    let n = c.len();
    c.insert("a:new".into(), icon(9));
    assert_eq!(c.len(), n);
    assert_eq!(c.get("a:new").map(|i| i.rgba[0]), Some(9));
    assert!(!c.is_empty());
}

// @R-010
#[test]
fn r010_a_key_that_could_reach_a_share_is_never_asked_for() {
    for good in [
        "e:txt",
        "e:",
        "e:7z",
        r"a:Microsoft.WindowsNotepad_8wekyb3d8bbwe!App",
        r"a:{1AC14E77-02E7-4E5D-B744-2EB1AE5198B7}\notepad.exe",
        r"a:C:\ProgramData\Microsoft\Windows\Start Menu\x.lnk",
    ] {
        assert!(askable(good), "{good}");
    }
    for bad in [
        "",
        "a:",
        r"p:C:\x.txt",
        r"a:\\evil\share\x.lnk",
        r"a:\evil",
        "a://evil/x",
        "a:https://evil/x",
        r"a:C:\\evil",
        "a:x\ny",
        "e:t.xt",
        "e:../x",
        "e:averyveryverylongextension",
        r"e:\\evil\share",
    ] {
        assert!(!askable(bad), "{bad:?}");
    }
    assert!(!askable(&format!("a:{}", "x".repeat(600))));
}

fn item(id: &str, kind: ItemKind) -> Item {
    Item {
        id: ItemId(id.into()),
        title: "t".into(),
        subtitle: String::new(),
        keywords: vec![],
        alias: None,
        kind,
        primary: ActionId("open".into()),
        secondary: None,
    }
}

// @R-012
#[test]
fn r012_rows_carry_an_icon_key_from_the_app_id_or_the_extension_never_the_path() {
    use naobi_core::command::{CommandSource, Cx, Outcome, Query};
    use naobi_core::db::Db;
    use naobi_core::item::Action;
    use naobi_launcher::frecency::Frecency;
    use naobi_launcher::{Clock, Launcher};
    use std::sync::Arc;

    struct Src(Vec<Item>);
    impl CommandSource for Src {
        fn items(&self, _q: &Query) -> Arc<[Item]> {
            self.0.clone().into()
        }
        fn actions(&self, _id: &ItemId) -> Vec<Action> {
            Vec::new()
        }
        fn run(&self, _id: &ItemId, _a: ActionId, _cx: &mut Cx) -> Outcome {
            Outcome::Done
        }
    }
    struct Now;
    impl Clock for Now {
        fn now_ms(&self) -> i64 {
            1_000_000_000_000
        }
    }
    let db = Db::open_in_memory().unwrap_or_else(|e| panic!("db: {e}"));
    let frecency = Frecency::open(db, 1_000_000_000).unwrap_or_else(|e| panic!("{e}"));
    let items = vec![
        item("app:Contoso.App!Main", ItemKind::App),
        item(r"file:C:\docs\Report.PDF", ItemKind::File),
        item(r"file:C:\docs\no-extension", ItemKind::File),
        item("cmd:lock", ItemKind::Command),
    ];
    let mut l = Launcher::new(vec![Box::new(Src(items))], vec![], frecency, Box::new(Now));
    let _ = l.handle(UiEvent::TextChanged("t".into()));
    let keys: Vec<Option<String>> = l
        .screen()
        .sections
        .iter()
        .flat_map(|s| s.rows.iter().map(|r| r.icon_key.clone()))
        .collect();
    assert!(
        keys.contains(&Some("a:Contoso.App!Main".to_owned())),
        "{keys:?}"
    );
    assert!(
        keys.contains(&Some("e:pdf".to_owned())),
        "lowercase extension: {keys:?}"
    );
    assert!(
        keys.contains(&Some("e:".to_owned())),
        "no extension: {keys:?}"
    );
    assert!(keys.contains(&None), "a command has none: {keys:?}");
    assert!(
        keys.iter().flatten().all(|k| !k.contains(r"C:\docs")),
        "no path in any key: {keys:?}"
    );
}
