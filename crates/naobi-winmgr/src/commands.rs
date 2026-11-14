//! The window commands as Launcher items. Running one hides the Launcher, gives focus back, and
//! then runs the command as its hotkey would, so a command from the list and its key do the same.
use crate::plan::{Command, ALL};
use naobi_core::command::{CommandSource, Cx, Outcome, Query};
use naobi_core::effect::Effect;
use naobi_core::ids::{ActionId, HotkeyId, ItemId};
use naobi_core::item::{Action, Item, ItemKind};
use std::collections::BTreeMap;
use std::sync::{Arc, RwLock};

struct Inner {
    gap: u32,
    /// Hotkey id to the chord as text, for the item's subtitle.
    chords: BTreeMap<String, String>,
}

/// The live settings the commands read: the settings load and every hot reload `set` them.
#[derive(Clone)]
pub struct Config(Arc<RwLock<Inner>>);

impl Default for Config {
    fn default() -> Self {
        Self(Arc::new(RwLock::new(Inner {
            gap: 8,
            chords: BTreeMap::new(),
        })))
    }
}

impl Config {
    pub fn set(&self, gap: u32, chords: BTreeMap<String, String>) {
        // A gap beyond 64 px only makes rectangles with no room in them.
        let gap = gap.min(64);
        *self.0.write().unwrap_or_else(|e| e.into_inner()) = Inner { gap, chords };
    }

    pub fn gap(&self) -> u32 {
        self.0.read().unwrap_or_else(|e| e.into_inner()).gap
    }
}

pub struct SnapCommands(Config);

impl SnapCommands {
    pub fn new(config: Config) -> Self {
        Self(config)
    }
}

const PREFIX: &str = "win.";

impl CommandSource for SnapCommands {
    fn items(&self, _q: &Query) -> Arc<[Item]> {
        let inner = self.0 .0.read().unwrap_or_else(|e| e.into_inner());
        ALL.iter()
            .map(|c| Item {
                id: ItemId(format!("{PREFIX}{}", c.id())),
                title: c.title().to_owned(),
                subtitle: inner.chords.get(c.id()).cloned().unwrap_or_default(),
                keywords: vec!["window".to_owned(), "snap".to_owned()],
                alias: None,
                kind: ItemKind::Command,
                primary: ActionId("run".into()),
                secondary: None,
            })
            .collect()
    }

    fn actions(&self, _id: &ItemId) -> Vec<Action> {
        vec![Action {
            id: ActionId("run".into()),
            label: "Move".into(),
            shortcut: None,
            enabled: Ok(()),
        }]
    }

    fn run(&self, id: &ItemId, _action: ActionId, _cx: &mut Cx) -> Outcome {
        match id.0.strip_prefix(PREFIX).and_then(Command::from_id) {
            Some(c) => Outcome::Effects(vec![
                Effect::Hide,
                Effect::RunHotkey(HotkeyId(c.id().to_owned())),
            ]),
            None => Outcome::Done,
        }
    }
}
