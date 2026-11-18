//! The apps provider through the public engine, with a fake catalog (no Win32).
use naobi_core::command::{CommandSource, Cx, Outcome, Query};
use naobi_core::db::Db;
use naobi_core::effect::{Effect, LaunchTarget};
use naobi_core::ids::{ActionId, ItemId};
use naobi_core::item::{Action, Item, ItemKind};
use naobi_core::ui::{Key, KeyChord, Screen, UiEvent};
use naobi_launcher::frecency::Frecency;
use naobi_launcher::providers::apps::{AppCatalog, AppEntry, AppsSource};
use naobi_launcher::{Clock, Launcher};
use std::sync::Arc;

struct Zero;
impl Clock for Zero {
    fn now_ms(&self) -> i64 {
        1_000_000_000_000
    }
}

/// A command that is always there, like the built-in commands: typing finds it while apps load.
struct Commands;
impl CommandSource for Commands {
    fn items(&self, _q: &Query) -> Arc<[Item]> {
        vec![Item {
            id: ItemId("cmd:lock".into()),
            title: "Lock screen".into(),
            subtitle: String::new(),
            keywords: vec![],
            alias: None,
            kind: ItemKind::Command,
            primary: ActionId("run".into()),
            secondary: None,
        }]
        .into()
    }
    fn actions(&self, _id: &ItemId) -> Vec<Action> {
        vec![]
    }
    fn run(&self, _id: &ItemId, _a: ActionId, _cx: &mut Cx) -> Outcome {
        Outcome::Done
    }
}

fn entry(id: &str, name: &str) -> AppEntry {
    AppEntry {
        id: id.into(),
        name: name.into(),
    }
}

fn launcher(catalog: &AppCatalog) -> Launcher {
    let db = Db::open_in_memory().unwrap_or_else(|e| panic!("db: {e}"));
    let frecency = Frecency::open(db, 0).unwrap_or_else(|e| panic!("frecency: {e}"));
    Launcher::new(
        vec![
            Box::new(Commands),
            Box::new(AppsSource::new(catalog.clone())),
        ],
        Vec::new(),
        frecency,
        Box::new(Zero),
    )
}

fn titles(s: &Screen) -> Vec<String> {
    s.sections
        .iter()
        .flat_map(|sec| sec.rows.iter().map(|r| r.title.clone()))
        .collect()
}

fn key(k: Key) -> UiEvent {
    UiEvent::Key(KeyChord {
        key: k,
        ctrl: false,
        shift: false,
        alt: false,
    })
}

// @R-025
#[test]
fn r025_application_row_has_no_path() {
    let catalog = AppCatalog::default();
    catalog.set(vec![entry("App.Id!App", "Notepad")]);
    let mut l = launcher(&catalog);
    l.refresh_sources();
    l.handle(UiEvent::TextChanged("note".into()));
    let s = l.screen();
    let row = &s.sections[0].rows[0];
    assert_eq!((row.title.as_str(), row.subtitle.as_str()), ("Notepad", ""));
}

// @R-012
#[test]
fn d013_indexing_row_until_catalog_ready() {
    let catalog = AppCatalog::default();
    let mut l = launcher(&catalog);
    l.handle(UiEvent::TextChanged("loc".into()));
    let s = l.screen();
    // Typing already finds the command; the status row trails it and cannot be selected.
    assert_eq!(titles(&s), vec!["Lock screen", "Indexing apps"]);
    let status = s
        .sections
        .iter()
        .flat_map(|x| &x.rows)
        .last()
        .map(|r| (r.disabled, r.kind.clone()));
    assert_eq!(status, Some((true, "Status".to_owned())));
    assert!(s.loading, "the loading bar runs while apps are indexed");
    l.handle(key(Key::Down));
    assert_eq!(l.screen().selected, 0, "the status row is not a hit");

    catalog.set(vec![entry(
        "Microsoft.WindowsNotepad_8wekyb3d8bbwe!App",
        "Notepad",
    )]);
    l.refresh_sources();
    let s = l.screen();
    assert!(!titles(&s).contains(&"Indexing apps".to_owned()));
    assert!(!s.loading);
    l.handle(UiEvent::TextChanged("note".into()));
    assert_eq!(titles(&l.screen()), vec!["Notepad"]);
}

// @R-012
#[test]
fn r012_enter_on_an_app_launches_it_through_the_apps_folder_and_hides() {
    let catalog = AppCatalog::default();
    catalog.set(vec![
        entry("Microsoft.WindowsNotepad_8wekyb3d8bbwe!App", "Notepad"),
        entry(
            r"{1AC14E77-02E7-4E5D-B744-2EB1AE5198B7}\calc.exe",
            "Calculator",
        ),
    ]);
    let mut l = launcher(&catalog);
    l.handle(UiEvent::TextChanged("calc".into()));
    assert_eq!(l.screen().footer.primary.as_deref(), Some("Open"));
    let effects = l.handle(key(Key::Enter));
    assert_eq!(
        effects,
        vec![
            Effect::Launch(LaunchTarget::ShellItem(
                r"shell:AppsFolder\{1AC14E77-02E7-4E5D-B744-2EB1AE5198B7}\calc.exe".into()
            )),
            Effect::Hide,
        ]
    );
}

// @R-012
#[test]
fn r012_a_new_scan_replaces_the_catalog_and_duplicate_ids_collapse() {
    let catalog = AppCatalog::default();
    let source = AppsSource::new(catalog.clone());
    assert!(!catalog.is_ready());
    assert_eq!(source.pending(), Some("Indexing apps"));
    catalog.set(vec![
        entry("a", "Alpha"),
        entry("a", "Alpha again"),
        entry("b", "Beta"),
    ]);
    let q = Query::default();
    let first = source.items(&q);
    assert_eq!(first.len(), 2);
    assert_eq!(source.pending(), None);
    catalog.set(vec![entry("c", "Gamma")]);
    // A query that started earlier keeps the snapshot it took.
    assert_eq!(first.len(), 2);
    assert_eq!(source.items(&q).len(), 1);
}

// @R-012
#[test]
fn t003_the_catalog_lists_only_apps_and_refuses_ids_it_did_not_make() {
    let catalog = AppCatalog::default();
    catalog.set(vec![entry("a", "Alpha")]);
    let source = AppsSource::new(catalog);
    let q = Query::default();
    assert!(source
        .items(&q)
        .iter()
        .all(|i| i.kind == ItemKind::App && i.id.0.starts_with("app:")));
    let outcome = source.run(
        &ItemId(r"file:C:\Downloads\alpha.exe".into()),
        ActionId("open".into()),
        &mut Cx::default(),
    );
    assert!(
        matches!(outcome, Outcome::Failed(_)),
        "a non-app id is never launched"
    );
}
