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
    assert_eq!(s.view.name, "Clipboard");
    assert_eq!(s.footer.primary.as_deref(), Some("Paste"));
    assert_eq!(s.footer.secondary.as_deref(), Some("Copy"));
    // Ctrl+K lists every action with its key.
    l.handle(key(Key::K, true));
    let panel = l.screen().action_panel.unwrap_or_else(|| panic!("panel"));
    let keys: Vec<_> = panel
        .actions
        .iter()
        .filter_map(|a| a.shortcut.clone())
        .collect();
    assert_eq!(
        keys,
        vec![
            "Enter",
            "Ctrl+Enter",
            "Ctrl+Shift+Enter",
            "Ctrl+.",
            "Ctrl+X"
        ]
    );
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
fn r027_preview_alone_matches_the_pane_preview() {
    use naobi_core::ids::ItemId;
    use naobi_core::view_source::ViewSource;
    let (_, h, _) = setup();
    copy(&h, "a note");
    let view = ClipboardView::new(h.clone());
    let id = ItemId(format!("clip:{}", ids(&h)[0]));
    let pane = view.pane(&id).map(|(p, _)| p);
    assert!(pane.is_some());
    assert_eq!(view.preview(&id), pane);
}

// @R-027
#[test]
fn r027_schema_version_unchanged() {
    assert!(
        MIGRATIONS
            .iter()
            .skip(1)
            .all(|m| !m.contains("clip_entry") && !m.contains("clip_payload")),
        "the clipboard tables and their facts come only from V1 stored data"
    );
}

fn ctrl_shift_enter() -> UiEvent {
    UiEvent::Key(KeyChord {
        key: Key::Enter,
        ctrl: true,
        alt: false,
        shift: true,
    })
}

// @R-015
#[test]
fn r015_ctrl_shift_enter_pastes_text_entry_and_skips_non_text_cleanly() {
    let (mut l, h, _) = setup();
    copy(&h, "plain text note");
    let text_id = ids(&h)[0];
    open(&mut l);
    assert_eq!(
        l.handle(ctrl_shift_enter()),
        vec![Effect::Paste(EntryId(text_id)), Effect::Hide],
        "Ctrl+Shift+Enter on text entry emits Paste and Hide"
    );

    h.lock()
        .unwrap_or_else(|e| e.into_inner())
        .on_change(Captured {
            flags: Flags::default(),
            marker: false,
            owner_exe: None,
            content: Content::Files(vec![r"C:\a.txt".into()]),
        })
        .unwrap_or_else(|e| panic!("on_change: {e}"));
    open(&mut l);
    assert!(
        l.handle(ctrl_shift_enter()).is_empty(),
        "Ctrl+Shift+Enter on non-text entry behaves cleanly without pasting or crashing"
    );

    l.handle(key(Key::K, true));
    let panel = l.screen().action_panel.unwrap_or_else(|| panic!("panel"));
    let plain = panel
        .actions
        .iter()
        .find(|a| a.label == "Paste as Plain Text")
        .unwrap_or_else(|| panic!("Paste as Plain Text action"));
    assert!(plain.reason.is_some(), "disabled on non-text entry");
}

fn run_panel_action(l: &mut Launcher, label: &str) -> Vec<Effect> {
    l.handle(key(Key::K, true));
    l.handle(UiEvent::PanelFilter(label.into()));
    let Some(panel) = l.screen().action_panel else {
        panic!("panel open");
    };
    assert_eq!(
        panel.actions.first().map(|a| a.label.as_str()),
        Some(label),
        "expected action {label:?} in panel"
    );
    l.handle(key(Key::Enter, false))
}

// @R-015
#[test]
fn r015_paste_queue_actions_add_paste_next_and_clear_in_order() {
    let (mut l, h, _) = setup();
    copy(&h, "first queued");
    copy(&h, "second queued");
    // List is newest-first: [second, first].
    let list_ids = ids(&h);
    let second_id = list_ids[0];
    let first_id = list_ids[1];
    open(&mut l);

    // Before queuing, the panel exposes all three queue actions (next/clear disabled).
    l.handle(key(Key::K, true));
    let labels: Vec<String> = l
        .screen()
        .action_panel
        .unwrap_or_else(|| panic!("panel"))
        .actions
        .iter()
        .map(|a| a.label.clone())
        .collect();
    assert!(labels.contains(&"Add to paste queue".to_owned()));
    assert!(labels.contains(&"Paste next in queue".to_owned()));
    assert!(labels.contains(&"Clear paste queue".to_owned()));
    l.handle(key(Key::Escape, false));

    // Queue "first queued" (row 1), then "second queued" (row 0).
    l.handle(key(Key::Down, false));
    assert!(run_panel_action(&mut l, "Add to paste queue").is_empty());
    l.handle(key(Key::Up, false));
    assert!(run_panel_action(&mut l, "Add to paste queue").is_empty());

    let subs: Vec<String> = l.screen().sections[0]
        .rows
        .iter()
        .map(|r| r.subtitle.clone())
        .collect();
    assert!(subs[1].ends_with("Queue #1"), "{}", subs[1]);
    assert!(subs[0].ends_with("Queue #2"), "{}", subs[0]);

    // Paste next removes first_id and emits Paste(first_id) + Hide.
    assert_eq!(
        run_panel_action(&mut l, "Paste next in queue"),
        vec![Effect::Paste(EntryId(first_id)), Effect::Hide]
    );

    // Re-open and verify second_id is now Queue #1 and pastes next.
    open(&mut l);
    assert!(l.screen().sections[0].rows[0]
        .subtitle
        .ends_with("Queue #1"));
    assert_eq!(
        run_panel_action(&mut l, "Paste next in queue"),
        vec![Effect::Paste(EntryId(second_id)), Effect::Hide]
    );

    // Queue again and clear.
    open(&mut l);
    assert!(run_panel_action(&mut l, "Add to paste queue").is_empty());
    assert!(run_panel_action(&mut l, "Clear paste queue").is_empty());
    assert!(!l.screen().sections[0].rows[0].subtitle.contains("Queue"));
    l.handle(key(Key::K, true));
    let next_action = l
        .screen()
        .action_panel
        .unwrap_or_else(|| panic!("panel"))
        .actions
        .into_iter()
        .find(|a| a.label == "Paste next in queue")
        .unwrap_or_else(|| panic!("Paste next in queue"));
    assert!(next_action.reason.is_some(), "disabled after clear");
}

