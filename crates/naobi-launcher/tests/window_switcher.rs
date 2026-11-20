//! Window switcher through `Launcher` with a fake `WindowOps` adapter.
use naobi_core::db::Db;
use naobi_core::effect::Effect;
use naobi_core::ids::ViewId;
use naobi_core::notice::NoticeKind;
use naobi_core::ui::{Key, KeyChord, Screen, UiEvent};
use naobi_launcher::armed::ARM_MS;
use naobi_launcher::frecency::Frecency;
use naobi_launcher::providers::windows::{
    OpenWindow, WindowActionError, WindowCommands, WindowOps, WindowSource, WindowsView, VIEW_ID,
};
use naobi_launcher::{Clock, Launcher};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex};

struct FakeClock(Arc<AtomicI64>);

impl Clock for FakeClock {
    fn now_ms(&self) -> i64 {
        self.0.load(Ordering::SeqCst)
    }
}

#[derive(Default)]
struct FakeWindowOps {
    windows: Mutex<Vec<OpenWindow>>,
    calls: Mutex<Vec<(&'static str, isize)>>,
    elevated: Mutex<Vec<isize>>,
    refused: Mutex<Vec<isize>>,
}

impl FakeWindowOps {
    fn with_windows(list: Vec<OpenWindow>) -> Self {
        Self {
            windows: Mutex::new(list),
            ..Self::default()
        }
    }

