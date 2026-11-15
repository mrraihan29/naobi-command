use crate::verdict::{record, verdict};
use anyhow::{bail, Result};
use naobi_win32::autostart;
use naobi_win32::process::ProcessMetrics;
use serde_json::json;
use std::path::Path;
use std::process::{Child, Command};
use std::time::{Duration, Instant};

const MB: f64 = 1_000_000.0;
/// Start-up work (framework init, index build) is not idle; sampling starts after this.
const SETTLE: Duration = Duration::from_secs(3);

/// Starts `exe`, waits `SETTLE` (3 s) for start-up to finish, then samples for `seconds` and
/// prints working set, private bytes and CPU percent of one core as JSON. Records the R-006
/// verdicts. Fails if the process exits before the end. The process is never trimmed.
pub fn run(exe: &Path, seconds: u64, out: &Path, pinned: bool, awake: bool) -> Result<()> {
    // The release exe syncs the Owner's HKCU Run value with its settings; the guard puts it back.
    let _run_value = RunValue::save()?;
    let sandbox = std::env::temp_dir().join(format!("naobi-idle-{}", std::process::id()));
    // A data directory seeded beforehand (for example with 1,000 clipboard entries) is used as given
    // and left alone; otherwise the run starts from an empty one.
    let seeded = std::env::var_os("NAOBI_IDLE_DATA_DIR").map(std::path::PathBuf::from);
    let mut child = Command::new(exe)
        .env(
            "NAOBI_DATA_DIR",
            seeded.clone().unwrap_or_else(|| sandbox.join("data")),
        )
        .env("NAOBI_SETTINGS_DIR", sandbox.join("settings"))
        // The perf build leaves the app catalog and clipboard history off; idle is measured with them on.
        .env("NAOBI_TEST_APPS", "1")
        .env("NAOBI_TEST_CLIPBOARD", "1")
        .spawn()?;
    // `--pinned 1`: one helper window is made topmost (so one border and one hook exist) while idle
    // is measured. The helper belongs to this process; it is closed after the run.
    let _helper = if pinned { pin_one(&child) } else { None };
    if pinned && _helper.is_none() {
        let _ = child.kill();
        bail!(
            "ENV: the pinned helper window did not open; the idle run would measure nothing pinned"
        );
    }
    // `--awake 1`: `awake 30m` is typed into the Launcher, so a power request and the two Awake
    // timers exist while idle is measured.
    if awake && !start_awake() {
        let _ = child.kill();
        bail!("ENV: Awake did not start; the idle run would measure Awake off");
    }
    let result = measure(&mut child, seconds);
    let _ = child.kill();
    let _ = child.wait();
    let _ = std::fs::remove_dir_all(&sandbox);
    let (m, cpu) = result?;
    println!(
        "{}",
        json!({ "working_set_bytes": m.working_set, "private_bytes": m.private_bytes, "cpu_percent": cpu })
    );
    record(
        out,
        verdict("idle_working_set", m.working_set as f64 / MB, 40.0, "MB"),
    )?;
    // The budget is "< 0.5%"; the verdict file uses <=, the same at this precision.
    record(out, verdict("idle_cpu", cpu, 0.5, "percent"))
}

/// The Owner's Run value as it was, written back on drop (or removed again when there was none).
struct RunValue(Option<String>);

impl RunValue {
    fn save() -> Result<Self> {
        let saved = autostart::value(autostart::RUN_KEY);
        if let Some(v) = &saved {
            let plain = v.len() >= 2
                && v.starts_with('"')
                && v.ends_with('"')
                && !v[1..v.len() - 1].contains('"');
            if !plain {
                bail!("ENV: the Run value has a form this harness cannot restore exactly; stop the run");
            }
        }
        Ok(Self(saved))
    }
}

impl Drop for RunValue {
    fn drop(&mut self) {
        let (path, enabled) = match &self.0 {
            Some(v) => (v.trim_matches('"').to_owned(), true),
            None => (String::new(), false),
        };
        if let Err(e) = autostart::set_enabled(autostart::RUN_KEY, Path::new(&path), enabled) {
            eprintln!("could not restore the Owner's Run value: {e}");
        }
    }
}

/// Opens a helper window, gives it focus and presses the topmost hotkey. None when the window did
/// not open; the run then measures without one and says so.
fn pin_one(_child: &Child) -> Option<naobi_win32::edit_window::EditWindow> {
    std::thread::sleep(SETTLE);
    let w = naobi_win32::edit_window::EditWindow::open()?;
    w.focus();
    std::thread::sleep(Duration::from_millis(300));
    naobi_win32::inject::win_ctrl_t();
    std::thread::sleep(Duration::from_millis(500));
    if naobi_win32::edit_window::EditWindow::border_rect().is_none() {
        eprintln!("no topmost border appeared");
        return None;
    }
    Some(w)
}

/// Types `awake 30m` into the Launcher and runs it.
fn start_awake() -> bool {
    std::thread::sleep(SETTLE);
    naobi_win32::inject::launcher();
    std::thread::sleep(Duration::from_millis(600));
    naobi_win32::inject::text("awake 30m");
    std::thread::sleep(Duration::from_millis(600));
    naobi_win32::inject::enter();
    std::thread::sleep(Duration::from_millis(500));
    naobi_win32::power::execution_state() & 1 != 0
}

/// An exited child still has a queryable (torn-down) process object, so exit is checked
/// with `try_wait`, not by a failing sample.
fn gone(child: &mut Child) -> Result<()> {
    if child.try_wait()?.is_some() {
        bail!("process exited before the idle window ended; nothing to measure");
    }
    Ok(())
}

fn measure(child: &mut Child, seconds: u64) -> Result<(ProcessMetrics, f64)> {
    std::thread::sleep(SETTLE);
    gone(child)?;
    let first = ProcessMetrics::sample(child.id())?;
    let t0 = Instant::now();
    std::thread::sleep(Duration::from_secs(seconds));
    gone(child)?;
    let last = ProcessMetrics::sample(child.id())?;
    let busy = last.cpu_100ns.saturating_sub(first.cpu_100ns) as f64 / 1e7;
    Ok((last, busy / t0.elapsed().as_secs_f64() * 100.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exited_process_is_an_error_not_a_pass() {
        let out = std::env::temp_dir().join(format!("naobi-idle-{}.json", std::process::id()));
        let r = run(
            Path::new(r"C:\Windows\System32\whoami.exe"),
            1,
            &out,
            false,
            false,
        );
        assert!(r.is_err());
        assert!(!out.exists());
    }
}
