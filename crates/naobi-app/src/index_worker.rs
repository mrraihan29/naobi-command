//! The index worker (REPORT 3.5): one thread that owns every directory watch and runs the jobs that
//! follow them (the settings reload, the app catalog scan). It has no timer while idle: the thread
//! sleeps in one wait on the watches' events, and a job's debounce is that wait's timeout.
use std::path::PathBuf;
use std::time::{Duration, Instant};

use naobi_win32::dirwatch::{wait_any, DirWatcher};

/// An editor's save writes several events; the job waits this long after the last one.
const DEBOUNCE: Duration = Duration::from_millis(250);
/// A folder that keeps changing still runs the job, at most this long after its first change.
const MAX_WAIT: Duration = Duration::from_secs(1);

/// Work for the worker: `run` goes on the worker thread once the folders have been quiet for
/// `DEBOUNCE` after a change (or `MAX_WAIT` after the first change of a burst), and once at start
/// when `initial` is set. With `settle`, it runs once more that long after a change-triggered run,
/// unless another change comes first: a source that lags behind the folder (the shell's app list
/// follows the Start Menu by a few seconds) is read again once it has caught up.
pub struct Job {
    /// Folders to watch, each with whether the folders below it count.
    pub dirs: Vec<(PathBuf, bool)>,
    pub initial: bool,
    pub settle: Option<Duration>,
    pub run: Box<dyn Fn() + Send>,
}

/// When a job runs next after a burst of changes.
#[derive(Default)]
struct Pending {
    due: Option<Instant>,
    burst: Option<Instant>,
    /// The next run is the settle run, which does not schedule another.
    settling: bool,
}

impl Pending {
    /// A change arrived at `now`: quiet for `DEBOUNCE`, but never later than `MAX_WAIT` after the
    /// first change of the burst.
    fn changed(&mut self, now: Instant) {
        if self.settling {
            *self = Self::default();
        }
        let first = *self.burst.get_or_insert(now);
        self.due = Some((now + DEBOUNCE).min(first + MAX_WAIT));
    }

    /// True once, when the job is due at `now`. After a change-triggered run, `settle` schedules the
    /// one more.
    fn take_due(&mut self, now: Instant, settle: Option<Duration>) -> bool {
        let due = self.due.is_some_and(|t| t <= now);
        if due {
            let again = settle.filter(|_| !self.settling);
            *self = Self::default();
            if let Some(after) = again {
                self.due = Some(now + after);
                self.settling = true;
            }
        }
        due
    }
}

/// Starts the worker. A folder that cannot be watched is logged and skipped; the other jobs go on.
pub fn start(jobs: Vec<Job>) -> anyhow::Result<()> {
    std::thread::Builder::new()
        .name("index".into())
        .spawn(move || run(&jobs))?;
    Ok(())
}

