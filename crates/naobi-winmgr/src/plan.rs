//! The geometry of every command, with no Windows in it (D-022).
use crate::Monitor;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl Rect {
    pub fn new(left: i32, top: i32, right: i32, bottom: i32) -> Self {
        Self {
            left,
            top,
            right,
            bottom,
        }
    }

    pub fn width(&self) -> i32 {
        self.right - self.left
    }

    pub fn height(&self) -> i32 {
        self.bottom - self.top
    }

    /// This rectangle grown by `by` on each side (`by` holds one amount per side).
    pub fn grow(&self, by: &Rect) -> Rect {
        Rect::new(
            self.left - by.left,
            self.top - by.top,
            self.right + by.right,
            self.bottom + by.bottom,
        )
    }

    /// This rectangle shrunk by `by` on each side.
    pub fn shrink(&self, by: &Rect) -> Rect {
        Rect::new(
            self.left + by.left,
            self.top + by.top,
            self.right - by.right,
            self.bottom - by.bottom,
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    LeftHalf,
    RightHalf,
    TopHalf,
    BottomHalf,
    LeftThird,
    CenterThird,
    RightThird,
    FirstTwoThirds,
    LastTwoThirds,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
    Center,
    Maximize,
    AlmostMaximize,
    Restore,
    NextDisplay,
}

/// Every command, in the order the Launcher lists them.
pub const ALL: [Command; 18] = [
    Command::LeftHalf,
    Command::RightHalf,
    Command::TopHalf,
    Command::BottomHalf,
    Command::LeftThird,
    Command::CenterThird,
    Command::RightThird,
    Command::FirstTwoThirds,
    Command::LastTwoThirds,
    Command::TopLeft,
    Command::TopRight,
    Command::BottomLeft,
    Command::BottomRight,
    Command::Center,
    Command::Maximize,
    Command::AlmostMaximize,
    Command::Restore,
    Command::NextDisplay,
];

impl Command {
    /// The hotkey id, and the part of the item id after `win.`. The four halves, `maximize`, `center`
    /// and `restore` are ids the settings know; the others have no hotkey.
    pub fn id(self) -> &'static str {
        match self {
            Self::LeftHalf => "snap_left",
            Self::RightHalf => "snap_right",
            Self::TopHalf => "snap_up",
            Self::BottomHalf => "snap_down",
            Self::LeftThird => "snap_left_third",
            Self::CenterThird => "snap_center_third",
            Self::RightThird => "snap_right_third",
            Self::FirstTwoThirds => "snap_first_two_thirds",
            Self::LastTwoThirds => "snap_last_two_thirds",
            Self::TopLeft => "snap_top_left",
            Self::TopRight => "snap_top_right",
            Self::BottomLeft => "snap_bottom_left",
            Self::BottomRight => "snap_bottom_right",
            Self::Center => "center",
            Self::Maximize => "maximize",
            Self::AlmostMaximize => "almost_maximize",
            Self::Restore => "restore",
            Self::NextDisplay => "next_display",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        ALL.into_iter().find(|c| c.id() == id)
    }

    pub fn title(self) -> &'static str {
        match self {
            Self::LeftHalf => "Snap Left Half",
            Self::RightHalf => "Snap Right Half",
            Self::TopHalf => "Snap Top Half",
            Self::BottomHalf => "Snap Bottom Half",
            Self::LeftThird => "Snap Left Third",
            Self::CenterThird => "Snap Center Third",
            Self::RightThird => "Snap Right Third",
            Self::FirstTwoThirds => "Snap First Two Thirds",
            Self::LastTwoThirds => "Snap Last Two Thirds",
            Self::TopLeft => "Snap Top Left",
            Self::TopRight => "Snap Top Right",
            Self::BottomLeft => "Snap Bottom Left",
            Self::BottomRight => "Snap Bottom Right",
            Self::Center => "Center Window",
            Self::Maximize => "Maximize Window",
            Self::AlmostMaximize => "Almost Maximize Window",
            Self::Restore => "Restore Window",
            Self::NextDisplay => "Move Window to Next Display",
        }
    }

    /// `[columns, first, end, rows, first, end]`: the cells of an even grid the window covers.
    fn cells(self) -> Option<[i32; 6]> {
        Some(match self {
            Self::LeftHalf => [2, 0, 1, 1, 0, 1],
            Self::RightHalf => [2, 1, 2, 1, 0, 1],
            Self::TopHalf => [1, 0, 1, 2, 0, 1],
            Self::BottomHalf => [1, 0, 1, 2, 1, 2],
            Self::LeftThird => [3, 0, 1, 1, 0, 1],
            Self::CenterThird => [3, 1, 2, 1, 0, 1],
            Self::RightThird => [3, 2, 3, 1, 0, 1],
            Self::FirstTwoThirds => [3, 0, 2, 1, 0, 1],
            Self::LastTwoThirds => [3, 1, 3, 1, 0, 1],
            Self::TopLeft => [2, 0, 1, 2, 0, 1],
            Self::TopRight => [2, 1, 2, 2, 0, 1],
            Self::BottomLeft => [2, 0, 1, 2, 1, 2],
            Self::BottomRight => [2, 1, 2, 2, 1, 2],
            _ => return None,
        })
    }
}

/// The gap in logical pixels, scaled to the display's DPI.
pub fn scaled_gap(gap: u32, dpi: u32) -> i32 {
    ((i64::from(gap) * i64::from(dpi) + 48) / 96) as i32
}

/// One axis: cells `a..b` of `n` over `start..end`. The margin to the edge of the work area is the
/// whole gap; between two snapped neighbours the gap is shared, each side taking its part.
fn span(start: i32, end: i32, n: i32, a: i32, b: i32, gap: i32) -> (i32, i32) {
    let len = i64::from(end - start);
    let at = |i: i32| start + (len * i64::from(i) / i64::from(n)) as i32;
    let lo = at(a) + if a == 0 { gap } else { gap / 2 };
    let hi = at(b) - if b == n { gap } else { gap - gap / 2 };
    (lo, hi)
}

/// Where the visible frame goes for a snap, a center or an almost-maximize on `on`. `visible` is
/// the visible frame now. None for the commands that are not a placement.
pub fn target(cmd: Command, visible: Rect, on: &Monitor, gap: u32) -> Option<Rect> {
    let work = on.work;
    if let Some([cols, c0, c1, rows, r0, r1]) = cmd.cells() {
        let g = scaled_gap(gap, on.dpi);
        let (left, right) = span(work.left, work.right, cols, c0, c1, g);
        let (top, bottom) = span(work.top, work.bottom, rows, r0, r1, g);
        return Some(Rect::new(left, top, right, bottom));
    }
    match cmd {
        // The same size, unless it does not fit the work area.
        Command::Center => Some(centered(
            work,
            visible.width().min(work.width()),
            visible.height().min(work.height()),
        )),
        Command::AlmostMaximize => Some(centered(
            work,
            work.width() * 9 / 10,
            work.height() * 9 / 10,
        )),
        _ => None,
    }
}

fn centered(work: Rect, w: i32, h: i32) -> Rect {
    let left = work.left + (work.width() - w) / 2;
    let top = work.top + (work.height() - h) / 2;
    Rect::new(left, top, left + w, top + h)
}

/// The window moved from display `from` to `to`, keeping its place and size as fractions of the work
/// area (D-022).
pub fn next_display(visible: Rect, from: &Monitor, to: &Monitor) -> Rect {
    let scale = |v: i32, f_origin: i32, f_len: i32, t_origin: i32, t_len: i32| {
        t_origin + (i64::from(v - f_origin) * i64::from(t_len) / i64::from(f_len.max(1))) as i32
    };
    let (f, t) = (from.work, to.work);
    Rect::new(
        scale(visible.left, f.left, f.width(), t.left, t.width()),
        scale(visible.top, f.top, f.height(), t.top, t.height()),
        scale(visible.right, f.left, f.width(), t.left, t.width()),
        scale(visible.bottom, f.top, f.height(), t.top, t.height()),
    )
}
