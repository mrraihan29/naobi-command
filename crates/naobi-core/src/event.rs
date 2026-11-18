//! Messages the platform thread forwards to the controller.
use crate::ids::{HotkeyId, TimerId};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppEvent {
    Hotkey(HotkeyId),
    HotkeyConflict(HotkeyId),
    /// After a registration pass: the hotkeys the app holds now.
    HotkeysHeld(Vec<HotkeyId>),
    /// Clipboard sequence number.
    ClipboardChanged(u32),
    ThemeChanged,
    Resume,
    DisplayChanged,
    ShowRequested,
    SettingsChanged,
    Timer(TimerId),
}
