//! The controller (REPORT 3.4): owns the launcher engine, runs its Effects and keeps focus return.
//! Everything that touches Windows or the window sits behind `Host`, so the tests below assert the
//! effects without a desktop.
use crate::modules::Router;
use naobi_core::command::Cx;
use naobi_core::effect::{Effect, LaunchTarget};
use naobi_core::event::AppEvent;
use naobi_core::ids::{EntryId, HotkeyId, ViewId};
use naobi_core::item::Item;
use naobi_core::notice::{Notice, NoticeKind};
use naobi_core::ui::{Screen, UiEvent};
use naobi_launcher::engine::ItemHotkeyOutcome;
use naobi_launcher::icons::{askable, Answer, IconCache};
use naobi_launcher::{Clock, Launcher};
use std::collections::{HashMap, HashSet};

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
    /// Calls `Controller::finish_close` after `ms` milliseconds.
    fn close_later(&mut self, ms: u64);
    /// Puts a clipboard history entry on the clipboard (Ctrl+Enter).
    fn copy_entry(&mut self, id: EntryId) -> std::io::Result<()>;
    /// Pastes a history entry into `target`, the window that had focus before the Launcher. The
    /// Launcher is already hidden; the host does the sequence off the UI thread.
    fn paste(&mut self, id: EntryId, target: Option<isize>);
    /// Pastes plain text into `target`, the window that had focus before the Launcher.
    fn paste_text(&mut self, text: &str, target: Option<isize>);
    /// Pastes file paths (`CF_HDROP`) into `target`.
    fn paste_files(&mut self, files: &[String], target: Option<isize>);
    /// Starts a file search for `query`; the answer comes back through
    /// `Controller::on_file_results` with the same `generation`.
    fn search_files(&mut self, generation: u64, query: &str);
    /// Starts loading the icons for `keys`; the answer comes back through `Controller::on_icons`.
    fn load_icons(&mut self, keys: Vec<String>);
    /// Uses this Theme for the current Windows mode. `Err` is the message for the Owner.
    fn apply_theme(&mut self, id: &str) -> Result<(), String>;
    /// Schedules `Controller::on_scratchpad_save_timer` for `generation` after `ms` milliseconds.
    fn schedule_scratchpad_save(&mut self, generation: u64, ms: u64);
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
    /// Where the modules' hotkeys and events go.
    router: Router,
    /// The window that had focus when the Launcher opened (D-013).
    previous: Option<isize>,
    /// A copy asked to hide: the Launcher shows "Copied" first and closes (with this focus choice)
    /// when `finish_close` runs.
    pending_close: Option<bool>,
    /// A Theme was applied by the last event: the footer says so.
    applied: bool,
    /// The generation a file search was last asked for, so a query is asked once.
    asked: Option<u64>,
    icons: IconCache,
    /// Icon keys asked for and not yet answered with a picture.
    icons_asked: HashSet<String>,
    /// How many times the icon child failed on a key.
    icon_failures: HashMap<String, u8>,
}

/// A key whose icon failed this many times (a crashing or hanging handler) is given up on.
const MAX_ICON_TRIES: u8 = 2;

/// How long "Copied" shows before the Launcher hides (DESIGN_SYSTEM).
const CONFIRM_MS: u64 = 120;

impl Controller {
    pub fn new(launcher: Launcher, router: Router) -> Self {
        Self {
            launcher,
            router,
            previous: None,
            pending_close: None,
            applied: false,
            asked: None,
            icons: IconCache::default(),
            icons_asked: HashSet::new(),
            icon_failures: HashMap::new(),
        }
    }

    pub fn set_aliases(&mut self, aliases: std::collections::BTreeMap<String, String>) {
        let mapped = aliases
            .into_iter()
            .map(|(k, v)| (k, naobi_core::ids::ItemId(v)))
            .collect();
        self.launcher.set_aliases(mapped);
    }

    pub fn set_show_home(&mut self, show: bool) {
        self.launcher.set_show_home(show);
    }

    /// The engine's snapshot, with the "Copied" confirmation while a copy waits to close, and the
    /// icons that are loaded. The keys of rows whose icon is not loaded and not yet asked for go to
    /// `missing`.
    fn view(&mut self, missing: &mut Vec<String>) -> Screen {
        let mut s = self.launcher.screen();
        if self.pending_close.is_some() {
            s.footer.confirmation = Some("Copied".to_owned());
        } else if self.applied {
            s.footer.confirmation = Some("Applied".to_owned());
        }
        for row in s.sections.iter_mut().flat_map(|sec| sec.rows.iter_mut()) {
            let Some(key) = row.icon_key.clone() else {
                continue;
            };
            match self.icons.get(&key) {
                Some(icon) => row.icon = Some(icon),
                None if askable(&key) && self.icons_asked.insert(key.clone()) => missing.push(key),
                None => {}
            }
        }
        s
    }

    /// Draws the current snapshot, and asks for the icons it lacks.
    fn present(&mut self, host: &mut dyn Host) {
        let mut missing = Vec::new();
        let screen = self.view(&mut missing);
        host.post(screen);
        if !missing.is_empty() {
            host.load_icons(missing);
        }
    }

    /// Icons arrived, or did not: the next snapshot shows them. A key that has none is not asked for
    /// again; a key whose child died or hung is asked for again, twice at most.
    pub fn on_icons(&mut self, found: Vec<(String, Answer)>, host: &mut dyn Host) {
        for (key, answer) in found {
            match answer {
                Answer::Icon(icon) => {
                    self.icons_asked.remove(&key);
                    self.icon_failures.remove(&key);
                    self.icons.insert(key, icon);
                }
                Answer::Missing => {}
                Answer::Retry => {
                    let tries = self.icon_failures.entry(key.clone()).or_insert(0);
                    *tries += 1;
                    if *tries < MAX_ICON_TRIES {
                        self.icons_asked.remove(&key);
                    }
                }
            }
        }
        // A hidden Launcher is not redrawn for a late answer; the cache keeps the icon for the next show.
        if host.is_visible() {
            self.present(host);
        }
    }

    /// A Notice raised by a worker thread (the color picker's clipboard write failed).
    pub fn on_notice(&mut self, n: Notice, host: &mut dyn Host) {
        host.notice(n);
    }

    /// The confirmation has shown long enough: hides the Launcher now.
    pub fn finish_close(&mut self, host: &mut dyn Host) {
        if let Some(restore) = self.pending_close.take() {
            self.close(host, restore);
        }
    }

    /// Opens the Launcher at the root with an empty query (D-021), remembering where focus was.
    pub fn show(&mut self, host: &mut dyn Host) {
        // A copy that was waiting to close must not hide the Launcher that opens now.
        self.pending_close = None;
        self.previous = host.foreground();
        self.launcher.show_root();
        self.present(host);
        host.show();
    }

