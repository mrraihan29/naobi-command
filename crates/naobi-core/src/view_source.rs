//! A command source that the launcher hosts as a view (clipboard history, color history).
use crate::command::CommandSource;
use crate::ids::{ItemId, ViewId};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Preview {
    Text(String),
    Image {
        png: Vec<u8>,
        w: u32,
        h: u32,
    },
    /// A decoded picture, 8-bit RGBA row by row (`rgba.len() == w * h * 4`).
    Rgba {
        w: u32,
        h: u32,
        rgba: Vec<u8>,
    },
    Files(Vec<String>),
}

pub trait ViewSource: CommandSource {
    fn view_id(&self) -> ViewId;
    /// The name shown in the search header and the footer.
    fn title(&self) -> &'static str {
        "View"
    }
    /// True when the view has its own hotkey, so Esc hides instead of popping.
    fn opened_by_hotkey(&self) -> bool;
    fn preview(&self, id: &ItemId) -> Option<Preview>;
    /// False when the view filters and orders its own items (a history is newest first, and its
    /// search looks inside the text), so the engine keeps them as they come.
    fn ranked(&self) -> bool {
        true
    }
    /// The search field's hint while the query is empty.
    fn placeholder(&self) -> &'static str {
        "Search..."
    }
    /// The empty state's title when the view lists nothing and the query is empty.
    fn empty_title(&self) -> &'static str {
        "Nothing here yet"
    }
}
