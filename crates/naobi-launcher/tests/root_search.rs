//! Root search through the public engine: ranking, frecency order, the view stack, the Esc ladder,
//! file rows and armed commands. Each test crosses `handle` and reads `screen` or the effects.
use naobi_core::command::{CommandSource, Cx, Outcome, Query};
use naobi_core::db::Db;
use naobi_core::effect::{Effect, LaunchTarget};
use naobi_core::ids::{ActionId, EntryId, ItemId, ViewId};
use naobi_core::item::{Action, Item, ItemKind};
use naobi_core::notice::{Notice, NoticeKind};
use naobi_core::ui::{Key, KeyChord, PreviewView, Screen, UiEvent};
use naobi_core::view_source::{Preview, ViewSource};
use naobi_launcher::frecency::Frecency;
use naobi_launcher::{Clock, Launcher};
use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;

/// Milliseconds the fake clock starts at.
const T0_MS: i64 = 1_000_000_000_000;

#[derive(Clone)]
struct FakeClock(Rc<Cell<i64>>);

impl Clock for FakeClock {
    fn now_ms(&self) -> i64 {
        self.0.get()
    }
}

fn action(id: &str, label: &str) -> Action {
    Action {
        id: ActionId(id.into()),
        label: label.into(),
        shortcut: None,
        enabled: Ok(()),
    }
}

fn item(id: &str, title: &str) -> Item {
    Item {
        id: ItemId(id.into()),
        title: title.into(),
        subtitle: String::new(),
        keywords: vec![],
        alias: None,
        kind: ItemKind::App,
        primary: ActionId("open".into()),
        secondary: Some(ActionId("copy".into())),
    }
}

/// Root source. Its run result depends on the id: `push:*` pushes the clipboard view, `fail:*`
/// returns a Notice, anything else launches `<id>:<action>` so tests can see the action that ran.
struct Apps {
    items: Vec<Item>,
}

impl CommandSource for Apps {
    fn items(&self, _q: &Query) -> Arc<[Item]> {
        self.items.clone().into()
    }
    fn actions(&self, _id: &ItemId) -> Vec<Action> {
        vec![action("open", "Open"), action("copy", "Copy path")]
    }
    fn run(&self, id: &ItemId, act: ActionId, _cx: &mut Cx) -> Outcome {
        if id.0.starts_with("push:") {
            Outcome::Effects(vec![Effect::PushView(ViewId("clip".into()))])
        } else if id.0.starts_with("note:") {
            Outcome::Effects(vec![
                Effect::Notice(Notice {
                    subject: "Copied".into(),
                    message: "path".into(),
                    kind: NoticeKind::Info,
                }),
                Effect::Hide,
            ])
        } else if id.0.starts_with("fail:") {
            Outcome::Failed(Notice {
                subject: "Restart".into(),
                message: "Refused by Windows.".into(),
                kind: NoticeKind::Error,
            })
        } else {
            Outcome::Effects(vec![Effect::Launch(LaunchTarget::ShellItem(format!(
                "{}:{}",
                id.0, act.0
            )))])
        }
    }
}

/// A clipboard-like view: its entries, and Enter pastes.
struct Clip {
    hotkey: bool,
    entries: Vec<Item>,
}

impl CommandSource for Clip {
    fn items(&self, _q: &Query) -> Arc<[Item]> {
        self.entries.clone().into()
    }
    fn actions(&self, _id: &ItemId) -> Vec<Action> {
        vec![]
    }
    fn run(&self, _id: &ItemId, _act: ActionId, _cx: &mut Cx) -> Outcome {
        Outcome::Effects(vec![Effect::Paste(EntryId(1))])
    }
}

impl ViewSource for Clip {
    fn view_id(&self) -> ViewId {
        ViewId("clip".into())
    }
    fn title(&self) -> &'static str {
        "Clipboard"
    }
    fn opened_by_hotkey(&self) -> bool {
        self.hotkey
    }
    fn preview(&self, id: &ItemId) -> Option<Preview> {
        Some(Preview::Text(format!("body of {}", id.0)))
    }
}

fn clip_view(hotkey: bool) -> Vec<Box<dyn ViewSource>> {
    vec![Box::new(Clip {
        hotkey,
        entries: vec![item("entry:1", "entry one"), item("entry:2", "entry two")],
    })]
}

fn engine(apps: Vec<Item>, views: Vec<Box<dyn ViewSource>>) -> (Launcher, Rc<Cell<i64>>) {
    let now = Rc::new(Cell::new(T0_MS));
    let db = Db::open_in_memory().unwrap_or_else(|e| panic!("in-memory db: {e}"));
    let frecency =
        Frecency::open(db, T0_MS / 1000).unwrap_or_else(|e| panic!("frecency open: {e}"));
    let launcher = Launcher::new(
        vec![Box::new(Apps { items: apps })],
        views,
        frecency,
        Box::new(FakeClock(now.clone())),
    );
    (launcher, now)
}

fn type_text(l: &mut Launcher, text: &str) {
    assert!(l.handle(UiEvent::TextChanged(text.into())).is_empty());
}

fn key(l: &mut Launcher, key: Key) -> Vec<Effect> {
    l.handle(UiEvent::Key(KeyChord {
        key,
        ctrl: false,
        alt: false,
        shift: false,
    }))
}

fn ctrl(l: &mut Launcher, key: Key) -> Vec<Effect> {
    l.handle(UiEvent::Key(KeyChord {
        key,
        ctrl: true,
        alt: false,
        shift: false,
    }))
}

fn titles(s: &Screen) -> Vec<String> {
    s.sections
        .iter()
        .flat_map(|sec| sec.rows.iter().map(|r| r.title.clone()))
        .collect()
}

fn launch(target: &str) -> Effect {
    Effect::Launch(LaunchTarget::ShellItem(target.into()))
}

// @R-012
#[test]
fn r012_exact_alias_ranks_first() {
    let mut zed = item("app:zed", "Zed editor");
    zed.alias = Some("code".into());
    let (mut l, _) = engine(vec![item("app:vs", "Visual Studio Code"), zed], vec![]);
    type_text(&mut l, "code");
    assert_eq!(titles(&l.screen())[0], "Zed editor");
}

// @R-012
#[test]
fn r012_more_opened_item_ranks_above_equal_fuzzy_match() {
    let (mut l, _) = engine(vec![item("app:a", "Notes"), item("app:b", "Notes")], vec![]);
    type_text(&mut l, "notes");
    // Equal titles keep insertion order, so row 1 is "app:b": open it once.
    key(&mut l, Key::Down);
    assert_eq!(
        key(&mut l, Key::Enter),
        vec![launch("app:b:open"), Effect::Hide]
    );

    l.show_root();
    type_text(&mut l, "notes");
    assert_eq!(
        key(&mut l, Key::Enter),
        vec![launch("app:b:open"), Effect::Hide]
    );
}

