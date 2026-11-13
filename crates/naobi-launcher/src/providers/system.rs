//! The system commands (R-020, D-024): lock, sleep, hibernate, restart, shut down, sign out, empty
//! recycle bin, toggle dark mode and open Windows settings, plus Open Settings File, Pause Hotkeys
//! and Quit. Restart, shut down, sign out and empty recycle bin need a second Enter (the engine's
//! armed state). The commands themselves run through the `SystemOps` port.
use naobi_core::command::{CommandSource, Cx, Outcome, Query};
use naobi_core::ids::{ActionId, ItemId};
use naobi_core::item::{Action, Item, ItemKind};
use naobi_core::notice::{Notice, NoticeKind};
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SystemCommand {
    Lock,
    Sleep,
    Hibernate,
    Restart,
    ShutDown,
    SignOut,
    EmptyRecycleBin,
    ToggleDarkMode,
    OpenWindowsSettings,
    OpenSettingsFile,
    PauseHotkeys,
    Quit,
}

impl SystemCommand {
    pub const ALL: [SystemCommand; 12] = [
        Self::Lock,
        Self::Sleep,
        Self::Hibernate,
        Self::Restart,
        Self::ShutDown,
        Self::SignOut,
        Self::EmptyRecycleBin,
        Self::ToggleDarkMode,
        Self::OpenWindowsSettings,
        Self::OpenSettingsFile,
        Self::PauseHotkeys,
        Self::Quit,
    ];

    /// The command id: the item id, and what the log records (T-009).
    pub fn id(self) -> &'static str {
        match self {
            Self::Lock => "sys.lock",
            Self::Sleep => "sys.sleep",
            Self::Hibernate => "sys.hibernate",
            Self::Restart => "sys.restart",
            Self::ShutDown => "sys.shut_down",
            Self::SignOut => "sys.sign_out",
            Self::EmptyRecycleBin => "sys.empty_recycle_bin",
            Self::ToggleDarkMode => "sys.toggle_dark_mode",
            Self::OpenWindowsSettings => "sys.open_windows_settings",
            Self::OpenSettingsFile => "sys.open_settings_file",
            Self::PauseHotkeys => "sys.pause_hotkeys",
            Self::Quit => "sys.quit",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Self::Lock => "Lock",
            Self::Sleep => "Sleep",
            Self::Hibernate => "Hibernate",
            Self::Restart => "Restart",
            Self::ShutDown => "Shut Down",
            Self::SignOut => "Sign Out",
            Self::EmptyRecycleBin => "Empty Recycle Bin",
            Self::ToggleDarkMode => "Toggle Dark Mode",
            Self::OpenWindowsSettings => "Open Windows Settings",
            Self::OpenSettingsFile => "Open Settings File",
            Self::PauseHotkeys => "Pause Hotkeys",
            Self::Quit => "Quit Naobi Command",
        }
    }

    /// True when the first Enter only arms the command (R-020).
    pub fn armed(self) -> bool {
        matches!(
            self,
            Self::Restart | Self::ShutDown | Self::SignOut | Self::EmptyRecycleBin
        )
    }

    fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|c| c.id() == id)
    }
}

/// What the host does for a command. The error is a short sentence for the Notice.
pub trait SystemOps: Send + Sync {
    fn run(&self, command: SystemCommand) -> Result<(), String>;
}

pub struct SystemSource {
    ops: Arc<dyn SystemOps>,
    items: Arc<[Item]>,
}

impl SystemSource {
    pub fn new(ops: Arc<dyn SystemOps>) -> Self {
        let items = SystemCommand::ALL
            .into_iter()
            .map(|c| Item {
                id: ItemId(c.id().into()),
                title: c.title().into(),
                subtitle: if c.armed() {
                    "Press Enter twice to confirm".into()
                } else {
                    String::new()
                },
                keywords: Vec::new(),
                alias: None,
                kind: ItemKind::Command,
                primary: ActionId("run".into()),
                secondary: None,
            })
            .collect();
        Self { ops, items }
    }
}

impl CommandSource for SystemSource {
    fn items(&self, _q: &Query) -> Arc<[Item]> {
        self.items.clone()
    }

    fn armed(&self) -> Vec<ItemId> {
        SystemCommand::ALL
            .into_iter()
            .filter(|c| c.armed())
            .map(|c| ItemId(c.id().into()))
            .collect()
    }

    fn actions(&self, id: &ItemId) -> Vec<Action> {
        let label = SystemCommand::from_id(&id.0).map_or("Run", |c| c.title());
        vec![Action {
            id: ActionId("run".into()),
            label: label.into(),
            shortcut: None,
            enabled: Ok(()),
        }]
    }

    fn run(&self, id: &ItemId, _action: ActionId, _cx: &mut Cx) -> Outcome {
        let Some(command) = SystemCommand::from_id(&id.0) else {
            return Outcome::Done;
        };
        match self.ops.run(command) {
            Ok(()) => Outcome::Done,
            Err(message) => Outcome::Failed(Notice {
                subject: command.title().into(),
                message,
                kind: NoticeKind::Error,
            }),
        }
    }
}
