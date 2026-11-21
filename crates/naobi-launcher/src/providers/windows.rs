//! Window switcher: open desktop windows in Root search and the `windows` View.
use naobi_core::command::{CommandSource, Cx, Outcome, Query};
use naobi_core::effect::Effect;
use naobi_core::ids::{ActionId, ItemId, ViewId};
use naobi_core::item::{Action, Item, ItemKind};
use naobi_core::notice::{Notice, NoticeKind};
use naobi_core::view_source::{Preview, ViewSource};
use std::sync::Arc;

pub const ID_PREFIX: &str = "win:";
pub const VIEW_ID: &str = "windows";
pub const OPEN_CMD_ID: &str = "win.open";

/// One visible, non-cloaked top-level Open window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenWindow {
    pub hwnd: isize,
    pub title: String,
    pub process_name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowActionError {
    /// Windows UIPI blocks control because the target runs at higher integrity (R-011).
    Elevated,
    /// Windows refused the operation or the window closed.
    Refused,
}

/// Port for enumerating and controlling Open windows. Production impl lives in `naobi-win32`.
pub trait WindowOps: Send + Sync {
    fn list(&self) -> Vec<OpenWindow>;
    fn focus(&self, hwnd: isize) -> Result<(), WindowActionError>;
    fn minimize(&self, hwnd: isize) -> Result<(), WindowActionError>;
    fn close(&self, hwnd: isize) -> Result<(), WindowActionError>;
    fn force_quit(&self, hwnd: isize) -> Result<(), WindowActionError>;
}

fn hwnd_of(id: &ItemId) -> Option<isize> {
    id.0.strip_prefix(ID_PREFIX)?.parse().ok()
}

fn window_item(w: OpenWindow) -> Item {
    let keywords = if w.process_name.is_empty() {
        Vec::new()
    } else {
        vec![w.process_name.clone()]
    };
    Item {
        id: ItemId(format!("{ID_PREFIX}{}", w.hwnd)),
        title: w.title,
        subtitle: w.process_name,
        keywords,
        alias: None,
        kind: ItemKind::Window,
        primary: ActionId("focus".into()),
        secondary: None,
    }
}

fn window_actions() -> Vec<Action> {
    let act = |id: &str, label: &str| Action {
        id: ActionId(id.into()),
        label: label.into(),
        shortcut: None,
        enabled: Ok(()),
    };
    vec![
        act("focus", "Focus"),
        act("minimize", "Minimize"),
        act("close", "Close"),
        act("force_quit", "Force Quit"),
    ]
}

fn action_verb(action: &str) -> &'static str {
    match action {
        "focus" => "focus",
        "minimize" => "minimize",
        "close" => "close",
        "force_quit" => "force quit",
        _ => "control",
    }
}

fn run_window_action(ops: &dyn WindowOps, id: &ItemId, action: &ActionId) -> Outcome {
    let Some(hwnd) = hwnd_of(id) else {
        return Outcome::Done;
    };
    let res = match action.0.as_str() {
        "focus" => ops.focus(hwnd),
        "minimize" => ops.minimize(hwnd),
        "close" => ops.close(hwnd),
        "force_quit" => ops.force_quit(hwnd),
        _ => return Outcome::Done,
    };
    match res {
        Ok(()) => Outcome::Done,
        Err(err) => {
            let title = ops
                .list()
                .into_iter()
                .find(|w| w.hwnd == hwnd)
                .map(|w| w.title)
                .filter(|t| !t.is_empty())
                .unwrap_or_else(|| "this window".to_owned());
            let message = match err {
                WindowActionError::Elevated => "it runs as administrator",
                WindowActionError::Refused => "Windows refused the command",
            };
            Outcome::Failed(Notice {
                subject: format!("Cannot {} {title}", action_verb(&action.0)),
                message: message.to_owned(),
                kind: NoticeKind::Warning,
            })
        }
    }
}

/// Exposes Open windows as searchable Items in Root search.
pub struct WindowSource {
    ops: Arc<dyn WindowOps>,
}

impl WindowSource {
    pub fn new(ops: Arc<dyn WindowOps>) -> Self {
        Self { ops }
    }
}

impl CommandSource for WindowSource {
    fn items(&self, _q: &Query) -> Arc<[Item]> {
        self.ops.list().into_iter().map(window_item).collect()
    }

    fn needs_confirm(&self, _id: &ItemId, action: &ActionId) -> bool {
        action.0 == "force_quit"
    }

    fn actions(&self, _id: &ItemId) -> Vec<Action> {
        window_actions()
    }

    fn run(&self, id: &ItemId, action: ActionId, _cx: &mut Cx) -> Outcome {
        run_window_action(&*self.ops, id, &action)
    }
}

/// Root command to open the `windows` View.
pub struct WindowCommands;

impl CommandSource for WindowCommands {
    fn items(&self, _q: &Query) -> Arc<[Item]> {
        vec![Item {
            id: ItemId(OPEN_CMD_ID.into()),
            title: "Switch Windows".into(),
            subtitle: String::new(),
            keywords: vec![
                "windows".into(),
                "open windows".into(),
                "window switcher".into(),
            ],
            alias: None,
            kind: ItemKind::Command,
            primary: ActionId("run".into()),
            secondary: None,
        }]
        .into()
    }

    fn actions(&self, _id: &ItemId) -> Vec<Action> {
        vec![Action {
            id: ActionId("run".into()),
            label: "Open".into(),
            shortcut: None,
            enabled: Ok(()),
        }]
    }

    fn run(&self, id: &ItemId, _action: ActionId, _cx: &mut Cx) -> Outcome {
        if id.0 == OPEN_CMD_ID {
            Outcome::Effects(vec![Effect::PushView(ViewId(VIEW_ID.into()))])
        } else {
            Outcome::Done
        }
    }
}

/// Dedicated `windows` View preserving OS Z-order and filtering by title or process name.
pub struct WindowsView {
    ops: Arc<dyn WindowOps>,
}

impl WindowsView {
    pub fn new(ops: Arc<dyn WindowOps>) -> Self {
        Self { ops }
    }
}

impl CommandSource for WindowsView {
    fn items(&self, q: &Query) -> Arc<[Item]> {
        let needle = q.text.trim().to_lowercase();
        self.ops
            .list()
            .into_iter()
            .filter(|w| {
                needle.is_empty()
                    || w.title.to_lowercase().contains(&needle)
                    || w.process_name.to_lowercase().contains(&needle)
            })
            .map(window_item)
            .collect()
    }

    fn needs_confirm(&self, _id: &ItemId, action: &ActionId) -> bool {
        action.0 == "force_quit"
    }

    fn actions(&self, _id: &ItemId) -> Vec<Action> {
        window_actions()
    }

    fn run(&self, id: &ItemId, action: ActionId, _cx: &mut Cx) -> Outcome {
        run_window_action(&*self.ops, id, &action)
    }
}

impl ViewSource for WindowsView {
    fn view_id(&self) -> ViewId {
        ViewId(VIEW_ID.into())
    }

    fn title(&self) -> &'static str {
        "Windows"
    }

    fn opened_by_hotkey(&self) -> bool {
        true
    }

    fn ranked(&self) -> bool {
        false
    }

    fn placeholder(&self) -> &'static str {
        "Search open windows..."
    }

    fn empty_title(&self) -> &'static str {
        "No open windows"
    }

    fn preview(&self, _id: &ItemId) -> Option<Preview> {
        None
    }
}
