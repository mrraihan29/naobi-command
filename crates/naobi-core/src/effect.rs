//! Side effects a command asks the app to perform.
use crate::ids::{EntryId, ViewId};
use crate::notice::Notice;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LaunchTarget {
    /// A path, or `shell:AppsFolder\<AUMID>`.
    ShellItem(String),
    Url(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    Launch(LaunchTarget),
    Hide,
    CopyText(String),
    Notice(Notice),
    PushView(ViewId),
    Paste(EntryId),
}
