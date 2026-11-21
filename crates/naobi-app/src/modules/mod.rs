//! The module registry (REPORT 3.4): where each module plugs into the controller. A module item adds
//! one line to `register_all` and never edits `controller.rs`.
pub mod aot;
pub mod apps;
pub mod awake;
pub mod builtin;
pub mod clipboard;
pub mod clipboard_capture;
pub mod emoji;
pub mod files;
pub mod icons;
pub mod picker;
pub mod themes;
pub mod windows;

use naobi_core::command::{CommandSource, Cx};
use naobi_core::event::AppEvent;
use naobi_core::ids::HotkeyId;
use naobi_core::view_source::ViewSource;

/// Puts a transient child in the app's kill-on-close job, so it cannot outlive the app (a crash, a kill,
/// a child that ignores its closed stdin). A refusal is logged and the child runs as before.
pub fn keep_in_job(child: &std::process::Child) {
    static JOB: std::sync::OnceLock<Option<naobi_win32::job::Job>> = std::sync::OnceLock::new();
    let job = JOB.get_or_init(|| naobi_win32::job::Job::kill_on_close().ok());
    if !job.as_ref().is_some_and(|j| j.assign(child)) {
        tracing::warn!("a child process could not be put in the job object");
    }
}

/// The `AppEvent`s a module may subscribe to. Hotkeys have their own `subscribe`; the Launcher's own
/// hotkey, `ShowRequested`, theme changes and conflicts are the controller's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventKind {
    ClipboardChanged,
    Resume,
    DisplayChanged,
    SettingsChanged,
    Timer,
}

impl EventKind {
    pub fn of(ev: &AppEvent) -> Option<Self> {
        match ev {
            AppEvent::ClipboardChanged(_) => Some(Self::ClipboardChanged),
            AppEvent::Resume => Some(Self::Resume),
            AppEvent::DisplayChanged => Some(Self::DisplayChanged),
            AppEvent::SettingsChanged => Some(Self::SettingsChanged),
            AppEvent::Timer(_) => Some(Self::Timer),
            _ => None,
        }
    }
}

/// A handler runs on the controller's thread. It raises its Notices, launches and copies through
/// `Cx`, and the controller executes what it collected.
pub type Handler = Box<dyn FnMut(&AppEvent, &mut Cx)>;

#[derive(Default)]
pub struct Modules {
    sources: Vec<Box<dyn CommandSource>>,
    views: Vec<Box<dyn ViewSource>>,
    hotkeys: Vec<(HotkeyId, Handler)>,
    events: Vec<(EventKind, Handler)>,
}

// The module items (NCMD-16 to 24) are the callers of the registration methods; until the first of
// them lands only the tests call them.
#[allow(dead_code)]
impl Modules {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a root source: its items are searched from the root view.
    pub fn register_source(&mut self, source: Box<dyn CommandSource>) {
        self.sources.push(source);
    }

    /// Adds a view the Launcher can push, or open by its own hotkey.
    pub fn register_view(&mut self, view: Box<dyn ViewSource>) {
        self.views.push(view);
    }

    /// Runs `handler` when the hotkey `id` is pressed.
    pub fn subscribe(&mut self, id: HotkeyId, handler: Handler) {
        self.hotkeys.push((id, handler));
    }

    /// Runs `handler` for every event of `kind`.
    pub fn subscribe_event(&mut self, kind: EventKind, handler: Handler) {
        self.events.push((kind, handler));
    }

    /// Splits the registry: the sources and views go to the engine, the handlers stay with the
    /// controller.
    pub fn into_parts(self) -> Parts {
        Parts {
            sources: self.sources,
            views: self.views,
            router: Router {
                hotkeys: self.hotkeys,
                events: self.events,
            },
        }
    }
}

pub struct Parts {
    pub sources: Vec<Box<dyn CommandSource>>,
    pub views: Vec<Box<dyn ViewSource>>,
    pub router: Router,
}

/// Delivers hotkeys and events to their subscribers, in registration order.
#[derive(Default)]
pub struct Router {
    hotkeys: Vec<(HotkeyId, Handler)>,
    events: Vec<(EventKind, Handler)>,
}

