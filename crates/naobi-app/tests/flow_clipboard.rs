//! Tier 2 (interactive desktop, dogfood instance quit): copy text, open the history with Alt+Shift+V,
//! press Enter, and the text appears in the window that had focus. The Owner's clipboard text is put
//! back afterwards.
use naobi_core::db::Db;
use naobi_win32::edit_window::EditWindow;
use naobi_win32::{autostart, dwm, inject};
use std::path::Path;
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

fn powershell(script: &str) -> String {
    Command::new("powershell")
        .args(["-NoProfile", "-Command", script])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default()
}

fn entries(db_file: &Path) -> i64 {
    Db::open(db_file, || [0; 16])
        .ok()
        .and_then(|db| {
            db.conn()
                .query_row("SELECT COUNT(*) FROM clip_entry", [], |r| r.get(0))
                .ok()
        })
        .unwrap_or(0)
}

// @R-015
#[test]
#[ignore = "tier2: interactive desktop"] // naobi-allow: tier 2 test, D-025
fn r015_copy_open_enter_pastes_into_previous_app() {
    let dir = std::env::temp_dir().join(format!("naobi-flow-paste-{}", std::process::id()));
    let data = dir.join("data");
    let run_key = RunKey(format!(
        r"Software\NaobiCommandTest\flow-paste-{}",
        std::process::id()
    ));
    let marker = format!("zq-paste-marker-{}", std::process::id());
    let previous = powershell("Get-Clipboard -Raw");
    let mut app = match Command::new(env!("CARGO_BIN_EXE_naobi-app"))
        .env("NAOBI_TEST_RUN_KEY", &run_key.0)
        .env("NAOBI_TEST_CLIPBOARD", "1")
        .env("NAOBI_PERF_ITEMS", "0")
        .env("NAOBI_DATA_DIR", &data)
        .env("NAOBI_SETTINGS_DIR", dir.join("settings"))
        .stdout(Stdio::null())
        .spawn()
    {
        Ok(child) => child,
        Err(e) => panic!("spawn naobi-app: {e}"),
    };
    let launcher = || dwm::find_window(naobi_ui::TITLE);
    let ready = until(Duration::from_secs(5), || launcher().is_some());
    std::thread::sleep(Duration::from_millis(800));
    // Copy, and wait until the history has it.
    powershell(&format!("Set-Clipboard -Value '{marker}'"));
    let stored = until(Duration::from_secs(8), || {
        entries(&data.join("naobi.db")) >= 1
    });
    // The paste target: a window of this process that takes the foreground.
    let target = EditWindow::open();
    let focused = target.as_ref().is_some_and(|t| {
        t.focus();
        until(Duration::from_secs(2), || {
            naobi_win32::paste::foreground_window() == t.hwnd()
        })
    });
    let shown = focused && {
        inject::alt_shift_v();
        until(Duration::from_secs(3), || {
            launcher().is_some_and(|h| !dwm::is_cloaked(h))
        })
    };
    let pasted = shown && {
        // The newest entry is first; give the input focus a moment, as a person would.
        std::thread::sleep(Duration::from_millis(500));
        inject::enter();
        until(Duration::from_secs(5), || {
            target.as_ref().is_some_and(|t| t.text().contains(&marker))
        })
    };
    let text = target.as_ref().map(EditWindow::text).unwrap_or_default();
    drop(target);
    let _ = app.kill();
    let _ = app.wait();
    std::thread::sleep(Duration::from_millis(200));
    if !previous.trim().is_empty() {
        powershell(&format!(
            "Set-Clipboard -Value '{}'",
            previous.trim_end().replace('\'', "''")
        ));
    }
    if !pasted {
        let log = data.join("logs").join("naobi-command.log");
        eprintln!(
            "app log:
{}",
            std::fs::read_to_string(log).unwrap_or_default()
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
    assert!(ready, "launcher window never appeared");
    assert!(stored, "the copy was not stored");
    assert!(focused, "the target window did not take the foreground");
    assert!(shown, "Alt+Shift+V did not open the history");
    assert!(pasted, "the entry was not pasted into the target: {text:?}");
}
