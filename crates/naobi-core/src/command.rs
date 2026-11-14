//! The port every module crate implements to contribute launcher items.
use crate::effect::{Effect, LaunchTarget};
use crate::ids::{ActionId, ItemId};
use crate::item::{Action, Item};
use crate::notice::Notice;
use std::sync::Arc;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Query {
    pub text: String,
}

/// Collects the effects a running command requests.
#[derive(Debug, Default)]
pub struct Cx {
    effects: Vec<Effect>,
}

impl Cx {
    pub fn notice(&mut self, n: Notice) {
        self.effects.push(Effect::Notice(n));
    }
    pub fn hide(&mut self) {
        self.effects.push(Effect::Hide);
    }
    pub fn copy_text(&mut self, text: String) {
        self.effects.push(Effect::CopyText(text));
    }
    pub fn launch(&mut self, t: LaunchTarget) {
        self.effects.push(Effect::Launch(t));
    }
    /// Opens a view that has its own hotkey (a module's hotkey handler asks for it).
    pub fn open_view(&mut self, id: crate::ids::ViewId) {
        self.effects.push(Effect::PushView(id));
    }
    /// Takes the effects raised so far.
    pub fn take(&mut self) -> Vec<Effect> {
        std::mem::take(&mut self.effects)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Done,
    /// The action changed what the view lists (pin, delete): the Launcher stays open and ranks again.
    Refresh,
    Effects(Vec<Effect>),
    Failed(Notice),
}

pub trait CommandSource: Send + Sync {
    /// The candidates for `q`. A source with an index hands out a shared snapshot (an `Arc` clone),
    /// so a keystroke copies nothing and the index can be swapped while the Launcher is open; a
    /// source that builds items per query (a calculator) returns a fresh list. The engine ranks the
    /// list and clones only the rows it shows (R-006).
    fn items(&self, q: &Query) -> Arc<[Item]>;
    /// What this source is still loading ("Indexing apps"), or None when it is ready. The
    /// Launcher shows it as a status row while any root source is loading.
    fn pending(&self) -> Option<&'static str> {
        None
    }
    /// Ids of items whose primary action needs a second Enter (R-020).
    fn armed(&self) -> Vec<ItemId> {
        Vec::new()
    }
    /// True for the source whose actions run the file rows the search hands in (R-013).
    fn takes_files(&self) -> bool {
        false
    }
    fn actions(&self, id: &ItemId) -> Vec<Action>;
    fn run(&self, id: &ItemId, action: ActionId, cx: &mut Cx) -> Outcome;
}
