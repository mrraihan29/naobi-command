//! Tier 2 (interactive desktop, dogfood instance quit): Win+Ctrl+T makes the window in front topmost
//! with a 2 px border around its visible frame, the border follows the window when it moves, and a
//! second press takes both away. The helper window belongs to the test process.
use naobi_aot::Rect;
use naobi_win32::edit_window::EditWindow;
use naobi_win32::{autostart, dwm, inject};
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

/// The ring the app should draw: the visible frame grown by 2 px on every side.
fn ring(frame: Rect) -> Rect {
    Rect::new(
        frame.left - 2,
        frame.top - 2,
        frame.right + 2,
        frame.bottom + 2,
    )
}

// @R-018
#[test]
#[ignore = "tier2: interactive desktop"] // naobi-allow: tier 2 test, D-025
fn r018_border_follows_window_and_disappears() {
    naobi_win32::edit_window::dpi_aware();
    let dir = std::env::temp_dir().join(format!("naobi-flow-topmost-{}", std::process::id()));
    let run_key = RunKey(format!(
        r"Software\NaobiCommandTest\flow-topmost-{}",
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

    let window = EditWindow::open();
    let focused = window.as_ref().is_some_and(|w| {
        w.focus();
        until(Duration::from_secs(2), || {
            naobi_win32::paste::foreground_window() == w.hwnd()
        })
    });
    let mut seen = (false, false, false, false);
    if let (Some(w), true) = (window.as_ref(), focused) {
        inject::win_ctrl_t();
        let on = until(Duration::from_secs(3), || {
            w.is_topmost() && EditWindow::border_rect().is_some()
        });
        let at_first = EditWindow::border_rect() == Some(ring(w.rects().1));
        // Move the window: the border follows.
        w.move_to(300, 260);
        let follows = until(Duration::from_secs(3), || {
            EditWindow::border_rect() == Some(ring(w.rects().1))
        });
        inject::win_ctrl_t();
        let off = until(Duration::from_secs(3), || {
            !w.is_topmost() && EditWindow::border_rect().is_none()
        });
        seen = (on && at_first, follows, off, true);
    }
    drop(window);
    let _ = app.kill();
    let _ = app.wait();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(ready, "launcher window never appeared");
    assert!(focused, "the helper window did not take the foreground");
    assert!(seen.3 && seen.0, "no topmost window with a 2 px border");
    assert!(seen.1, "the border did not follow the moved window");
    assert!(seen.2, "the second press did not take the border away");
}