    /// A source's index changed: ranks the current query again and posts it.
    pub fn refresh(&mut self, host: &mut dyn Host) {
        self.launcher.refresh_sources();
        // The file rows may be out of date too: ask for them again.
        self.asked = None;
        self.ask_files(host);
        self.present(host);
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
        self.pending_close = None;
        host.hide();
        // Back to the empty root: no query, command line or clipboard text stays decrypted behind
        // a hidden window (T-014, T-034).
        self.launcher.show_root();
        self.present(host);
        let previous = self.previous.take();
        if let (true, Some(w)) = (restore, previous) {
            host.restore_focus(w);
        }
    }

    pub fn on_ui_event(&mut self, ev: UiEvent, host: &mut dyn Host) {
        let restore = !matches!(ev, UiEvent::Dismissed);
        let hover = matches!(ev, UiEvent::RowHovered(_));
        let selected = self.launcher.selected();
        let effects = self.launcher.handle(ev);
        self.applied = false;
        // The pointer moving inside the selected row changes nothing; a redraw would rebuild every
        // row, the one being clicked included.
        if hover && self.launcher.selected() == selected {
            return;
        }
        self.execute(effects, restore, host);
        if let Some(generation) = self.launcher.take_pending_scratchpad_save() {
            host.schedule_scratchpad_save(
                generation,
                naobi_launcher::providers::scratchpad::AUTOSAVE_DEBOUNCE_MS,
            );
        }
        self.ask_files(host);
        self.present(host);
    }

    /// Runs a debounced scratchpad autosave if `generation` is still current.
    pub fn on_scratchpad_save_timer(&mut self, generation: u64, host: &mut dyn Host) {
        let effects = self.launcher.flush_scratchpad_if_gen(generation);
        if !effects.is_empty() {
            self.execute(effects, true, host);
            if host.is_visible() {
                self.present(host);
            }
        }
    }

    /// Asks for file results when the root shows a new query (R-013). The rows trail the others.
    fn ask_files(&mut self, host: &mut dyn Host) {
        let Some((generation, text)) = self.launcher.file_query() else {
            return;
        };
        if self.asked != Some(generation) {
            self.asked = Some(generation);
            host.search_files(generation, &text);
        }
    }

    /// A file search answered. Rows for a query that is gone are dropped. An empty answer still
    /// redraws, so an "unavailable" status shows.
    pub fn on_file_results(&mut self, generation: u64, items: Vec<Item>, host: &mut dyn Host) {
        let Some(source) = self.launcher.file_source() else {
            return;
        };
        if self.launcher.file_results(generation, source, items) && host.is_visible() {
            self.present(host);
        }
    }

    /// A module's hotkey or `[hotkeys.items]` binding: runs its handlers/primary action directly
    /// when non-parameterized, or opens the Launcher ready for input for views/parameterized items.
    pub fn on_hotkey(&mut self, id: &HotkeyId, host: &mut dyn Host) -> bool {
        let mut cx = Cx::default();
        let handled = self.router.hotkey(id, &mut cx);
        if handled {
            let (opens, rest): (Vec<_>, Vec<_>) = cx
                .take()
                .into_iter()
                .partition(|e| matches!(e, Effect::PushView(_)));
            self.execute(rest, true, host);
            // A module's hotkey that asks for its view opens the Launcher on it (D-021 for hotkey views).
            if let Some(Effect::PushView(view)) = opens.into_iter().next() {
                self.open_view(&view, host);
            }
            return true;
        }
        let Some(raw_item_id) = id.0.strip_prefix("item:") else {
            return false;
        };
        let target_id = naobi_core::ids::ItemId(raw_item_id.to_owned());
        match self.launcher.trigger_item_hotkey(&target_id) {
            ItemHotkeyOutcome::Direct(mut effects) => {
                let was_hidden = !host.is_visible();
                if was_hidden {
                    self.previous = host.foreground();
                    effects.retain(|e| !matches!(e, Effect::Hide));
                }
                self.execute(effects, true, host);
                true
            }
            ItemHotkeyOutcome::OpenLauncher { view, prefill } => {
                self.pending_close = None;
                self.previous = host.foreground();
                let opened = match view {
                    Some(v) => self.launcher.open_view_any(&v),
                    None => {
                        self.launcher.show_root();
                        if let Some(text) = prefill {
                            let _ = self.launcher.handle(UiEvent::TextChanged(text));
                        }
                        true
                    }
                };
                if opened {
                    self.present(host);
                    host.show();
                }
                opened
            }
            ItemHotkeyOutcome::NotFound => false,
        }
    }

    /// Opens the Launcher on a view that has its own hotkey; pressing it again closes the Launcher.
    fn open_view(&mut self, view: &ViewId, host: &mut dyn Host) {
        if host.is_visible() {
            self.close(host, true);
            return;
        }
        self.pending_close = None;
        self.previous = host.foreground();
        if self.launcher.show_view(view) {
            self.present(host);
            host.show();
        }
    }

    /// An event for the modules (clipboard change, resume, timer...). False when nobody listens.
    pub fn on_app_event(&mut self, ev: &AppEvent, host: &mut dyn Host) -> bool {
        let mut cx = Cx::default();
        let handled = self.router.event(ev, &mut cx);
        self.execute(cx.take(), true, host);
        handled
    }