    fn calls(&self) -> Vec<(&'static str, isize)> {
        self.calls.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    fn check(&self, hwnd: isize, op: &'static str) -> Result<(), WindowActionError> {
        self.calls
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push((op, hwnd));
        if self
            .elevated
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .contains(&hwnd)
        {
            return Err(WindowActionError::Elevated);
        }
        if self
            .refused
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .contains(&hwnd)
        {
            return Err(WindowActionError::Refused);
        }
        Ok(())
    }
}

impl WindowOps for FakeWindowOps {
    fn list(&self) -> Vec<OpenWindow> {
        self.windows
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    fn focus(&self, hwnd: isize) -> Result<(), WindowActionError> {
        self.calls
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(("focus", hwnd));
        if self
            .refused
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .contains(&hwnd)
        {
            return Err(WindowActionError::Refused);
        }
        Ok(())
    }

    fn minimize(&self, hwnd: isize) -> Result<(), WindowActionError> {
        self.check(hwnd, "minimize")
    }

    fn close(&self, hwnd: isize) -> Result<(), WindowActionError> {
        self.check(hwnd, "close")
    }

    fn force_quit(&self, hwnd: isize) -> Result<(), WindowActionError> {
        self.check(hwnd, "force_quit")
    }
}

fn win(hwnd: isize, title: &str, process_name: &str) -> OpenWindow {
    OpenWindow {
        hwnd,
        title: title.to_owned(),
        process_name: process_name.to_owned(),
    }
}

fn rig(list: Vec<OpenWindow>) -> (Launcher, Arc<FakeWindowOps>, Arc<AtomicI64>) {
    let ops = Arc::new(FakeWindowOps::with_windows(list));
    let clock = Arc::new(AtomicI64::new(1_000_000_000_000));
    let db = Db::open_in_memory().unwrap_or_else(|e| panic!("db: {e}"));
    let frecency = Frecency::open(db, 0).unwrap_or_else(|e| panic!("frecency: {e}"));
    let launcher = Launcher::new(
        vec![
            Box::new(WindowCommands),
            Box::new(WindowSource::new(ops.clone())),
        ],
        vec![Box::new(WindowsView::new(ops.clone()))],
        frecency,
        Box::new(FakeClock(clock.clone())),
    );
    (launcher, ops, clock)
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
fn root_search_finds_open_windows_by_title_or_process_and_enter_focuses_and_hides() {
    let (mut l, ops, _) = rig(vec![
        win(101, "GitHub - Issue #7", "firefox.exe"),
        win(202, "Command Prompt", "cmd.exe"),
    ]);

    l.handle(UiEvent::TextChanged("firefox".into()));
    let s = l.screen();
    assert_eq!(titles(&s), vec!["GitHub - Issue #7"]);
    assert_eq!(s.sections[0].rows[0].kind, "Open window");
    assert_eq!(s.sections[0].rows[0].subtitle, "firefox.exe");
    assert_eq!(s.footer.primary.as_deref(), Some("Focus"));

    assert_eq!(key(&mut l, Key::Enter, false), vec![Effect::Hide]);
    assert_eq!(ops.calls(), vec![("focus", 101)]);

    l.show_root();
    l.handle(UiEvent::TextChanged("command".into()));
    assert_eq!(titles(&l.screen()), vec!["Command Prompt"]);
}

#[test]
fn windows_view_lists_filters_and_pops_or_hides_on_esc() {
    let (mut l, ops, _) = rig(vec![
        win(10, "Zebra Notes", "notepad.exe"),
        win(20, "Alpha Terminal", "wt.exe"),
    ]);

    l.handle(UiEvent::TextChanged("switch windows".into()));
    assert_eq!(titles(&l.screen())[0], "Switch Windows");
    assert_eq!(
        key(&mut l, Key::Enter, false),
        vec![Effect::PushView(ViewId(VIEW_ID.into()))]
    );

    let s = l.screen();
    assert_eq!(s.view.name, "Windows");
    assert!(s.view.marked && s.view.back);
    assert_eq!(titles(&s), vec!["Zebra Notes", "Alpha Terminal"]);

    // Esc pops back to Root when pushed from Root.
    assert!(key(&mut l, Key::Escape, false).is_empty());
    assert_eq!(l.screen().view.name, "Launcher");

    // Push again and filter by process name.
    l.handle(UiEvent::TextChanged("switch windows".into()));
    key(&mut l, Key::Enter, false);
    l.handle(UiEvent::TextChanged("wt".into()));
    assert_eq!(titles(&l.screen()), vec!["Alpha Terminal"]);

    assert_eq!(key(&mut l, Key::Enter, false), vec![Effect::Hide]);
    assert_eq!(ops.calls(), vec![("focus", 20)]);

    // Opened via hotkey: Esc hides instead of popping to Root.
    assert!(l.show_view(&ViewId(VIEW_ID.into())));
    assert!(l.screen().view.marked && !l.screen().view.back);
    assert_eq!(key(&mut l, Key::Escape, false), vec![Effect::Hide]);

    // Empty state.
    let (mut empty, _, _) = rig(vec![]);
    assert!(empty.show_view(&ViewId(VIEW_ID.into())));
    assert_eq!(
        empty.screen().empty.map(|e| e.title),
        Some("No open windows".to_owned())
    );
}

#[test]
fn action_panel_lists_focus_minimize_close_and_destructive_force_quit() {
    let (mut l, ops, _) = rig(vec![win(303, "Notepad", "notepad.exe")]);
    l.handle(UiEvent::TextChanged("notepad".into()));
    key(&mut l, Key::K, true);

    let panel = l.screen().action_panel.expect("action panel open");
    let labels: Vec<_> = panel.actions.iter().map(|a| a.label.as_str()).collect();
    assert_eq!(labels, ["Focus", "Minimize", "Close", "Force Quit"]);
    assert!(!panel.actions[0].destructive);
    assert!(!panel.actions[1].destructive);
    assert!(!panel.actions[2].destructive);
    assert!(panel.actions[3].destructive);
    assert_eq!(panel.actions[3].group, 2);

    // Minimize runs on first Enter in panel.
    key(&mut l, Key::Down, false);
    assert_eq!(key(&mut l, Key::Enter, false), vec![Effect::Hide]);
    assert_eq!(ops.calls(), vec![("minimize", 303)]);

    // Close runs on first Enter in panel.
    l.show_root();
    l.handle(UiEvent::TextChanged("notepad".into()));
    key(&mut l, Key::K, true);
    l.handle(UiEvent::PanelFilter("close".into()));
    assert_eq!(key(&mut l, Key::Enter, false), vec![Effect::Hide]);
    assert_eq!(ops.calls(), vec![("minimize", 303), ("close", 303)]);
}

#[test]
fn force_quit_requires_second_enter_and_disarms_on_key_or_timeout() {
    let (mut l, ops, clock) = rig(vec![win(404, "Hung App", "hung.exe")]);
    l.handle(UiEvent::TextChanged("hung".into()));

    // First Enter on Force Quit in action panel only arms.
    key(&mut l, Key::K, true);
    l.handle(UiEvent::PanelFilter("force".into()));
    assert_eq!(key(&mut l, Key::Enter, false), vec![]);
    assert!(
        ops.calls().is_empty(),
        "force_quit must not run on 1st Enter"
    );
    let s = l.screen();
    assert!(s.sections[0].rows[0].armed);
    assert_eq!(s.footer.primary.as_deref(), Some("Confirm"));

    // Second Enter inside 5 s confirms and executes force_quit.
    clock.fetch_add(1_000, Ordering::SeqCst);
    assert_eq!(key(&mut l, Key::Enter, false), vec![Effect::Hide]);
    assert_eq!(ops.calls(), vec![("force_quit", 404)]);

    // Disarms on non-Enter key (e.g. Escape) so next Enter focuses instead.
    l.show_root();
    l.handle(UiEvent::TextChanged("hung".into()));
    key(&mut l, Key::K, true);
    l.handle(UiEvent::PanelFilter("force".into()));
    assert_eq!(key(&mut l, Key::Enter, false), vec![]);
    key(&mut l, Key::Escape, false);
    assert!(!l.screen().sections[0].rows[0].armed);
    assert_eq!(key(&mut l, Key::Enter, false), vec![Effect::Hide]);
    assert_eq!(
        ops.calls(),
        vec![("force_quit", 404), ("focus", 404)],
        "Escape disarms Force Quit; Enter runs Focus"
    );

    // Disarms after ARM_MS timeout so next Enter focuses instead.
    l.show_root();
    l.handle(UiEvent::TextChanged("hung".into()));
    key(&mut l, Key::K, true);
    l.handle(UiEvent::PanelFilter("force".into()));
    assert_eq!(key(&mut l, Key::Enter, false), vec![]);
    clock.fetch_add(ARM_MS, Ordering::SeqCst);
    assert!(!l.screen().sections[0].rows[0].armed);
    assert_eq!(key(&mut l, Key::Enter, false), vec![Effect::Hide]);
    assert_eq!(
        ops.calls(),
        vec![("force_quit", 404), ("focus", 404), ("focus", 404)],
        "Timeout disarms Force Quit; Enter runs Focus"
    );
}

// @R-011
#[test]
fn r011_elevated_window_close_and_force_quit_emit_admin_notice() {
    let (mut l, ops, _) = rig(vec![
        win(505, "Task Manager", "Taskmgr.exe"),
        win(606, "Gone Window", "gone.exe"),
    ]);
    ops.elevated.lock().unwrap().push(505);
    ops.refused.lock().unwrap().push(606);

    // Close an elevated window -> Admin-window notice, Launcher stays open.
    l.handle(UiEvent::TextChanged("task manager".into()));
    key(&mut l, Key::K, true);
    l.handle(UiEvent::PanelFilter("close".into()));
    let effects = key(&mut l, Key::Enter, false);
    assert!(
        matches!(
            effects.as_slice(),
            [Effect::Notice(n)]
                if n.kind == NoticeKind::Warning
                    && n.subject == "Cannot close Task Manager"
                    && n.message == "it runs as administrator"
        ),
        "{effects:?}"
    );
    assert!(l.screen().notice.is_some());

    // Force Quit an elevated window (from Windows view) -> 2x Enter -> Admin-window notice.
    assert!(l.show_view(&ViewId(VIEW_ID.into())));
    l.handle(UiEvent::TextChanged("task".into()));
    key(&mut l, Key::K, true);
    l.handle(UiEvent::PanelFilter("force".into()));
    assert!(key(&mut l, Key::Enter, false).is_empty());
    let effects = key(&mut l, Key::Enter, false);
    assert!(
        matches!(
            effects.as_slice(),
            [Effect::Notice(n)]
                if n.kind == NoticeKind::Warning
                    && n.subject == "Cannot force quit Task Manager"
                    && n.message == "it runs as administrator"
        ),
        "{effects:?}"
    );

    // Refused / vanished window -> Notice without crashing or hiding.
    l.show_root();
    l.handle(UiEvent::TextChanged("gone".into()));
    key(&mut l, Key::K, true);
    l.handle(UiEvent::PanelFilter("close".into()));
    let effects = key(&mut l, Key::Enter, false);
    assert!(
        matches!(
            effects.as_slice(),
            [Effect::Notice(n)]
                if n.kind == NoticeKind::Warning && n.subject == "Cannot close Gone Window"
        ),
        "{effects:?}"
    );
}
