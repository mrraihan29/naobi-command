//! File Locksmith pushed view (Ticket #25): inspects current-user processes locking a chosen file
//! (`R-011` — elevated/other-user locks require elevation to inspect or terminate) and terminates a
//! chosen process with second-Enter confirmation (`R-020`).
use naobi_core::command::{CommandSource, Cx, Outcome, Query};
use naobi_core::effect::Effect;
use naobi_core::ids::{ActionId, ItemId, ViewId};
use naobi_core::item::{Action, Item, ItemKind};
use naobi_core::notice::{Notice, NoticeKind};
use naobi_core::view_source::{Preview, ViewSource};
use std::sync::{Arc, Mutex};

pub const VIEW_ID: &str = "file-locks";
const END_TASK: &str = "end_task";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LockingProcess {
    pub pid: u32,
    pub name: String,
    pub kind: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LockEndOutcome {
    Ended,
    Elevated,
    Failed,
}

pub trait LockOps: Send + Sync {
    fn locking_processes(&self, path: &str) -> Result<Vec<LockingProcess>, &'static str>;
    fn end_task(&self, pid: u32) -> LockEndOutcome;
}

/// Target file shared between `FileCommands` and `LocksmithView`.
pub type SharedLockTarget = Arc<Mutex<Option<String>>>;

pub struct LocksmithView {
    ops: Box<dyn LockOps>,
    target: SharedLockTarget,
}

impl LocksmithView {
    pub fn new(ops: Box<dyn LockOps>, target: SharedLockTarget) -> Self {
        Self { ops, target }
    }

    fn current_target(&self) -> Option<String> {
        self.target
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }
}

impl CommandSource for LocksmithView {
    fn items(&self, q: &Query) -> Arc<[Item]> {
        let Some(path) = self.current_target() else {
            return Arc::new([]);
        };
        let list = match self.ops.locking_processes(&path) {
            Ok(v) => v,
            Err(reason) => {
                return Arc::from([Item {
                    id: ItemId("lock:error".into()),
                    title: reason.into(),
                    subtitle: "Current-user inspection only (run elevated for system locks)".into(),
                    keywords: Vec::new(),
                    alias: None,
                    kind: ItemKind::Command,
                    primary: ActionId("none".into()),
                    secondary: None,
                }]);
            }
        };
        let needle = q.text.trim().to_ascii_lowercase();
        let items: Vec<Item> = list
            .into_iter()
            .filter(|p| {
                needle.is_empty()
                    || p.name.to_ascii_lowercase().contains(&needle)
                    || p.pid.to_string().contains(&needle)
            })
            .map(|p| Item {
                id: ItemId(format!("lock:{}", p.pid)),
                title: p.name,
                subtitle: format!("PID {} · {} · current-user locks only", p.pid, p.kind),
                keywords: vec![p.pid.to_string()],
                alias: None,
                kind: ItemKind::Command,
                primary: ActionId(END_TASK.into()),
                secondary: None,
            })
            .collect();
        Arc::from(items)
    }

    fn armed(&self) -> Vec<ItemId> {
        self.items(&Query::default())
            .iter()
            .filter(|i| i.id.0 != "lock:error")
            .map(|i| i.id.clone())
            .collect()
    }

    fn needs_confirm(&self, id: &ItemId, action: &ActionId) -> bool {
        action.0 == END_TASK && id.0.starts_with("lock:") && id.0 != "lock:error"
    }

    fn actions(&self, id: &ItemId) -> Vec<Action> {
        if !id.0.starts_with("lock:") || id.0 == "lock:error" {
            return Vec::new();
        }
        vec![Action {
            id: ActionId(END_TASK.into()),
            label: "End task".into(),
            shortcut: None,
            enabled: Ok(()),
        }]
    }

    fn run(&self, id: &ItemId, action: ActionId, _cx: &mut Cx) -> Outcome {
        if action.0 != END_TASK {
            return Outcome::Done;
        }
        let Some(pid) =
            id.0.strip_prefix("lock:")
                .and_then(|s| s.parse::<u32>().ok())
        else {
            return Outcome::Done;
        };
        match self.ops.end_task(pid) {
            LockEndOutcome::Ended => Outcome::Stay {
                effects: vec![Effect::Notice(Notice {
                    subject: "File Locksmith".into(),
                    message: format!("Ended process PID {pid}."),
                    kind: NoticeKind::Info,
                })],
                select: None,
            },
            LockEndOutcome::Elevated => Outcome::Failed(Notice {
                subject: "File Locksmith".into(),
                message: "That process requires administrator privileges to end.".into(),
                kind: NoticeKind::Error,
            }),
            LockEndOutcome::Failed => Outcome::Failed(Notice {
                subject: "File Locksmith".into(),
                message: "Windows could not end that process.".into(),
                kind: NoticeKind::Error,
            }),
        }
    }
}

impl ViewSource for LocksmithView {
    fn view_id(&self) -> ViewId {
        ViewId(VIEW_ID.into())
    }

    fn title(&self) -> &'static str {
        "File Locksmith"
    }

    fn opened_by_hotkey(&self) -> bool {
        false
    }

    fn ranked(&self) -> bool {
        false
    }

    fn placeholder(&self) -> &'static str {
        "Filter locking processes (current user only)..."
    }

    fn empty_title(&self) -> &'static str {
        "No current-user locks found (run elevated to inspect all)"
    }

    fn preview(&self, _id: &ItemId) -> Option<Preview> {
        None
    }
}
