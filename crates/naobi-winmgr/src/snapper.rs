//! Runs a command on the foreground window and remembers where windows were (D-022): the restore
//! rectangle is recorded at the first command only and at most 64 are kept.
use crate::plan::{next_display, target, Command, Rect};
use crate::{Monitor, WindowOps};

const MAX_RESTORE: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Moved,
    /// There is no window a command may move.
    NoWindow,
    /// The window runs as administrator; nothing was changed (R-011, T-027). Holds its title.
    Elevated(String),
    /// Windows refused the move. Holds the title.
    Refused(String),
    /// Restore on a window that was never moved by a command. Holds the title.
    NothingToRestore(String),
    /// Next Display with only one display.
    OneDisplay,
}

#[derive(Default)]
pub struct Snapper {
    /// Oldest first.
    restore: Vec<(isize, Rect)>,
}

/// The display that holds the middle of `r`, else the first one.
fn monitor_of(monitors: &[Monitor], r: Rect) -> Monitor {
    let (cx, cy) = (r.left + r.width() / 2, r.top + r.height() / 2);
    monitors
        .iter()
        .find(|m| {
            (m.work.left..m.work.right).contains(&cx) && (m.work.top..m.work.bottom).contains(&cy)
        })
        .or(monitors.first())
        .copied()
        .unwrap_or(Monitor { work: r, dpi: 96 })
}

impl Snapper {
    pub fn run(&mut self, cmd: Command, ops: &mut dyn WindowOps, gap: u32) -> Outcome {
        let Some(first) = ops.foreground() else {
            return Outcome::NoWindow;
        };
        // Checked by token before anything changes, never by trying and failing.
        if first.elevated {
            return Outcome::Elevated(first.title);
        }
        // The rectangle to remember is where the window returns to, read before anything moves.
        let recorded = first.outer;
        // A placement works from the restored window: its invisible border differs from the
        // maximized one, so the window is restored first and read again (RK-11).
        let places = !matches!(cmd, Command::Restore | Command::Maximize);
        let w = if first.maximized && places && ops.unmaximize(first.hwnd) {
            ops.foreground().unwrap_or(first)
        } else {
            first
        };
        let monitors = ops.monitors();
        let Some(here) = monitors.get(w.monitor).copied() else {
            return Outcome::NoWindow;
        };
        let visible = w.outer.shrink(&w.border);

        if cmd == Command::Restore {
            let Some(i) = self.restore.iter().position(|(h, _)| *h == w.hwnd) else {
                return Outcome::NothingToRestore(w.title);
            };
            let (_, rect) = self.restore[i];
            let moved = ops.move_to(w.hwnd, rect, monitor_of(&monitors, rect).dpi);
            if moved {
                self.restore.remove(i);
                return Outcome::Moved;
            }
            return Outcome::Refused(w.title);
        }

        let moved = match cmd {
            Command::Maximize => ops.maximize(w.hwnd),
            Command::NextDisplay => {
                if monitors.len() < 2 {
                    return Outcome::OneDisplay;
                }
                let to = monitors[(w.monitor + 1) % monitors.len()];
                let rect = next_display(visible, &here, &to).grow(&w.border);
                ops.move_to(w.hwnd, rect, to.dpi)
            }
            _ => match target(cmd, visible, &here, gap) {
                Some(rect) => ops.move_to(w.hwnd, rect.grow(&w.border), here.dpi),
                None => false,
            },
        };
        if !moved {
            return Outcome::Refused(w.title);
        }
        // Only the first command on a window records where it was.
        if !self.restore.iter().any(|(h, _)| *h == w.hwnd) {
            self.restore.push((w.hwnd, recorded));
            if self.restore.len() > MAX_RESTORE {
                self.restore.remove(0);
            }
        }
        Outcome::Moved
    }
}
