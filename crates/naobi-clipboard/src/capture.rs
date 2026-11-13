//! One capture: read the clipboard for a sequence number, keep the read only if the clipboard did not
//! change while it was read, and hand it to the history (REPORT 5.4).
use crate::history::{prepare, Error, History, Outcome, Purged};
use crate::ports::ClipSource;
use std::sync::Mutex;

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

/// The read, the filters and the PNG encode run without the lock, so a big screenshot never makes
/// the Launcher wait for the history; the lock covers the encryption and the transaction.
pub fn capture(src: &dyn ClipSource, seq: u32, history: &Mutex<History>) -> Result<Capture, Error> {
    let lock = || history.lock().unwrap_or_else(|e| e.into_inner());
    let Some(read) = src.read() else {
        return Ok(Capture::Nothing);
    };
    if src.sequence() != seq {
        return Ok(Capture::Discarded);
    }
    let ignored = lock().ignored().to_vec();
    match prepare(&read, &ignored) {
        Ok(p) => lock().store(p).map(Capture::Decided),
        Err(skip) => Ok(Capture::Decided(Outcome {
            result: Err(skip),
            purged: Purged::default(),
        })),
    }
}
