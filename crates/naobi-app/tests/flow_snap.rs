//! Tier 2 (interactive desktop, dogfood instance quit): Ctrl+Alt+Left snaps the window in front to
//! the left half of its display, Ctrl+Alt+Backspace puts it back, and a window command run from the
//! Launcher list does the same as its key. The helper window belongs to the test process.
use naobi_win32::edit_window::EditWindow;
use naobi_win32::window_ops::Ops;
use naobi_win32::{autostart, dwm, inject};
use naobi_winmgr::plan::{target, Command};
use naobi_winmgr::{Monitor, Rect, WindowOps};
use std::process::{Child, Command as Process, Stdio};
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

fn start(dir: &std::path::Path, run_key: &str) -> Child {
    match Process::new(env!("CARGO_BIN_EXE_naobi-app"))
        .env("NAOBI_TEST_RUN_KEY", run_key)
        .env("NAOBI_PERF_ITEMS", "0")
        .env("NAOBI_DATA_DIR", dir.join("data"))
        .env("NAOBI_SETTINGS_DIR", dir.join("settings"))
        .stdout(Stdio::null())
        .spawn()
    {
        Ok(child) => child,
        Err(e) => panic!("spawn naobi-app: {e}"),
    }
}

/// The display that holds the middle of `r`.
fn display_of(r: Rect) -> Monitor {
    let monitors = Ops.monitors();
    let (cx, cy) = (r.left + r.width() / 2, r.top + r.height() / 2);
    monitors
        .iter()
        .copied()
        .find(|m| {
            (m.work.left..m.work.right).contains(&cx) && (m.work.top..m.work.bottom).contains(&cy)
        })
        .or(monitors.first().copied())
        .unwrap_or_else(|| panic!("no display"))
}

// @R-016
#[test]
#[ignore = "tier2: interactive desktop"] // naobi-allow: tier 2 test, D-025
fn r016_ctrl_alt_left_snaps_foreground_window() {
    naobi_win32::edit_window::dpi_aware();
    let dir = std::env::temp_dir().join(format!("naobi-flow-snap-{}", std::process::id()));
    let run_key = RunKey(format!(
        r"Software\NaobiCommandTest\flow-snap-{}",
        std::process::id()
    ));
    let mut app = start(&dir, &run_key.0);
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
    let (start_outer, expected, snapped, restored) = match window.as_ref() {
        Some(w) if focused => {
            let (outer0, frame0) = w.rects();
            let on = display_of(frame0);
            let expected = target(Command::LeftHalf, frame0, &on, 8).unwrap_or_default();
            inject::ctrl_alt(0x25); // Left
            let snapped = until(Duration::from_secs(3), || {
                let (_, f) = w.rects();
                f == expected
            });
            let after = w.rects().1;
            inject::ctrl_alt(0x08); // Backspace: restore
            let restored = until(Duration::from_secs(3), || w.rects().0 == outer0);
            (outer0, expected, (snapped, after), restored)
        }
        _ => (
            Rect::default(),
            Rect::default(),
            (false, Rect::default()),
            false,
        ),
    };
    drop(window);
    let _ = app.kill();
    let _ = app.wait();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(ready, "launcher window never appeared");
    assert!(focused, "the helper window did not take the foreground");
    assert!(
        snapped.0,
        "visible frame {:?}, expected {expected:?} (started at {start_outer:?})",
        snapped.1
    );
    assert!(restored, "Ctrl+Alt+Backspace did not put the window back");
}
