//! Clipboard capture (NCMD-19): every copy the Owner is allowed to keep lands in the DPAPI-protected
//! history. P1 forwards the change event; a worker thread of its own reads the clipboard and stores
//! it (T-021), so the UI thread only sends a number down a channel.
use super::{EventKind, Modules};
use crate::logging;
use naobi_clipboard::capture::{capture, Capture};
use naobi_clipboard::limits::Limiter;
use naobi_clipboard::{Captured, Cipher, ClipSource, Clock, Content, Flags, History};
use naobi_core::db::Db;
use naobi_core::event::AppEvent;
use naobi_win32::clipboard::{self, Snapshot};
use naobi_win32::{dpapi, random};
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;
use zeroize::Zeroizing;

/// A change event: the sequence number and the exe that owned the clipboard when it arrived.
type Change = (u32, Option<String>);

/// Perf builds measure a fixed setup, so capture stays off unless a test asks for it.
fn enabled() -> bool {
    !cfg!(feature = "perf") || std::env::var_os("NAOBI_TEST_CLIPBOARD").is_some()
}

pub fn register(m: &mut Modules) {
    if !enabled() {
        return;
    }
    let Some(history) = open_history() else {
        tracing::warn!("clipboard history is off: the store could not be opened");
        return;
    };
    let (tx, rx): (Sender<Change>, Receiver<Change>) = mpsc::channel();
    let spawned = std::thread::Builder::new()
        .name("clipboard".into())
        .spawn(move || run(&rx, history));
    if spawned.is_err() {
        tracing::warn!("clipboard history is off: the worker did not start");
        return;
    }
    m.subscribe_event(
        EventKind::ClipboardChanged,
        Box::new(move |ev, _| {
            if let AppEvent::ClipboardChanged(seq) = ev {
                // The owner is asked now: it may be gone by the time the worker reads.
                let _ = tx.send((*seq, clipboard::owner_exe()));
            }
        }),
    );
    if !clipboard::install_listener() {
        tracing::warn!("clipboard history is off: the change listener was not installed");
    }
}

/// The history on its own connection to the data file: the worker thread owns it.
fn open_history() -> Option<History> {
    let path = naobi_core::paths::data_dir()?.join("naobi.db");
    std::fs::create_dir_all(path.parent()?).ok()?;
    let seed = random::random16().ok()?;
    let db = Db::open(&path, move || seed).ok()?;
    Some(History::new(
        db,
        Box::new(DpapiCipher),
        Box::new(SystemClock),
    ))
}

struct DpapiCipher;

impl Cipher for DpapiCipher {
    fn encrypt(&self, plain: &[u8]) -> Result<Vec<u8>, naobi_clipboard::ports::CipherError> {
        dpapi::protect(plain).map_err(|_| naobi_clipboard::ports::CipherError)
    }

    fn decrypt(
        &self,
        blob: &[u8],
    ) -> Result<Zeroizing<Vec<u8>>, naobi_clipboard::ports::CipherError> {
        dpapi::unprotect(blob)
            .map(Zeroizing::new)
            .map_err(|_| naobi_clipboard::ports::CipherError)
    }
}

struct SystemClock;

impl Clock for SystemClock {
    fn now_ms(&self) -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_millis() as i64)
    }
}

/// The clipboard as the history reads it. `owner` is who owned it when the change event arrived.
struct WinSource {
    owner: Option<String>,
}

impl ClipSource for WinSource {
    fn sequence(&self) -> u32 {
        clipboard::sequence()
    }

    fn read(&self) -> Option<Captured> {
        let mut captured = clipboard::read().and_then(to_captured)?;
        captured.owner_exe = self.owner.clone().or(captured.owner_exe);
        Some(captured)
    }
}

/// Files first, then text, then a bitmap, as the reader filled them. None when there is nothing.
fn to_captured(s: Snapshot) -> Option<Captured> {
    let content = if !s.files.is_empty() {
        Content::Files(s.files)
    } else if let Some(t) = s.text {
        Content::Text(t)
    } else {
        Content::Dib(s.dib?)
    };
    Some(Captured {
        flags: Flags {
            exclude_from_monitor: s.exclude_from_monitor,
            can_include_in_history: s.can_include_in_history,
            can_upload_to_cloud: s.can_upload_to_cloud,
        },
        marker: s.marker,
        owner_exe: s.owner_exe,
        content,
    })
}

/// Each change event waits its turn at the limiter, then captures what the clipboard holds by then:
/// the last state of a burst is the one stored (trailing edge).
fn run(rx: &Receiver<Change>, mut history: History) {
    let mut limiter = Limiter::default();
    let clock = SystemClock;
    while let Ok((mut seq, mut owner)) = rx.recv() {
        while let Err(wait) = limiter.admit(clock.now_ms()) {
            std::thread::sleep(Duration::from_millis(wait));
        }
        while let Ok(newer) = rx.try_recv() {
            (seq, owner) = newer;
        }
        match capture(&WinSource { owner }, seq, &mut history) {
            Ok(Capture::Decided(o)) => {
                let p = o.purged;
                if p.expired + p.excess + p.evicted > 0 {
                    logging::clipboard_purge(p.expired, p.excess, p.evicted);
                }
            }
            // The clipboard changed again while it was read: its own event captures the newer state.
            Ok(Capture::Discarded | Capture::Nothing) => {}
            Err(e) => tracing::warn!(error = %e, "a clipboard capture failed"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snap() -> Snapshot {
        Snapshot {
            owner_exe: Some("notepad.exe".into()),
            ..Snapshot::default()
        }
    }

    // @R-009
    #[test]
    fn r009_snapshot_maps_to_content_and_carries_the_privacy_flags() {
        let mut s = snap();
        s.text = Some("t".into());
        s.files = vec![r"C:\a.txt".into()];
        s.exclude_from_monitor = true;
        s.can_include_in_history = Some(0);
        let c = to_captured(s).expect("captured");
        assert_eq!(
            c.content,
            Content::Files(vec![r"C:\a.txt".into()]),
            "files win over text"
        );
        assert!(c.flags.exclude_from_monitor);
        assert_eq!(c.flags.can_include_in_history, Some(0));
        assert_eq!(c.owner_exe.as_deref(), Some("notepad.exe"));

        let mut s = snap();
        s.dib = Some(vec![1, 2, 3]);
        assert_eq!(
            to_captured(s).expect("dib").content,
            Content::Dib(vec![1, 2, 3])
        );
        assert!(to_captured(snap()).is_none(), "nothing readable");
    }
}
