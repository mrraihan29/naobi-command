//! "Toggle Always on Top" as a Launcher item. Running it hides the Launcher, gives focus back, and
//! then runs the command as its hotkey would, so the list and the key act on the same window.
use naobi_core::command::{CommandSource, Cx, Outcome, Query};
use naobi_core::effect::Effect;
use naobi_core::ids::{ActionId, HotkeyId, ItemId};
use naobi_core::item::{Action, Item, ItemKind};
use std::sync::{Arc, RwLock};

/// The hotkey id of the command (settings `hotkeys.topmost`).
pub const HOTKEY: &str = "topmost";

/// The chord as text for the item's subtitle: the settings load and every hot reload `set` it.
#[derive(Clone, Default)]
pub struct Config(Arc<RwLock<String>>);

impl Config {
    pub fn set(&self, chord: String) {
        *self.0.write().unwrap_or_else(|e| e.into_inner()) = chord;
    }
}

pub struct AotCommands(Config);

impl AotCommands {
    pub fn new(config: Config) -> Self {
        Self(config)
    }
}

const ID: &str = "aot.toggle";

impl CommandSource for AotCommands {
    fn items(&self, _q: &Query) -> Arc<[Item]> {
        vec![Item {
            id: ItemId(ID.into()),
            title: "Toggle Always on Top".into(),
            subtitle: self.0 .0.read().unwrap_or_else(|e| e.into_inner()).clone(),
            keywords: vec!["topmost".into(), "window".into()],
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
            label: "Toggle".into(),
            shortcut: None,
            enabled: Ok(()),
        }]
    }

    fn run(&self, id: &ItemId, _action: ActionId, _cx: &mut Cx) -> Outcome {
        if id.0 != ID {
            return Outcome::Done;
        }
        Outcome::Effects(vec![
            Effect::Hide,
            Effect::RunHotkey(HotkeyId(HOTKEY.into())),
        ])
    }
}
