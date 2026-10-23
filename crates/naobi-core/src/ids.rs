//! Plain identifiers shared by every module crate.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ItemId(pub String);
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ActionId(pub String);
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ViewId(pub String);
/// String key such as "launcher" or "snap_left", so module crates need not depend on each other.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct HotkeyId(pub String);
/// Clipboard history entry row id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EntryId(pub i64);
/// `WM_TIMER` id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TimerId(pub usize);
