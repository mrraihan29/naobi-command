//! The Windows Search file rows: the SQL built from user text, the rows, the "unavailable" row and
//! the actions. The search itself is the app's (tier 2 there).
use naobi_core::command::{CommandSource, Cx, Outcome, Query};
use naobi_core::db::Db;
use naobi_core::effect::{Effect, LaunchTarget};
use naobi_core::ids::{ActionId, ItemId};
use naobi_core::item::{Action, Item, ItemKind};
use naobi_core::ui::{Key, KeyChord, Screen, UiEvent};
use naobi_launcher::frecency::Frecency;
use naobi_launcher::providers::files::{contains_sql, like_sql, rows, FileCommands, UNAVAILABLE};
use naobi_launcher::{Clock, Launcher};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

struct Now;

impl Clock for Now {
    fn now_ms(&self) -> i64 {
        1_000_000_000_000
    }
}

struct Apps(Vec<Item>);

impl CommandSource for Apps {
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

fn app(title: &str) -> Item {
    Item {
        id: ItemId(format!("app:{title}")),
        title: title.into(),
        subtitle: String::new(),
        keywords: vec![],
        alias: None,
        kind: ItemKind::App,
        primary: ActionId("open".into()),
        secondary: None,
    }
}

fn launcher(apps: Vec<Item>, flag: Arc<AtomicBool>) -> Launcher {
    let db = Db::open_in_memory().unwrap_or_else(|e| panic!("db: {e}"));
    let frecency = Frecency::open(db, 1_000_000_000).unwrap_or_else(|e| panic!("frecency: {e}"));
    Launcher::new(
        vec![Box::new(Apps(apps)), Box::new(FileCommands::new(flag))],
        Vec::new(),
        frecency,
        Box::new(Now),
    )
}

fn type_text(l: &mut Launcher, text: &str) {
    let _ = l.handle(UiEvent::TextChanged(text.into()));
}

fn titles(s: &Screen, header: Option<&str>) -> Vec<String> {
    s.sections
        .iter()
        .filter(|sec| sec.header.as_deref() == header)
        .flat_map(|sec| sec.rows.iter().map(|r| r.title.clone()))
        .collect()
}

// @R-013
#[test]
fn r013_query_with_quote_and_wildcards_escaped() {
    let tail = " AND scope='file:' ORDER BY System.Search.Rank DESC";
    let c = contains_sql("o'brien", 30).unwrap_or_default();
    assert!(c.starts_with("SELECT TOP 30 System.ItemPathDisplay FROM SystemIndex WHERE "));
    assert!(
        c.contains("CONTAINS(System.FileName, '\"o''brien*\"')"),
        "{c}"
    );
    assert!(c.ends_with(tail), "the scope clause is last: {c}");
    // Words are ANDed and each is wrapped in double quotes.
    let c = contains_sql("my  report", 30).unwrap_or_default();
    assert!(c.contains("'\"my*\" AND \"report*\"'"), "{c}");
    // A crafted quote cannot end the literal or the scope clause: it is doubled, and the double
    // quotes and the wildcard star the user typed are gone.
    let c = contains_sql("x' OR 1=1 --\"*", 30).unwrap_or_default();
    assert!(
        c.contains("\"x''*\" AND \"OR*\" AND \"1=1*\" AND \"--*\""),
        "{c}"
    );
    assert_eq!(c.matches("scope='file:'").count(), 1);
    assert!(c.ends_with(tail));
    // LIKE: quotes doubled, %, _ and [ match themselves, the scope clause is still last.
    let l = like_sql("50%", 30).unwrap_or_default();
    assert!(l.contains("System.FileName LIKE '%50[%]%'"), "{l}");
    let l = like_sql("a_b", 30).unwrap_or_default();
    assert!(l.contains("LIKE '%a[_]b%'"), "{l}");
    let l = like_sql("[x]'y", 30).unwrap_or_default();
    assert!(l.contains("LIKE '%[[]x]''y%'"), "{l}");
    assert!(l.ends_with(" AND scope='file:'"), "{l}");
    // Typed placeholders stay text: `replace` does not rescan what it inserted.
    let c = contains_sql("{terms} {top}", 30).unwrap_or_default();
    assert!(c.contains("'\"{terms}*\" AND \"{top}*\"'"), "{c}");
    let l = like_sql("{likes}", 30).unwrap_or_default();
    assert!(l.contains("LIKE '%{likes}%'"), "{l}");
    // Nothing usable, nothing asked; long text is cut; control characters split words.
    for blank in ["", "   ", "\"\"", "*", "\u{7}"] {
        assert_eq!(contains_sql(blank, 30), None, "{blank:?}");
        assert_eq!(like_sql(blank, 30), None, "{blank:?}");
    }
    let long = "a".repeat(500);
    assert!(contains_sql(&long, 30).unwrap_or_default().len() < 400);
    let c = contains_sql("a\u{0}b\nc", 7).unwrap_or_default();
    assert!(
        c.contains("TOP 7") && c.contains("\"a*\" AND \"b*\" AND \"c*\""),
        "{c}"
    );
}

// @R-013
#[test]
fn r013_search_disabled_shows_unavailable_row() {
    let flag = Arc::new(AtomicBool::new(false));
    let mut l = launcher(vec![app("Notes")], flag.clone());
    type_text(&mut l, "notes");
    assert!(titles(&l.screen(), None).iter().all(|t| t != UNAVAILABLE));
    // The search found the service stopped: one status row, and the sync rows stay.
    flag.store(true, Ordering::Relaxed);
    let s = l.screen();
    let status: Vec<_> = s
        .sections
        .iter()
        .flat_map(|sec| &sec.rows)
        .filter(|r| r.title == UNAVAILABLE)
        .collect();
    assert_eq!(status.len(), 1);
    assert!(status[0].disabled, "a status row is not selectable");
    assert!(!s.loading, "unavailable is not loading: no bar");
    assert!(titles(&s, None).contains(&"Notes".to_owned()));
    // With no query typed the row is not shown, and still no bar.
    type_text(&mut l, "");
    let s = l.screen();
    assert!(titles(&s, None).iter().all(|t| t != UNAVAILABLE));
    assert!(!s.loading);
    flag.store(false, Ordering::Relaxed);
    type_text(&mut l, "notes");
    assert!(titles(&l.screen(), None).iter().all(|t| t != UNAVAILABLE));
}

// @R-013
#[test]
fn r013_file_results_never_in_apps_section() {
    let mut l = launcher(vec![app("Chrome")], Arc::new(AtomicBool::new(false)));
    type_text(&mut l, "chrome");
    let (gen, source) = (l.generation(), l.file_source());
    assert_eq!(source, Some(1), "the source that takes files");
    let files = rows(&[r"C:\Users\me\Downloads\Chrome.exe".to_owned()]);
    assert_eq!(files[0].kind, ItemKind::File);
    assert!(l.file_results(gen, source.unwrap_or(0), files));
    let s = l.screen();
    assert_eq!(titles(&s, None), ["Chrome"], "only the real app up front");
    assert_eq!(titles(&s, Some("Files")), ["Chrome.exe"]);
    let row = &s.sections[1].rows[0];
    // The file name is the title and its folder the subtitle, so it is never taken for the app.
    assert_eq!(row.subtitle, r"C:\Users\me\Downloads");
    assert_eq!(row.kind, "File");
}

// @R-013
#[test]
fn r013_rows_have_name_title_path_subtitle_and_are_capped() {
    let paths: Vec<String> = (0..40).map(|i| format!(r"D:\docs\file{i}.txt")).collect();
    let r = rows(&paths);
    assert_eq!(r.len(), 30);
    assert_eq!(r[0].title, "file0.txt");
    assert_eq!(r[0].subtitle, r"D:\docs\file0.txt");
    assert_eq!(rows(&["notes".to_owned()])[0].title, "notes");
}

// @R-013
#[test]
fn r013_the_file_query_is_asked_only_at_the_root_with_text() {
    let mut l = launcher(vec![app("Notes")], Arc::new(AtomicBool::new(false)));
    assert_eq!(l.file_query(), None, "empty query");
    type_text(&mut l, "  ");
    assert_eq!(l.file_query(), None, "blank query");
    type_text(&mut l, "notes");
    assert_eq!(l.file_query(), Some((l.generation(), "notes".to_owned())));
    let first = l.generation();
    type_text(&mut l, "notes2");
    assert_ne!(l.file_query().map(|q| q.0), Some(first), "a new generation");
}

// @R-013
#[test]
fn r013_actions_open_the_file_or_its_folder() {
    let c = FileCommands::new(Arc::new(AtomicBool::new(false)));
    let id = ItemId(r"file:C:\Users\me\a b.txt".into());
    let labels: Vec<_> = c.actions(&id).into_iter().map(|a| a.label).collect();
    assert_eq!(labels, ["Open", "Open Containing Folder"]);
    let run = |act: &str, id: &ItemId| c.run(id, ActionId(act.into()), &mut Cx::default());
    let launch = |t: &str| {
        Outcome::Effects(vec![
            Effect::Launch(LaunchTarget::ShellItem(t.into())),
            Effect::Hide,
        ])
    };
    assert_eq!(run("open", &id), launch(r"C:\Users\me\a b.txt"));
    assert_eq!(run("folder", &id), launch(r"C:\Users\me"));
    // A file in a drive's root opens the root, not "C:".
    assert_eq!(
        run("folder", &ItemId(r"file:C:\a.txt".into())),
        launch(r"C:\")
    );
    assert_eq!(run("nope", &id), Outcome::Done);
    assert_eq!(run("open", &ItemId("app:x".into())), Outcome::Done);
    assert!(c.items(&Query::default()).is_empty());
}

// @R-013
#[test]
fn r013_opening_a_file_row_is_not_counted_as_usage() {
    let path = std::env::temp_dir().join(format!("naobi-files-freq-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let open = || Db::open(&path, || [7u8; 16]).unwrap_or_else(|e| panic!("db: {e}"));
    let frecency = Frecency::open(open(), 1_000_000_000).unwrap_or_else(|e| panic!("{e}"));
    let flag = Arc::new(AtomicBool::new(false));
    let mut l = Launcher::new(
        vec![Box::new(Apps(vec![])), Box::new(FileCommands::new(flag))],
        Vec::new(),
        frecency,
        Box::new(Now),
    );
    type_text(&mut l, "report");
    let (gen, source) = (l.generation(), l.file_source().unwrap_or(0));
    assert!(l.file_results(gen, source, rows(&[r"C:\docs\report.pdf".to_owned()])));
    let effects = l.handle(UiEvent::Key(KeyChord {
        key: Key::Enter,
        ctrl: false,
        alt: false,
        shift: false,
    }));
    assert!(
        effects.iter().any(|e| matches!(e, Effect::Launch(_))),
        "the file opened: {effects:?}"
    );
    drop(l);
    let stored = open().raw_rows().unwrap_or_default();
    let _ = std::fs::remove_file(&path);
    assert!(stored.is_empty(), "no usage row for a file: {stored:?}");
}
