//! The clipboard history view, its commands and the paste (NCMD-20). The capture side is
//! `clipboard_capture`; both share one `History`.
use super::clipboard_capture::{shared, to_captured};
use super::Modules;
use naobi_clipboard::filters::{skip_reason, Skip, DEFAULT_IGNORED};
use naobi_clipboard::limits::{MAX_BITMAP_BYTES, MAX_FILE_PATHS, MAX_TEXT_BYTES};
use naobi_clipboard::paste::{paste, Abort, ForegroundOps, Outcome, Pasteboard, Payload, Snapshot};
use naobi_clipboard::view::{ClipboardCommands, ClipboardView, Shared, VIEW_ID};
use naobi_clipboard::Content;
use naobi_core::ids::{EntryId, HotkeyId, ViewId};
use naobi_core::notice::{Notice, NoticeKind};
use naobi_win32::{clipboard, foreground, paste as win};
use std::io;

pub fn register(m: &mut Modules) {
    let Some(history) = shared() else { return };
    m.register_source(Box::new(ClipboardCommands::new(history.clone())));
    m.register_view(Box::new(ClipboardView::new(history)));
    // Alt+Shift+V opens the history; the controller shows the Launcher on it.
    m.subscribe(
        HotkeyId("clipboard".into()),
        Box::new(|_, cx| cx.open_view(ViewId(VIEW_ID.into()))),
    );
}

fn notice(message: &str, kind: NoticeKind) -> Notice {
    Notice {
        subject: "Clipboard".into(),
        message: message.into(),
        kind,
    }
}

/// The previous clipboard within what a paste can bring back: text, a bitmap or a file list inside
/// the D-015 limits. Anything else is not restored (M3).
fn restorable(content: Content) -> Option<Content> {
    let fits = match &content {
        Content::Text(t) => t.len() <= MAX_TEXT_BYTES,
        Content::Dib(d) => d.len() <= MAX_BITMAP_BYTES,
        Content::Files(f) => f.len() <= MAX_FILE_PATHS,
    };
    fits.then_some(content)
}

struct Board;

impl Pasteboard for Board {
    fn snapshot(&mut self) -> Snapshot {
        let Some(captured) = clipboard::read().and_then(to_captured) else {
            return Snapshot {
                content: None,
                excluded: false,
            };
        };
        let ignored: Vec<String> = DEFAULT_IGNORED.iter().map(|s| (*s).to_owned()).collect();
        // A privacy format or an ignored owner means the previous clipboard must not come back. A
        // write that carries our own marker is fine to restore.
        let excluded = matches!(
            skip_reason(&captured, &ignored),
            Some(Skip::Excluded | Skip::IgnoredApp)
        );
        Snapshot {
            content: restorable(captured.content),
            excluded,
        }
    }

    fn write(&mut self, payload: &Payload) -> bool {
        write_payload(payload).is_ok()
    }

    fn restore(&mut self, content: &Content) -> bool {
        match content {
            Content::Text(t) => win::write_text(t),
            Content::Dib(d) => win::write_dib(d),
            Content::Files(f) => win::write_files(f),
        }
        .is_ok()
    }

    fn sequence(&self) -> u32 {
        clipboard::sequence()
    }
}

fn write_payload(payload: &Payload) -> io::Result<()> {
    match payload {
        Payload::Text(t) => win::write_text(t),
        Payload::Image { png, dib } => win::write_image(png, dib),
        Payload::Files(f) => win::write_files(f),
    }
}

struct Foreground;

impl ForegroundOps for Foreground {
    fn set_foreground(&mut self, target: isize) {
        foreground::restore(target);
    }
    fn foreground(&self) -> isize {
        win::foreground_window()
    }
    fn is_elevated(&self, target: isize) -> bool {
        win::is_elevated(target)
    }
    fn send_paste(&mut self) -> bool {
        win::send_ctrl_v()
    }
    fn sleep_ms(&mut self, ms: u64) {
        std::thread::sleep(std::time::Duration::from_millis(ms));
    }
}

fn payload_of(history: &Shared, id: EntryId) -> Option<Payload> {
    history
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .entry_payload(id.0)
        .ok()
        .flatten()
}

/// Ctrl+Enter: the entry goes on the clipboard with the marker, and nothing is pasted.
pub fn copy_entry(id: EntryId) -> io::Result<()> {
    let payload = shared()
        .and_then(|h| payload_of(&h, id))
        .ok_or_else(|| io::Error::other("the entry is gone"))?;
    write_payload(&payload).map_err(|_| io::Error::other("the clipboard is busy"))
}

/// Enter: runs the paste sequence on a thread of its own, so the UI thread never waits for the
/// target window or for the 300 ms before the restore. `raise` shows a Notice.
pub fn paste_entry(id: EntryId, target: Option<isize>, raise: fn(Notice)) {
    let spawned = std::thread::Builder::new()
        .name("paste".into())
        .spawn(move || {
            let Some(payload) = shared().and_then(|h| payload_of(&h, id)) else {
                raise(notice(
                    "That entry is no longer there.",
                    NoticeKind::Warning,
                ));
                return;
            };
            if let Outcome::Aborted(why) = paste(&payload, target, &mut Board, &mut Foreground) {
                raise(notice(abort_message(why), NoticeKind::Warning));
            }
        });
    if spawned.is_err() {
        raise(notice("The paste could not start.", NoticeKind::Error));
    }
}

fn abort_message(why: Abort) -> &'static str {
    match why {
        Abort::NoTarget => "There is no window to paste into.",
        Abort::WriteFailed => "The clipboard is busy, try again.",
        Abort::ForegroundChanged => "Paste cancelled: the window in front changed.",
        Abort::Elevated => "Paste is blocked: that window runs as administrator.",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // @R-015
    #[test]
    fn r015_every_abort_has_its_own_message_and_names_the_administrator_case() {
        let all = [
            Abort::NoTarget,
            Abort::WriteFailed,
            Abort::ForegroundChanged,
            Abort::Elevated,
        ];
        let msgs: std::collections::HashSet<_> = all.iter().map(|a| abort_message(*a)).collect();
        assert_eq!(msgs.len(), 4);
        assert!(abort_message(Abort::Elevated).contains("administrator"));
    }

    // @D-019
    #[test]
    fn d019_restore_covers_text_image_filelist_only_and_within_the_limits() {
        assert!(restorable(Content::Text("x".into())).is_some());
        assert!(restorable(Content::Text("x".repeat(MAX_TEXT_BYTES + 1))).is_none());
        assert!(restorable(Content::Dib(vec![0; MAX_BITMAP_BYTES + 1])).is_none());
        assert!(restorable(Content::Files(vec!["a".into(); MAX_FILE_PATHS + 1])).is_none());
    }
}
