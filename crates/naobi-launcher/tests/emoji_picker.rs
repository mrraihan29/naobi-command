//! Emoji & Symbol picker through `Launcher`: `:` Root trigger, `emoji` View, Enter paste and Ctrl+Enter copy.
use naobi_core::db::Db;
use naobi_core::effect::Effect;
use naobi_core::ids::ViewId;
use naobi_core::ui::{Key, KeyChord, Screen, UiEvent};
use naobi_launcher::frecency::Frecency;
use naobi_launcher::providers::emoji::{EmojiSource, EmojiView, VIEW_ID};
use naobi_launcher::{Clock, Launcher};

struct Zero;
impl Clock for Zero {
    fn now_ms(&self) -> i64 {
        1_000_000_000_000
    }
}

fn rig() -> Launcher {
    let db = Db::open_in_memory().unwrap_or_else(|e| panic!("db: {e}"));
    let frecency = Frecency::open(db, 0).unwrap_or_else(|e| panic!("frecency: {e}"));
    Launcher::new(
        vec![Box::new(EmojiSource)],
        vec![Box::new(EmojiView)],
        frecency,
        Box::new(Zero),
    )
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
fn root_colon_trigger_searches_emojis_and_symbols_with_enter_paste_and_ctrl_enter_copy() {
    let mut l = rig();

    // `:` alone shows common emojis/symbols and claims the root query (no file query).
    l.handle(UiEvent::TextChanged(":".into()));
    assert!(l.file_query().is_none());
    let all = titles(&l.screen());
    assert!(!all.is_empty());

    // `:fire` finds Fire first; Enter pastes via PasteText + Hide.
    l.handle(UiEvent::TextChanged(":fire".into()));
    let s = l.screen();
    assert_eq!(s.sections[0].rows[0].title, "🔥 Fire");
    assert_eq!(s.footer.primary.as_deref(), Some("Paste"));
    assert_eq!(s.footer.secondary.as_deref(), Some("Copy"));
    assert_eq!(
        key(&mut l, Key::Enter, false),
        vec![Effect::PasteText("🔥".into()), Effect::Hide]
    );

    // `:arrow` finds arrow symbols; Ctrl+Enter copies via CopyText + Hide.
    l.handle(UiEvent::TextChanged(":arrow".into()));
    let arrow_titles = titles(&l.screen());
    assert!(
        arrow_titles.iter().any(|t| t.contains('→')),
        "{arrow_titles:?}"
    );
    assert_eq!(
        key(&mut l, Key::Enter, true),
        vec![Effect::CopyText("→".into()), Effect::Hide]
    );

    // Non-matching `:query` shows empty state cleanly without run-history prompt.
    l.handle(UiEvent::TextChanged(":zzzznotfound".into()));
    let s = l.screen();
    assert!(s.sections.is_empty());
    let empty = s.empty.expect("empty view");
    assert_ne!(empty.title, "No command lines run yet");
}

#[test]
fn emoji_view_navigation_esc_behavior_and_symbol_search() {
    let mut l = rig();

    // Root command pushes the `emoji` view.
    l.handle(UiEvent::TextChanged("emoji".into()));
    assert_eq!(titles(&l.screen()), vec!["Emoji & Symbols"]);
    assert_eq!(
        key(&mut l, Key::Enter, false),
        vec![Effect::PushView(ViewId(VIEW_ID.into()))]
    );
    let s = l.screen();
    assert_eq!(s.view.name, "Emoji & Symbols");
    assert!(s.view.marked && s.view.back);
    assert!(!s.sections[0].rows.is_empty());

    // Esc on empty query pops back to Root when pushed.
    assert!(key(&mut l, Key::Escape, false).is_empty());
    assert_eq!(l.screen().view.name, "Launcher");

    // Push again, search inside view: first Esc clears query, second Esc pops to Root.
    l.handle(UiEvent::TextChanged("emoji".into()));
    key(&mut l, Key::Enter, false);
    l.handle(UiEvent::TextChanged("copyright".into()));
    assert_eq!(titles(&l.screen()), vec!["© Copyright Sign"]);
    assert!(key(&mut l, Key::Escape, false).is_empty());
    assert_eq!(l.screen().query, "");
    assert_eq!(l.screen().view.name, "Emoji & Symbols");
    assert!(key(&mut l, Key::Escape, false).is_empty());
    assert_eq!(l.screen().view.name, "Launcher");

    // Opened via hotkey: Esc with non-empty query clears query, Esc on empty hides.
    assert!(l.show_view(&ViewId(VIEW_ID.into())));
    assert!(l.screen().view.marked && !l.screen().view.back);
    l.handle(UiEvent::TextChanged("degree".into()));
    assert_eq!(titles(&l.screen()), vec!["° Degree Sign"]);
    // Action panel lists Paste and Copy.
    key(&mut l, Key::K, true);
    let labels: Vec<_> = l
        .screen()
        .action_panel
        .expect("action panel")
        .actions
        .into_iter()
        .map(|a| a.label)
        .collect();
    assert_eq!(labels, ["Paste", "Copy"]);
    // Esc closes panel, Esc clears query, Esc hides hotkey-opened view.
    assert!(key(&mut l, Key::Escape, false).is_empty());
    assert!(l.screen().action_panel.is_none());
    assert!(key(&mut l, Key::Escape, false).is_empty());
    assert_eq!(l.screen().query, "");
    assert_eq!(key(&mut l, Key::Escape, false), vec![Effect::Hide]);

    // Multi-word and math symbol queries work in `emoji` view too.
    assert!(l.show_view(&ViewId(VIEW_ID.into())));
    l.handle(UiEvent::TextChanged("thumbs up".into()));
    assert_eq!(titles(&l.screen()), vec!["👍 Thumbs Up"]);
    assert_eq!(
        key(&mut l, Key::Enter, false),
        vec![Effect::PasteText("👍".into()), Effect::Hide]
    );

    assert!(l.show_view(&ViewId(VIEW_ID.into())));
    l.handle(UiEvent::TextChanged("math".into()));
    assert!(titles(&l.screen()).iter().any(|t| t.contains('∞')));
    l.handle(UiEvent::TextChanged("infinity".into()));
    assert_eq!(
        key(&mut l, Key::Enter, true),
        vec![Effect::CopyText("∞".into()), Effect::Hide]
    );
}