// @R-012
#[test]
fn r012_frecency_boost_never_exceeds_20_percent_of_fuzzy_score() {
    // "Notes" fuzzy-scores about 140; "Nxoxtxexs long text here" scores about 88. Opened 50 times,
    // the second gets at most 20% of 88 (about 17.6) more, so 105.6 < 140 and the first stays first.
    let (mut l, _) = engine(
        vec![
            item("app:a", "Notes"),
            item("app:b", "Nxoxtxexs long text here"),
        ],
        vec![],
    );
    // Open the second item through a query only it matches, so the opening never depends on the order.
    for _ in 0..50 {
        l.show_root();
        type_text(&mut l, "nxox");
        key(&mut l, Key::Enter);
    }
    l.show_root();
    type_text(&mut l, "notes");
    assert_eq!(
        titles(&l.screen()),
        vec!["Notes", "Nxoxtxexs long text here"]
    );
}

// @R-012
#[test]
fn r012_enter_on_app_launches_and_hides() {
    let (mut l, _) = engine(vec![item("app:notepad", "Notepad")], vec![]);
    type_text(&mut l, "notepad");
    assert_eq!(
        key(&mut l, Key::Enter),
        vec![launch("app:notepad:open"), Effect::Hide]
    );
}

// @R-012
#[test]
fn r012_ctrl_k_lists_actions_of_selected_result() {
    let (mut l, _) = engine(vec![item("app:notepad", "Notepad")], vec![]);
    type_text(&mut l, "notepad");
    assert!(ctrl(&mut l, Key::K).is_empty());
    let panel = l.screen().action_panel.expect("panel open");
    let labels: Vec<_> = panel.actions.iter().map(|a| a.label.as_str()).collect();
    assert_eq!(labels, vec!["Open", "Copy path", "Copy item ID"]);
}

// @R-012
#[test]
fn r012_ctrl_enter_runs_secondary_action() {
    let (mut l, _) = engine(vec![item("app:notepad", "Notepad")], vec![]);
    type_text(&mut l, "notepad");
    assert_eq!(
        ctrl(&mut l, Key::Enter),
        vec![launch("app:notepad:copy"), Effect::Hide]
    );
}

// @R-012
#[test]
fn r012_esc_clears_query_then_pops_view_then_hides() {
    let (mut l, _) = engine(
        vec![item("push:clip", "Clipboard history")],
        clip_view(false),
    );
    type_text(&mut l, "clip");
    assert_eq!(
        key(&mut l, Key::Enter),
        vec![Effect::PushView(ViewId("clip".into()))],
        "a push keeps the Launcher open"
    );
    type_text(&mut l, "entry");
    assert_eq!(titles(&l.screen()), vec!["entry one", "entry two"]);

    // Query first: the view stays and its list comes back unfiltered.
    assert!(key(&mut l, Key::Escape).is_empty());
    assert_eq!(l.screen().query, "");
    assert_eq!(titles(&l.screen()), vec!["entry one", "entry two"]);

    // Then the view pops to the root.
    assert!(key(&mut l, Key::Escape).is_empty());
    assert_eq!(titles(&l.screen()), vec!["Clipboard history"]);

    // Then the Launcher hides.
    assert_eq!(key(&mut l, Key::Escape), vec![Effect::Hide]);
}

// @R-012
#[test]
fn r012_esc_in_hotkey_opened_view_hides() {
    let (mut l, _) = engine(vec![item("app:notepad", "Notepad")], clip_view(true));
    assert!(l.show_view(&ViewId("clip".into())));
    type_text(&mut l, "x");
    assert!(
        key(&mut l, Key::Escape).is_empty(),
        "first Esc clears the query"
    );
    assert_eq!(key(&mut l, Key::Escape), vec![Effect::Hide]);
}

// @R-012
#[test]
fn r012_esc_in_pushed_view_pops_to_root() {
    let (mut l, _) = engine(
        vec![
            item("push:clip", "Clipboard history"),
            item("app:notepad", "Notepad"),
        ],
        clip_view(true),
    );
    type_text(&mut l, "clip");
    key(&mut l, Key::Enter);
    assert!(key(&mut l, Key::Escape).is_empty());
    assert_eq!(titles(&l.screen()), vec!["Clipboard history", "Notepad"]);
}

// @R-012
#[test]
fn r012_esc_closes_panel_then_notice_then_query_then_hides() {
    let (mut l, _) = engine(vec![item("fail:restart", "Restart")], vec![]);
    type_text(&mut l, "restart");
    assert_eq!(
        key(&mut l, Key::Enter),
        vec![Effect::Notice(Notice {
            subject: "Restart".into(),
            message: "Refused by Windows.".into(),
            kind: NoticeKind::Error,
        })],
        "a failed run keeps the Launcher open"
    );
    assert_eq!(
        l.screen().notice.map(|n| n.message).as_deref(),
        Some("Restart: Refused by Windows.")
    );
    ctrl(&mut l, Key::K);
    assert!(l.screen().action_panel.is_some());

    assert!(key(&mut l, Key::Escape).is_empty());
    assert!(l.screen().action_panel.is_none());
    assert!(
        l.screen().notice.is_some(),
        "notice survives the panel closing"
    );

    assert!(key(&mut l, Key::Escape).is_empty());
    assert!(l.screen().notice.is_none());
    assert_eq!(l.screen().query, "restart", "query survives the Notice");

    assert!(key(&mut l, Key::Escape).is_empty());
    assert_eq!(l.screen().query, "");
    assert_eq!(key(&mut l, Key::Escape), vec![Effect::Hide]);
}

// @R-012
#[test]
fn r012_file_results_trail_other_results() {
    let (mut l, _) = engine(vec![item("app:notes", "Notes")], vec![]);
    type_text(&mut l, "notes");
    let gen = l.generation();
    let mut file = item("file:notes.txt", "notes.txt");
    file.kind = ItemKind::File;
    assert!(l.file_results(gen, 0, vec![file]));

    let s = l.screen();
    assert_eq!(s.sections.len(), 2);
    assert_eq!(s.sections[0].header, None);
    assert_eq!(s.sections[0].rows[0].title, "Notes");
    assert_eq!(s.sections[1].header.as_deref(), Some("Files"));
    assert_eq!(s.sections[1].rows[0].title, "notes.txt");
}

