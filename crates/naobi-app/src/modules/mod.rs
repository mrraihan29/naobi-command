//! The module registry (REPORT 3.4): where each module plugs into the controller. A module item adds
//! one line to `register_all` and never edits `controller.rs`.
pub mod apps;
pub mod builtin;
pub mod clipboard;
pub mod clipboard_capture;

use naobi_core::command::{CommandSource, Cx};
use naobi_core::event::AppEvent;
use naobi_core::ids::HotkeyId;
use naobi_core::view_source::ViewSource;

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
// seam NCMD-17, 21 to 24: files, snap, topmost, awake and picker register here.
pub fn register_all(modules: &mut Modules) {
    builtin::register(modules);
    clipboard_capture::register(modules);
    clipboard::register(modules);
    apps::register(modules);
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
                hk("snap_left"),
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
        assert!(router.hotkey(&hk("snap_left"), &mut cx));
        assert_eq!(cx.take().len(), 2, "both handlers raised a Notice");
        assert!(!router.hotkey(&hk("nobody"), &mut cx));
        assert!(router.event(&AppEvent::ClipboardChanged(7), &mut cx));
        // Nobody subscribed to these kinds, and the Launcher's own events are not for modules.
        assert!(!router.event(&AppEvent::Timer(TimerId(1)), &mut cx));
        assert!(!router.event(&AppEvent::ThemeChanged, &mut cx));
        assert!(!router.event(&AppEvent::Hotkey(hk("snap_left")), &mut cx));
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