impl Router {
    /// Runs the handlers of `id`. False when nobody subscribed to it.
    pub fn hotkey(&mut self, id: &HotkeyId, cx: &mut Cx) -> bool {
        let ev = AppEvent::Hotkey(id.clone());
        let mut any = false;
        for (h, f) in &mut self.hotkeys {
            if h == id {
                any = true;
                f(&ev, cx);
            }
        }
        any
    }

    /// Runs the handlers of the event's kind. False when nobody subscribed to it.
    pub fn event(&mut self, ev: &AppEvent, cx: &mut Cx) -> bool {
        let Some(kind) = EventKind::of(ev) else {
            return false;
        };
        let mut any = false;
        for (k, f) in &mut self.events {
            if *k == kind {
                any = true;
                f(ev, cx);
            }
        }
        any
    }
}

/// One line per module. The module items add theirs here.
// seam NCMD-17, 24: files and picker register here.
pub fn register_all(modules: &mut Modules) {
    builtin::register(modules);
    clipboard_capture::register(modules);
    clipboard::register(modules);
    apps::register(modules);
    aot::register(modules);
    awake::register(modules);
    picker::register(modules);
    files::register(modules);
    themes::register(modules);
    windows::register(modules);
    emoji::register(modules);
}

#[cfg(test)]
mod tests {
    use super::*;
    use naobi_core::ids::{ActionId, ItemId, TimerId};
    use naobi_core::item::{Action, Item};
    use naobi_core::notice::{Notice, NoticeKind};
    use std::cell::RefCell;
    use std::rc::Rc;

    struct Empty;
    impl CommandSource for Empty {
        fn items(&self, _q: &naobi_core::command::Query) -> std::sync::Arc<[Item]> {
            std::sync::Arc::from(Vec::new())
        }
        fn actions(&self, _id: &ItemId) -> Vec<Action> {
            vec![]
        }
        fn run(&self, _id: &ItemId, _a: ActionId, _cx: &mut Cx) -> naobi_core::command::Outcome {
            naobi_core::command::Outcome::Done
        }
    }

    fn hk(s: &str) -> HotkeyId {
        HotkeyId(s.into())
    }

    fn note(m: &str) -> Notice {
        Notice {
            subject: "t".into(),
            message: m.into(),
            kind: NoticeKind::Info,
        }
    }

    // @R-004
    #[test]
    fn d021_registry_routes_hotkey_and_event_to_subscribers() {
        let seen = Rc::new(RefCell::new(Vec::<String>::new()));
        let mut m = Modules::new();
        m.register_source(Box::new(Empty));
        for name in ["a", "b"] {
            let seen = seen.clone();
            m.subscribe(
                hk("clipboard"),
                Box::new(move |_, cx| {
                    seen.borrow_mut().push(format!("hotkey {name}"));
                    cx.notice(note(name));
                }),
            );
        }
        let s2 = seen.clone();
        m.subscribe(
            hk("topmost"),
            Box::new(move |_, _| s2.borrow_mut().push("topmost".into())),
        );
        let s3 = seen.clone();
        m.subscribe_event(
            EventKind::ClipboardChanged,
            Box::new(move |ev, _| s3.borrow_mut().push(format!("{ev:?}"))),
        );
        let Parts {
            sources,
            views,
            mut router,
        } = m.into_parts();
        assert_eq!((sources.len(), views.len()), (1, 0));

        let mut cx = Cx::default();
        assert!(router.hotkey(&hk("clipboard"), &mut cx));
        assert_eq!(cx.take().len(), 2, "both handlers raised a Notice");
        assert!(!router.hotkey(&hk("nobody"), &mut cx));
        assert!(router.event(&AppEvent::ClipboardChanged(7), &mut cx));
        // Nobody subscribed to these kinds, and the Launcher's own events are not for modules.
        assert!(!router.event(&AppEvent::Timer(TimerId(1)), &mut cx));
        assert!(!router.event(&AppEvent::ThemeChanged, &mut cx));
        assert!(!router.event(&AppEvent::Hotkey(hk("clipboard")), &mut cx));
        assert_eq!(
            *seen.borrow(),
            vec![
                "hotkey a".to_string(),
                "hotkey b".to_string(),
                "ClipboardChanged(7)".to_string()
            ]
        );
    }