// @R-012
#[test]
fn r012_late_file_rows_do_not_block_sync_rows() {
    let (mut l, _) = engine(vec![item("app:notes", "Notes")], vec![]);
    type_text(&mut l, "notes");
    let stale = l.generation();
    type_text(&mut l, "notes");
    assert!(
        !l.file_results(stale, 0, vec![item("file:n", "n")]),
        "stale generation dropped"
    );
    let s = l.screen();
    assert_eq!(titles(&s), vec!["Notes"], "sync rows stay, no file section");

    let current = l.generation();
    assert!(l.file_results(current, 0, vec![item("file:n", "n")]));
}

/// A root source with one last-row fallback (R-034) for every query; a `>` query is claimed.
struct Web;

impl CommandSource for Web {
    fn items(&self, q: &Query) -> Arc<[Item]> {
        if q.text.starts_with('>') {
            vec![item("cmd:run", "Run it")].into()
        } else {
            Vec::new().into()
        }
    }
    fn claims(&self, q: &Query) -> bool {
        q.text.starts_with('>')
    }
    fn fallback(&self, q: &Query) -> Option<Item> {
        let mut i = item("qf:web", &format!("Search Web for {}", q.text));
        i.kind = ItemKind::Quicklink;
        Some(i)
    }
    fn actions(&self, _id: &ItemId) -> Vec<Action> {
        vec![action("open", "Open")]
    }
    fn run(&self, id: &ItemId, act: ActionId, _cx: &mut Cx) -> Outcome {
        Outcome::Effects(vec![Effect::Launch(LaunchTarget::ShellItem(format!(
            "{}:{}",
            id.0, act.0
        )))])
    }
}

fn web_engine(apps: Vec<Item>, views: Vec<Box<dyn ViewSource>>) -> Launcher {
    let db = Db::open_in_memory().unwrap_or_else(|e| panic!("in-memory db: {e}"));
    let frecency =
        Frecency::open(db, T0_MS / 1000).unwrap_or_else(|e| panic!("frecency open: {e}"));
    Launcher::new(
        vec![Box::new(Apps { items: apps }), Box::new(Web)],
        views,
        frecency,
        Box::new(FakeClock(Rc::new(Cell::new(T0_MS)))),
    )
}

// @R-034
#[test]
fn r034_fallback_is_the_last_row_after_ranked_and_file_rows_and_never_the_start_selection() {
    let mut l = web_engine(vec![item("app:notes", "Notes")], vec![]);
    type_text(&mut l, "notes");
    assert_eq!(titles(&l.screen()), vec!["Notes", "Search Web for notes"]);
    assert_eq!(l.selected(), 0);

    let gen = l.generation();
    let mut file = item("file:notes.txt", "notes.txt");
    file.kind = ItemKind::File;
    assert!(l.file_results(gen, 0, vec![file]));
    let s = l.screen();
    assert_eq!(
        titles(&s),
        vec!["Notes", "notes.txt", "Search Web for notes"]
    );
    assert_eq!(s.sections.len(), 3);
    assert_eq!(s.sections[1].header.as_deref(), Some("Files"));
    assert_eq!(s.sections[2].header.as_deref(), Some("Web"));
    assert_eq!(l.selected(), 0);

    // Down twice reaches the fallback; Enter runs it through the source that owns it.
    key(&mut l, Key::Down);
    key(&mut l, Key::Down);
    assert_eq!(
        key(&mut l, Key::Enter),
        vec![launch("qf:web:open"), Effect::Hide]
    );
}

// @R-034
#[test]
fn r034_late_file_rows_do_not_move_the_selection_off_the_fallback_row() {
    let mut l = web_engine(vec![item("app:notes", "Notes")], vec![]);
    type_text(&mut l, "notes");
    key(&mut l, Key::Down);
    assert_eq!(l.selected(), 1, "the fallback is selected");
    let gen = l.generation();
    let mut file = item("file:notes.txt", "notes.txt");
    file.kind = ItemKind::File;
    assert!(l.file_results(gen, 0, vec![file]));
    assert_eq!(l.selected(), 2, "still the fallback, now after the file");
    assert_eq!(
        key(&mut l, Key::Enter),
        vec![launch("qf:web:open"), Effect::Hide]
    );
}

// @R-034
#[test]
fn r034_without_files_the_fallback_has_no_header_and_is_the_only_row_when_nothing_matches() {
    let mut l = web_engine(vec![item("app:notes", "Notes")], vec![]);
    type_text(&mut l, "zzzz qqqq");
    let s = l.screen();
    assert_eq!(titles(&s), vec!["Search Web for zzzz qqqq"]);
    assert_eq!(s.sections.len(), 1);
    assert_eq!(s.sections[0].header, None);
    assert_eq!(
        key(&mut l, Key::Enter),
        vec![launch("qf:web:open"), Effect::Hide]
    );
}

// @R-034
#[test]
fn r034_empty_text_a_claimed_query_and_a_pushed_view_list_no_fallback() {
    let mut l = web_engine(vec![item("push:clip", "Clipboard")], clip_view(false));
    assert!(!titles(&l.screen()).iter().any(|t| t.starts_with("Search")));
    type_text(&mut l, "> echo");
    assert_eq!(titles(&l.screen()), vec!["Run it"]);

    type_text(&mut l, "clipboard");
    assert_eq!(
        key(&mut l, Key::Enter),
        vec![Effect::PushView(ViewId("clip".into()))]
    );
    assert!(!titles(&l.screen()).iter().any(|t| t.starts_with("Search")));
    type_text(&mut l, "entry");
    assert!(!titles(&l.screen()).iter().any(|t| t.starts_with("Search")));
}

// @R-020
#[test]
fn r020_single_enter_arms_second_executes() {
    let (mut l, _) = engine(vec![item("cmd:restart", "Restart")], vec![]);
    l.set_armed([ItemId("cmd:restart".into())]);
    type_text(&mut l, "restart");

    assert!(key(&mut l, Key::Enter).is_empty(), "first Enter only arms");
    let s = l.screen();
    assert!(s.sections[0].rows[0].armed);
    assert_eq!(s.footer.primary.as_deref(), Some("Confirm"));

    assert_eq!(
        key(&mut l, Key::Enter),
        vec![launch("cmd:restart:open"), Effect::Hide]
    );
}

// @R-020
#[test]
fn r020_other_key_or_timeout_disarms() {
    let (mut l, now) = engine(vec![item("cmd:restart", "Restart")], vec![]);
    l.set_armed([ItemId("cmd:restart".into())]);
    type_text(&mut l, "restart");

    assert!(key(&mut l, Key::Enter).is_empty());
    // Any other key disarms: Down is a selection change even with one row.
    assert!(key(&mut l, Key::Down).is_empty());
    assert!(key(&mut l, Key::Enter).is_empty(), "re-armed, not executed");

    // Five seconds on the fake clock disarms: the next Enter arms again instead of running.
    now.set(now.get() + 5_000);
    assert!(key(&mut l, Key::Enter).is_empty());
    assert_eq!(
        key(&mut l, Key::Enter),
        vec![launch("cmd:restart:open"), Effect::Hide]
    );
}

