//! A command source that the launcher hosts as a view (clipboard history, color history).
use crate::command::CommandSource;
use crate::ids::{ItemId, ViewId};
use crate::tokens::Rgba;

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
    /// A Theme's colors with their token names (R-031).
    Swatches(Vec<Swatch>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Swatch {
    pub label: String,
    pub color: Rgba,
}

/// One labelled fact shown under a preview. It never holds the entry's content (R-027).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fact {
    pub label: String,
    pub value: String,
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
    /// The preview and the facts about it, read together.
    fn pane(&self, id: &ItemId) -> Option<(Preview, Vec<Fact>)> {
        self.preview(id).map(|p| (p, Vec::new()))
    }
    /// The Theme to show while this row is selected (R-031).
    fn look(&self, _id: &ItemId) -> Option<String> {
        None
    }
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
    /// True when this view hosts the multiline editor instead of a result list.
    fn is_editor(&self) -> bool {
        false
    }
    /// Called when the editor view becomes active; returns initial buffer text or an error Notice.
    fn open_editor(&self) -> Result<String, crate::notice::Notice> {
        Ok(String::new())
    }
    /// Current editor text while active (`None` for list views).
    fn editor_text(&self) -> Option<String> {
        None
    }
    /// Updates the editor buffer; returns a debounce generation when a delayed save is needed.
    fn set_editor_text(&self, _text: String) -> Option<u64> {
        None
    }
    /// Flushes pending edits to disk immediately (on Esc, Dismissed, Back, or timer).
    /// When `generation` is `Some(g)`, flushes only if it still matches the latest edit.
    fn flush_editor(&self, _generation: Option<u64>) -> Result<(), crate::notice::Notice> {
        Ok(())
    }
}