    #[test]
    fn event_kinds_cover_the_module_events_only() {
        assert_eq!(EventKind::of(&AppEvent::Resume), Some(EventKind::Resume));
        assert_eq!(
            EventKind::of(&AppEvent::SettingsChanged),
            Some(EventKind::SettingsChanged)
        );
        assert_eq!(
            EventKind::of(&AppEvent::DisplayChanged),
            Some(EventKind::DisplayChanged)
        );
        assert_eq!(EventKind::of(&AppEvent::ShowRequested), None);
        assert_eq!(EventKind::of(&AppEvent::HotkeyConflict(hk("x"))), None);
    }
}

/// R-004: the whole registry, built the way `run` builds it but in a throwaway data folder. Returns
/// every item `(id, title)` the sources and views offer for a spread of queries, the view ids and the
/// hotkey ids.
#[cfg(test)]
mod catalog {
    use super::*;
    use naobi_core::command::Query;
    use std::collections::BTreeSet;

    struct Catalog {
        items: BTreeSet<(String, String)>,
        views: BTreeSet<String>,
        hotkeys: BTreeSet<String>,
    }

    fn build() -> Catalog {
        let dir = std::env::temp_dir().join(format!("naobi-catalog-{}", std::process::id()));
        std::env::set_var("NAOBI_DATA_DIR", dir.join("data"));
        std::env::set_var("NAOBI_SETTINGS_DIR", dir.join("settings"));
        // The perf build keeps clipboard capture off unless a test asks for it.
        std::env::set_var("NAOBI_TEST_CLIPBOARD", "1");
        let mut m = Modules::new();
        register_all(&mut m);
        let parts = m.into_parts();
        let mut items = BTreeSet::new();
        let sources = parts
            .sources
            .iter()
            .map(|s| &**s)
            .chain(parts.views.iter().map(|v| v.as_ref() as &dyn CommandSource));
        for source in sources {
            for text in [
                "", "2+2", "awake", "color", "lock", "clip", "snippet", "ocr",
            ] {
                for i in source.items(&Query { text: text.into() }).iter() {
                    if !matches!(
                        i.kind,
                        naobi_core::item::ItemKind::App
                            | naobi_core::item::ItemKind::File
                            | naobi_core::item::ItemKind::Window
                    ) {
                        items.insert((i.id.0.clone(), i.title.clone()));
                    }
                }
            }
        }
        let views = parts.views.iter().map(|v| v.view_id().0).collect();
        let hotkeys = parts
            .router
            .hotkeys
            .iter()
            .map(|(h, _)| h.0.clone())
            .collect();
        let _ = std::fs::remove_dir_all(dir);
        Catalog {
            items,
            views,
            hotkeys,
        }
    }