#[test]
fn d016_two_letter_query_has_no_floor_and_exact_title_ranks_first() {
    // Round 2: a two-letter query has no floor, so the scattered matches stay; "ZQ" still ranks first.
    let (mut l, _) = engine(
        vec![
            item("app:quarry", "Zebra Quarry"),
            item("app:zoom", "Zoom quick"),
            item("app:zq", "ZQ"),
        ],
        vec![],
    );
    type_text(&mut l, "zq");
    let shown = titles(&l.screen());
    assert_eq!(shown.len(), 3);
    assert_eq!(shown[0], "ZQ");
}

#[test]
fn d016_empty_query_lists_ten_most_frecent() {
    let apps: Vec<Item> = (1..=12)
        .map(|i| item(&format!("app:{i}"), &format!("Item {i:02}")))
        .collect();
    let (mut l, _) = engine(apps, vec![]);
    type_text(&mut l, "item 11");
    assert_eq!(
        key(&mut l, Key::Enter),
        vec![launch("app:11:open"), Effect::Hide]
    );

    l.show_root();
    let s = l.screen();
    let shown = titles(&s);
    assert_eq!(shown.len(), 10);
    assert_eq!(shown[0], "Item 11", "the opened item is most frecent");
    assert_eq!(shown[1], "Item 01", "ties break by title");
}

#[test]
fn d021_every_show_starts_at_root_with_empty_query() {
    let (mut l, _) = engine(
        vec![
            item("push:clip", "Clipboard history"),
            item("app:notepad", "Notepad"),
        ],
        clip_view(false),
    );
    type_text(&mut l, "clip");
    key(&mut l, Key::Enter);
    type_text(&mut l, "entry");

    l.show_root();
    let s = l.screen();
    assert_eq!(s.query, "");
    assert_eq!(titles(&s), vec!["Clipboard history", "Notepad"]);
}

/// Words for the synthetic 10,000-item catalog (the same vocabulary as the naobi-app fixture).
const WORDS: &[&str] = &[
    "visual",
    "studio",
    "code",
    "terminal",
    "profile",
    "chrome",
    "notes",
    "calendar",
    "mail",
    "photos",
    "music",
    "video",
    "maps",
    "weather",
    "calculator",
    "clock",
    "settings",
    "system",
    "network",
    "bluetooth",
    "display",
    "sound",
    "storage",
    "update",
    "security",
    "privacy",
    "backup",
    "recorder",
    "snipping",
    "paint",
    "editor",
    "browser",
    "player",
    "reader",
    "writer",
    "sheet",
    "slides",
    "draw",
    "design",
    "build",
    "deploy",
    "docker",
    "git",
    "cargo",
    "python",
    "node",
    "java",
    "rust",
    "swift",
    "kotlin",
    "monitor",
];

fn synthetic_catalog(n: usize) -> Vec<Item> {
    (0..n)
        .map(|i| {
            let (a, b) = (WORDS[i % WORDS.len()], WORDS[(i * 7 + 3) % WORDS.len()]);
            item(&format!("app:{i}"), &format!("{a} {b} {i}"))
        })
        .collect()
}

fn row_count(s: &Screen) -> usize {
    s.sections.iter().map(|sec| sec.rows.len()).sum()
}

/// True when every character of `q` appears in `title` in order, ignoring case.
fn is_subsequence(q: &str, title: &str) -> bool {
    let mut t = title.chars().flat_map(char::to_lowercase);
    q.chars()
        .flat_map(char::to_lowercase)
        .all(|qc| t.any(|tc| tc == qc))
}

// D-016: a one- or two-letter query shows only real matches, ranked, on a 10,000-item catalog.
#[test]
fn d016_one_and_two_letter_queries_show_only_real_matches_on_10k() {
    let (mut l, _) = engine(synthetic_catalog(10_000), vec![]);
    for q in ["c", "v", "vs", "wt", "pt", "pw", "ch"] {
        type_text(&mut l, q);
        let shown = titles(&l.screen());
        assert!(!shown.is_empty(), "query {q:?} has real matches");
        assert!(shown.len() <= 50, "query {q:?} shows at most 50 rows");
        for t in &shown {
            assert!(
                is_subsequence(q, t),
                "query {q:?} shows {t:?}, which has no match"
            );
        }
    }
}

// D-016: a short query that matches nothing shows no rows. The vocabulary has no x, q or z.
#[test]
fn d016_short_query_with_no_match_shows_no_rows_on_10k() {
    let (mut l, _) = engine(synthetic_catalog(10_000), vec![]);
    for q in ["x", "z", "qq", "xj"] {
        type_text(&mut l, q);
        assert_eq!(
            row_count(&l.screen()),
            0,
            "query {q:?} matches nothing, so no rows"
        );
    }
}

// D-016: a longer junk query matches nothing, so no garbage rows.
#[test]
fn d016_six_letter_junk_returns_no_rows_on_10k() {
    let (mut l, _) = engine(synthetic_catalog(10_000), vec![]);
    for q in ["qzxjwk", "vsnmcd", "wtbxpk", "kdmfqz"] {
        type_text(&mut l, q);
        assert_eq!(
            row_count(&l.screen()),
            0,
            "junk query {q:?} should show no rows"
        );
    }
}

// R-020: Ctrl+Enter on an armed command arms it; the second Ctrl+Enter runs it.
// @R-020
#[test]
fn r020_ctrl_enter_on_armed_command_arms_then_confirms() {
    let (mut l, _) = engine(vec![item("cmd:restart", "Restart")], vec![]);
    l.set_armed([ItemId("cmd:restart".into())]);
    type_text(&mut l, "restart");
    assert!(
        ctrl(&mut l, Key::Enter).is_empty(),
        "first Ctrl+Enter only arms"
    );
    assert_eq!(
        ctrl(&mut l, Key::Enter),
        vec![launch("cmd:restart:copy"), Effect::Hide]
    );
}

// R-020: Ctrl+K then Enter arms the armed command; the next Enter runs it.
// @R-020
#[test]
fn r020_action_panel_enter_on_armed_command_arms_then_confirms() {
    let (mut l, _) = engine(vec![item("cmd:restart", "Restart")], vec![]);
    l.set_armed([ItemId("cmd:restart".into())]);
    type_text(&mut l, "restart");
    assert!(ctrl(&mut l, Key::K).is_empty());
    assert!(
        key(&mut l, Key::Enter).is_empty(),
        "Enter in the panel only arms"
    );
    assert!(
        l.screen().action_panel.is_none(),
        "the panel closes when it arms"
    );
    assert_eq!(
        key(&mut l, Key::Enter),
        vec![launch("cmd:restart:open"), Effect::Hide]
    );
}

