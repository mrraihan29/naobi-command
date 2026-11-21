//! Rows the launcher can show, and the actions on them.
use crate::chord::KeyChord;
use crate::ids::{ActionId, ItemId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemKind {
    App,
    File,
    Command,
    Quicklink,
    Calculator,
    ClipboardEntry,
    Color,
    CommandLine,
    Theme,
    Utility,
    Window,
    Emoji,
    Snippet,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub id: ItemId,
    pub title: String,
    pub subtitle: String,
    pub keywords: Vec<String>,
    pub alias: Option<String>,
    pub kind: ItemKind,
    pub primary: ActionId,
    pub secondary: Option<ActionId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Action {
    pub id: ActionId,
    pub label: String,
    pub shortcut: Option<KeyChord>,
    /// `Err` carries the reason the action is disabled.
    pub enabled: Result<(), String>,
}
