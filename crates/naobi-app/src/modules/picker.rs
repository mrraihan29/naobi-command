//! The color picker (NCMD-24): its hotkey, the pick on a thread of its own, the copy and the history.
//! The overlay and its message loop live and die on that thread, so the platform thread keeps only
//! forwarding (T-021) and no thread exists while no pick is active.
use super::Modules;
use naobi_core::db::Db;
use naobi_core::ids::HotkeyId;
use naobi_core::notice::{Notice, NoticeKind};
use naobi_picker::view::HOTKEY;
use naobi_picker::{pick, ColorHistory, ColorHistoryView, PickerCommands, Shared};
use naobi_win32::picker::{overlay, ScreenGrab};
use naobi_win32::{clipboard_marker, random};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

/// The history on its own connection to the data file; in memory when the file cannot be opened,
/// so a pick still copies.
fn open_history() -> Option<Shared> {
    let on_disk = naobi_core::paths::data_dir().and_then(|dir| {
        std::fs::create_dir_all(&dir).ok()?;
        let seed = random::random16().ok()?;
        Db::open(&dir.join("naobi.db"), move || seed).ok()
    });
    let db = match on_disk {
        Some(db) => db,
        None => {
            tracing::warn!(
                "the color history is kept in memory: the data file could not be opened"
            );
            Db::open_in_memory().ok()?
        }
    };
    Some(Arc::new(Mutex::new(ColorHistory::new(db))))
}

pub fn register(m: &mut Modules) {
    let Some(history) = open_history() else {
        return;
    };
    m.register_source(Box::new(PickerCommands));
    m.register_view(Box::new(ColorHistoryView::new(history.clone())));
    let active = Arc::new(AtomicBool::new(false));
    m.subscribe(
        HotkeyId(HOTKEY.into()),
        Box::new(move |_, cx| {
            // One pick at a time: a second press while the overlay is up does nothing.
            if active.swap(true, Ordering::SeqCst) {
                return;
            }
            let (history, flag) = (history.clone(), active.clone());
            // Closes the Launcher when the hotkey came while it was open.
            cx.hide();
            let spawned = std::thread::Builder::new()
                .name("color-picker".into())
                .spawn(move || {
                    let _free = Free(flag);
                    run(&history);
                });
            if spawned.is_err() {
                active.store(false, Ordering::SeqCst);
                cx.notice(Notice {
                    subject: "Color picker".into(),
                    message: "it could not start".into(),
                    kind: NoticeKind::Error,
                });
            }
        }),
    );
}

/// Lets the next pick start when this one ends, however it ends.
struct Free(Arc<AtomicBool>);

impl Drop for Free {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

/// One pick, start to end, on the picker thread. Nothing of the screen is logged (T-015).
fn run(history: &Shared) {
    let palette = crate::active_palette();
    let Some(color) = pick(&mut ScreenGrab, |snap| overlay(snap, &palette)) else {
        tracing::info!(picked = false, "color pick");
        return;
    };
    tracing::info!(picked = true, "color pick");
    if clipboard_marker::write_text(&color.hex()).is_err() {
        tracing::warn!("the picked color could not be put on the clipboard");
        warn("The color could not be copied to the clipboard.");
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64);
    let pushed = history
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .push(color, now);
    if pushed.is_err() {
        tracing::warn!("the picked color could not be added to the history");
        warn("The color was not added to the history.");
    }
}

/// A Notice from the picker thread, shown by the controller (the thread has no `Cx`).
fn warn(message: &str) {
    let n = Notice {
        subject: "Color picker".into(),
        message: message.into(),
        kind: NoticeKind::Warning,
    };
    crate::to_modules(move |c, h| c.on_notice(n, h));
}