// R-020: the confirm must be the same action that armed; another action arms again.
// @R-020
#[test]
fn r020_confirm_needs_the_same_action() {
    let (mut l, _) = engine(vec![item("cmd:restart", "Restart")], vec![]);
    l.set_armed([ItemId("cmd:restart".into())]);
    type_text(&mut l, "restart");
    assert!(
        key(&mut l, Key::Enter).is_empty(),
        "Enter arms the primary action"
    );
    assert!(
        ctrl(&mut l, Key::Enter).is_empty(),
        "Ctrl+Enter arms the secondary, does not run the primary"
    );
    assert_eq!(
        ctrl(&mut l, Key::Enter),
        vec![launch("cmd:restart:copy"), Effect::Hide]
    );
}

// R-020: a second Enter inside 5 s still runs; the window is exactly ARM_MS long.
// @R-020
#[test]
fn r020_second_enter_inside_five_seconds_runs() {
    let (mut l, now) = engine(vec![item("cmd:restart", "Restart")], vec![]);
    l.set_armed([ItemId("cmd:restart".into())]);
    type_text(&mut l, "restart");
    assert!(key(&mut l, Key::Enter).is_empty());
    now.set(now.get() + 4_999);
    assert_eq!(
        key(&mut l, Key::Enter),
        vec![launch("cmd:restart:open"), Effect::Hide]
    );
}

// D-018: a Notice raised by a run that hides is kept for the next show; the first Esc dismisses
// the banner and the second hides. The notice is not cleared on show.
#[test]
fn d018_notice_from_a_hidden_run_waits_for_next_show_and_esc_dismisses_it() {
    let (mut l, _) = engine(vec![item("note:copy", "Copy path")], vec![]);
    type_text(&mut l, "copy");
    assert_eq!(
        key(&mut l, Key::Enter),
        vec![
            Effect::Notice(Notice {
                subject: "Copied".into(),
                message: "path".into(),
                kind: NoticeKind::Info,
            }),
            Effect::Hide,
        ]
    );
    l.show_root();
    assert!(
        l.screen().notice.is_some(),
        "the notice waits for the next show"
    );
    assert!(
        key(&mut l, Key::Escape).is_empty(),
        "the first Esc dismisses the banner"
    );
    assert!(l.screen().notice.is_none());
    assert_eq!(key(&mut l, Key::Escape), vec![Effect::Hide]);
}

const THIRTY_DAYS_MS: i64 = 30 * 24 * 60 * 60 * 1000;

// D-016 frecency: an open decays by half every 30 days, so a recent open outranks an older one.
#[test]
fn d016_recent_open_outranks_an_older_one_after_decay() {
    let (mut l, now) = engine(vec![item("app:a", "Alpha"), item("app:b", "Beta")], vec![]);
    type_text(&mut l, "alpha");
    key(&mut l, Key::Enter);
    now.set(now.get() + THIRTY_DAYS_MS);
    l.show_root();
    type_text(&mut l, "beta");
    key(&mut l, Key::Enter);
    l.show_root();
    assert_eq!(titles(&l.screen()), vec!["Beta", "Alpha"]);
}

// D-016: a run that failed did not open the item, so it does not count for frecency.
#[test]
fn d016_failed_run_does_not_count_as_an_open() {
    let (mut l, _) = engine(
        vec![item("app:alpha", "Alpha"), item("fail:restart", "Restart")],
        vec![],
    );
    type_text(&mut l, "restart");
    assert!(
        !key(&mut l, Key::Enter).is_empty(),
        "the failure raises a Notice"
    );
    l.show_root();
    assert_eq!(titles(&l.screen())[0], "Alpha");
}

// Up moves the selection back up; Down and Up are inverses.
#[test]
fn up_and_down_move_the_selection() {
    let (mut l, _) = engine(
        vec![item("app:a", "Notes A"), item("app:b", "Notes B")],
        vec![],
    );
    type_text(&mut l, "notes");
    key(&mut l, Key::Down);
    assert_eq!(l.screen().selected, 1);
    key(&mut l, Key::Up);
    assert_eq!(l.screen().selected, 0);
}

// A plain K types into the field; only Ctrl+K opens the action panel.
#[test]
fn plain_k_does_not_open_the_action_panel() {
    let (mut l, _) = engine(vec![item("app:notepad", "Notepad")], vec![]);
    type_text(&mut l, "notepad");
    assert!(key(&mut l, Key::K).is_empty());
    assert!(l.screen().action_panel.is_none());
}

// The footer names the selected row's primary action.
#[test]
fn footer_names_the_primary_action() {
    let (mut l, _) = engine(vec![item("app:notepad", "Notepad")], vec![]);
    type_text(&mut l, "notepad");
    assert_eq!(l.screen().footer.primary.as_deref(), Some("Open"));
    assert!(l.screen().footer.actions);
}

// RowActivated runs that row; an index past the end does nothing.
#[test]
fn row_activation_runs_the_row_and_ignores_a_bad_index() {
    let (mut l, _) = engine(vec![item("app:notepad", "Notepad")], vec![]);
    type_text(&mut l, "notepad");
    assert!(l.handle(UiEvent::RowActivated(5)).is_empty());
    assert_eq!(
        l.handle(UiEvent::RowActivated(0)),
        vec![launch("app:notepad:open"), Effect::Hide]
    );
}

// show_view opens only a hosted view that has its own hotkey.
#[test]
fn show_view_refuses_a_view_without_a_hotkey() {
    let (mut l, _) = engine(vec![item("app:notepad", "Notepad")], clip_view(false));
    assert!(!l.show_view(&ViewId("clip".into())));
}

#[test]
fn show_view_refuses_an_unknown_view() {
    let (mut l, _) = engine(vec![item("app:notepad", "Notepad")], clip_view(true));
    assert!(!l.show_view(&ViewId("nope".into())));
}

// The view's preview reaches the screen for the selected row.
#[test]
fn view_preview_reaches_the_screen() {
    let (mut l, _) = engine(vec![], clip_view(true));
    assert!(l.show_view(&ViewId("clip".into())));
    type_text(&mut l, "entry");
    assert_eq!(
        l.screen().preview,
        Some(PreviewView::Text("body of entry:1".into()))
    );
}

