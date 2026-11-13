//! Tier 2 (interactive desktop, dogfood instance quit): another program holds Alt+Space, so the app
//! cannot register its Launcher hotkey and must raise the conflict Notice (R-008, D-018).
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

// @R-008
#[test]
#[ignore = "tier2: interactive desktop"] // naobi-allow: tier 2 test, D-025
fn r008_conflict_notice_names_hotkey() {
    let dir = std::env::temp_dir().join(format!("naobi-flow-notice-{}", std::process::id()));
    let run_key = RunKey(format!(
        r"Software\NaobiCommandTest\flow-notice-{}",
        std::process::id()
    ));
    // This process owns the chord first (if another program already does, the conflict is there too).
    let _held = inject::hold_alt_space();
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
    let log = dir.join("data").join("logs").join("naobi-command.log");
    let raised = until(Duration::from_secs(5), || {
        std::fs::read_to_string(&log).is_ok_and(|t| t.contains("notice raised"))
    });
    let text = std::fs::read_to_string(&log).unwrap_or_default();
    let _ = app.kill();
    let _ = app.wait();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(ready, "launcher window never appeared");
    assert!(raised, "no Notice was raised for the taken hotkey: {text}");
    assert!(
        !text.contains("hotkey pressed"),
        "no hotkey was pressed in this test"
    );
}
