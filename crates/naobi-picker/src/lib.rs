//! Color picker (NCMD-24, R-017, D-023): the color formats, the last 20 picks, the Launcher's Color
//! History view and the two commands. The overlay and the screen grab are `naobi-win32`'s; this crate
//! only defines the port they implement.
#![forbid(unsafe_code)]

pub mod format;
pub mod history;
pub mod view;

pub use format::{Format, Rgb};
pub use history::{ColorHistory, Pick};
pub use view::{ColorHistoryView, PickerCommands, Shared, VIEW_ID};

/// Takes a frozen picture of every monitor. `Frame` holds the pixels and nothing else, so dropping
/// it is dropping the snapshot (T-015).
pub trait ScreenCapture {
    type Frame;
    fn capture(&mut self) -> Option<Self::Frame>;
}

/// Grabs the screen, lets `overlay` choose a color from it, and drops the snapshot before it
/// returns, whether a color was picked or not.
pub fn pick<C: ScreenCapture>(
    capture: &mut C,
    overlay: impl FnOnce(&C::Frame) -> Option<Rgb>,
) -> Option<Rgb> {
    let frame = capture.capture()?;
    let picked = overlay(&frame);
    drop(frame);
    picked
}

/// Where the readout box goes: right of and below the cursor, flipped to the other side when it
/// would leave the screen `[0, screen)`. `gap` is the distance to the cursor.
pub fn flip(cursor: i32, size: i32, screen: i32, gap: i32) -> i32 {
    let after = cursor + gap;
    if after + size <= screen {
        after
    } else {
        (cursor - gap - size).max(0)
    }
}