#[test]
fn save_as_snippet_persists_text_entry_notifies_and_disables_non_text() {
    use naobi_core::notice::NoticeKind;
    use naobi_core::snippet::{SnippetsFile, FILE_NAME};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    let dir = std::env::temp_dir().join(format!("naobi-clip-snip-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("dir");

    let clock = FakeClock::at(1_700_000_000_000);
    let db = Db::open_in_memory().unwrap_or_else(|e| panic!("db: {e}"));
    let shared = share(History::new(
        db,
        Box::new(TaggedCipher),
        Box::new(clock.clone()),
    ));
    let saved_count = Arc::new(AtomicUsize::new(0));
    let saved_clone = saved_count.clone();
    let view = ClipboardView::new(shared.clone())
        .with_snippets_dir(dir.clone())
        .with_on_saved(Arc::new(move || {
            saved_clone.fetch_add(1, Ordering::SeqCst);
        }));

    let fdb = Db::open_in_memory().unwrap_or_else(|e| panic!("db: {e}"));
    let frecency = Frecency::open(fdb, 0).unwrap_or_else(|e| panic!("frecency: {e}"));
    let mut l = Launcher::new(
        vec![Box::new(ClipboardCommands::new(shared.clone()))],
        vec![Box::new(view)],
        frecency,
        Box::new(Zero),
    );

    copy(&shared, "SELECT * FROM users;\n-- {date}");
    open(&mut l);

    let effects = run_panel_action(&mut l, "Save as Snippet");
    assert!(
        matches!(
            effects.as_slice(),
            [Effect::Notice(n)] if n.kind == NoticeKind::Info && n.subject == "Snippets"
        ),
        "{effects:?}"
    );
    assert_eq!(saved_count.load(Ordering::SeqCst), 1);

    let toml_path = dir.join(FILE_NAME);
    let written = std::fs::read_to_string(&toml_path).expect("snippets.toml created");
    let parsed = SnippetsFile::parse(&written).expect("valid snippets.toml");
    assert_eq!(parsed.snippets.len(), 1);
    assert_eq!(parsed.snippets[0].name, "SELECT * FROM users;");
    assert_eq!(parsed.snippets[0].text, "SELECT * FROM users;\n-- {date}");

    // Broken snippets.toml raises an Error notice and is not overwritten.
    std::fs::write(&toml_path, "[[snippets]\nbad").expect("break");
    let err_effects = run_panel_action(&mut l, "Save as Snippet");
    assert!(
        matches!(
            err_effects.as_slice(),
            [Effect::Notice(n)] if n.kind == NoticeKind::Error && n.subject == "Snippets"
        ),
        "{err_effects:?}"
    );
    assert_eq!(saved_count.load(Ordering::SeqCst), 1);
    assert_eq!(
        std::fs::read_to_string(&toml_path).expect("kept"),
        "[[snippets]\nbad"
    );

    // Non-text entry disables "Save as Snippet".
    shared
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .on_change(Captured {
            flags: Flags::default(),
            marker: false,
            owner_exe: None,
            content: Content::Files(vec![r"C:\a.txt".into()]),
        })
        .unwrap_or_else(|e| panic!("on_change: {e}"));
    open(&mut l);
    l.handle(key(Key::K, true));
    let panel = l.screen().action_panel.unwrap_or_else(|| panic!("panel"));
    let save_action = panel
        .actions
        .iter()
        .find(|a| a.label == "Save as Snippet")
        .unwrap_or_else(|| panic!("Save as Snippet action"));
    assert!(save_action.reason.is_some(), "disabled on non-text entry");

    let _ = std::fs::remove_dir_all(&dir);
}
