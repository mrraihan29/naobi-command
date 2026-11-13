//! Tier 2 (interactive desktop, dogfood instance quit, PowerToys Run off): Alt+Space, type a name,
//! Enter starts the target and the Launcher cloaks. The `perf` feature is on for this file through
//! the self dev-dependency, so `inject` and the test target source exist. The spawned exe gets a
//! throwaway autostart key, removed when the test ends.
use naobi_win32::{autostart, dwm, inject};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

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

struct RunKey(String);

impl Drop for RunKey {
    fn drop(&mut self) {
        autostart::remove_key_tree(&self.0);
    }
}

// @R-012
#[test]
#[ignore = "tier2: interactive desktop"] // naobi-allow: tier 2 test, D-025
fn r012_hotkey_to_app_launch() {
    let dir = std::env::temp_dir().join(format!("naobi-flow-launch-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let marker = dir.join("started.txt");
    let script = dir.join("target.cmd");
    let body = format!("@echo off\r\necho started> \"{}\"\r\n", marker.display());
    assert!(std::fs::write(&script, body).is_ok(), "write the target");
    let run_key = RunKey(format!(
        r"Software\NaobiCommandTest\flow-launch-{}",
        std::process::id()
    ));
    let mut app = match Command::new(env!("CARGO_BIN_EXE_naobi-app"))
        .env("NAOBI_TEST_RUN_KEY", &run_key.0)
        .env("NAOBI_TEST_TARGET", &script)
        .env("NAOBI_PERF_ITEMS", "0")
        .env("NAOBI_DATA_DIR", dir.join("data"))
        .env("NAOBI_SETTINGS_DIR", dir.join("settings"))
        .stdout(Stdio::null())
        .spawn()
    {
        Ok(child) => child,
        Err(e) => panic!("spawn naobi-app: {e}"),
    };
    let launcher = || dwm::find_window(naobi_ui::TITLE);
    let ready = until(Duration::from_secs(5), || launcher().is_some());
    let shown = ready && {
        inject::alt_space();
        until(Duration::from_secs(2), || {
            launcher().is_some_and(|h| !dwm::is_cloaked(h))
        })
    };
    let launched = shown && {
        // The first keys wait for the input to take focus; a person is never faster than this.
        std::thread::sleep(Duration::from_millis(300));
        inject::text("naobitarget");
        std::thread::sleep(Duration::from_millis(300));
        inject::enter();
        until(Duration::from_secs(5), || marker.exists())
    };
    let cloaked = launched
        && until(Duration::from_secs(2), || {
            launcher().is_some_and(dwm::is_cloaked)
        });
    let _ = app.kill();
    let _ = app.wait();
    if cloaked {
        let _ = std::fs::remove_dir_all(&dir);
    } else {
        // Kept for the post-mortem: the app's log is under it.
        eprintln!("test data kept in {}", dir.display());
    }
    assert!(ready, "launcher window never appeared");
    assert!(shown, "Alt+Space did not show the launcher");
    assert!(launched, "Enter did not start the target");
    assert!(cloaked, "the launcher did not cloak after the launch");
}

fn notepad_pids() -> Vec<u32> {
    let out = Command::new("tasklist")
        .args(["/FO", "CSV", "/NH", "/FI", "IMAGENAME eq Notepad.exe"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default();
    out.lines()
        .filter_map(|l| l.split("\",\"").nth(1)?.parse().ok())
        .collect()
}

// @R-012
#[test]
#[ignore = "tier2: interactive desktop"] // naobi-allow: tier 2 test, D-025
fn r012_hotkey_to_an_installed_app_from_the_catalog() {
    let dir = std::env::temp_dir().join(format!("naobi-flow-apps-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let run_key = RunKey(format!(
        r"Software\NaobiCommandTest\flow-apps-{}",
        std::process::id()
    ));
    let before = notepad_pids();
    let mut app = match Command::new(env!("CARGO_BIN_EXE_naobi-app"))
        .env("NAOBI_TEST_RUN_KEY", &run_key.0)
        .env("NAOBI_TEST_APPS", "1")
        .env("NAOBI_PERF_ITEMS", "0")
        .env("NAOBI_DATA_DIR", dir.join("data"))
        .env("NAOBI_SETTINGS_DIR", dir.join("settings"))
        .stdout(Stdio::null())
        .spawn()
    {
        Ok(child) => child,
        Err(e) => panic!("spawn naobi-app: {e}"),
    };
    let launcher = || dwm::find_window(naobi_ui::TITLE);
    let ready = until(Duration::from_secs(5), || launcher().is_some());
    // The catalog is scanned in a child process after start; the shell needs about a second.
    std::thread::sleep(Duration::from_secs(5));
    let shown = ready && {
        inject::alt_space();
        until(Duration::from_secs(2), || {
            launcher().is_some_and(|h| !dwm::is_cloaked(h))
        })
    };
    let started = shown && {
        inject::text("notepad");
        std::thread::sleep(Duration::from_millis(500));
        inject::enter();
        until(Duration::from_secs(10), || {
            notepad_pids().iter().any(|p| !before.contains(p))
        })
    };
    let cloaked = started
        && until(Duration::from_secs(2), || {
            launcher().is_some_and(dwm::is_cloaked)
        });
    for pid in notepad_pids().into_iter().filter(|p| !before.contains(p)) {
        let _ = Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/F"])
            .output();
    }
    let _ = app.kill();
    let _ = app.wait();
    if cloaked {
        let _ = std::fs::remove_dir_all(&dir);
    } else {
        eprintln!("test data kept in {}", dir.display());
    }
    assert!(ready, "launcher window never appeared");
    assert!(shown, "Alt+Space did not show the launcher");
    assert!(started, "Enter on \"notepad\" did not start Notepad");
    assert!(cloaked, "the launcher did not cloak after the launch");
}
