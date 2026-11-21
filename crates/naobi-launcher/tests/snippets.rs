//! Snippets provider and view through `Launcher` with a fake `SnippetEnv`.
use naobi_core::db::Db;
use naobi_core::effect::Effect;
use naobi_core::ids::ViewId;
use naobi_core::snippet::Snippet;
use naobi_core::ui::{Key, KeyChord, PreviewView, Screen, UiEvent};
use naobi_launcher::frecency::Frecency;
use naobi_launcher::providers::snippets::{
    LocalDateTime, SnippetEnv, SnippetSet, SnippetSource, SnippetsCommands, SnippetsView, VIEW_ID,
};
use naobi_launcher::{Clock, Launcher};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

struct Zero;
impl Clock for Zero {
    fn now_ms(&self) -> i64 {
        1_700_000_000_000
    }
}

struct FakeEnv {
    clipboard: Mutex<Option<String>>,
    clipboard_calls: AtomicUsize,
    random_calls: AtomicUsize,
    now: LocalDateTime,
}

impl Default for FakeEnv {
    fn default() -> Self {
        Self {
            clipboard: Mutex::new(Some("https://naobi.dev".into())),
            clipboard_calls: AtomicUsize::new(0),
            random_calls: AtomicUsize::new(0),
            now: LocalDateTime {
                year: 2026,
                month: 11,
                day: 21,
                hour: 9,
                minute: 5,
                second: 3,
            },
        }
    }
}

