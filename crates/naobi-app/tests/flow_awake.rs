//! Tier 2 (interactive desktop, dogfood instance quit): type `awake 1m` into the Launcher and the
//! system is held awake; `awake off` lets it sleep again. The request is read back through
//! `SystemExecutionState`, which needs no elevated `powercfg`.
use naobi_win32::power::execution_state;
use naobi_win32::{autostart, dwm, inject};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const SYSTEM: u32 = 0x1;

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

/// Opens the Launcher, types `line` and presses Enter.
fn command(line: &str) -> bool {
    let visible = || dwm::find_window(naobi_ui::TITLE).is_some_and(|h| !dwm::is_cloaked(h));
    if !visible() {
        inject::launcher();
        if !until(Duration::from_secs(3), visible) {
            return false;
        }
    }
    std::thread::sleep(Duration::from_millis(400));
    inject::text(line);
    std::thread::sleep(Duration::from_millis(600));
    inject::enter();
    true
}

// @R-019
#[test]
#[ignore = "tier2: interactive desktop"] // naobi-allow: tier 2 test, D-025
fn r019_awake_command_holds_and_releases_the_system() {
    let dir = std::env::temp_dir().join(format!("naobi-flow-awake-{}", std::process::id()));
    let run_key = RunKey(format!(
        r"Software\NaobiCommandTest\flow-awake-{}",
        std::process::id()
    ));
    let before = execution_state();
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

    let sent_on = ready && command("awake 1m");
    let held = until(Duration::from_secs(4), || execution_state() & SYSTEM != 0);
    let sent_off = held && command("awake off");
    // Another program may hold the system awake by itself; then only "held" can be asserted.
    let released =
        before & SYSTEM != 0 || until(Duration::from_secs(4), || execution_state() & SYSTEM == 0);
    let _ = app.kill();
    let _ = app.wait();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(ready, "launcher window never appeared");
    assert!(sent_on, "the Launcher did not open");
    assert!(held, "awake 1m did not hold the system awake");
    assert!(sent_off, "the Launcher did not open for the second command");
    assert!(released, "awake off did not release the system");
}
