//! The controller (REPORT 3.4): owns the launcher engine, runs its Effects and keeps focus return.
//! Everything that touches Windows or the window sits behind `Host`, so the tests below assert the
//! effects without a desktop.
use naobi_core::effect::{Effect, LaunchTarget};
use naobi_core::notice::{Notice, NoticeKind};
use naobi_core::ui::{Screen, UiEvent};
use naobi_launcher::{Clock, Launcher};

/// What the controller needs from the outside.
pub trait Host {
    /// Hides the Launcher window.
    fn hide(&mut self);
    /// Shows the Launcher window.
    fn show(&mut self);
    /// Whether the Launcher is on screen.
    fn is_visible(&self) -> bool;
    /// The window that has focus now, other than the Launcher.
    fn foreground(&self) -> Option<isize>;
    fn restore_focus(&mut self, window: isize);
    /// Opens the target with the shell.
    fn launch(&mut self, target: &LaunchTarget) -> std::io::Result<()>;
    fn copy_text(&mut self, text: &str) -> std::io::Result<()>;
    /// Raises a Notice (balloon, tray and the banner).
    fn notice(&mut self, n: Notice);
    /// Draws a new snapshot.
    fn post(&mut self, screen: Screen);
}

pub struct SystemClock;

impl Clock for SystemClock {
    fn now_ms(&self) -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_millis() as i64)
    }
}

pub struct Controller {
    launcher: Launcher,
    /// The window that had focus when the Launcher opened (D-013).
    previous: Option<isize>,
}

impl Controller {
    pub fn new(launcher: Launcher) -> Self {
        Self {
            launcher,
            previous: None,
        }
    }

    /// The engine's current snapshot.
    pub fn screen(&self) -> Screen {
        self.launcher.screen()
    }

    /// Opens the Launcher at the root with an empty query (D-021), remembering where focus was.
    pub fn show(&mut self, host: &mut dyn Host) {
        self.previous = host.foreground();
        self.launcher.show_root();
        host.post(self.launcher.screen());
        host.show();
    }

    /// The Launcher hotkey: closes an open Launcher like Esc does, otherwise opens it.
    pub fn toggle(&mut self, host: &mut dyn Host) {
        if host.is_visible() {
            self.close(host, true);
        } else {
            self.show(host);
        }
    }

    /// Hides the Launcher. `restore` gives focus back to the window that had it; a click elsewhere
    /// (`Dismissed`) must not take focus back from where the Owner clicked.
    fn close(&mut self, host: &mut dyn Host, restore: bool) {
        host.hide();
        let previous = self.previous.take();
        if let (true, Some(w)) = (restore, previous) {
            host.restore_focus(w);
        }
    }

    pub fn on_ui_event(&mut self, ev: UiEvent, host: &mut dyn Host) {
        let restore = !matches!(ev, UiEvent::Dismissed);
        let effects = self.launcher.handle(ev);
        self.execute(effects, restore, host);
        host.post(self.launcher.screen());
    }

    /// Executes what the engine asked for. A Launch hides the Launcher without giving focus back:
    /// the launched program takes it.
    fn execute(&mut self, effects: Vec<Effect>, restore: bool, host: &mut dyn Host) {
        let mut hide = false;
        let mut launched = false;
        for e in effects {
            match e {
                Effect::Launch(t) => {
                    launched = true;
                    if let Err(e) = host.launch(&t) {
                        tracing::warn!(code = ?e.raw_os_error(), "launch failed");
                        host.notice(failure("Open", "Windows could not open it."));
                    }
                }
                Effect::Hide => hide = true,
                Effect::CopyText(text) => {
                    if host.copy_text(&text).is_err() {
                        host.notice(failure("Copy", "The clipboard is busy, try again."));
                    }
                }
                Effect::Notice(n) => host.notice(n),
                // The engine switched the view itself.
                Effect::PushView(_) => {}
                // ponytail: no clipboard view yet (NCMD-19/20), so nothing can ask for a paste.
                Effect::Paste(_) => {}
            }
        }
        // Every Notice was handed to the host, which shows it once.
        self.launcher.clear_notice();
        if hide {
            self.close(host, restore && !launched);
        }
    }
}

