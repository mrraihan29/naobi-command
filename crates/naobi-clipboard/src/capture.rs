//! One capture: read the clipboard for a sequence number, keep the read only if the clipboard did not
//! change while it was read, and hand it to the history (REPORT 5.4).
use crate::history::{Error, History, Outcome};
use crate::ports::ClipSource;

#[derive(Debug, PartialEq, Eq)]
pub enum Capture {
    /// The history decided: stored, or skipped with a reason, and what retention removed.
    Decided(Outcome),
    /// Nothing readable was on the clipboard.
    Nothing,
    /// The clipboard changed during the read, so the read may be a mix. The next change event
    /// captures the newer state.
    Discarded,
}

pub fn capture(src: &dyn ClipSource, seq: u32, history: &mut History) -> Result<Capture, Error> {
    let Some(read) = src.read() else {
        return Ok(Capture::Nothing);
    };
    if src.sequence() != seq {
        return Ok(Capture::Discarded);
    }
    history.on_change(read).map(Capture::Decided)
}