fn run(jobs: &[Job]) {
    let mut watchers = Vec::new();
    let mut owner = Vec::new();
    for (j, job) in jobs.iter().enumerate() {
        for (dir, subtree) in &job.dirs {
            let opened = if *subtree {
                DirWatcher::open_subtree(dir)
            } else {
                DirWatcher::open(dir)
            };
            match opened {
                Ok(w) => {
                    watchers.push(w);
                    owner.push(j);
                }
                Err(e) => {
                    tracing::warn!(error = %e, "a folder cannot be watched; its changes are not seen")
                }
            }
        }
    }
    // Watches are open first, so a change during the first scan is not lost.
    for job in jobs.iter().filter(|j| j.initial) {
        (job.run)();
    }
    if watchers.is_empty() {
        return;
    }
    let mut pending: Vec<Pending> = jobs.iter().map(|_| Pending::default()).collect();
    loop {
        // The wait counts whole milliseconds, so one more keeps it from waking just before due.
        let timeout = pending
            .iter()
            .filter_map(|p| p.due)
            .min()
            .map(|t| t.saturating_duration_since(Instant::now()) + Duration::from_millis(1));
        match wait_any(&mut watchers, timeout) {
            Ok(Some(i)) => pending[owner[i]].changed(Instant::now()),
            Ok(None) => {
                let now = Instant::now();
                for (job, p) in jobs.iter().zip(&mut pending) {
                    if p.take_due(now, job.settle) {
                        (job.run)();
                    }
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, "a folder watch stopped; the index worker ends");
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("naobi-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");
        dir
    }

    #[test]
    fn a_burst_runs_once_after_the_quiet_time_and_never_later_than_the_cap() {
        let t0 = Instant::now();
        let mut p = Pending::default();
        p.changed(t0);
        assert!(!p.take_due(t0 + Duration::from_millis(249), None));
        // A second change pushes the due time out by the debounce.
        p.changed(t0 + Duration::from_millis(200));
        assert!(!p.take_due(t0 + Duration::from_millis(449), None));
        assert!(p.take_due(t0 + Duration::from_millis(450), None));
        assert!(!p.take_due(t0 + Duration::from_secs(5), None), "runs once");
        // A folder that never goes quiet still runs at the cap.
        let mut p = Pending::default();
        for ms in (0..1000).step_by(100) {
            p.changed(t0 + Duration::from_millis(ms));
        }
        assert!(p.take_due(t0 + MAX_WAIT, None));
    }

    #[test]
    fn a_settle_run_follows_once_and_a_new_change_cancels_it() {
        let t0 = Instant::now();
        let settle = Some(Duration::from_secs(5));
        let mut p = Pending::default();
        p.changed(t0);
        assert!(p.take_due(t0 + DEBOUNCE, settle));
        // The settle run is due 5 s after the first run, and is the last one.
        assert!(!p.take_due(t0 + Duration::from_secs(5), settle));
        assert!(p.take_due(t0 + DEBOUNCE + Duration::from_secs(5), settle));
        assert!(p.due.is_none(), "no settle after the settle run");
        // A change during the wait starts a fresh burst instead.
        p.changed(t0 + Duration::from_secs(10));
        assert!(p.take_due(t0 + Duration::from_secs(10) + DEBOUNCE, settle));
        p.changed(t0 + Duration::from_secs(12));
        assert!(!p.settling && p.due.is_some_and(|d| d < t0 + Duration::from_secs(13)));
    }

    /// D-009 "applies within 1 s": a save in the watched folder reaches the job within a second
    /// (250 ms of quiet after the change). Real files in a temp folder; the worker thread ends with
    /// the test process.
    #[test]
    fn d009_settings_change_applies_within_one_second() {
        let dir = temp("watch");
        let (tx, rx) = mpsc::channel();
        start(vec![Job {
            dirs: vec![(dir.clone(), false)],
            initial: false,
            settle: None,
            run: Box::new(move || {
                let _ = tx.send(());
            }),
        }])
        .expect("start the worker");
        std::fs::write(dir.join("settings.toml"), "[general]\nautostart = true\n").expect("save");
        assert!(
            rx.recv_timeout(Duration::from_secs(1)).is_ok(),
            "the change did not reach the job within 1 s"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    // @R-012
    #[test]
    fn r012_initial_job_runs_at_start_and_a_subfolder_change_runs_only_its_own_job() {
        let (flat, tree) = (temp("flat"), temp("tree"));
        std::fs::create_dir_all(tree.join("Sub")).expect("sub folder");
        let (tx, rx) = mpsc::channel();
        let (txa, txb) = (tx.clone(), tx);
        start(vec![
            Job {
                dirs: vec![(flat.clone(), false)],
                initial: false,
                settle: None,
                run: Box::new(move || {
                    let _ = txa.send("flat");
                }),
            },
            Job {
                dirs: vec![
                    (tree.clone(), true),
                    (PathBuf::from(r"C:\no\such\folder"), true),
                ],
                initial: true,
                settle: None,
                run: Box::new(move || {
                    let _ = txb.send("tree");
                }),
            },
        ])
        .expect("start the worker");
        assert_eq!(
            rx.recv_timeout(Duration::from_secs(2)),
            Ok("tree"),
            "initial run"
        );
        std::fs::write(tree.join("Sub").join("New App.lnk"), "x").expect("new shortcut");
        assert_eq!(
            rx.recv_timeout(Duration::from_secs(2)),
            Ok("tree"),
            "a change below the folder reruns the job"
        );
        assert!(
            rx.recv_timeout(Duration::from_millis(600)).is_err(),
            "the other job did not run"
        );
        let _ = std::fs::remove_dir_all(&flat);
        let _ = std::fs::remove_dir_all(&tree);
    }
}