impl SnippetEnv for FakeEnv {
    fn clipboard_text(&self) -> Option<String> {
        self.clipboard_calls.fetch_add(1, Ordering::SeqCst);
        self.clipboard
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    fn random16(&self) -> Option<[u8; 16]> {
        let n = self.random_calls.fetch_add(1, Ordering::SeqCst) as u8;
        Some([
            0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd, 0xee,
            0xff, n,
        ])
    }

    fn local_now(&self) -> LocalDateTime {
        self.now
    }
}

fn rig(snippets: Vec<Snippet>) -> (Launcher, SnippetSet, Arc<FakeEnv>) {
    let set = SnippetSet::default();
    set.set(snippets);
    let env = Arc::new(FakeEnv::default());
    let db = Db::open_in_memory().unwrap_or_else(|e| panic!("db: {e}"));
    let frecency = Frecency::open(db, 0).unwrap_or_else(|e| panic!("frecency: {e}"));
    let launcher = Launcher::new(
        vec![
            Box::new(SnippetsCommands),
            Box::new(SnippetSource::new(set.clone(), env.clone())),
        ],
        vec![Box::new(SnippetsView::new(set.clone(), env.clone()))],
        frecency,
        Box::new(Zero),
    );
    (launcher, set, env)
}

fn key(l: &mut Launcher, key: Key, ctrl: bool) -> Vec<Effect> {
    l.handle(UiEvent::Key(KeyChord {
        key,
        ctrl,
        shift: false,
        alt: false,
    }))
}

fn titles(s: &Screen) -> Vec<String> {
    s.sections
        .iter()
        .flat_map(|sec| sec.rows.iter().map(|r| r.title.clone()))
        .collect()
}

#[test]
fn root_search_finds_snippets_by_name_or_exact_alias_and_emits_paste_or_copy() {
    let (mut l, _, env) = rig(vec![
        Snippet {
            name: "Email Signature".into(),
            alias: Some("sig".into()),
            text: "Best,\nRaihan".into(),
        },
        Snippet {
            name: "Standup Template".into(),
            alias: Some("st".into()),
            text: "Date: {date} {time}\nID: {uuid}\nRef: {clipboard}\nLiteral: {other}".into(),
        },
    ]);

    // Exact alias ranks first in Root search.
    l.handle(UiEvent::TextChanged("sig".into()));
    let s = l.screen();
    assert_eq!(titles(&s)[0], "Email Signature");
    assert_eq!(s.sections[0].rows[0].kind, "Snippet");
    assert_eq!(s.footer.primary.as_deref(), Some("Paste"));
    assert_eq!(s.footer.secondary.as_deref(), Some("Copy"));

    // Enter emits PasteText + Hide without reading clipboard when {clipboard} is absent.
    assert_eq!(
        key(&mut l, Key::Enter, false),
        vec![Effect::PasteText("Best,\nRaihan".into()), Effect::Hide]
    );
    assert_eq!(env.clipboard_calls.load(Ordering::SeqCst), 0);

    // Search by name and expand all placeholders at activation time.
    l.show_root();
    l.handle(UiEvent::TextChanged("standup".into()));
    assert_eq!(titles(&l.screen())[0], "Standup Template");
    assert_eq!(
        key(&mut l, Key::Enter, false),
        vec![
            Effect::PasteText(
                "Date: 2026-11-21 09:05:03\nID: 11223344-5566-4788-99aa-bbccddeeff00\nRef: https://naobi.dev\nLiteral: {other}"
                    .into()
            ),
            Effect::Hide
        ]
    );
    assert_eq!(env.clipboard_calls.load(Ordering::SeqCst), 1);

    // Ctrl+Enter copies expanded text; Action panel exposes Paste and Copy.
    l.show_root();
    l.handle(UiEvent::TextChanged("sig".into()));
    key(&mut l, Key::K, true);
    let panel = l.screen().action_panel.expect("panel");
    let labels: Vec<_> = panel.actions.iter().map(|a| a.label.as_str()).collect();
    assert_eq!(labels, ["Paste", "Copy"]);
    key(&mut l, Key::Escape, false);
    assert_eq!(
        key(&mut l, Key::Enter, true),
        vec![Effect::CopyText("Best,\nRaihan".into()), Effect::Hide]
    );
}

#[test]
fn clipboard_placeholder_does_not_reexpand_placeholders_inside_clipboard_and_handles_empty() {
    let (mut l, _, env) = rig(vec![Snippet {
        name: "Wrap".into(),
        alias: Some("wrap".into()),
        text: "[{clipboard}]".into(),
    }]);

    // Clipboard text containing "{date}" must stay literal.
    *env.clipboard.lock().unwrap() = Some("literal {date} {uuid}".into());
    l.handle(UiEvent::TextChanged("wrap".into()));
    assert_eq!(
        key(&mut l, Key::Enter, false),
        vec![
            Effect::PasteText("[literal {date} {uuid}]".into()),
            Effect::Hide
        ]
    );

    // Empty/non-text clipboard expands {clipboard} to empty string cleanly.
    *env.clipboard.lock().unwrap() = None;
    l.show_root();
    l.handle(UiEvent::TextChanged("wrap".into()));
    assert_eq!(
        key(&mut l, Key::Enter, false),
        vec![Effect::PasteText("[]".into()), Effect::Hide]
    );
}

#[test]
fn snippets_view_opens_previews_filters_and_pops_or_hides_on_esc() {
    let (mut l, set, _) = rig(vec![
        Snippet {
            name: "Alpha".into(),
            alias: Some("a".into()),
            text: "First snippet".into(),
        },
        Snippet {
            name: "Beta".into(),
            alias: None,
            text: "Second body".into(),
        },
    ]);

    // Open via "Snippets" command in Root search.
    l.handle(UiEvent::TextChanged("snippets".into()));
    assert_eq!(titles(&l.screen())[0], "Snippets");
    assert_eq!(
        key(&mut l, Key::Enter, false),
        vec![Effect::PushView(ViewId(VIEW_ID.into()))]
    );

    let s = l.screen();
    assert_eq!(s.view.name, "Snippets");
    assert!(s.view.marked && s.view.back);
    assert_eq!(titles(&s), vec!["Alpha", "Beta"]);
    assert_eq!(s.preview, Some(PreviewView::Text("First snippet".into())));

    // Filter by body or alias.
    l.handle(UiEvent::TextChanged("body".into()));
    assert_eq!(titles(&l.screen()), vec!["Beta"]);
    assert_eq!(
        key(&mut l, Key::Enter, false),
        vec![Effect::PasteText("Second body".into()), Effect::Hide]
    );

    // Hot-reload updates live SnippetSet.
    set.set(vec![Snippet {
        name: "Gamma".into(),
        alias: Some("g".into()),
        text: "Updated".into(),
    }]);
    assert!(l.show_view(&ViewId(VIEW_ID.into())));
    assert!(l.screen().view.marked && !l.screen().view.back);
    assert_eq!(titles(&l.screen()), vec!["Gamma"]);
    assert_eq!(key(&mut l, Key::Escape, false), vec![Effect::Hide]);
}
