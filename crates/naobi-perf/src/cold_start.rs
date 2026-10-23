use crate::hotkey::App;
use crate::percentile::percentile;
use crate::verdict::{record, verdict};
use anyhow::Result;
use naobi_win32::platform::{qpc, qpc_to_ms};
use std::path::Path;
use std::time::Duration;

/// Starts `exe` `runs` times and reports p95 of process start to the `tray_ready` and `window_ready`
/// lines (R-006: 300 ms and 1.5 s). The first run is a cold start for the OS file cache.
pub fn run(exe: &Path, runs: usize, out: &Path) -> Result<()> {
    crate::hotkey::check_env()?;
    let (mut tray, mut window) = (Vec::new(), Vec::new());
    for _ in 0..runs {
        let start = qpc();
        let mut app = App::spawn(exe, 10_000)?;
        tray.push(qpc_to_ms(app.wait("tray_ready")? - start));
        window.push(qpc_to_ms(app.wait("window_ready")? - start));
        drop(app);
        // The next instance needs Alt+Space back.
        std::thread::sleep(Duration::from_millis(500));
    }
    let p95 = |v: &[f64]| percentile(v, 95.0).unwrap_or(f64::NAN);
    for (name, v, limit) in [
        ("start_to_tray_ready", &tray, 300.0),
        ("start_to_window_ready", &window, 1500.0),
    ] {
        let mut e = verdict(name, p95(v), limit, "ms");
        e["runs_ms"] = serde_json::json!(v);
        println!("{e}");
        record(out, e)?;
    }
    Ok(())
}
