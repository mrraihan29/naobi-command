use naobi_core::db::Db;
use naobi_core::effect::Effect;
use naobi_core::ids::ViewId;
use naobi_core::ui::{Key, KeyChord, UiEvent};
use naobi_launcher::armed::ARM_MS;
use naobi_launcher::frecency::Frecency;
use naobi_launcher::providers::scratchpad::{
    FileScratchpadStore, ScratchpadCommands, ScratchpadStore, ScratchpadView, VIEW_ID,
};
use naobi_launcher::{Clock, Launcher};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

struct FakeClock(Arc<AtomicI64>);

impl Clock for FakeClock {
    fn now_ms(&self) -> i64 {
        self.0.load(Ordering::SeqCst)
    }
}

#[derive(Default)]
struct SpyStore {
    content: Mutex<String>,
    saves: Mutex<Vec<String>>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl SpyStore {
    fn with_text(text: &str) -> Arc<Self> {
        Arc::new(Self {
            content: Mutex::new(text.to_owned()),
            saves: Mutex::new(Vec::new()),
        })
    }

    fn saved(&self) -> Vec<String> {
        lock(&self.saves).clone()
    }

    fn text(&self) -> String {
        lock(&self.content).clone()
    }
}

impl ScratchpadStore for SpyStore {
    fn load(&self) -> std::io::Result<String> {
        Ok(lock(&self.content).clone())
    }

