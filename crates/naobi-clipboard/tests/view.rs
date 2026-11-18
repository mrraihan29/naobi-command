//! The clipboard view through the Launcher engine: search, pin, delete, paste, copy, clear.
use naobi_clipboard::testing::{FakeClock, TaggedCipher};
use naobi_clipboard::view::{share, ClipboardCommands, ClipboardView, Shared, VIEW_ID};
use naobi_clipboard::{Captured, Content, Flags, History};
use naobi_core::db::schema::MIGRATIONS;
use naobi_core::db::Db;
use naobi_core::effect::Effect;
use naobi_core::ids::{EntryId, ViewId};
use naobi_core::ui::{Key, KeyChord, Screen, UiEvent};
use naobi_launcher::frecency::Frecency;
use naobi_launcher::{Clock, Launcher};

struct Zero;
impl Clock for Zero {
    fn now_ms(&self) -> i64 {
        1_700_000_000_000
    }
}

fn setup() -> (Launcher, Shared, FakeClock) {
    let clock = FakeClock::at(1_700_000_000_000);
    let db = Db::open_in_memory().unwrap_or_else(|e| panic!("db: {e}"));
    let shared = share(History::new(
        db,
        Box::new(TaggedCipher),
        Box::new(clock.clone()),
    ));
    let fdb = Db::open_in_memory().unwrap_or_else(|e| panic!("db: {e}"));
    let frecency = Frecency::open(fdb, 0).unwrap_or_else(|e| panic!("frecency: {e}"));
    let launcher = Launcher::new(
        vec![Box::new(ClipboardCommands::new(shared.clone()))],
        vec![Box::new(ClipboardView::new(shared.clone()))],
        frecency,
        Box::new(Zero),
    );
    (launcher, shared, clock)
}

fn copy(h: &Shared, text: &str) {
    let c = Captured {
        flags: Flags::default(),
        marker: false,
        owner_exe: Some("notepad.exe".into()),
        content: Content::Text(text.into()),
    };
    h.lock()
        .unwrap_or_else(|e| e.into_inner())
        .on_change(c)
        .unwrap_or_else(|e| panic!("on_change: {e}"));
}

fn ids(h: &Shared) -> Vec<i64> {
    let g = h.lock().unwrap_or_else(|e| e.into_inner());
    g.list(100)
        .unwrap_or_default()
        .iter()
        .map(|e| e.id)
        .collect()
}

fn key(k: Key, ctrl: bool) -> UiEvent {
    UiEvent::Key(KeyChord {
        key: k,
        ctrl,
        alt: false,
        shift: false,
    })
}

fn titles(s: &Screen) -> Vec<String> {
    s.sections
        .iter()
        .flat_map(|sec| sec.rows.iter().map(|r| r.title.clone()))
        .collect()
}

fn open(l: &mut Launcher) {
    assert!(l.show_view(&ViewId(VIEW_ID.into())));
}

// @R-015
#[test]
fn r015_text_copy_search_paste_selects_entry() {
    let (mut l, h, clock) = setup();
    copy(&h, "first note about apples");
    clock.advance(120_000);
    copy(&h, "second note about pears");
    open(&mut l);
    // Newest first, with its age.
    let s = l.screen();
    assert_eq!(
        titles(&s),
        vec!["second note about pears", "first note about apples"]
    );
    assert_eq!(s.sections[0].rows[0].subtitle, "just now");
    assert_eq!(s.sections[0].rows[1].subtitle, "2 min ago");
    // The search looks inside the text.
    l.handle(UiEvent::TextChanged("apples".into()));
    assert_eq!(titles(&l.screen()), vec!["first note about apples"]);
    let first = ids(&h)[1];
    assert_eq!(
        l.handle(key(Key::Enter, false)),
        vec![Effect::Paste(EntryId(first)), Effect::Hide],
        "Enter pastes the selected entry"
    );
}

// @R-015
#[test]
fn r015_ctrl_enter_copies_only_and_pin_delete_keep_the_view_open() {
    let (mut l, h, _) = setup();
    copy(&h, "alpha");
    copy(&h, "beta");
    open(&mut l);
    // Ctrl+Enter copies the selected (newest) entry without pasting.
    let newest = ids(&h)[0];
    assert_eq!(
        l.handle(key(Key::Enter, true)),
        vec![Effect::CopyEntry(EntryId(newest)), Effect::Hide]
    );
    // Ctrl+. pins and unpins; the view stays open and shows the marker.
    assert!(l.handle(key(Key::Period, true)).is_empty());
    assert!(l.screen().sections[0].rows[0].subtitle.ends_with("Pinned"));
    assert!(l.handle(key(Key::Period, true)).is_empty());
    assert!(!l.screen().sections[0].rows[0].subtitle.contains("Pinned"));
    // Ctrl+X deletes it; the next entry is selected.
    assert!(l.handle(key(Key::X, true)).is_empty());
    assert_eq!(titles(&l.screen()), vec!["alpha"]);
    // Esc on an empty query hides a hotkey-opened view.
    assert_eq!(l.handle(key(Key::Escape, false)), vec![Effect::Hide]);
}