    /// Executes what the engine asked for. A Launch hides the Launcher without giving focus back:
    /// the launched program takes it.
    fn execute(&mut self, effects: Vec<Effect>, restore: bool, host: &mut dyn Host) {
        let mut hide = false;
        let mut launched = false;
        let mut copied = false;
        let mut paste = None;
        let mut paste_text = None;
        let mut paste_files = None;
        let mut hotkeys = Vec::new();
        // Where a paste goes: the window that had focus before the Launcher (`close` forgets it).
        let target = self.previous;
        for e in effects {
            match e {
                Effect::Launch(t) => {
                    // T-006 in depth: only http and https URLs are opened.
                    if let Some(scheme) = t.refused_scheme() {
                        host.notice(failure(
                            "Open",
                            &format!("The {scheme}: scheme is not allowed."),
                        ));
                        continue;
                    }
                    // Only a launch that started gives focus away; a failed one restores it (D-013).
                    match host.launch(&t) {
                        Ok(()) => launched = true,
                        Err(e) => {
                            tracing::warn!(code = ?e.raw_os_error(), "launch failed");
                            let (subject, message) = match &t {
                                LaunchTarget::Console(_) => {
                                    ("Run", "PowerShell could not be started.")
                                }
                                LaunchTarget::Program { .. } | LaunchTarget::RunAsAdmin(_) => {
                                    ("Run", "Windows could not start that program.")
                                }
                                LaunchTarget::RevealInExplorer(_) => {
                                    ("Reveal", "Explorer could not open that folder.")
                                }
                                _ => ("Open", "Windows could not open it."),
                            };
                            host.notice(failure(subject, message));
                        }
                    }
                }
                Effect::Hide => hide = true,
                Effect::CopyText(text) => {
                    if host.copy_text(&text).is_err() {
                        host.notice(failure("Copy", "The clipboard is busy, try again."));
                    } else {
                        copied = true;
                    }
                }
                Effect::Notice(n) => host.notice(n),
                // The engine switched the view itself.
                Effect::PushView(_) => {}
                // The paste runs after the Launcher is hidden.
                Effect::Paste(id) => paste = Some(id),
                Effect::PasteText(text) => paste_text = Some(text),
                Effect::PasteFiles(files) => paste_files = Some(files),
                Effect::RunHotkey(id) => hotkeys.push(id),
                Effect::ApplyTheme(id) => match host.apply_theme(&id) {
                    Ok(()) => self.applied = true,
                    Err(message) => host.notice(failure("Theme", &message)),
                },
                Effect::CopyEntry(id) => match host.copy_entry(id) {
                    Ok(()) => copied = true,
                    Err(_) => host.notice(failure("Copy", "The clipboard is busy, try again.")),
                },
            }
        }
        // Every Notice was handed to the host, which shows it once.
        self.launcher.clear_notice();
        if hide && copied && !launched {
            // Say "Copied" for 120 ms, then hide: a Notice would stay until seen.
            self.pending_close = Some(restore);
            host.close_later(CONFIRM_MS);
        } else if hide {
            // A paste gives focus to its target itself; focusing another Open window already moved foreground.
            let focused_other = matches!(host.foreground(), Some(w) if Some(w) != self.previous);
            self.close(
                host,
                restore
                    && !launched
                    && paste.is_none()
                    && paste_text.is_none()
                    && paste_files.is_none()
                    && !focused_other,
            );
        }
        if let Some(id) = paste {
            host.paste(id, target);
        }
        if let Some(text) = paste_text {
            host.paste_text(&text, target);
        }
        if let Some(files) = paste_files {
            host.paste_files(&files, target);
        }
        // After the Launcher is hidden and focus is back: the handlers act on the window in front.
        for id in hotkeys {
            self.on_hotkey(&id, host);
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
    use naobi_core::ui::Icon;
    use naobi_core::ui::{Key, KeyChord};
    use naobi_launcher::frecency::Frecency;
    use std::sync::Arc;

    #[derive(Default)]
    struct Fake {
        visible: bool,
        fg: Option<isize>,
        log: Vec<String>,
        launch_fails: bool,
        copy_fails: bool,
        posts: usize,
        last_post: Option<Screen>,
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
        fn post(&mut self, s: Screen) {
            self.posts += 1;
            self.last_post = Some(s);
        }
        fn close_later(&mut self, ms: u64) {
            self.log.push(format!("close_later {ms}"));
        }
        fn copy_entry(&mut self, id: EntryId) -> std::io::Result<()> {
            self.log.push(format!("copy_entry {}", id.0));
            Ok(())
        }
        fn paste(&mut self, id: EntryId, target: Option<isize>) {
            self.log.push(format!("paste {} {target:?}", id.0));
        }
        fn paste_text(&mut self, text: &str, target: Option<isize>) {
            self.log.push(format!("paste_text {text} {target:?}"));
        }
        fn paste_files(&mut self, files: &[String], target: Option<isize>) {
            self.log
                .push(format!("paste_files {} {target:?}", files.join(",")));
        }
        fn search_files(&mut self, generation: u64, query: &str) {
            self.log.push(format!("search {generation} {query}"));
        }
        fn load_icons(&mut self, keys: Vec<String>) {
            self.log.push(format!("icons {}", keys.join(",")));
        }
        fn apply_theme(&mut self, id: &str) -> Result<(), String> {
            self.log.push(format!("theme {id}"));
            Ok(())
        }
        fn schedule_scratchpad_save(&mut self, generation: u64, ms: u64) {
            self.log.push(format!("save_later {generation} {ms}"));
        }
    }

    /// One app item; Enter launches it, Ctrl+Enter copies its path.
    struct Apps;

    impl CommandSource for Apps {
        fn items(&self, _q: &Query) -> Arc<[Item]> {
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
            .into()
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

    /// One item whose Enter runs `outcome`.
    struct One(Outcome);

    impl CommandSource for One {
        fn items(&self, q: &Query) -> Arc<[Item]> {
            Apps.items(q)
        }
        fn actions(&self, _id: &ItemId) -> Vec<Action> {
            vec![]
        }
        fn run(&self, _id: &ItemId, _a: ActionId, _cx: &mut Cx) -> Outcome {
            self.0.clone()
        }
    }

    fn controller_with(source: Box<dyn CommandSource>, router: Router) -> Controller {
        let db = Db::open_in_memory().unwrap_or_else(|e| panic!("db: {e}"));
        let frecency = Frecency::open(db, 0).unwrap_or_else(|e| panic!("frecency: {e}"));
        Controller::new(
            Launcher::new(vec![source], vec![], frecency, Box::new(Zero)),
            router,
        )
    }

    fn controller() -> Controller {
        let db = Db::open_in_memory().unwrap_or_else(|e| panic!("db: {e}"));
        let frecency = Frecency::open(db, 0).unwrap_or_else(|e| panic!("frecency: {e}"));
        Controller::new(
            Launcher::new(vec![Box::new(Apps)], vec![], frecency, Box::new(Zero)),
            Router::default(),
        )
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
        // "Copied" shows first; the Launcher hides when the 120 ms are over.
        assert_eq!(
            h.log,
            vec![
                r"copy C:\Windows\notepad.exe".to_string(),
                "close_later 120".to_string()
            ]
        );
        let confirmation = |h: &Fake| {
            h.last_post
                .as_ref()
                .and_then(|s| s.footer.confirmation.clone())
        };
        assert_eq!(confirmation(&h).as_deref(), Some("Copied"));
        h.log.clear();
        c.finish_close(&mut h);
        assert_eq!(h.log, vec!["hide".to_string(), "restore 77".to_string()]);
        assert_eq!(confirmation(&h), None, "gone once the Launcher closed");
        // A second call does nothing.
        h.log.clear();
        c.finish_close(&mut h);
        assert!(h.log.is_empty());
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

    #[test]
    fn closing_leaves_no_query_behind_the_hidden_window() {
        let (mut c, mut h) = shown(None);
        c.on_ui_event(UiEvent::TextChanged("secret".into()), &mut h);
        c.toggle(&mut h);
        let query = h.last_post.as_ref().map(|s| s.query.as_str());
        assert_eq!(query, Some(""));
    }

    #[test]
    fn a_pointer_moving_inside_the_selected_row_does_not_redraw() {
        let (mut c, mut h) = shown(None);
        let posts = h.posts;
        c.on_ui_event(UiEvent::RowHovered(0), &mut h);
        assert_eq!(h.posts, posts);
    }

    // @R-023
    #[test]
    fn r023_hotkey_toggles_and_a_show_resets_to_the_root() {
        // The icon requests of the first snapshot are not what this test is about.
        let quiet = |h: &Fake| -> Vec<String> {
            h.log
                .iter()
                .filter(|l| !l.starts_with("icons "))
                .cloned()
                .collect()
        };
        let mut c = controller();
        let mut h = Fake {
            fg: Some(9),
            ..Fake::default()
        };
        c.toggle(&mut h);
        assert_eq!(quiet(&h), vec!["show".to_string()]);
        c.toggle(&mut h);
        assert_eq!(
            quiet(&h),
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

    // @R-021
    #[test]
    fn r021_controller_refuses_non_http_url() {
        for (url, scheme) in [
            ("file:///C:/Windows/System32/cmd.exe", "file"),
            ("ms-settings:display", "ms-settings"),
        ] {
            let mut c = controller_with(
                Box::new(One(Outcome::Effects(vec![Effect::Launch(
                    LaunchTarget::Url(url.into()),
                )]))),
                Router::default(),
            );
            let mut h = Fake::default();
            c.show(&mut h);
            h.log.clear();
            c.on_ui_event(key(Key::Enter, false), &mut h);
            assert!(
                !h.log.iter().any(|l| l.starts_with("launch")),
                "nothing was opened for {url}"
            );
            assert!(
                h.log.contains(&format!(
                    "notice Open: The {scheme}: scheme is not allowed."
                )),
                "{:?}",
                h.log
            );
        }
    }

    // @R-021
    #[test]
    fn r021_https_url_is_opened() {
        let mut c = controller_with(
            Box::new(One(Outcome::Effects(vec![Effect::Launch(
                LaunchTarget::Url("https://naobi.dev/work/".into()),
            )]))),
            Router::default(),
        );
        let mut h = Fake::default();
        c.show(&mut h);
        h.log.clear();
        c.on_ui_event(key(Key::Enter, false), &mut h);
        assert_eq!(
            h.log,
            vec![
                r#"launch Url("https://naobi.dev/work/")"#.to_string(),
                "hide".to_string()
            ]
        );
    }

    // @R-015
    #[test]
    fn r015_paste_runs_after_the_launcher_is_hidden_and_does_not_restore_focus() {
        let mut c = controller_with(
            Box::new(One(Outcome::Effects(vec![Effect::Paste(EntryId(4))]))),
            Router::default(),
        );
        let mut h = Fake {
            fg: Some(77),
            ..Fake::default()
        };
        c.show(&mut h);
        h.log.clear();
        c.on_ui_event(key(Key::Enter, false), &mut h);
        // Hidden first, then the paste into the window that had focus; no separate focus restore.
        assert_eq!(
            h.log,
            vec!["hide".to_string(), "paste 4 Some(77)".to_string()]
        );
    }

    // @R-015
    #[test]
    fn r015_a_module_hotkey_opens_its_view_and_pressing_it_again_closes() {
        use crate::modules::{Modules, Parts};
        let mut m = Modules::new();
        m.subscribe(
            HotkeyId("clipboard".into()),
            Box::new(|_, cx| cx.open_view(ViewId("clipboard".into()))),
        );
        let Parts { router, .. } = m.into_parts();
        // The fake engine hosts no view, so opening reports nothing to show; the handler still runs
        // and a visible Launcher is closed by the same hotkey.
        let mut c = controller_with(Box::new(Apps), router);
        let mut h = Fake::default();
        c.show(&mut h);
        h.log.clear();
        assert!(c.on_hotkey(&HotkeyId("clipboard".into()), &mut h));
        assert_eq!(h.log, vec!["hide".to_string()]);
    }

    #[test]
    fn module_hotkeys_and_events_run_their_handlers_and_their_effects() {
        use crate::modules::{EventKind, Modules, Parts};
        let mut m = Modules::new();
        m.subscribe(
            HotkeyId("topmost".into()),
            Box::new(|_, cx| {
                cx.notice(Notice {
                    subject: "Topmost".into(),
                    message: "on".into(),
                    kind: NoticeKind::Info,
                })
            }),
        );
        m.subscribe_event(
            EventKind::Resume,
            Box::new(|_, cx| cx.copy_text("woke".into())),
        );
        let Parts { router, .. } = m.into_parts();
        let mut c = controller_with(Box::new(Apps), router);
        let mut h = Fake::default();
        assert!(c.on_hotkey(&HotkeyId("topmost".into()), &mut h));
        assert_eq!(h.log, vec!["notice Topmost: on".to_string()]);
        h.log.clear();
        assert!(c.on_app_event(&AppEvent::Resume, &mut h));
        assert_eq!(h.log, vec!["copy woke".to_string()]);
        assert!(!c.on_hotkey(&HotkeyId("nobody".into()), &mut h));
        assert!(!c.on_app_event(&AppEvent::DisplayChanged, &mut h));
    }

    // @R-004
    #[test]
    fn r016_a_command_from_the_list_hides_gives_focus_back_then_runs_its_hotkey() {
        use crate::modules::{Modules, Parts};
        let mut m = Modules::new();
        m.subscribe(
            HotkeyId("clipboard".into()),
            Box::new(|_, cx| cx.copy_text("snapped".into())),
        );
        let Parts { router, .. } = m.into_parts();
        let outcome = Outcome::Effects(vec![
            Effect::Hide,
            Effect::RunHotkey(HotkeyId("clipboard".into())),
        ]);
        let mut c = controller_with(Box::new(One(outcome)), router);
        let mut h = Fake {
            fg: Some(77),
            ..Fake::default()
        };
        c.show(&mut h);
        h.log.clear();
        c.on_ui_event(UiEvent::TextChanged("n".into()), &mut h);
        c.on_ui_event(key(Key::Enter, false), &mut h);
        // The Launcher is hidden and focus is back before the handler acts on the window in front.
        let hide = h.log.iter().position(|l| l == "hide");
        let focus = h.log.iter().position(|l| l == "restore 77");
        let ran = h.log.iter().position(|l| l == "copy snapped");
        assert!(hide < focus && focus < ran, "{:?}", h.log);
    }

    // @R-013
    #[test]
    fn r013_a_new_root_query_asks_for_files_once_and_late_rows_are_dropped() {
        use naobi_launcher::providers::files::{rows, FileCommands};
        use std::sync::atomic::AtomicBool;
        let db = Db::open_in_memory().unwrap_or_else(|e| panic!("db: {e}"));
        let frecency = Frecency::open(db, 0).unwrap_or_else(|e| panic!("frecency: {e}"));
        let files = FileCommands::new(Arc::new(AtomicBool::new(false)));
        let mut c = Controller::new(
            Launcher::new(
                vec![Box::new(Apps), Box::new(files)],
                vec![],
                frecency,
                Box::new(Zero),
            ),
            Router::default(),
        );
        let mut h = Fake::default();
        c.show(&mut h);
        let searches = |h: &Fake| h.log.iter().filter(|l| l.starts_with("search ")).count();
        // An empty query asks nothing.
        c.on_ui_event(UiEvent::TextChanged(String::new()), &mut h);
        assert_eq!(searches(&h), 0);
        c.on_ui_event(UiEvent::TextChanged("note".into()), &mut h);
        assert_eq!(searches(&h), 1);
        let asked = h.log.last().cloned().unwrap_or_default();
        assert!(asked.ends_with(" note"), "{asked}");
        // A key that does not change the query does not ask again.
        c.on_ui_event(key(Key::Down, false), &mut h);
        assert_eq!(searches(&h), 1);
        // The answer for the current query redraws with a Files section ...
        let generation: u64 = asked
            .split_whitespace()
            .nth(1)
            .and_then(|g| g.parse().ok())
            .unwrap_or(0);
        let posts = h.posts;
        c.on_file_results(generation, rows(&[r"C:\notes.txt".to_owned()]), &mut h);
        assert_eq!(h.posts, posts + 1);
        let shown = h.last_post.as_ref().map(|s| {
            s.sections
                .iter()
                .any(|sec| sec.header.as_deref() == Some("Files"))
        });
        assert_eq!(shown, Some(true));
        // ... and one for an older query is dropped without a redraw.
        c.on_ui_event(UiEvent::TextChanged("notes".into()), &mut h);
        assert_eq!(searches(&h), 2);
        let posts = h.posts;
        c.on_file_results(generation, rows(&[r"C:\old.txt".to_owned()]), &mut h);
        assert_eq!(h.posts, posts);
        // A source refresh ranks again and asks for the file rows again.
        let before = searches(&h);
        c.refresh(&mut h);
        assert_eq!(searches(&h), before + 1);
    }

    // @R-012
    #[test]
    fn r012_a_row_asks_for_its_icon_once_and_shows_it_when_it_arrives() {
        let mut c = controller();
        let mut h = Fake::default();
        c.show(&mut h);
        let asked = |h: &Fake| h.log.iter().filter(|l| l.starts_with("icons ")).count();
        c.on_ui_event(UiEvent::TextChanged("note".into()), &mut h);
        assert_eq!(asked(&h), 1);
        assert!(h.log.iter().any(|l| l == "icons a:notepad"), "{:?}", h.log);
        // Another keystroke shows the same row: it is not asked for again while it is pending.
        c.on_ui_event(UiEvent::TextChanged("notep".into()), &mut h);
        assert_eq!(asked(&h), 1);
        let icon_of = |h: &Fake| {
            h.last_post
                .as_ref()
                .and_then(|s| s.sections.first())
                .and_then(|sec| sec.rows.first())
                .and_then(|r| r.icon.clone())
        };
        assert_eq!(icon_of(&h), None);
        // The picture arrives: the redrawn snapshot carries it.
        let icon = Icon {
            w: 1,
            h: 1,
            rgba: vec![9, 9, 9, 255],
        };
        c.on_icons(
            vec![("a:notepad".into(), Answer::Icon(icon.clone()))],
            &mut h,
        );
        assert_eq!(icon_of(&h).as_deref(), Some(&icon));
        assert_eq!(asked(&h), 1, "loaded: nothing more to ask");
        // A key with no icon is not asked for again either.
        c.on_icons(vec![("a:other".into(), Answer::Missing)], &mut h);
        assert_eq!(asked(&h), 1);
    }

    // @R-012
    #[test]
    fn r012_a_key_whose_child_died_is_asked_again_twice_then_given_up() {
        let mut c = controller();
        let mut h = Fake::default();
        c.show(&mut h);
        c.on_ui_event(UiEvent::TextChanged("note".into()), &mut h);
        let asked = |h: &Fake| h.log.iter().filter(|l| *l == "icons a:notepad").count();
        assert_eq!(asked(&h), 1);
        // The child died with the key in flight: the same key is asked for once more ...
        c.on_icons(vec![("a:notepad".into(), Answer::Retry)], &mut h);
        assert_eq!(asked(&h), 2);
        // ... and when that fails too the key is given up on: no loop of requests and redraws.
        c.on_icons(vec![("a:notepad".into(), Answer::Retry)], &mut h);
        assert_eq!(asked(&h), 2);
        c.on_ui_event(UiEvent::TextChanged("notepa".into()), &mut h);
        assert_eq!(asked(&h), 2);
    }

    // @R-012
    #[test]
    fn r012_a_late_answer_does_not_redraw_a_hidden_launcher() {
        let mut c = controller();
        let mut h = Fake::default();
        c.show(&mut h);
        c.on_ui_event(UiEvent::TextChanged("note".into()), &mut h);
        h.hide();
        let before = h.posts;
        let icon = Icon {
            w: 1,
            h: 1,
            rgba: vec![9, 9, 9, 255],
        };
        c.on_icons(vec![("a:notepad".into(), Answer::Icon(icon))], &mut h);
        c.on_file_results(0, Vec::new(), &mut h);
        assert_eq!(h.posts, before, "nothing is drawn into a hidden window");
    }

    /// A real Launcher holding a RunSource on an in-memory history, a fake lookup for `notepad`.
    fn run_controller() -> Controller {
        use naobi_core::cipher::TaggedCipher;
        use naobi_launcher::providers::run::{Program, ProgramLookup, RunSource};
        use naobi_launcher::run_history::RunHistory;
        struct Lookup;
        impl ProgramLookup for Lookup {
            fn find(&self, token: &str) -> Option<Program> {
                (token == "notepad").then(|| Program {
                    path: r"C:\Windows\notepad.exe".into(),
                    gui: true,
                })
            }
        }
        let history = Db::open_in_memory()
            .ok()
            .and_then(|db| RunHistory::open(db, Box::new(TaggedCipher), 0).ok());
        let source = RunSource::new(history, Box::new(Lookup), Box::new(|| 0));
        let db = Db::open_in_memory().unwrap_or_else(|e| panic!("db: {e}"));
        let frecency = Frecency::open(db, 0).unwrap_or_else(|e| panic!("frecency: {e}"));
        Controller::new(
            Launcher::new(vec![Box::new(source)], vec![], frecency, Box::new(Zero)),
            Router::default(),
        )
    }

    fn run_line(h: &mut Fake, line: &str) -> Controller {
        let mut c = run_controller();
        c.show(h);
        c.on_ui_event(UiEvent::TextChanged(format!(">{line}")), h);
        h.log.clear();
        c.on_ui_event(key(Key::Enter, false), h);
        c
    }

    // @R-024
    #[test]
    fn r024_enter_on_run_row_launches_console_and_hides() {
        let mut h = Fake {
            fg: Some(7),
            ..Fake::default()
        };
        run_line(&mut h, "echo hi");
        assert_eq!(
            h.log,
            vec![
                r#"launch Console("echo hi")"#.to_string(),
                "hide".to_string()
            ]
        );
    }

    // @R-024
    #[test]
    fn r024_gui_line_launches_program() {
        let mut h = Fake::default();
        run_line(&mut h, "notepad a.txt");
        assert_eq!(
            h.log,
            vec![
                format!(
                    "launch {:?}",
                    LaunchTarget::Program {
                        path: r"C:\Windows\notepad.exe".into(),
                        args: "a.txt".into()
                    }
                ),
                "hide".to_string()
            ]
        );
    }

    // @R-024
    #[test]
    fn r024_failed_console_shows_run_notice() {
        let mut h = Fake {
            fg: Some(7),
            launch_fails: true,
            ..Fake::default()
        };
        run_line(&mut h, "echo hi");
        assert!(
            h.log
                .contains(&"notice Run: PowerShell could not be started.".to_string()),
            "{:?}",
            h.log
        );
        assert!(h.log.contains(&"restore 7".to_string()), "{:?}", h.log);
    }

    // @R-024
    #[test]
    fn r024_failed_program_shows_run_notice() {
        let mut h = Fake {
            launch_fails: true,
            ..Fake::default()
        };
        run_line(&mut h, "notepad");
        assert!(h
            .log
            .contains(&"notice Run: Windows could not start that program.".to_string()));
    }

    #[test]
    fn focusing_another_window_hides_without_restoring_previous_foreground() {
        use naobi_launcher::providers::windows::{
            OpenWindow, WindowActionError, WindowOps, WindowSource,
        };
        use std::sync::Mutex;

        struct FocusOps(Arc<Mutex<Option<isize>>>);
        impl WindowOps for FocusOps {
            fn list(&self) -> Vec<OpenWindow> {
                vec![OpenWindow {
                    hwnd: 200,
                    title: "Notepad".into(),
                    process_name: "notepad.exe".into(),
                }]
            }
            fn focus(&self, hwnd: isize) -> Result<(), WindowActionError> {
                *self.0.lock().unwrap() = Some(hwnd);
                Ok(())
            }
            fn minimize(&self, _hwnd: isize) -> Result<(), WindowActionError> {
                Ok(())
            }
            fn close(&self, _hwnd: isize) -> Result<(), WindowActionError> {
                Ok(())
            }
            fn force_quit(&self, _hwnd: isize) -> Result<(), WindowActionError> {
                Ok(())
            }
        }

        struct DynamicHost {
            fg: Arc<Mutex<Option<isize>>>,
            fake: Fake,
        }
        impl Host for DynamicHost {
            fn hide(&mut self) {
                self.fake.hide();
            }
            fn show(&mut self) {
                self.fake.show();
            }
            fn is_visible(&self) -> bool {
                self.fake.is_visible()
            }
            fn foreground(&self) -> Option<isize> {
                *self.fg.lock().unwrap()
            }
            fn restore_focus(&mut self, w: isize) {
                self.fake.restore_focus(w);
            }
            fn launch(&mut self, t: &LaunchTarget) -> std::io::Result<()> {
                self.fake.launch(t)
            }
            fn copy_text(&mut self, text: &str) -> std::io::Result<()> {
                self.fake.copy_text(text)
            }
            fn notice(&mut self, n: Notice) {
                self.fake.notice(n);
            }
            fn post(&mut self, s: Screen) {
                self.fake.post(s);
            }
            fn close_later(&mut self, ms: u64) {
                self.fake.close_later(ms);
            }
            fn copy_entry(&mut self, id: EntryId) -> std::io::Result<()> {
                self.fake.copy_entry(id)
            }
            fn paste(&mut self, id: EntryId, target: Option<isize>) {
                self.fake.paste(id, target);
            }
            fn paste_text(&mut self, text: &str, target: Option<isize>) {
                self.fake.paste_text(text, target);
            }
            fn paste_files(&mut self, files: &[String], target: Option<isize>) {
                self.fake.paste_files(files, target);
            }
            fn search_files(&mut self, generation: u64, query: &str) {
                self.fake.search_files(generation, query);
            }
            fn load_icons(&mut self, keys: Vec<String>) {
                self.fake.load_icons(keys);
            }
            fn apply_theme(&mut self, id: &str) -> Result<(), String> {
                self.fake.apply_theme(id)
            }
            fn schedule_scratchpad_save(&mut self, generation: u64, ms: u64) {
                self.fake.schedule_scratchpad_save(generation, ms);
            }
        }

        let fg = Arc::new(Mutex::new(Some(77)));
        let mut c = controller_with(
            Box::new(WindowSource::new(Arc::new(FocusOps(fg.clone())))),
            Router::default(),
        );
        let mut h = DynamicHost {
            fg: fg.clone(),
            fake: Fake::default(),
        };
        c.show(&mut h);
        h.fake.log.clear();
        c.on_ui_event(key(Key::Enter, false), &mut h);
        assert_eq!(*fg.lock().unwrap(), Some(200));
        assert_eq!(h.fake.log, vec!["hide".to_string()]);
    }

    // @R-015
    #[test]
    fn r015_clipboard_plain_text_and_paste_queue_flow_through_controller() {
        use naobi_clipboard::testing::{FakeClock, TaggedCipher};
        use naobi_clipboard::view::{share, ClipboardView, VIEW_ID};
        use naobi_clipboard::{Captured, Content, Flags, History};
        use naobi_core::ids::HotkeyId;

        let clock = FakeClock::at(1_700_000_000_000);
        let cdb = Db::open_in_memory().unwrap_or_else(|e| panic!("db: {e}"));
        let shared = share(History::new(cdb, Box::new(TaggedCipher), Box::new(clock)));
        for text in ["first", "second"] {
            shared
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .on_change(Captured {
                    flags: Flags::default(),
                    marker: false,
                    owner_exe: None,
                    content: Content::Text(text.into()),
                })
                .unwrap_or_else(|e| panic!("capture: {e}"));
        }
        let ids: Vec<i64> = shared
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .list(10)
            .unwrap_or_default()
            .into_iter()
            .map(|e| e.id)
            .collect();
        let (second_id, first_id) = (ids[0], ids[1]);

        let fdb = Db::open_in_memory().unwrap_or_else(|e| panic!("db: {e}"));
        let frecency = Frecency::open(fdb, 0).unwrap_or_else(|e| panic!("frecency: {e}"));
        let launcher = Launcher::new(
            vec![],
            vec![Box::new(ClipboardView::new(shared))],
            frecency,
            Box::new(Zero),
        );
        let mut m = crate::modules::Modules::new();
        m.subscribe(
            HotkeyId("clipboard".into()),
            Box::new(|_, cx| cx.open_view(ViewId(VIEW_ID.into()))),
        );
        let crate::modules::Parts { router, .. } = m.into_parts();
        let mut c = Controller::new(launcher, router);
        let mut h = Fake {
            fg: Some(77),
            ..Fake::default()
        };

        assert!(c.on_hotkey(&HotkeyId("clipboard".into()), &mut h));
        h.log.clear();

        // Ctrl+Shift+Enter on selected ("second") pastes into Paste target after hiding.
        c.on_ui_event(
            UiEvent::Key(KeyChord {
                key: Key::Enter,
                ctrl: true,
                alt: false,
                shift: true,
            }),
            &mut h,
        );
        assert_eq!(
            h.log,
            vec!["hide".to_string(), format!("paste {second_id} Some(77)")]
        );

        // Re-open, queue "first" then "second", then paste next twice.
        assert!(c.on_hotkey(&HotkeyId("clipboard".into()), &mut h));
        h.log.clear();
        c.on_ui_event(key(Key::Down, false), &mut h);
        c.on_ui_event(key(Key::K, true), &mut h);
        c.on_ui_event(UiEvent::PanelFilter("Add to paste queue".into()), &mut h);
        c.on_ui_event(key(Key::Enter, false), &mut h);
        c.on_ui_event(key(Key::Up, false), &mut h);
        c.on_ui_event(key(Key::K, true), &mut h);
        c.on_ui_event(UiEvent::PanelFilter("Add to paste queue".into()), &mut h);
        c.on_ui_event(key(Key::Enter, false), &mut h);
        assert!(h.visible, "adding to paste queue keeps clipboard view open");
        assert!(h.log.is_empty());

        c.on_ui_event(key(Key::K, true), &mut h);
        c.on_ui_event(UiEvent::PanelFilter("Paste next in queue".into()), &mut h);
        c.on_ui_event(key(Key::Enter, false), &mut h);
        assert_eq!(
            h.log,
            vec!["hide".to_string(), format!("paste {first_id} Some(77)")]
        );

        assert!(c.on_hotkey(&HotkeyId("clipboard".into()), &mut h));
        h.log.clear();
        c.on_ui_event(key(Key::K, true), &mut h);
        c.on_ui_event(UiEvent::PanelFilter("Paste next in queue".into()), &mut h);
        c.on_ui_event(key(Key::Enter, false), &mut h);
        assert_eq!(
            h.log,
            vec!["hide".to_string(), format!("paste {second_id} Some(77)")]
        );
    }

    #[test]
    fn emoji_picker_enter_pastes_text_after_hide_and_ctrl_enter_copies() {
        use naobi_core::ids::HotkeyId;
        use naobi_launcher::providers::emoji::{EmojiSource, EmojiView, VIEW_ID};

        let fdb = Db::open_in_memory().unwrap_or_else(|e| panic!("db: {e}"));
        let frecency = Frecency::open(fdb, 0).unwrap_or_else(|e| panic!("frecency: {e}"));
        let launcher = Launcher::new(
            vec![Box::new(EmojiSource)],
            vec![Box::new(EmojiView)],
            frecency,
            Box::new(Zero),
        );
        let mut m = crate::modules::Modules::new();
        m.subscribe(
            HotkeyId("emoji".into()),
            Box::new(|_, cx| cx.open_view(ViewId(VIEW_ID.into()))),
        );
        let crate::modules::Parts { router, .. } = m.into_parts();
        let mut c = Controller::new(launcher, router);
        let mut h = Fake {
            fg: Some(77),
            ..Fake::default()
        };

        c.show(&mut h);
        h.log.clear();
        c.on_ui_event(UiEvent::TextChanged(":fire".into()), &mut h);
        c.on_ui_event(key(Key::Enter, false), &mut h);
        assert_eq!(
            h.log,
            vec!["hide".to_string(), "paste_text 🔥 Some(77)".to_string()]
        );

        assert!(c.on_hotkey(&HotkeyId("emoji".into()), &mut h));
        assert!(h.visible);
        h.log.clear();
        c.on_ui_event(UiEvent::TextChanged("rocket".into()), &mut h);
        c.on_ui_event(key(Key::Enter, true), &mut h);
        assert_eq!(
            h.log,
            vec!["copy 🚀".to_string(), "close_later 120".to_string()]
        );
        assert_eq!(
            h.last_post
                .as_ref()
                .and_then(|s| s.footer.confirmation.as_deref()),
            Some("Copied")
        );
        h.log.clear();
        c.finish_close(&mut h);
        assert_eq!(h.log, vec!["hide".to_string(), "restore 77".to_string()]);
    }

    #[test]
    fn paste_text_runs_after_the_launcher_is_hidden_and_does_not_restore_focus() {
        let mut c = controller_with(
            Box::new(One(Outcome::Effects(vec![Effect::PasteText(
                "expanded snippet".into(),
            )]))),
            Router::default(),
        );
        let mut h = Fake {
            fg: Some(77),
            ..Fake::default()
        };
        c.show(&mut h);
        h.log.clear();
        c.on_ui_event(key(Key::Enter, false), &mut h);
        assert_eq!(
            h.log,
            vec![
                "hide".to_string(),
                "paste_text expanded snippet Some(77)".to_string()
            ]
        );
    }

    // @R-029
    #[test]
    fn r029_scratchpad_autosave_esc_copy_all_and_armed_clear_flow_through_controller() {
        use naobi_core::ids::HotkeyId;
        use naobi_launcher::providers::scratchpad::{
            FileScratchpadStore, ScratchpadCommands, ScratchpadView, VIEW_ID,
        };
        use std::sync::Arc;

        let dir = std::env::temp_dir().join(format!(
            "naobi-controller-scratchpad-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("scratchpad.md");

        let fdb = Db::open_in_memory().unwrap_or_else(|e| panic!("db: {e}"));
        let frecency = Frecency::open(fdb, 0).unwrap_or_else(|e| panic!("frecency: {e}"));
        let launcher = Launcher::new(
            vec![Box::new(ScratchpadCommands)],
            vec![Box::new(ScratchpadView::new(Arc::new(
                FileScratchpadStore::new(path.clone()),
            )))],
            frecency,
            Box::new(Zero),
        );
        let mut m = crate::modules::Modules::new();
        m.subscribe(
            HotkeyId("scratchpad".into()),
            Box::new(|_, cx| cx.open_view(ViewId(VIEW_ID.into()))),
        );
        let crate::modules::Parts { router, .. } = m.into_parts();
        let mut c = Controller::new(launcher, router);
        let mut h = Fake {
            fg: Some(77),
            ..Fake::default()
        };
        // 1) Open via optional hotkey -> shows single Launcher window in Scratchpad editor mode.
        assert!(c.on_hotkey(&HotkeyId("scratchpad".into()), &mut h));
        assert!(h.visible);
        assert_eq!(
            h.last_post.as_ref().and_then(|s| s.editor.as_deref()),
            Some("")
        );
        h.log.clear();

        // 2) Typing schedules debounced 500 ms save; stale timer is ignored, latest persists.
        c.on_ui_event(UiEvent::ScratchpadChanged("draft 1".into()), &mut h);
        c.on_ui_event(
            UiEvent::ScratchpadChanged("draft 1\ndraft 2".into()),
            &mut h,
        );
        let scheduled: Vec<u64> = h
            .log
            .iter()
            .filter_map(|line| {
                let rest = line.strip_prefix("save_later ")?;
                rest.split_whitespace().next()?.parse::<u64>().ok()
            })
            .collect();
        assert_eq!(scheduled.len(), 2);
        c.on_scratchpad_save_timer(scheduled[0], &mut h);
        assert!(!path.exists(), "stale timer must not save");
        c.on_scratchpad_save_timer(scheduled[1], &mut h);
        assert_eq!(
            std::fs::read_to_string(&path).ok().as_deref(),
            Some("draft 1\ndraft 2")
        );

        // 3) Esc saves immediate edit and hides hotkey-opened view with focus restore.
        h.log.clear();
        c.on_ui_event(UiEvent::ScratchpadChanged("saved on esc".into()), &mut h);
        c.on_ui_event(key(Key::Escape, false), &mut h);
        assert_eq!(h.log.last().map(String::as_str), Some("restore 77"));
        assert!(!h.visible);
        assert_eq!(
            std::fs::read_to_string(&path).ok().as_deref(),
            Some("saved on esc")
        );

        // 4) Re-open from root search (pushed view): Copy all copies full text, Armed Clear needs 2 Enters.
        c.show(&mut h);
        c.on_ui_event(UiEvent::TextChanged("scratchpad".into()), &mut h);
        c.on_ui_event(key(Key::Enter, false), &mut h);
        assert_eq!(
            h.last_post.as_ref().and_then(|s| s.editor.as_deref()),
            Some("saved on esc")
        );
        h.log.clear();
        c.on_ui_event(key(Key::K, true), &mut h);
        c.on_ui_event(key(Key::Enter, false), &mut h);
        assert!(h.log.contains(&"copy saved on esc".to_string()));
        assert!(h.log.contains(&"close_later 120".to_string()));
        c.finish_close(&mut h);

        // Armed Clear scratchpad requires 2 Enters and clears both buffer and scratchpad.md.
        c.show(&mut h);
        c.on_ui_event(UiEvent::TextChanged("scratchpad".into()), &mut h);
        c.on_ui_event(key(Key::Enter, false), &mut h);
        c.on_ui_event(key(Key::K, true), &mut h);
        c.on_ui_event(key(Key::Down, false), &mut h);
        c.on_ui_event(key(Key::Enter, false), &mut h);
        assert_eq!(
            std::fs::read_to_string(&path).ok().as_deref(),
            Some("saved on esc")
        );
        c.on_ui_event(key(Key::Enter, false), &mut h);
        assert_eq!(std::fs::read_to_string(&path).ok().as_deref(), Some(""));
        assert_eq!(
            h.last_post.as_ref().and_then(|s| s.editor.as_deref()),
            Some("")
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn t27_item_hotkey_executes_direct_silently_and_opens_launcher_for_view_or_param() {
        use crate::modules::Modules;
        use naobi_core::command::CommandSource;
        use naobi_core::item::{Item, ItemKind};
        use naobi_core::view_source::{Preview, ViewSource};
        use naobi_launcher::providers::quicklink::{QuicklinkSet, QuicklinkSource};

        struct DirectCmd;
        impl CommandSource for DirectCmd {
            fn items(&self, _q: &Query) -> Arc<[Item]> {
                Arc::from([
                    Item {
                        id: ItemId("fake.open".into()),
                        title: "Lock Screen".into(),
                        subtitle: String::new(),
                        keywords: vec![],
                        alias: None,
                        kind: ItemKind::Command,
                        primary: ActionId("run".into()),
                        secondary: None,
                    },
                    Item {
                        id: ItemId("fake.view".into()),
                        title: "Open Clips".into(),
                        subtitle: String::new(),
                        keywords: vec![],
                        alias: None,
                        kind: ItemKind::Command,
                        primary: ActionId("open".into()),
                        secondary: None,
                    },
                ])
            }
            fn actions(&self, _id: &ItemId) -> Vec<Action> {
                vec![]
            }
            fn run(&self, id: &ItemId, _action: ActionId, _cx: &mut Cx) -> Outcome {
                if id.0 == "fake.view" {
                    Outcome::Effects(vec![Effect::PushView(ViewId("clips".into()))])
                } else {
                    Outcome::Effects(vec![
                        Effect::Launch(LaunchTarget::ShellItem("notepad.exe".into())),
                        Effect::Hide,
                    ])
                }
            }
        }

        struct ClipsView;
        impl CommandSource for ClipsView {
            fn items(&self, _q: &Query) -> Arc<[Item]> {
                Arc::new([])
            }
            fn actions(&self, _id: &ItemId) -> Vec<Action> {
                vec![]
            }
            fn run(&self, _id: &ItemId, _a: ActionId, _cx: &mut Cx) -> Outcome {
                Outcome::Done
            }
        }
        impl ViewSource for ClipsView {
            fn view_id(&self) -> ViewId {
                ViewId("clips".into())
            }
            fn title(&self) -> &'static str {
                "Clips"
            }
            fn opened_by_hotkey(&self) -> bool {
                false
            }
            fn preview(&self, _id: &ItemId) -> Option<Preview> {
                None
            }
        }

        let links = QuicklinkSet::default();
        links.set(vec![naobi_core::quicklink::Quicklink {
            name: "Google".into(),
            alias: "g".into(),
            url: "https://example.com/?q={query}".into(),
        }]);
        let mut reg = Modules::new();
        reg.register_source(Box::new(DirectCmd));
        reg.register_source(Box::new(QuicklinkSource::new(links)));
        reg.register_view(Box::new(ClipsView));
        let parts = reg.into_parts();
        let db = Db::open_in_memory().unwrap_or_else(|e| panic!("db: {e}"));
        let frecency = Frecency::open(db, 0).unwrap_or_else(|e| panic!("frecency: {e}"));
        let mut c = Controller::new(
            Launcher::new(parts.sources, parts.views, frecency, Box::new(Zero)),
            parts.router,
        );
        let mut h = Fake {
            fg: Some(77),
            ..Default::default()
        };

        // 1) Non-parameterized item (`fake.open`) launches silently without showing Launcher.
        assert!(c.on_hotkey(&HotkeyId("item:fake.open".into()), &mut h));
        assert!(!h.visible, "silent direct launch keeps Launcher hidden");
        assert!(h.log.iter().any(|l| l.starts_with("launch ")));

        // 2) View opener (`fake.view`) opens Launcher straight into its View.
        h.log.clear();
        assert!(c.on_hotkey(&HotkeyId("item:fake.view".into()), &mut h));
        assert!(h.visible);
        assert_eq!(
            h.last_post.as_ref().map(|s| s.view.name.as_str()),
            Some("Clips")
        );

        // Hide again, then trigger parameterized quicklink (`ql:g`): opens Launcher prefilled with "g ".
        h.visible = false;
        h.log.clear();
        assert!(c.on_hotkey(&HotkeyId("item:ql:g".into()), &mut h));
        assert!(h.visible);
        assert_eq!(h.last_post.as_ref().map(|s| s.query.as_str()), Some("g "));

        // Unknown ItemId returns false without showing Launcher.
        h.visible = false;
        assert!(!c.on_hotkey(&HotkeyId("item:missing.id".into()), &mut h));
        assert!(!h.visible);
    }
}
