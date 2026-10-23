//! A command source that the launcher hosts as a view (clipboard history, color history).
use crate::command::CommandSource;
use crate::ids::{ItemId, ViewId};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Preview {
    Text(String),
    Image { png: Vec<u8>, w: u32, h: u32 },
    Files(Vec<String>),
}

pub trait ViewSource: CommandSource {
    fn view_id(&self) -> ViewId;
    /// True when the view has its own hotkey, so Esc hides instead of popping.
    fn opened_by_hotkey(&self) -> bool;
    fn preview(&self, id: &ItemId) -> Option<Preview>;
}
