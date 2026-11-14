//! Tier 2 (interactive desktop, dogfood instance quit): Win+Shift+C opens the overlay on a thread of
//! its own, a click on a window of a known color puts its HEX on the clipboard with the private
//! marker and records the pick, and the overlay is gone afterwards. The helper window belongs to the
//! test process.
use naobi_core::db::Db;
use naobi_picker::{ColorHistory, Rgb};
use naobi_win32::edit_window::ColorWindow;
use naobi_win32::{autostart, clipboard_marker, dwm, inject};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

fn until(timeout: Duration, mut cond: impl FnMut() -> bool) -> bool {
    let end = Instant::now() + timeout;
    while Instant::now() < end {
        if cond() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    cond()
}

struct RunKey(String);

impl Drop for RunKey {
    fn drop(&mut self) {
        autostart::remove_key_tree(&self.0);
    }
}

// @R-017
#[test]
#[ignore = "tier2: interactive desktop"] // naobi-allow: tier 2 test, D-025
fn r017_pick_known_pixel_copies_hex() {
    naobi_win32::edit_window::dpi_aware();
    let dir = std::env::temp_dir().join(format!("naobi-flow-picker-{}", std::process::id()));
    let run_key = RunKey(format!(
        r"Software\NaobiCommandTest\flow-picker-{}",
        std::process::id()
    ));
    let mut app = match Command::new(env!("CARGO_BIN_EXE_naobi-app"))
        .env("NAOBI_TEST_RUN_KEY", &run_key.0)
        .env("NAOBI_PERF_ITEMS", "0")
        .env("NAOBI_DATA_DIR", dir.join("data"))
        .env("NAOBI_SETTINGS_DIR", dir.join("settings"))
        .stdout(Stdio::null())
        .spawn()
    {
        Ok(child) => child,
        Err(e) => panic!("spawn naobi-app: {e}"),
    };
    let ready = until(Duration::from_secs(5), || {
        dwm::find_window(naobi_ui::TITLE).is_some()
    });
    std::thread::sleep(Duration::from_millis(800));

    let window = ColorWindow::open(0x914A3B);
    // The overlay's window thread must be neither the Launcher's nor the platform thread's: the
    // picker runs on a thread of its own (T-021).
    let mut overlay_thread = None;
    let mut copied = None;
    if let Some(w) = window.as_ref() {
        std::thread::sleep(Duration::from_millis(300));
        inject::win_shift_c();
        let shown = until(Duration::from_secs(3), || {
            dwm::find_window(naobi_win32::picker::OVERLAY_TITLE).is_some()
        });
        if shown {
            overlay_thread =
                dwm::find_window(naobi_win32::picker::OVERLAY_TITLE).map(dwm::window_thread);
            let (x, y) = w.center();
            inject::click_at(x, y);
            copied = until(Duration::from_secs(3), || {
                clipboard_marker::read_text().as_deref() == Some("#914A3B")
                    && clipboard_marker::has_marker()
            })
            .then_some(true);
        }
    }
    let gone = until(Duration::from_secs(3), || {
        dwm::find_window(naobi_win32::picker::OVERLAY_TITLE).is_none()
    });
    let launcher_thread = dwm::find_window(naobi_ui::TITLE).map(dwm::window_thread);
    let _ = app.kill();
    let _ = app.wait();
    // The pick is in the history on disk.
    let history = Db::open(&dir.join("data").join("naobi.db"), || [0; 16])
        .ok()
        .and_then(|db| ColorHistory::new(db).list().ok())
        .unwrap_or_default();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(ready, "launcher window never appeared");
    assert!(window.is_some(), "the helper window did not open");
    assert!(overlay_thread.is_some(), "the overlay never appeared");
    assert_ne!(
        overlay_thread, launcher_thread,
        "d023_overlay_runs_off_the_ui_thread"
    );
    assert_eq!(copied, Some(true), "the clipboard did not get #914A3B");
    assert!(gone, "the overlay stayed after the click");
    assert_eq!(
        history.first().map(|p| p.rgb),
        Some(Rgb {
            r: 0x91,
            g: 0x4A,
            b: 0x3B
        })
    );
}
