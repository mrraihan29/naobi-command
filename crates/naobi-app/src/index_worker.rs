//! The index worker (REPORT 3.5): one thread that owns the directory watch on the settings folder. It
//! has no thread of its own for settings (review minor M12) and no timer while idle. The apps item
//! adds its job queue here.
use std::path::Path;
use std::time::{Duration, Instant};

use naobi_win32::dirwatch::{DirWatcher, Wake};

/// An editor's save writes several events; the reload waits this long after the last one.
const DEBOUNCE: Duration = Duration::from_millis(250);
/// A folder that keeps changing still reloads, at most this long after its first change.
const MAX_WAIT: Duration = Duration::from_secs(1);

/// Starts the worker on `dir`. `on_change` runs on the worker thread once the folder has been quiet
/// for `DEBOUNCE` after a change, or `MAX_WAIT` after the first change of a burst.
pub fn start(dir: &Path, on_change: impl Fn() + Send + 'static) -> anyhow::Result<()> {
    let mut watch = DirWatcher::open(dir)?;
    std::thread::Builder::new()
        .name("index".into())
        .spawn(move || {
            let mut due: Option<Instant> = None;
            let mut burst: Option<Instant> = None;
            loop {
                let timeout = due.map(|t| t.saturating_duration_since(Instant::now()));
                match watch.wait(timeout) {
                    Ok(Wake::Changed) => {
                        let now = Instant::now();
                        let first = *burst.get_or_insert(now);
                        due = Some((now + DEBOUNCE).min(first + MAX_WAIT));
                    }
                    Ok(Wake::Timeout) => {
                        due = None;
                        burst = None;
                        on_change();
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "settings folder watch stopped");
                        return;
                    }
                }
            }
        })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    /// D-009 "applies within 1 s": a save in the watched folder reaches `on_change` within a second
    /// (250 ms of quiet after the change). Real files in a temp folder; the worker thread ends with the
    /// test process.
    #[test]
    fn d009_settings_change_applies_within_one_second() {
        let dir = std::env::temp_dir().join(format!("naobi-watch-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");
        let (tx, rx) = mpsc::channel();
        start(&dir, move || {
            let _ = tx.send(());
        })
        .expect("start the worker");
        std::fs::write(dir.join("settings.toml"), "[general]\nautostart = true\n").expect("save");
        assert!(
            rx.recv_timeout(Duration::from_secs(1)).is_ok(),
            "the change did not reach the reload within 1 s"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
