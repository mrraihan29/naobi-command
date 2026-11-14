//! Window management (R-016, D-022): snap the foreground window to halves, thirds, quarters, the
//! center, maximize or almost-maximize, restore it, or send it to the next display. The geometry is
//! pure integer arithmetic in physical pixels (`plan`); Windows is behind `WindowOps`.
#![forbid(unsafe_code)]

pub mod commands;
pub mod plan;
pub mod snapper;

pub use commands::{Config, SnapCommands};
pub use plan::{Command, Rect};
pub use snapper::{Outcome, Snapper};

/// One display: its work area (physical pixels) and its DPI (96 is 100%).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Monitor {
    pub work: Rect,
    pub dpi: u32,
}

/// The window a command acts on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowInfo {
    pub hwnd: isize,
    pub title: String,
    /// The rectangle `SetWindowPos` takes (shadow and resize borders included). A maximized window
    /// reports the rectangle it returns to.
    pub outer: Rect,
    /// How far the outer rectangle reaches past the visible frame on each side (RK-11).
    pub border: Rect,
    /// Index into `WindowOps::monitors` of the display the window is on.
    pub monitor: usize,
    pub elevated: bool,
}

/// Windows, as the commands need it. The production one is in `naobi-win32`; the tests use a fake.
pub trait WindowOps {
    /// The window in front, or None when there is none a command may move (the desktop, the Launcher).
    fn foreground(&mut self) -> Option<WindowInfo>;
    fn monitors(&mut self) -> Vec<Monitor>;
    /// Puts the window's outer rectangle at `outer`, restoring it first when it is maximized. `dpi`
    /// is the target display's. False when Windows refused.
    fn move_to(&mut self, hwnd: isize, outer: Rect, dpi: u32) -> bool;
    fn maximize(&mut self, hwnd: isize) -> bool;
}
