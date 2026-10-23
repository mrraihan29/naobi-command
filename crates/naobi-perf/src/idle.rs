use crate::verdict::{record, verdict};
use anyhow::{bail, Result};
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
pub fn run(exe: &Path, seconds: u64, out: &Path) -> Result<()> {
    let mut child = Command::new(exe).spawn()?;
    let result = measure(&mut child, seconds);
    let _ = child.kill();
    let _ = child.wait();
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
        let r = run(Path::new(r"C:\Windows\System32\whoami.exe"), 1, &out);
        assert!(r.is_err());
        assert!(!out.exists());
    }
}
