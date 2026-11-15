//! Tier 2 (interactive desktop, dogfood instance quit, PowerToys Run off): drives the real exe with
//! Ctrl+Space. The `perf` feature is on for this file through the self dev-dependency, so `inject` exists.
//! Every spawned exe gets a throwaway autostart key, removed when the test ends, so a run never
//! rewrites the Owner's Run value.
use naobi_win32::{autostart, dwm, inject};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// One app at a time: they all own Ctrl+Space.
static DESKTOP: Mutex<()> = Mutex::new(());

/// A throwaway autostart key for one test's spawns. Removed on drop, even when an assertion fails.
struct RunKey(String);

impl RunKey {
    fn new(test: &str) -> Self {
        RunKey(format!(
            r"Software\NaobiCommandTest\flow-{test}-{}",
            std::process::id()
        ))
    }
}

impl Drop for RunKey {
    fn drop(&mut self) {
        autostart::remove_key_tree(&self.0);
    }
}

fn until(timeout: Duration, mut cond: impl FnMut() -> bool) -> bool {
    let end = Instant::now() + timeout;
    while Instant::now() < end {
        if cond() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    cond()
}

fn launcher() -> Option<isize> {
    dwm::find_window(naobi_ui::TITLE)
}

fn visible() -> bool {
    launcher().is_some_and(|h| !dwm::is_cloaked(h))
}

fn spawn_app(run_key: &RunKey) -> Child {
    match Command::new(env!("CARGO_BIN_EXE_naobi-app"))
        .env("NAOBI_TEST_RUN_KEY", &run_key.0)
        .stdout(Stdio::null())
        .spawn()
    {
        Ok(child) => child,
        Err(e) => panic!("spawn naobi-app: {e}"),
    }
}

// @R-008
#[test]
#[ignore = "tier2: interactive desktop"] // naobi-allow: tier 2 test, D-025
fn r008_ctrl_space_opens_launcher() {
    let _desktop = DESKTOP.lock().unwrap_or_else(|e| e.into_inner());
    let run_key = RunKey::new("r008");
    let mut app = spawn_app(&run_key);
    let ready = until(Duration::from_secs(5), || launcher().is_some());
    let shown = ready && {
        inject::launcher();
        until(Duration::from_secs(2), visible)
    };
    let _ = app.kill();
    let _ = app.wait();
    assert!(ready, "launcher window never appeared");
    assert!(shown, "Ctrl+Space did not show the launcher");
}

// @R-023
#[test]
#[ignore = "tier2: interactive desktop"] // naobi-allow: tier 2 test, D-025
fn r023_second_instance_shows_first_and_exits() {
    let _desktop = DESKTOP.lock().unwrap_or_else(|e| e.into_inner());
    let run_key = RunKey::new("r023");
    let mut first = spawn_app(&run_key);
    let ready = until(Duration::from_secs(5), || launcher().is_some());
    let mut second = spawn_app(&run_key);
    let exited = until(Duration::from_secs(5), || {
        second.try_wait().ok().flatten().is_some()
    });
    let first_alive = first.try_wait().ok().flatten().is_none();
    let shown = until(Duration::from_secs(2), visible);
    let _ = second.kill();
    let _ = second.wait();
    let _ = first.kill();
    let _ = first.wait();
    assert!(ready, "first instance never showed its launcher window");
    assert!(exited, "second instance did not exit");
    assert!(first_alive, "first instance is no longer running");
    assert!(shown, "the second launch did not show the first's Launcher");
}
