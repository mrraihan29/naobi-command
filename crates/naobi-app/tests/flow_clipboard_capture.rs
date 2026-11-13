//! Tier 2 (interactive desktop, dogfood instance quit): a real copy lands in the real DPAPI store and
//! the marker text is nowhere in the database file or its WAL. The Owner's text on the clipboard is
//! put back afterwards.
use naobi_core::db::Db;
use naobi_win32::{autostart, dwm};
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
    let out = Command::new("powershell")
        .args(["-NoProfile", "-Command", script])
        .output();
    out.map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default()
}

fn entries(db_file: &Path) -> Option<(i64, Option<String>)> {
    let db = Db::open(db_file, || [0; 16]).ok()?;
    db.conn()
        .query_row(
            "SELECT COUNT(*), MAX(source_exe) FROM clip_entry",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .ok()
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}

// @R-009
#[test]
#[ignore = "tier2: interactive desktop"] // naobi-allow: tier 2 test, D-025
fn r009_content_not_plaintext_in_db_file() {
    let dir = std::env::temp_dir().join(format!("naobi-flow-clip-{}", std::process::id()));
    let data = dir.join("data");
    let run_key = RunKey(format!(
        r"Software\NaobiCommandTest\flow-clip-{}",
        std::process::id()
    ));
    let marker = format!("zq-clip-marker-{}-7731", std::process::id());
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
    let ready = until(Duration::from_secs(5), || {
        dwm::find_window(naobi_ui::TITLE).is_some()
    });
    // The listener is installed on P1 right after the window is ready.
    std::thread::sleep(Duration::from_millis(800));
    powershell(&format!("Set-Clipboard -Value '{marker}'"));
    let db_file = data.join("naobi.db");
    let stored = until(Duration::from_secs(8), || {
        entries(&db_file).is_some_and(|(n, _)| n >= 1)
    });
    let owner = entries(&db_file).and_then(|(_, exe)| exe);
    let _ = app.kill();
    let _ = app.wait();
    let (main, wal) = (
        std::fs::read(&db_file).unwrap_or_default(),
        std::fs::read(data.join("naobi.db-wal")).unwrap_or_default(),
    );
    // Put the Owner's text back (an empty clipboard stays empty).
    if !previous.trim().is_empty() {
        powershell(&format!(
            "Set-Clipboard -Value '{}'",
            previous.trim_end().replace('\'', "''")
        ));
    }
    let utf16: Vec<u8> = marker.encode_utf16().flat_map(u16::to_le_bytes).collect();
    let leaked = [&main, &wal]
        .iter()
        .any(|f| contains(f, marker.as_bytes()) || contains(f, &utf16));
    let _ = std::fs::remove_dir_all(&dir);
    assert!(ready, "launcher window never appeared");
    assert!(stored, "the copy was not stored");
    assert!(
        owner
            .as_ref()
            .is_some_and(|o| o.to_ascii_lowercase().contains("powershell")),
        "the owner exe was not recorded: {owner:?}"
    );
    assert!(
        !main.is_empty() || !wal.is_empty(),
        "no database files to scan"
    );
    assert!(!leaked, "the marker text is readable in the database file");
}
