//! Tier 2 (interactive desktop, dogfood instance quit): on the real window, Alt pressed alone does not
//! open the system menu (typing still reaches the search field), and Alt+1 runs the first visible
//! result: `2+3` then Alt+1 copies `5` and the Launcher closes (R-028, RK-18).
use naobi_win32::{autostart, clipboard_marker, dwm, inject, paste};
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

// @R-028
#[test]
#[ignore = "tier2: interactive desktop"] // naobi-allow: tier 2 test, D-025
fn r028_alt_alone_keeps_typing_and_alt_1_runs_the_first_result() {
    let dir = std::env::temp_dir().join(format!("naobi-flow-alt-{}", std::process::id()));
    let run_key = RunKey(format!(
        r"Software\NaobiCommandTest\flow-alt-{}",
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
    let window = || dwm::find_window(naobi_ui::TITLE);
    let visible = || window().is_some_and(|h| !dwm::is_cloaked(h));
    let ready = until(Duration::from_secs(5), || window().is_some());
    std::thread::sleep(Duration::from_millis(800));

    inject::launcher();
    let focused = until(Duration::from_secs(3), || {
        visible() && window() == Some(paste::foreground_window())
    });
    let mut copied = false;
    let mut closed = false;
    if focused {
        std::thread::sleep(Duration::from_millis(400));
        inject::alt(None);
        std::thread::sleep(Duration::from_millis(300));
        // With the system menu open the `2` would be eaten and the first result would read 3.
        inject::text("2+3");
        std::thread::sleep(Duration::from_millis(600));
        inject::alt(Some(u16::from(b'1')));
        copied = until(Duration::from_secs(3), || {
            clipboard_marker::read_text().as_deref() == Some("5")
        });
        closed = until(Duration::from_secs(2), || !visible());
    }
    let _ = app.kill();
    let _ = app.wait();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(ready, "launcher window never appeared");
    assert!(focused, "the Launcher did not take the foreground");
    assert!(copied, "Alt+1 did not copy the first result (5)");
    assert!(closed, "the Launcher stayed open after Alt+1");
}
