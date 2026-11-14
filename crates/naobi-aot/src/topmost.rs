//! The toggle: which window, whether it may be touched, and what the border follows.
use crate::Style;

/// The window in front.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    pub hwnd: isize,
    pub title: String,
    pub elevated: bool,
}

/// Windows, as the toggle needs it. The production one is in `naobi-win32`; the tests use a fake.
pub trait TopmostOps {
    /// The window in front that a command may touch, or None.
    fn foreground(&mut self) -> Option<Target>;
    fn is_topmost(&mut self, hwnd: isize) -> bool;
    /// False when Windows refused.
    fn set_topmost(&mut self, hwnd: isize, on: bool) -> bool;
    /// Starts or stops following `hwnd` with a border. Nothing runs while no window is followed.
    fn follow(&mut self, hwnd: isize, on: bool);
    /// Changes the look of every border.
    fn set_style(&mut self, style: Style);
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// The window is topmost now.
    On,
    /// The window is not topmost any more.
    Off,
    NoWindow,
    /// The window runs as administrator; nothing was changed (R-011, T-027). Holds its title.
    Elevated(String),
    /// Windows refused. Holds the title.
    Refused(String),
}

/// Toggles the window in front. The set of topmost windows is Windows' own (`WS_EX_TOPMOST`), so
/// nothing is remembered here.
pub fn toggle(ops: &mut dyn TopmostOps) -> Outcome {
    let Some(w) = ops.foreground() else {
        return Outcome::NoWindow;
    };
    // Checked by token before anything changes (T-027).
    if w.elevated {
        return Outcome::Elevated(w.title);
    }
    let on = !ops.is_topmost(w.hwnd);
    if !ops.set_topmost(w.hwnd, on) {
        return Outcome::Refused(w.title);
    }
    ops.follow(w.hwnd, on);
    if on {
        Outcome::On
    } else {
        Outcome::Off
    }
}