fn failure(subject: &str, message: &str) -> Notice {
    Notice {
        subject: subject.to_owned(),
        message: message.to_owned(),
        kind: NoticeKind::Error,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use naobi_core::command::{CommandSource, Cx, Outcome, Query};
    use naobi_core::db::Db;
    use naobi_core::ids::{ActionId, ItemId};
    use naobi_core::item::{Action, Item, ItemKind};
    use naobi_core::ui::{Key, KeyChord};
    use naobi_launcher::frecency::Frecency;

    #[derive(Default)]
    struct Fake {
        visible: bool,
        fg: Option<isize>,
        log: Vec<String>,
        launch_fails: bool,
        copy_fails: bool,
        posts: usize,
    }

    impl Host for Fake {
        fn hide(&mut self) {
            self.visible = false;
            self.log.push("hide".into());
        }
        fn show(&mut self) {
            self.visible = true;
            self.log.push("show".into());
        }
        fn is_visible(&self) -> bool {
            self.visible
        }
        fn foreground(&self) -> Option<isize> {
            self.fg
        }
        fn restore_focus(&mut self, w: isize) {
            self.log.push(format!("restore {w}"));
        }
        fn launch(&mut self, t: &LaunchTarget) -> std::io::Result<()> {
            self.log.push(format!("launch {t:?}"));
            if self.launch_fails {
                Err(std::io::Error::from_raw_os_error(2))
            } else {
                Ok(())
            }
        }
        fn copy_text(&mut self, text: &str) -> std::io::Result<()> {
            self.log.push(format!("copy {text}"));
            if self.copy_fails {
                Err(std::io::Error::from_raw_os_error(5))
            } else {
                Ok(())
            }
        }
        fn notice(&mut self, n: Notice) {
            self.log
                .push(format!("notice {}: {}", n.subject, n.message));
        }
        fn post(&mut self, _s: Screen) {
            self.posts += 1;
        }
    }

    /// One app item; Enter launches it, Ctrl+Enter copies its path.
    struct Apps;

    impl CommandSource for Apps {
        fn items(&self, _q: &Query) -> Vec<Item> {
            vec![Item {
                id: ItemId("app:notepad".into()),
                title: "Notepad".into(),
                subtitle: String::new(),
                keywords: vec![],
                alias: None,
                kind: ItemKind::App,
                primary: ActionId("open".into()),
                secondary: Some(ActionId("copy".into())),
            }]
        }
        fn actions(&self, _id: &ItemId) -> Vec<Action> {
            vec![]
        }
        fn run(&self, _id: &ItemId, a: ActionId, cx: &mut Cx) -> Outcome {
            if a.0 == "copy" {
                cx.copy_text(r"C:\Windows\notepad.exe".into());
            } else {
                cx.launch(LaunchTarget::ShellItem(r"C:\Windows\notepad.exe".into()));
            }
            Outcome::Done
        }
    }

    struct Zero;
    impl Clock for Zero {
        fn now_ms(&self) -> i64 {
            1_000_000_000_000
        }
    }

    fn controller() -> Controller {
        let db = Db::open_in_memory().unwrap_or_else(|e| panic!("db: {e}"));
        let frecency = Frecency::open(db, 0).unwrap_or_else(|e| panic!("frecency: {e}"));
        Controller::new(Launcher::new(
            vec![Box::new(Apps)],
            vec![],
            frecency,
            Box::new(Zero),
        ))
    }

    fn key(k: Key, ctrl: bool) -> UiEvent {
        UiEvent::Key(KeyChord {
            key: k,
            ctrl,
            alt: false,
            shift: false,
        })
    }

    fn shown(fg: Option<isize>) -> (Controller, Fake) {
        let mut c = controller();
        let mut h = Fake {
            fg,
            ..Fake::default()
        };
        c.show(&mut h);
        h.log.clear();
        (c, h)
    }

    // @R-012
    #[test]
    fn r012_enter_launches_then_hides_and_does_not_take_focus_back() {
        let (mut c, mut h) = shown(Some(77));
        c.on_ui_event(key(Key::Enter, false), &mut h);
        assert_eq!(
            h.log,
            vec![
                r#"launch ShellItem("C:\\Windows\\notepad.exe")"#.to_string(),
                "hide".to_string()
            ]
        );
    }

    // @R-012
    #[test]
    fn r012_ctrl_enter_copies_then_hides_and_gives_focus_back() {
        let (mut c, mut h) = shown(Some(77));
        c.on_ui_event(key(Key::Enter, true), &mut h);
        assert_eq!(
            h.log,
            vec![
                r"copy C:\Windows\notepad.exe".to_string(),
                "hide".to_string(),
                "restore 77".to_string()
            ]
        );
    }

    // @R-012
    #[test]
    fn d013_esc_hides_and_gives_focus_back_to_the_window_that_had_it() {
        let (mut c, mut h) = shown(Some(42));
        c.on_ui_event(key(Key::Escape, false), &mut h);
        assert_eq!(h.log, vec!["hide".to_string(), "restore 42".to_string()]);
        // Only once: the next close has nothing to give back.
        c.on_ui_event(key(Key::Escape, false), &mut h);
        assert_eq!(h.log.last().map(String::as_str), Some("hide"));
        assert_eq!(h.log.iter().filter(|l| l.starts_with("restore")).count(), 1);
    }

    // @R-012
    #[test]
    fn d013_a_click_elsewhere_hides_without_taking_focus_back() {
        let (mut c, mut h) = shown(Some(42));
        c.on_ui_event(UiEvent::Dismissed, &mut h);
        assert_eq!(h.log, vec!["hide".to_string()]);
    }

    // @R-023
    #[test]
    fn r023_hotkey_toggles_and_a_show_resets_to_the_root() {
        let mut c = controller();
        let mut h = Fake {
            fg: Some(9),
            ..Fake::default()
        };
        c.toggle(&mut h);
        assert_eq!(h.log, vec!["show".to_string()]);
        c.toggle(&mut h);
        assert_eq!(
            h.log,
            vec![
                "show".to_string(),
                "hide".to_string(),
                "restore 9".to_string()
            ]
        );
        assert!(!h.visible);
    }

    #[test]
    fn a_failed_launch_raises_a_notice_and_the_notice_is_shown_once() {
        let (mut c, mut h) = shown(None);
        h.launch_fails = true;
        c.on_ui_event(key(Key::Enter, false), &mut h);
        assert!(h
            .log
            .contains(&"notice Open: Windows could not open it.".to_string()));
        // The engine's own banner was cleared: its Esc ladder must not swallow an Esc for it.
        h.log.clear();
        c.on_ui_event(key(Key::Escape, false), &mut h);
        assert_eq!(h.log, vec!["hide".to_string()]);
    }

    #[test]
    fn a_failed_copy_raises_a_notice() {
        let (mut c, mut h) = shown(None);
        h.copy_fails = true;
        c.on_ui_event(key(Key::Enter, true), &mut h);
        assert!(h
            .log
            .contains(&"notice Copy: The clipboard is busy, try again.".to_string()));
    }

    #[test]
    fn every_event_posts_a_fresh_screen() {
        let (mut c, mut h) = shown(None);
        let before = h.posts;
        c.on_ui_event(UiEvent::TextChanged("note".into()), &mut h);
        assert_eq!(h.posts, before + 1);
    }
}
