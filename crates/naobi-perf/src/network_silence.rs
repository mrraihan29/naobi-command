//! `network-silence --minutes n` (R-010, T-016): the app, and any child of it, opens no socket during a scripted
//! session of hotkeys, typing, clipboard writes and snaps. The system's TCP and UDP tables are read once a
//! second and every row owned by a `naobi-app.exe` process is a finding. The sampling interval is the known
//! ceiling (a socket opened and closed inside one second is missed); `cargo deny` bans remain the structural
//! guard. The session performs no explicit URL action, so any socket at all is a failure.
use crate::hotkey::{check_env, naobi_ui_title, App};
use crate::verdict::{record, verdict};
use anyhow::{bail, Result};
use naobi_win32::net_table::{sockets, Socket};
use naobi_win32::{clipboard_marker, dwm, edit_window::EditWindow, inject};
use serde_json::json;
use std::collections::BTreeSet;
use std::path::Path;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

const INTERVAL: Duration = Duration::from_secs(1);
const IMAGE: &str = "naobi-app.exe";
const VK_LEFT: u16 = 0x25;
const VK_RIGHT: u16 = 0x27;

/// The pids in `tasklist /FO CSV /NH` output whose image is `IMAGE`.
fn parse_pids(csv: &str) -> Vec<u32> {
    csv.lines()
        .filter_map(|l| {
            let mut f = l.split("\",\"");
            let name = f.next()?.trim_matches('"');
            let pid = f.next()?.trim_matches('"').parse().ok()?;
            name.eq_ignore_ascii_case(IMAGE).then_some(pid)
        })
        .collect()
}

fn app_pids() -> Vec<u32> {
    Command::new("tasklist")
        .args(["/FO", "CSV", "/NH", "/FI", &format!("IMAGENAME eq {IMAGE}")])
        .output()
        .map(|o| parse_pids(&String::from_utf8_lossy(&o.stdout)))
        .unwrap_or_default()
}

/// The rows of `table` that belong to one of `pids`.
fn owned(table: &[(u32, Socket)], pids: &[u32]) -> Vec<(u32, Socket)> {
    table
        .iter()
        .filter(|(p, _)| pids.contains(p))
        .copied()
        .collect()
}

fn hide_launcher() {
    if let Some(h) = dwm::find_window(naobi_ui_title()) {
        if !dwm::is_cloaked(h) {
            inject::launcher();
        }
    }
}

fn nap(stop: &AtomicBool, ms: u64) {
    let end = Instant::now() + Duration::from_millis(ms);
    while !stop.load(Ordering::Relaxed) && Instant::now() < end {
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Uses the app the way a day does: Launcher, typing, a calculation, the clipboard, snapping a window.
fn session(stop: &AtomicBool) {
    let edit = EditWindow::open();
    while !stop.load(Ordering::Relaxed) {
        inject::launcher();
        nap(stop, 500);
        inject::text("calc 2+2");
        nap(stop, 500);
        inject::enter();
        nap(stop, 500);
        hide_launcher();
        let _ = clipboard_marker::write_text("naobi network silence check");
        nap(stop, 500);
        if let Some(w) = &edit {
            w.focus();
            inject::ctrl_alt(VK_LEFT);
            nap(stop, 400);
            inject::ctrl_alt(VK_RIGHT);
            nap(stop, 400);
        }
        inject::alt_shift_v();
        nap(stop, 600);
        hide_launcher();
        nap(stop, 4000);
    }
}

pub fn run(exe: &Path, minutes: u64, out: &Path) -> Result<()> {
    if minutes == 0 {
        bail!("--minutes must be at least 1: a verdict needs a session");
    }
    check_env()?;
    // The perf build leaves the app catalog, clipboard history, and File index off; the session runs with them on, so
    // the verdict covers the code that scans Start Menu folders and configured roots and listens to the clipboard.
    let mut app = App::spawn_with(
        exe,
        1_000,
        &[
            ("NAOBI_TEST_APPS", "1"),
            ("NAOBI_TEST_CLIPBOARD", "1"),
            ("NAOBI_TEST_FILES", "1"),
        ],
    )?;
    app.wait("tray_ready")?;
    app.wait("window_ready")?;
    let stop = AtomicBool::new(false);
    let end = Instant::now() + Duration::from_secs(minutes * 60);
    let (mut seen, mut polls, mut pids, mut peak_pids) = (BTreeSet::new(), 0u64, Vec::new(), 0);
    std::thread::scope(|s| -> Result<()> {
        s.spawn(|| session(&stop));
        let mut result = Ok(());
        while Instant::now() < end {
            // Every poll, so a child that lives a few seconds (`--icons`, `--search-files`, `--list-apps`)
            // is seen while it runs. A dead app or an empty list is not silence, it is no observation.
            pids = app_pids();
            peak_pids = peak_pids.max(pids.len());
            if matches!(app.child.try_wait(), Ok(Some(_))) || pids.is_empty() {
                result = Err(anyhow::anyhow!(
                    "the app is not running: nothing was observed"
                ));
                break;
            }
            match sockets() {
                Ok(table) => seen.extend(owned(&table, &pids)),
                Err(e) => {
                    result = Err(e.into());
                    break;
                }
            }
            polls += 1;
            std::thread::sleep(INTERVAL);
        }
        stop.store(true, Ordering::Relaxed);
        result
    })?;
    drop(app);
    let mut v = verdict(
        "r010_no_socket_during_session",
        seen.len() as f64,
        0.0,
        "sockets",
    );
    v["minutes"] = json!(minutes);
    v["polls"] = json!(polls);
    v["sampling_interval_ms"] = json!(INTERVAL.as_millis());
    v["known_ceiling"] = json!("a socket opened and closed between two polls is not seen");
    v["max_app_processes_seen"] = json!(peak_pids);
    v["url_actions_in_session"] = json!(0);
    v["found"] = json!(seen
        .iter()
        .map(|(pid, s)| format!("pid {pid} {} :{}", s.proto, s.port))
        .collect::<Vec<_>>());
    println!("{v}");
    let silent = v["pass"] == true;
    record(out, v)?;
    if !silent {
        bail!("FAIL: a socket was open during the session");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // @R-010
    #[test]
    fn r010_only_rows_of_the_apps_own_processes_count() {
        let tcp = |port| Socket {
            proto: "tcp4",
            port,
        };
        let table = [(10, tcp(1)), (11, tcp(2)), (12, tcp(3))];
        assert_eq!(owned(&table, &[11, 12]), [(11, tcp(2)), (12, tcp(3))]);
        assert!(owned(&table, &[]).is_empty());
        let csv = "\"naobi-app.exe\",\"4242\",\"Console\",\"1\",\"27,000 K\"\r\n\"naobi-app.exe\",\"77\",\"Console\",\"1\",\"9 K\"\r\n\"other.exe\",\"5\",\"Console\",\"1\",\"9 K\"\r\nINFO: No tasks are running";
        assert_eq!(parse_pids(csv), [4242, 77]);
    }
}
