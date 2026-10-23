//! The port every module crate implements to contribute launcher items.
use crate::effect::{Effect, LaunchTarget};
use crate::ids::{ActionId, ItemId};
use crate::item::{Action, Item};
use crate::notice::Notice;

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
    /// Takes the effects raised so far.
    pub fn take(&mut self) -> Vec<Effect> {
        std::mem::take(&mut self.effects)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Done,
    Effects(Vec<Effect>),
    Failed(Notice),
}

pub trait CommandSource: Send + Sync {
    fn items(&self, q: &Query) -> Vec<Item>;
    fn actions(&self, id: &ItemId) -> Vec<Action>;
    fn run(&self, id: &ItemId, action: ActionId, cx: &mut Cx) -> Outcome;
}
