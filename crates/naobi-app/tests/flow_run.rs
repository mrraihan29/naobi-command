//! Tier 2 (interactive desktop, dogfood instance quit): Ctrl+Space, `>` and a command line, Enter opens
//! a PowerShell console that runs exactly that line and stays open, the Launcher cloaks, and the line
//! is in no file of the data directory in plaintext (R-024, T-034). The `perf` feature is on for this
//! file through the self dev-dependency, so `inject` exists.
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

fn alive(pid: u32) -> bool {
    Command::new("tasklist")
        .args(["/FO", "CSV", "/NH", "/FI", &format!("PID eq {pid}")])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).contains(&format!("\"{pid}\"")))
        .unwrap_or(false)
}

/// True when any file under `dir` holds `needle` as UTF-8 or UTF-16LE bytes.
fn any_file_holds(dir: &Path, needle: &str) -> bool {
    let utf8 = needle.as_bytes().to_vec();
    let utf16: Vec<u8> = needle.encode_utf16().flat_map(u16::to_le_bytes).collect();
    let has = |hay: &[u8], n: &[u8]| hay.windows(n.len()).any(|w| w == n);
    let Ok(entries) = std::fs::read_dir(dir) else {
        return false;
    };
    entries.flatten().any(|e| {
        let p = e.path();
        if p.is_dir() {
            return any_file_holds(&p, needle);
        }
        std::fs::read(&p).is_ok_and(|b| has(&b, &utf8) || has(&b, &utf16))
    })
}

// @R-024
#[test]
#[ignore = "tier2: interactive desktop"] // naobi-allow: tier 2 test, D-025
fn r024_run_flow_opens_console_and_leaves_no_plaintext() {
    let dir = std::env::temp_dir().join(format!("naobi-flow-run-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let marker = dir.join("ran.txt");
    let secret = format!("naobirun{}", std::process::id());
    let run_key = RunKey(format!(
        r"Software\NaobiCommandTest\flow-run-{}",
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
    let launcher = || dwm::find_window(naobi_ui::TITLE);
    let ready = until(Duration::from_secs(5), || launcher().is_some());
    let shown = ready && {
        inject::launcher();
        until(Duration::from_secs(2), || {
            launcher().is_some_and(|h| !dwm::is_cloaked(h))
        })
    };
    let launched = shown && {
        std::thread::sleep(Duration::from_millis(300));
        let line = format!(
            ">Set-Content -LiteralPath '{}' -Value \"$PID {secret}\"",
            marker.display()
        );
        inject::text(&line);
        std::thread::sleep(Duration::from_millis(500));
        inject::enter();
        until(Duration::from_secs(10), || {
            std::fs::read_to_string(&marker).is_ok_and(|s| s.contains(&secret))
        })
    };
    let cloaked = launched
        && until(Duration::from_secs(2), || {
            launcher().is_some_and(dwm::is_cloaked)
        });
    // The marker holds the shell's PID; -NoExit keeps it alive after the line ended.
    let pid: Option<u32> = std::fs::read_to_string(&marker)
        .ok()
        .and_then(|s| s.split_whitespace().next()?.parse().ok());
    std::thread::sleep(Duration::from_secs(1));
    let stayed = pid.is_some_and(alive);
    if let Some(p) = pid {
        let _ = Command::new("taskkill")
            .args(["/PID", &p.to_string(), "/T", "/F"])
            .output();
    }
    let _ = app.kill();
    let _ = app.wait();
    let leaked = any_file_holds(&dir.join("data"), &secret);
    if cloaked && stayed && !leaked {
        let _ = std::fs::remove_dir_all(&dir);
    } else {
        eprintln!("test data kept in {}", dir.display());
    }
    assert!(ready, "launcher window never appeared");
    assert!(shown, "Ctrl+Space did not show the launcher");
    assert!(launched, "Enter on the > line did not run it");
    assert!(cloaked, "the launcher did not cloak after the run");
    assert!(stayed, "the console did not stay open after the line ended");
    assert!(
        !leaked,
        "the command line is in plaintext under the data dir"
    );
}
