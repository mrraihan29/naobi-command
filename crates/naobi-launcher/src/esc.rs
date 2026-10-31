//! The Esc ladder in one place (R-012): close the action panel or dismiss a Notice, then clear the
//! query, then pop a pushed view, then hide. A view opened by its own hotkey hides instead of popping.

/// Where the Launcher is when Esc arrives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Root,
    Pushed,
    HotkeyOpened,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EscStep {
    ClosePanel,
    DismissNotice,
    ClearQuery,
    PopView,
    Hide,
}

pub fn esc_step(panel_open: bool, notice: bool, query_nonempty: bool, level: Level) -> EscStep {
    if panel_open {
        EscStep::ClosePanel
    } else if notice {
        EscStep::DismissNotice
    } else if query_nonempty {
        EscStep::ClearQuery
    } else {
        match level {
            Level::Root | Level::HotkeyOpened => EscStep::Hide,
            Level::Pushed => EscStep::PopView,
        }
    }
}