    // @R-004
    #[test]
    fn r004_module_commands_found_by_name() {
        use naobi_core::item::ItemKind;
        use naobi_launcher::rank::{Ranker, Source};
        let dir = std::env::temp_dir().join(format!("naobi-names-{}", std::process::id()));
        std::env::set_var("NAOBI_DATA_DIR", dir.join("data"));
        std::env::set_var("NAOBI_SETTINGS_DIR", dir.join("settings"));
        std::env::set_var("NAOBI_TEST_CLIPBOARD", "1");
        let mut m = Modules::new();
        register_all(&mut m);
        let parts = m.into_parts();
        // Root search ranks root sources only; pushed views are searched only after opening them.
        let sources: Vec<&dyn CommandSource> = parts.sources.iter().map(|s| &**s).collect();
        let table: [(&str, &str); 32] = [
            ("color picker", "picker.pick"),
            ("picker", "picker.pick"),
            ("eyedropper", "picker.pick"),
            ("pick color", "picker.pick"),
            ("color history", "picker.history"),
            ("paste history", "clipboard.open"),
            ("clipboard", "clipboard.open"),
            ("switch windows", "win.open"),
            ("refresh currency rates", "calc.refresh_rates"),
            ("emoji", "emoji.open"),
            ("symbols", "emoji.open"),
            ("shutdown", "sys.shut_down"),
            ("power off", "sys.shut_down"),
            ("reboot", "sys.restart"),
            ("log off", "sys.sign_out"),
            ("dark mode", "sys.toggle_dark_mode"),
            ("lock", "sys.lock"),
            ("sleep displays", "sys.sleep_displays"),
            ("toggle mute", "sys.toggle_mute"),
            ("volume 0", "sys.volume_0"),
            ("volume 25", "sys.volume_25"),
            ("volume 50", "sys.volume_50"),
            ("volume 75", "sys.volume_75"),
            ("volume 100", "sys.volume_100"),
            ("copy local ip", "sys.copy_local_ip"),
            ("night light", "sys.open_night_light_settings"),
            ("sound settings", "sys.open_sound_settings"),
            ("wi-fi", "sys.open_wifi_settings"),
            ("bluetooth", "sys.open_bluetooth_settings"),
            ("recycle bin", "sys.empty_recycle_bin"),
            ("always on top", "aot.toggle"),
            ("topmost", "aot.toggle"),
        ];
        let first = |q: &str| {
            // Apps, files and live open windows depend on the Owner's machine; leave them out.
            let pool: Vec<_> = sources
                .iter()
                .flat_map(|s| s.items(&Query { text: q.into() }).to_vec())
                .filter(|i| !matches!(i.kind, ItemKind::App | ItemKind::File | ItemKind::Window))
                .collect();
            let hits =
                Ranker::default().rank(q, pool.iter().map(|i| (Source::Root(0), i)), |_| 0.0, 50);
            hits.first()
                .map(|h| h.item.id.0.clone())
                .unwrap_or_default()
        };
        for (q, id) in table {
            assert_eq!(first(q), id, "query {q:?}");
        }
        assert!(first("awake").starts_with("awake."), "query awake");
        let _ = std::fs::remove_dir_all(dir);
    }

    // @R-004
    #[test]
    fn r004_every_module_reachable_and_no_phase2_command() {
        let c = build();
        let item = |id: &str| c.items.iter().any(|(i, _)| i == id);
        let hotkey = |id: &str| c.hotkeys.contains(id);
        // The six modules, each by one thing the Owner can reach: a Launcher row or its own hotkey.
        assert!(item("sys.lock"), "launcher: system commands");
        assert!(item("calc:4"), "calculator");
        assert!(
            hotkey("clipboard") && c.views.contains(naobi_clipboard::view::VIEW_ID),
            "clipboard history"
        );
        assert!(
            item("picker.pick") && hotkey("color_picker") && c.views.contains("colors"),
            "color picker"
        );
        assert!(item("aot.toggle") && hotkey("topmost"), "always on top");
        assert!(item("awake.on"), "awake");
        assert!(item("calc.refresh_rates"), "refresh currency rates");
        assert!(naobi_ui::has_own_glyph("dev"), "dev micro-tools glyph");
        assert!(
            item("win.open")
                && c.views
                    .contains(naobi_launcher::providers::windows::VIEW_ID),
            "window switcher"
        );
        assert!(
            naobi_ui::has_own_glyph("calc.refresh_rates"),
            "refresh currency rates glyph"
        );
        assert!(
            item("emoji.open") && c.views.contains(naobi_launcher::providers::emoji::VIEW_ID),
            "emoji picker"
        );
        // Every module command has a drawn icon of its own (NCMD-29), not the generic one.
        for (id, _) in &c.items {
            let module = [
                "sys.",
                "win.",
                "awake.",
                "aot.",
                "picker.",
                "clipboard.",
                "emoji.",
            ];
            if module.iter().any(|p| id.starts_with(p)) {
                assert!(naobi_ui::has_own_glyph(id), "no glyph for {id}");
            }
        }
        // Phase 2 (R-004): out-of-scope items remain absent from the build, by id or by title.
        let phase2 = [
            "snippet",
            "ocr",
            "extractor",
            "remap",
            "powerrename",
            "rename",
            "hosts",
            "peek",
            "resizer",
        ];
        let named = c
            .items
            .iter()
            .flat_map(|(i, t)| [i.as_str(), t.as_str()])
            .chain(c.views.iter().map(String::as_str))
            .chain(c.hotkeys.iter().map(String::as_str));
        for name in named {
            let lower = name.to_ascii_lowercase();
            assert!(
                !phase2.iter().any(|p| lower.contains(p)),
                "a phase-2 name in the MVP build: {name}"
            );
        }
    }
}