// File rows count in the selection: Down reaches them, and Enter runs the selected file row.
#[test]
fn file_rows_count_in_the_selection() {
    let (mut l, _) = engine(vec![item("app:notes", "Notes")], vec![]);
    type_text(&mut l, "notes");
    let gen = l.generation();
    let mut f1 = item("file:a.txt", "notes a.txt");
    f1.kind = ItemKind::File;
    let mut f2 = item("file:b.txt", "notes b.txt");
    f2.kind = ItemKind::File;
    assert!(l.file_results(gen, 0, vec![f1, f2]));
    key(&mut l, Key::Down);
    key(&mut l, Key::Down);
    assert_eq!(l.screen().selected, 2);
    assert_eq!(
        key(&mut l, Key::Enter),
        vec![launch("file:b.txt:open"), Effect::Hide]
    );
}

// File results need a query and the root: an empty query and a pushed view both refuse them.
#[test]
fn file_results_need_a_query_and_the_root() {
    let (mut l, _) = engine(
        vec![item("push:clip", "Clipboard history")],
        clip_view(false),
    );
    let gen = l.generation();
    assert!(
        !l.file_results(gen, 0, vec![item("file:x", "x")]),
        "no query, no files"
    );
    type_text(&mut l, "clip");
    key(&mut l, Key::Enter);
    type_text(&mut l, "entry");
    let gen = l.generation();
    assert!(
        !l.file_results(gen, 0, vec![item("file:x", "x")]),
        "a pushed view takes no files"
    );
}

// Every show advances the generation, so answers from before it are stale.
#[test]
fn every_show_advances_the_generation() {
    let (mut l, _) = engine(vec![item("app:notes", "Notes")], vec![]);
    let before = l.generation();
    l.show_root();
    assert!(l.generation() > before);
}

// Each row names its kind; only a calculator row is flagged as a calculator.
#[test]
fn rows_name_their_kind() {
    let mut calc = item("calc:1", "Notes calc");
    calc.kind = ItemKind::Calculator;
    let mut cmd = item("cmd:2", "Notes command");
    cmd.kind = ItemKind::Command;
    let (mut l, _) = engine(vec![item("app:notes", "Notes"), calc, cmd], vec![]);
    type_text(&mut l, "notes");
    let rows: Vec<_> = l
        .screen()
        .sections
        .into_iter()
        .flat_map(|s| s.rows)
        .collect();
    let by_title = |t: &str| rows.iter().find(|r| r.title == t).cloned().expect("row");
    let app = by_title("Notes");
    assert_eq!((app.kind.as_str(), app.calculator), ("Application", false));
    let calc = by_title("Notes calc");
    assert_eq!((calc.kind.as_str(), calc.calculator), ("Calculator", true));
    assert_eq!(by_title("Notes command").kind, "Command");
}

// Only the armed row is flagged as armed; a row that merely matches the same query is not.
#[test]
fn only_the_armed_row_is_flagged() {
    let (mut l, _) = engine(
        vec![
            item("cmd:restart", "Restart"),
            item("cmd:restart2", "Restart later"),
        ],
        vec![],
    );
    l.set_armed([ItemId("cmd:restart".into())]);
    type_text(&mut l, "restart");
    assert!(key(&mut l, Key::Enter).is_empty(), "first Enter arms");
    let rows: Vec<_> = l
        .screen()
        .sections
        .into_iter()
        .flat_map(|s| s.rows)
        .collect();
    let armed: Vec<_> = rows
        .iter()
        .filter(|r| r.armed)
        .map(|r| r.title.clone())
        .collect();
    assert_eq!(armed, vec!["Restart"]);
}

fn selected_title(l: &Launcher) -> String {
    let s = l.screen();
    titles(&s)[s.selected].clone()
}

fn many(n: usize) -> Vec<Item> {
    (0..n)
        .map(|i| item(&format!("app:{i}"), &format!("App {i:02}")))
        .collect()
}

// @R-012
#[test]
fn r012_page_keys_move_a_page_and_stop_at_the_ends() {
    let (mut l, _) = engine(many(20), vec![]);
    // An empty query lists only the ten most frecent; a query lists up to fifty.
    type_text(&mut l, "app");
    assert_eq!(selected_title(&l), "App 00");
    assert!(key(&mut l, Key::PageDown).is_empty());
    assert_eq!(selected_title(&l), "App 08");
    key(&mut l, Key::PageDown);
    key(&mut l, Key::PageDown);
    assert_eq!(selected_title(&l), "App 19", "stops at the last row");
    key(&mut l, Key::PageUp);
    assert_eq!(selected_title(&l), "App 11");
    key(&mut l, Key::PageUp);
    key(&mut l, Key::PageUp);
    assert_eq!(selected_title(&l), "App 00", "stops at the first row");
}

// @R-012
#[test]
fn r012_backspace_on_an_empty_field_pops_a_pushed_view_only() {
    let (mut l, _) = engine(
        vec![
            item("push:clip", "Clipboard history"),
            item("app:notepad", "Notepad"),
        ],
        clip_view(true),
    );
    // Root: nothing to pop.
    assert!(key(&mut l, Key::Backspace).is_empty());
    assert_eq!(titles(&l.screen()), vec!["Clipboard history", "Notepad"]);
    type_text(&mut l, "clip");
    key(&mut l, Key::Enter);
    assert_eq!(titles(&l.screen()), vec!["entry one", "entry two"]);
    // A query is still there: the field owns Backspace, the engine leaves the view alone.
    type_text(&mut l, "one");
    assert!(key(&mut l, Key::Backspace).is_empty());
    assert_eq!(titles(&l.screen()), vec!["entry one"]);
    type_text(&mut l, "");
    assert!(key(&mut l, Key::Backspace).is_empty());
    assert_eq!(titles(&l.screen()), vec!["Clipboard history", "Notepad"]);
}

// @R-012
#[test]
fn r012_backspace_does_not_leave_a_view_opened_by_its_own_hotkey() {
    let (mut l, _) = engine(vec![], clip_view(true));
    assert!(l.show_view(&ViewId("clip".into())));
    assert!(key(&mut l, Key::Backspace).is_empty());
    assert_eq!(titles(&l.screen()), vec!["entry one", "entry two"]);
    assert_eq!(key(&mut l, Key::Escape), vec![Effect::Hide]);
}

// @R-012
#[test]
fn clear_notice_drops_the_engine_banner() {
    let (mut l, _) = engine(vec![item("fail:x", "Broken")], vec![]);
    key(&mut l, Key::Enter);
    assert!(l.screen().notice.is_some());
    l.clear_notice();
    assert!(l.screen().notice.is_none());
}