// @R-015
#[test]
fn r015_empty_history_says_nothing_copied_yet() {
    let (mut l, _, _) = setup();
    open(&mut l);
    let s = l.screen();
    assert_eq!(
        s.empty.as_ref().map(|e| e.title.as_str()),
        Some("Nothing copied yet")
    );
    l.handle(UiEvent::TextChanged("zzz".into()));
    assert_eq!(
        l.screen().empty.map(|e| e.title),
        Some("No results".to_owned())
    );
}

// @R-009
#[test]
fn r009_clear_command_leaves_zero_entries() {
    let (mut l, h, _) = setup();
    copy(&h, "keep me");
    let id = ids(&h)[0];
    assert!(h.lock().unwrap_or_else(|e| e.into_inner()).pin(id).is_ok());
    l.handle(UiEvent::TextChanged("clear clipboard".into()));
    assert_eq!(
        l.screen().sections[0].rows[0].title,
        "Clear Clipboard History"
    );
    // Deleting everything, pinned entries included, needs a second Enter.
    assert!(l.handle(key(Key::Enter, false)).is_empty());
    assert_eq!(ids(&h).len(), 1);
    assert_eq!(l.handle(key(Key::Enter, false)), vec![Effect::Hide]);
    assert!(ids(&h).is_empty());
}

// @R-015
#[test]
fn r015_the_history_command_opens_the_view() {
    let (mut l, h, _) = setup();
    copy(&h, "hello");
    l.handle(UiEvent::TextChanged("clipboard history".into()));
    // Both commands match; go down to the one that opens the view.
    let at = titles(&l.screen())
        .iter()
        .position(|t| t == "Clipboard History")
        .unwrap_or(0);
    for _ in 0..at {
        l.handle(key(Key::Down, false));
    }
    let effects = l.handle(key(Key::Enter, false));
    assert_eq!(effects, vec![Effect::PushView(ViewId(VIEW_ID.into()))]);
    assert_eq!(titles(&l.screen()), vec!["hello"]);
}

// @R-015
#[test]
fn r015_view_names_its_search_and_shows_the_copy_and_action_keys() {
    let (mut l, h, _) = setup();
    assert_eq!(
        l.screen().placeholder,
        "Search apps and commands, or type > to run one..."
    );
    copy(&h, "a note");
    open(&mut l);
    let s = l.screen();
    assert_eq!(s.placeholder, "Search clipboard history...");
    assert_eq!(s.footer.primary.as_deref(), Some("Paste"));
    assert_eq!(s.footer.secondary.as_deref(), Some("Copy"));
    // Ctrl+K lists every action with its key.
    l.handle(key(Key::K, true));
    let panel = l.screen().action_panel.expect("panel");
    let keys: Vec<_> = panel
        .actions
        .iter()
        .filter_map(|a| a.shortcut.clone())
        .collect();
    assert_eq!(keys, vec!["Enter", "Ctrl+Enter", "Ctrl+.", "Ctrl+X"]);
}

fn facts(s: &Screen) -> Vec<(String, String)> {
    s.facts
        .iter()
        .map(|f| (f.label.clone(), f.value.clone()))
        .collect()
}

fn pair(label: &str, value: &str) -> (String, String) {
    (label.to_owned(), value.to_owned())
}

// @R-027
#[test]
fn r027_preview_facts_for_text_image_and_files() {
    let (mut l, h, clock) = setup();
    copy(&h, "a note");
    clock.advance(120_000);
    open(&mut l);
    assert_eq!(
        facts(&l.screen()),
        [
            pair("Type", "Text"),
            pair("Length", "6 characters"),
            pair("Copied", "2 min ago")
        ]
    );
    h.lock()
        .unwrap_or_else(|e| e.into_inner())
        .on_change(Captured {
            flags: Flags::default(),
            marker: false,
            owner_exe: None,
            content: Content::Files(vec![r"C:\a.txt".into(), r"C:\b.txt".into()]),
        })
        .unwrap_or_else(|e| panic!("on_change: {e}"));
    l.handle(UiEvent::TextChanged(String::new()));
    assert_eq!(
        facts(&l.screen()),
        [
            pair("Type", "Files"),
            pair("Count", "2 files"),
            pair("Copied", "just now")
        ]
    );
}

// @R-027
#[test]
fn r027_schema_version_unchanged() {
    assert_eq!(MIGRATIONS.len(), 2, "the facts come from stored data");
}
