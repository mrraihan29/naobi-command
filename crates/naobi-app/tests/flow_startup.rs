//! Tier 2 (interactive desktop, dogfood instance quit, PowerToys Run off): starts the real exe,
//! reads its readiness lines and drives it with Alt+Space. The `perf` feature is on for this file
//! through the self dev-dependency, so the exe prints its lines and `inject` exists.
use naobi_win32::{dwm, inject, platform, token};
use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::{mpsc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

// Helpers live in a cfg(test) module so clippy.toml's unwrap-in-tests allowance covers them.
#[cfg(test)]
mod support {
    use super::*;
    /// One app at a time: they all own Alt+Space.
    pub static DESKTOP: Mutex<()> = Mutex::new(());

    pub struct App {
        pub child: Child,
        lines: mpsc::Receiver<(String, i64)>,
        start: i64,
        _desktop: MutexGuard<'static, ()>,
    }

    impl Drop for App {
        fn drop(&mut self) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }

    impl App {
        pub fn launch(profile: Option<&std::path::Path>) -> App {
            let desktop = DESKTOP.lock().unwrap_or_else(|e| e.into_inner());
            let mut cmd = Command::new(env!("CARGO_BIN_EXE_naobi-app"));
            cmd.stdout(Stdio::piped());
            if let Some(dir) = profile {
                cmd.env("APPDATA", dir).env("LOCALAPPDATA", dir);
            }
            let start = platform::qpc();
            let mut child = cmd.spawn().unwrap();
            let out = child.stdout.take().unwrap();
            let (tx, lines) = mpsc::channel();
            std::thread::spawn(move || {
                for line in BufReader::new(out).lines().map_while(Result::ok) {
                    let v: serde_json::Value = serde_json::from_str(&line).unwrap();
                    let _ = tx.send((
                        v["ev"].as_str().unwrap().to_string(),
                        v["qpc"].as_i64().unwrap(),
                    ));
                }
            });
            App {
                child,
                lines,
                start,
                _desktop: desktop,
            }
        }

        /// Waits for the readiness line `ev` and returns its QPC ticks.
        pub fn wait(&self, ev: &str) -> i64 {
            let deadline = Instant::now() + Duration::from_secs(10);
            loop {
                let left = deadline.saturating_duration_since(Instant::now());
                let (name, qpc) = self
                    .lines
                    .recv_timeout(left)
                    .unwrap_or_else(|_| panic!("no {ev} line"));
                if name == ev {
                    return qpc;
                }
            }
        }

        pub fn ms_since_start(&self, qpc: i64) -> f64 {
            platform::qpc_to_ms(qpc - self.start)
        }
    }

    pub fn until(timeout: Duration, mut cond: impl FnMut() -> bool) -> bool {
        let end = Instant::now() + timeout;
        while Instant::now() < end {
            if cond() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        cond()
    }

    pub fn launcher() -> Option<isize> {
        dwm::find_window(naobi_ui::TITLE)
    }

    pub fn visible() -> bool {
        launcher().is_some_and(|h| !dwm::is_cloaked(h))
    }

    pub fn cloaked() -> bool {
        launcher().is_some_and(dwm::is_cloaked)
    }
}
use support::*;

// @R-006
#[test]
#[ignore = "tier2: interactive desktop"] // naobi-allow: tier 2 test, D-025
fn r006_start_to_tray_and_window_ready_within_budget() {
    let app = App::launch(None);
    let tray = app.ms_since_start(app.wait("tray_ready"));
    let window = app.ms_since_start(app.wait("window_ready"));
    assert!(tray <= 300.0, "tray-ready took {tray:.0} ms");
    assert!(window <= 1500.0, "window ready took {window:.0} ms");
}

// @R-006
#[test]
#[ignore = "tier2: interactive desktop"] // naobi-allow: tier 2 test, D-025
fn r006_hotkey_before_window_ready_is_served() {
    let app = App::launch(None);
    app.wait("tray_ready");
    let injected = platform::qpc();
    inject::alt_space();
    let ready = app.wait("window_ready");
    assert!(
        ready > injected,
        "window was ready before the hotkey: the test proves nothing"
    );
    assert!(
        until(Duration::from_secs(2), visible),
        "launcher never became visible"
    );
    let seen = app.ms_since_start(platform::qpc());
    assert!(seen <= 1500.0, "launcher visible after {seen:.0} ms");
}

// @R-023
#[test]
#[ignore = "tier2: interactive desktop"] // naobi-allow: tier 2 test, D-025
fn r023_hwnd_stable_across_100_cycles() {
    let app = App::launch(None);
    app.wait("window_ready");
    let hwnd = launcher().unwrap();
    for i in 0..100 {
        inject::alt_space();
        assert!(
            until(Duration::from_secs(2), visible),
            "cycle {i}: not shown"
        );
        assert_eq!(launcher(), Some(hwnd), "cycle {i}: HWND changed");
        inject::alt_space();
        assert!(
            until(Duration::from_secs(2), cloaked),
            "cycle {i}: not hidden"
        );
    }
}

// @R-022
#[test]
#[ignore = "tier2: interactive desktop"] // naobi-allow: tier 2 test, D-025
fn r022_launcher_not_in_taskbar_or_alt_tab() {
    let app = App::launch(None);
    app.wait("window_ready");
    assert!(dwm::is_tool_window(launcher().unwrap()));
}

// @R-010
#[test]
#[ignore = "tier2: interactive desktop"] // naobi-allow: tier 2 test, D-025
fn r010_main_token_not_elevated() {
    let app = App::launch(None);
    app.wait("tray_ready");
    assert!(!token::pid_is_elevated(app.child.id()).unwrap());
}

// @R-010
#[test]
fn r010_manifest_requests_as_invoker() {
    let manifest =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/app.manifest")).unwrap();
    assert!(manifest.contains(r#"level="asInvoker""#));
}

// @R-001
#[test]
#[ignore = "tier2: interactive desktop"] // naobi-allow: tier 2 test, D-025
fn r001_runs_without_account() {
    let empty = std::env::temp_dir().join(format!("naobi-empty-profile-{}", std::process::id()));
    std::fs::create_dir_all(&empty).unwrap();
    let app = App::launch(Some(&empty));
    app.wait("tray_ready");
    app.wait("window_ready");
    drop(app);
    let _ = std::fs::remove_dir_all(empty);
}
