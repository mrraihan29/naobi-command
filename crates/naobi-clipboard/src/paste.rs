//! The paste sequence (D-019 v5, T-008, T-032), over two ports so the order and the safety checks run
//! without Windows: write the entry with the private marker, give the target focus, check it is the
//! foreground and not elevated, send Ctrl+V, and about 300 ms later put the previous clipboard back
//! unless another copy happened or that clipboard was meant to stay out of history.
use crate::ports::Content;
use zeroize::Zeroizing;

/// What goes on the clipboard: text, a picture (the PNG and its bitmap form, so every program can
/// read one of them) or a list of files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Payload {
    Text(Zeroizing<String>),
    Image {
        png: Zeroizing<Vec<u8>>,
        dib: Zeroizing<Vec<u8>>,
    },
    // ponytail: paths only, not wiped; wrap them if file names ever count as secrets.
    Files(Vec<String>),
}

/// The clipboard as it was before the paste: only text, an image or a file list within the limits
/// come back; anything else is lost (M3). `excluded` is decided once, from the privacy formats and
/// the owner, when the snapshot is taken.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapshot {
    pub content: Option<Content>,
    pub excluded: bool,
}

pub trait Pasteboard {
    /// None when the clipboard cannot be read (busy): an empty clipboard is `Some` with no content.
    fn snapshot(&mut self) -> Option<Snapshot>;
    /// Writes with the private marker (D-019), so the capture never stores it.
    fn write(&mut self, payload: &Payload) -> bool;
    /// Writes the snapshot back, with the private marker.
    fn restore(&mut self, content: &Content) -> bool;
    fn sequence(&self) -> u32;
}

pub trait ForegroundOps {
    fn set_foreground(&mut self, target: isize);
    fn foreground(&self) -> isize;
    fn is_elevated(&self, target: isize) -> bool;
    /// Sends Ctrl+V to the foreground window.
    fn send_paste(&mut self) -> bool;
    fn sleep_ms(&mut self, ms: u64);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Abort {
    /// No window to paste into was recorded.
    NoTarget,
    /// The clipboard could not be written.
    WriteFailed,
    /// The target never became the foreground window, or stopped being it before the keys.
    ForegroundChanged,
    /// The target runs as administrator; this app never sends keys to it.
    Elevated,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Pasted,
    Aborted(Abort),
}

/// How long the target gets to become the foreground window, polled every `POLL_MS`.
const FOREGROUND_WAIT_MS: u64 = 100;
const POLL_MS: u64 = 10;
/// The previous clipboard comes back this long after the keystroke (D-019).
pub const RESTORE_AFTER_MS: u64 = 300;

/// Pastes `payload` into `target`. The caller has already cloaked the Launcher.
pub fn paste(
    payload: &Payload,
    target: Option<isize>,
    board: &mut impl Pasteboard,
    fg: &mut impl ForegroundOps,
) -> Outcome {
    let Some(target) = target else {
        return Outcome::Aborted(Abort::NoTarget);
    };
    // Without the previous clipboard there would be no way back from the write.
    let Some(snapshot) = board.snapshot() else {
        return Outcome::Aborted(Abort::WriteFailed);
    };
    if !board.write(payload) {
        // A write that fails half way has already emptied the clipboard: bring the snapshot back.
        if !snapshot.excluded {
            if let Some(content) = &snapshot.content {
                board.restore(content);
            }
        }
        return Outcome::Aborted(Abort::WriteFailed);
    }
    let written = board.sequence();
    fg.set_foreground(target);
    let mut waited = 0;
    while fg.foreground() != target && waited < FOREGROUND_WAIT_MS {
        fg.sleep_ms(POLL_MS);
        waited += POLL_MS;
    }
    if fg.foreground() != target {
        return aborted(board, &snapshot, written, Abort::ForegroundChanged);
    }
    if fg.is_elevated(target) {
        return aborted(board, &snapshot, written, Abort::Elevated);
    }
    // The last check before the keys: focus may have moved while the window was being checked.
    if fg.foreground() != target {
        return aborted(board, &snapshot, written, Abort::ForegroundChanged);
    }
    if !fg.send_paste() {
        return aborted(board, &snapshot, written, Abort::WriteFailed);
    }
    fg.sleep_ms(RESTORE_AFTER_MS);
    put_back(board, &snapshot, written);
    Outcome::Pasted
}

/// An abort puts the snapshot back under the same rule as the end of a paste.
fn aborted(
    board: &mut impl Pasteboard,
    snapshot: &Snapshot,
    written: u32,
    reason: Abort,
) -> Outcome {
    put_back(board, snapshot, written);
    Outcome::Aborted(reason)
}

/// The previous clipboard is restored only when nothing else was copied meanwhile, it was not meant
/// to stay out of history (T-032), and there was something to restore.
fn put_back(board: &mut impl Pasteboard, snapshot: &Snapshot, written: u32) {
    if snapshot.excluded || board.sequence() != written {
        return;
    }
    if let Some(content) = &snapshot.content {
        board.restore(content);
    }
}
