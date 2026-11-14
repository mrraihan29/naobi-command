//! Tier 2: opens a real console window (R-024). The test ends the console it started.
use std::process::Command;
use std::time::{Duration, Instant};

const EXPECTED: &str = "a  b%PATH%|&>\"q\"";

// @R-024
#[test]
#[ignore = "tier2: interactive desktop"] // naobi-allow: tier 2 test, D-025
fn r024_console_gets_line_unchanged() {
    let marker = std::env::temp_dir().join(format!("naobi-run-{}.txt", std::process::id()));
    let _ = std::fs::remove_file(&marker);
    let line = format!(
        "Set-Content -Encoding utf8 -LiteralPath '{}' -Value (\"a  b\" + '%PATH%|&>\"q\"' + $PID)",
        marker.display()
    );
    assert!(naobi_win32::run::run_in_console(&line).is_ok());

    let end = Instant::now() + Duration::from_secs(10);
    let mut text = String::new();
    while Instant::now() < end {
        if let Ok(t) = std::fs::read_to_string(&marker) {
            let t = t.trim_start_matches('\u{feff}').trim_end().to_string();
            if t.len() > EXPECTED.len() {
                text = t;
                break;
            }
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let _ = std::fs::remove_file(&marker);
    let pid: Option<u32> = text.strip_prefix(EXPECTED).and_then(|r| r.parse().ok());

    std::thread::sleep(Duration::from_secs(1));
    let alive = pid.is_some_and(|p| {
        let out = Command::new("tasklist")
            .args(["/FI", &format!("PID eq {p}"), "/FO", "CSV", "/NH"])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).to_lowercase())
            .unwrap_or_default();
        out.contains("pwsh") || out.contains("powershell")
    });
    if let Some(p) = pid {
        let _ = Command::new("taskkill")
            .args(["/PID", &p.to_string(), "/T", "/F"])
            .output();
    }
    assert!(pid.is_some(), "marker was {text:?}");
    assert!(alive, "-NoExit keeps the shell running");
}