// @R-017
#[test]
fn r017_a_color_row_draws_its_hex_as_the_swatch() {
    let color = |id: &str, title: &str| Item {
        kind: ItemKind::Color,
        ..item(id, title)
    };
    let (mut l, _) = engine(
        vec![
            color("c:1", "#914A3B"),
            color("c:2", "914A3C"),
            color("c:3", "#914A3"),
            item("a:1", "#914A3D app"),
        ],
        Vec::new(),
    );
    type_text(&mut l, "914A3");
    let s = l.screen();
    let swatch = |title: &str| {
        s.sections
            .iter()
            .flat_map(|sec| &sec.rows)
            .find(|r| r.title == title)
            .map(|r| r.swatch)
    };
    assert_eq!(swatch("#914A3B"), Some(Some(0x914A3B)));
    assert_eq!(swatch("914A3C"), Some(None), "no hash: no swatch");
    assert_eq!(swatch("#914A3"), Some(None), "not six digits: no swatch");
    assert_eq!(swatch("#914A3D app"), Some(None), "not a Color item");
}

// A catalog that changes in the background must not move the selection under the Owner's Enter.
#[test]
fn a_source_refresh_keeps_the_selected_item() {
    let (mut l, _) = engine(
        vec![item("app:notepad", "Notepad"), item("app:notes", "Notes")],
        vec![],
    );
    type_text(&mut l, "not");
    l.handle(UiEvent::RowHovered(1));
    l.refresh_sources();
    assert_eq!(l.screen().selected, 1);
}

// The pointer moving onto a row selects it, like the arrow keys; a bad index changes nothing.
#[test]
fn hovering_a_row_selects_it_and_ignores_a_bad_index() {
    let (mut l, _) = engine(
        vec![item("app:notepad", "Notepad"), item("app:notes", "Notes")],
        vec![],
    );
    type_text(&mut l, "not");
    assert_eq!(l.screen().selected, 0);
    assert!(l.handle(UiEvent::RowHovered(1)).is_empty());
    assert_eq!(l.screen().selected, 1);
    assert!(l.handle(UiEvent::RowHovered(9)).is_empty());
    assert_eq!(l.screen().selected, 1);
}

// @R-025
#[test]
fn r025_matched_title_letters_are_marked() {
    let (mut l, _) = engine(vec![item("a", "Visual Studio Code")], vec![]);
    type_text(&mut l, "code");
    assert_eq!(l.screen().sections[0].rows[0].matched, vec![14, 15, 16, 17]);
    type_text(&mut l, "");
    assert!(l.screen().sections[0].rows[0].matched.is_empty());
}

// @R-025
#[test]
fn r025_file_row_shows_its_folder_in_mono() {
    let (mut l, _) = engine(vec![], vec![]);
    type_text(&mut l, "notes");
    let file = Item {
        kind: ItemKind::File,
        subtitle: r"C:\Users\me\notes.txt".into(),
        ..item("file:n", "notes.txt")
    };
    assert!(l.file_results(l.generation(), 0, vec![file]));
    let s = l.screen();
    let row = &s.sections[0].rows[0];
    assert_eq!(row.subtitle, r"C:\Users\me");
    assert!(row.mono_subtitle);
    assert_eq!(row.matched, vec![0, 1, 2, 3, 4]);
}

// @R-025
#[test]
fn r025_a_clipboard_entry_shows_its_time_at_the_right() {
    let entry = Item {
        kind: ItemKind::ClipboardEntry,
        subtitle: "3 min ago".into(),
        ..item("clip:1", "text")
    };
    let (l, _) = engine(vec![entry], vec![]);
    let s = l.screen();
    assert_eq!(s.sections[0].rows[0].right.as_deref(), Some("3 min ago"));
}

// @R-028
#[test]
fn r028_pushed_view_has_name_and_back() {
    let (mut l, _) = engine(
        vec![item("push:clip", "Clipboard history")],
        clip_view(false),
    );
    assert_eq!(l.screen().view.name, "Launcher");
    assert!(!l.screen().view.marked);
    key(&mut l, Key::Enter);
    let view = l.screen().view;
    assert_eq!(view.name, "Clipboard");
    assert!(view.marked && view.back);
    assert!(l.handle(UiEvent::Back).is_empty());
    assert_eq!(l.screen().view.name, "Launcher");
    assert_eq!(titles(&l.screen()), vec!["Clipboard history"]);
}

// @R-028
#[test]
fn r028_a_view_opened_by_its_hotkey_is_named_without_a_back_control() {
    let (mut l, _) = engine(vec![], clip_view(true));
    assert!(l.show_view(&ViewId("clip".into())));
    let view = l.screen().view;
    assert_eq!(view.name, "Clipboard");
    assert!(view.marked && !view.back);
    l.handle(UiEvent::Back);
    assert_eq!(
        l.screen().view.name,
        "Clipboard",
        "Back does not apply here"
    );
}

/// A source whose actions come in the wrong order on purpose.
struct Menu;

impl CommandSource for Menu {
    fn items(&self, _q: &Query) -> Arc<[Item]> {
        vec![Item {
            primary: ActionId("open".into()),
            ..item("menu:1", "Report.txt")
        }]
        .into()
    }
    fn actions(&self, _id: &ItemId) -> Vec<Action> {
        vec![
            action("delete", "Delete"),
            action("pin", "Pin"),
            action("open", "Open"),
        ]
    }
    fn run(&self, _id: &ItemId, act: ActionId, _cx: &mut Cx) -> Outcome {
        Outcome::Effects(vec![Effect::Launch(LaunchTarget::ShellItem(act.0))])
    }
}

fn menu_engine() -> Launcher {
    let db = Db::open_in_memory().unwrap_or_else(|e| panic!("in-memory db: {e}"));
    let frecency =
        Frecency::open(db, T0_MS / 1000).unwrap_or_else(|e| panic!("frecency open: {e}"));
    Launcher::new(
        vec![Box::new(Menu)],
        vec![],
        frecency,
        Box::new(FakeClock(Rc::new(Cell::new(T0_MS)))),
    )
}

// @R-026
#[test]
fn r026_panel_has_title_groups_and_glyphs() {
    let mut l = menu_engine();
    ctrl(&mut l, Key::K);
    let panel = l.screen().action_panel.expect("panel open");
    assert_eq!(panel.title, "Report.txt");
    let groups: Vec<u8> = panel.actions.iter().map(|a| a.group).collect();
    assert_eq!(groups, [0, 1, 1, 2]);
    assert!(panel.actions.iter().all(|a| !a.glyph.is_empty()));
    assert_eq!(panel.actions[0].shortcut.as_deref(), Some("Enter"));
}