    fn save(&self, content: &str) -> std::io::Result<()> {
        *lock(&self.content) = content.to_owned();
        lock(&self.saves).push(content.to_owned());
        Ok(())
    }
}

fn rig(initial: &str) -> (Launcher, Arc<SpyStore>, Arc<AtomicI64>) {
    let store = SpyStore::with_text(initial);
    let clock = Arc::new(AtomicI64::new(10_000));
    let db = Db::open_in_memory().unwrap_or_else(|e| panic!("db: {e}"));
    let frecency = Frecency::open(db, 0).unwrap_or_else(|e| panic!("frecency: {e}"));
    let launcher = Launcher::new(
        vec![Box::new(ScratchpadCommands)],
        vec![Box::new(ScratchpadView::new(store.clone()))],
        frecency,
        Box::new(FakeClock(clock.clone())),
    );
    (launcher, store, clock)
}

fn key(l: &mut Launcher, key: Key, ctrl: bool) -> Vec<Effect> {
    l.handle(UiEvent::Key(KeyChord {
        key,
        ctrl,
        alt: false,
        shift: false,
    }))
}

#[test]
fn opening_scratchpad_pushes_multiline_editor_view_and_loads_existing_notes() {
    let (mut l, _, _) = rig("# Heading\n- task");
    l.handle(UiEvent::TextChanged("scratchpad".into()));

    let effects = key(&mut l, Key::Enter, false);
    assert_eq!(effects, vec![Effect::PushView(ViewId(VIEW_ID.into()))]);

    let s = l.screen();
    assert_eq!(s.editor.as_deref(), Some("# Heading\n- task"));
    assert!(s.sections.is_empty());
    assert!(s.empty.is_none());
    assert!(s.view.marked);
    assert!(s.view.back, "pushed view shows back control");
    assert_eq!(s.view.name, "Scratchpad");
    assert_eq!(s.footer.secondary.as_deref(), Some("Copy all"));
    assert!(s.footer.actions);
}

#[test]
fn edits_debounce_autosave_and_stale_generation_does_not_flush() {
    let (mut l, store, _) = rig("");
    l.handle(UiEvent::TextChanged("scratchpad".into()));
    key(&mut l, Key::Enter, false);

    l.handle(UiEvent::ScratchpadChanged("first".into()));
    let Some(gen1) = l.take_pending_scratchpad_save() else {
        panic!("expected debounced save generation");
    };

    l.handle(UiEvent::ScratchpadChanged("first\nsecond".into()));
    let Some(gen2) = l.take_pending_scratchpad_save() else {
        panic!("expected newer debounced save generation");
    };
    assert_ne!(gen1, gen2);

    // Stale generation must not flush early.
    assert!(l.flush_scratchpad_if_gen(gen1).is_empty());
    assert!(store.saved().is_empty());

    // Current generation persists to disk.
    assert!(l.flush_scratchpad_if_gen(gen2).is_empty());
    assert_eq!(store.saved(), vec!["first\nsecond".to_owned()]);
}

#[test]
fn esc_saves_immediately_and_follows_pushed_vs_hotkey_navigation() {
    // 1) Pushed from root: Esc saves immediately and pops to root.
    let (mut l, store, _) = rig("");
    l.handle(UiEvent::TextChanged("scratchpad".into()));
    key(&mut l, Key::Enter, false);
    l.handle(UiEvent::ScratchpadChanged("pushed note".into()));

    assert_eq!(key(&mut l, Key::Escape, false), vec![]);
    assert_eq!(store.text(), "pushed note");
    let root = l.screen();
    assert_eq!(root.editor, None);
    assert_eq!(root.view.name, "Launcher");
    assert!(!root.view.marked);

    // 2) Opened by optional hotkey: Esc saves immediately and hides Launcher.
    assert!(l.show_view(&ViewId(VIEW_ID.into())));
    let opened = l.screen();
    assert_eq!(opened.editor.as_deref(), Some("pushed note"));
    assert!(opened.view.marked);
    assert!(!opened.view.back, "hotkey view hides instead of popping");

    l.handle(UiEvent::ScratchpadChanged("hotkey note".into()));
    assert_eq!(key(&mut l, Key::Escape, false), vec![Effect::Hide]);
    assert_eq!(store.text(), "hotkey note");

    // 3) Backspace inside editor never pops the pushed view.
    l.show_root();
    l.handle(UiEvent::TextChanged("scratchpad".into()));
    key(&mut l, Key::Enter, false);
    assert_eq!(key(&mut l, Key::Backspace, false), vec![]);
    assert!(l.screen().editor.is_some());

    // 4) Dismissed flushes pending edits before hiding.
    l.handle(UiEvent::ScratchpadChanged("dismissed note".into()));
    assert_eq!(l.handle(UiEvent::Dismissed), vec![Effect::Hide]);
    assert_eq!(store.text(), "dismissed note");
}

#[test]
fn action_panel_copy_all_and_ctrl_enter_copy_full_text() {
    let (mut l, _, _) = rig("line 1\nline 2");
    assert!(l.show_view(&ViewId(VIEW_ID.into())));

    // Action panel exposes Copy all and Clear scratchpad.
    key(&mut l, Key::K, true);
    let Some(panel) = l.screen().action_panel else {
        panic!("action panel open");
    };
    let labels: Vec<_> = panel.actions.iter().map(|a| a.label.as_str()).collect();
    assert_eq!(labels, ["Copy all", "Clear scratchpad"]);
    assert!(!panel.actions[0].destructive);
    assert!(panel.actions[1].destructive);
    assert_eq!(panel.actions[1].group, 2);

    // First action is Copy all -> copies full scratchpad text.
    assert_eq!(
        key(&mut l, Key::Enter, false),
        vec![Effect::CopyText("line 1\nline 2".into()), Effect::Hide]
    );

    // Ctrl+Enter directly in editor also runs Copy all with latest edits.
    assert!(l.show_view(&ViewId(VIEW_ID.into())));
    l.handle(UiEvent::ScratchpadChanged("updated\nnotes".into()));
    assert_eq!(
        key(&mut l, Key::Enter, true),
        vec![Effect::CopyText("updated\nnotes".into()), Effect::Hide]
    );
}

#[test]
fn clear_scratchpad_requires_second_enter_and_disarms_on_edit_or_timeout() {
    let (mut l, store, clock) = rig("keep me");
    assert!(l.show_view(&ViewId(VIEW_ID.into())));

    // First Enter on Clear scratchpad only arms.
    key(&mut l, Key::K, true);
    key(&mut l, Key::Down, false);
    assert_eq!(key(&mut l, Key::Enter, false), vec![]);
    assert_eq!(store.text(), "keep me");
    assert_eq!(l.screen().editor.as_deref(), Some("keep me"));
    assert_eq!(l.screen().footer.primary.as_deref(), Some("Confirm"));

    // Second Enter inside 5 s confirms and clears both buffer and scratchpad.md.
    clock.fetch_add(1_000, Ordering::SeqCst);
    assert_eq!(key(&mut l, Key::Enter, false), vec![]);
    assert_eq!(store.text(), "");
    assert_eq!(l.screen().editor.as_deref(), Some(""));
    assert_eq!(l.screen().footer.primary, None);

    // Editing text after 1st Enter disarms without clearing.
    l.handle(UiEvent::ScratchpadChanged("important".into()));
    key(&mut l, Key::K, true);
    key(&mut l, Key::Down, false);
    assert_eq!(key(&mut l, Key::Enter, false), vec![]);
    assert_eq!(l.screen().footer.primary.as_deref(), Some("Confirm"));
    l.handle(UiEvent::ScratchpadChanged("important!".into()));
    assert_eq!(l.screen().footer.primary, None);
    assert_eq!(key(&mut l, Key::Enter, false), vec![]);
    assert_eq!(l.screen().editor.as_deref(), Some("important!"));

    // Timeout after ARM_MS disarms without clearing.
    key(&mut l, Key::K, true);
    key(&mut l, Key::Down, false);
    assert_eq!(key(&mut l, Key::Enter, false), vec![]);
    clock.fetch_add(ARM_MS, Ordering::SeqCst);
    assert_eq!(l.screen().footer.primary, None);
    assert_eq!(key(&mut l, Key::Enter, false), vec![]);
    assert_eq!(l.screen().editor.as_deref(), Some("important!"));
}

#[test]
fn file_scratchpad_store_persists_to_disk_and_handles_missing_file() {
    let dir = std::env::temp_dir().join(format!("naobi-scratchpad-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let path = dir.join("scratchpad.md");
    let store = FileScratchpadStore::new(path.clone());

    assert_eq!(store.load().ok().as_deref(), Some(""));
    assert!(store.save("# Title\nBody").is_ok());
    assert_eq!(
        std::fs::read_to_string(&path).ok().as_deref(),
        Some("# Title\nBody")
    );
    assert_eq!(store.load().ok().as_deref(), Some("# Title\nBody"));

    assert!(store.save("").is_ok());
    assert_eq!(store.load().ok().as_deref(), Some(""));
    let _ = std::fs::remove_dir_all(&dir);
}