// @R-026
#[test]
fn r026_destructive_actions_come_last() {
    let mut l = menu_engine();
    ctrl(&mut l, Key::K);
    let panel = l.screen().action_panel.expect("panel open");
    let labels: Vec<_> = panel.actions.iter().map(|a| a.label.as_str()).collect();
    assert_eq!(labels, ["Open", "Pin", "Copy item ID", "Delete"]);
    assert_eq!(
        panel
            .actions
            .iter()
            .map(|a| a.destructive)
            .collect::<Vec<_>>(),
        [false, false, false, true]
    );
}

// @R-026
#[test]
fn r026_the_filter_narrows_the_actions_and_enter_runs_what_is_left() {
    let mut l = menu_engine();
    ctrl(&mut l, Key::K);
    assert!(l.handle(UiEvent::PanelFilter("PI".into())).is_empty());
    let panel = l.screen().action_panel.expect("panel open");
    assert_eq!(panel.filter, "PI");
    assert_eq!(panel.actions.len(), 1);
    let effects = key(&mut l, Key::Enter);
    assert!(effects.contains(&Effect::Launch(LaunchTarget::ShellItem("pin".into()))));
}

#[test]
fn inline_color_converts_notations_draws_swatch_and_copies_via_enter_and_panel() {
    use naobi_launcher::providers::color::ColorSource;

    let db = Db::open_in_memory().unwrap_or_else(|e| panic!("in-memory db: {e}"));
    let frecency =
        Frecency::open(db, T0_MS / 1000).unwrap_or_else(|e| panic!("frecency open: {e}"));
    let mut l = Launcher::new(
        vec![
            Box::new(Apps {
                items: vec![item("app:notepad", "Notepad")],
            }),
            Box::new(ColorSource),
        ],
        vec![],
        frecency,
        Box::new(FakeClock(Rc::new(Cell::new(T0_MS)))),
    );

    for (query, want_swatch, want_copy) in [
        ("#914A3B", 0x914A3B, "rgb(145, 74, 59)"),
        ("#f0a", 0xFF00AA, "rgb(255, 0, 170)"),
        ("rgb(145, 74, 59)", 0x914A3B, "#914A3B"),
        ("hsl(0, 100%, 50%)", 0xFF0000, "#FF0000"),
        ("oklch(49.3% 0.099 33.1)", 0x914A3B, "#914A3B"),
    ] {
        l.show_root();
        type_text(&mut l, query);
        let s = l.screen();
        let row = &s.sections[0].rows[0];
        assert_eq!(row.kind, "Color", "query {query}");
        assert_eq!(row.swatch, Some(want_swatch), "query {query}");
        assert_eq!(row.title, want_copy, "query {query}");
        assert!(row.mono_subtitle, "query {query}");
        assert_eq!(
            key(&mut l, Key::Enter),
            vec![Effect::CopyText(want_copy.into()), Effect::Hide],
            "enter for {query}"
        );
    }

    // Subtitle shows the other converted notations; Ctrl+K offers all four copy actions.
    l.show_root();
    type_text(&mut l, "rgb(145, 74, 59)");
    let row = &l.screen().sections[0].rows[0];
    assert!(row.subtitle.contains("hsl(10, 42%, 40%)"));
    assert!(row.subtitle.contains("oklch(49.3% 0.099 33.1)"));

    ctrl(&mut l, Key::K);
    let panel = l.screen().action_panel.expect("action panel open");
    let labels: Vec<_> = panel.actions.iter().map(|a| a.label.as_str()).collect();
    assert_eq!(
        labels,
        ["Copy as HEX", "Copy as RGB", "Copy as HSL", "Copy as OKLCH"]
    );
    key(&mut l, Key::Down);
    key(&mut l, Key::Down);
    key(&mut l, Key::Down);
    assert_eq!(
        key(&mut l, Key::Enter),
        vec![
            Effect::CopyText("oklch(49.3% 0.099 33.1)".into()),
            Effect::Hide
        ]
    );

    // Invalid inputs produce no Color row.
    for bad in [
        "#12345",
        "rgb(256, 0, 0)",
        "hsl(0, 110%, 50%)",
        "oklch(150% 0.1 0)",
        "notepad",
    ] {
        l.show_root();
        type_text(&mut l, bad);
        let has_color = l
            .screen()
            .sections
            .iter()
            .flat_map(|sec| &sec.rows)
            .any(|r| r.kind == "Color");
        assert!(!has_color, "spurious Color row for {bad:?}");
    }
}

#[test]
fn copy_item_id_custom_alias_and_inline_args() {
    use std::collections::BTreeMap;

    // 1. Copy item ID from Action panel copies the ItemId and hides.
    let (mut l, _) = engine(vec![item("app:wt.exe", "Windows Terminal")], vec![]);
    type_text(&mut l, "terminal");
    ctrl(&mut l, Key::K);
    let panel = l.screen().action_panel.expect("panel open");
    assert!(panel.actions.iter().any(|a| a.label == "Copy item ID"));
    let idx = panel
        .actions
        .iter()
        .position(|a| a.label == "Copy item ID")
        .unwrap();
    for _ in 0..idx {
        key(&mut l, Key::Down);
    }
    assert_eq!(
        key(&mut l, Key::Enter),
        vec![Effect::CopyText("app:wt.exe".into()), Effect::Hide]
    );

    // 2. Custom [aliases] ranks exact alias target first.
    let (mut l, _) = engine(
        vec![
            item("app:termit", "Termit"),
            item("app:wt.exe", "Windows Terminal"),
        ],
        vec![],
    );
    l.set_aliases(BTreeMap::from([("term".into(), ItemId("app:wt.exe".into()))]));
    type_text(&mut l, "term");
    assert_eq!(titles(&l.screen())[0], "Windows Terminal");

    // 3. Parameterized alias forwards trailing argument to dev micro-tool.
    struct ParamDev;
    impl CommandSource for ParamDev {
        fn items(&self, q: &Query) -> Arc<[Item]> {
            if let Some(arg) = q.text.strip_prefix("b64 ") {
                vec![item("dev:b64", &format!("b64:{arg}"))].into()
            } else {
                Vec::new().into()
            }
        }
        fn actions(&self, _id: &ItemId) -> Vec<Action> {
            vec![action("copy", "Copy")]
        }
        fn run(&self, id: &ItemId, _act: ActionId, cx: &mut Cx) -> Outcome {
            cx.copy_text(id.0.clone());
            Outcome::Done
        }
    }
    let mut l = Launcher::new(
        vec![Box::new(ParamDev)],
        vec![],
        Frecency::open(Db::open_in_memory().unwrap(), 0).unwrap(),
        Box::new(FakeClock(Rc::new(Cell::new(T0_MS)))),
    );
    l.set_aliases(BTreeMap::from([("enc".into(), ItemId("dev:b64".into()))]));
    type_text(&mut l, "enc hello world");
    assert_eq!(titles(&l.screen()), vec!["b64:hello world"]);
}
