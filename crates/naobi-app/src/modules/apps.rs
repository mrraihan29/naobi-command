//! The apps module (NCMD-16): installed apps as Launcher results. The catalog is scanned on the index
//! worker once at start and again, after the debounce, when a Start Menu folder changes.
use super::Modules;
use crate::index_worker::Job;
use naobi_core::notice::{Notice, NoticeKind};
use naobi_launcher::providers::apps::{AppCatalog, AppEntry, AppsSource};
use std::ffi::OsString;
use std::path::PathBuf;
use std::sync::OnceLock;

/// The one catalog: the source the Launcher searches and the worker job that fills it share it.
fn catalog() -> &'static AppCatalog {
    static CATALOG: OnceLock<AppCatalog> = OnceLock::new();
    CATALOG.get_or_init(AppCatalog::default)
}

/// Perf builds measure a fixed index, so real apps stay out unless a test asks for them.
pub fn enabled() -> bool {
    !cfg!(feature = "perf") || std::env::var_os("NAOBI_TEST_APPS").is_some()
}

pub fn register(m: &mut Modules) {
    if enabled() {
        m.register_source(Box::new(AppsSource::new(catalog().clone())));
    }
}

/// The user's and the shared Start Menu folders. A missing variable skips its folder.
fn start_menu_dirs(appdata: Option<OsString>, program_data: Option<OsString>) -> Vec<PathBuf> {
    [appdata, program_data]
        .into_iter()
        .flatten()
        .map(|base| {
            PathBuf::from(base)
                .join("Microsoft")
                .join("Windows")
                .join("Start Menu")
                .join("Programs")
        })
        .collect()
}

/// The worker job. `done` runs after every scan, with a Notice when the scan failed, and the
/// Launcher ranks again. Without an earlier scan a failure ends the "Indexing apps" row with an empty
/// catalog; with one, the apps already known stay.
pub fn job(done: fn(Option<Notice>)) -> Option<Job> {
    enabled().then(|| make_job(done))
}

fn make_job(done: fn(Option<Notice>)) -> Job {
    Job {
        dirs: start_menu_dirs(std::env::var_os("APPDATA"), std::env::var_os("ProgramData"))
            .into_iter()
            .map(|d| (d, true))
            .collect(),
        initial: true,
        // The shell's app list follows the Start Menu by 2 to 4 seconds (measured).
        settle: Some(std::time::Duration::from_secs(5)),
        run: Box::new(move || done(scan())),
    }
}

fn scan() -> Option<Notice> {
    match naobi_win32::apps::list() {
        Ok(apps) => {
            catalog().set(
                apps.into_iter()
                    .map(|a| AppEntry {
                        id: a.id,
                        name: a.name,
                    })
                    .collect(),
            );
            None
        }
        Err(e) => {
            tracing::warn!(error = %e, "the app scan failed");
            if !catalog().is_ready() {
                catalog().set(Vec::new());
            }
            Some(Notice {
                subject: "Apps".into(),
                message: "Windows would not list the installed apps.".into(),
                kind: NoticeKind::Error,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use naobi_core::command::{CommandSource, Query};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::{Duration, Instant};

    static SCANS: AtomicUsize = AtomicUsize::new(0);

    fn count_scan(_failure: Option<Notice>) {
        SCANS.fetch_add(1, Ordering::SeqCst);
    }

    fn until(timeout: Duration, mut cond: impl FnMut() -> bool) -> bool {
        let end = Instant::now() + timeout;
        while Instant::now() < end {
            if cond() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        cond()
    }

    /// Tier 2: the real index worker, the real `AppsFolder` and the user's real Start Menu folder,
    /// where the test puts one throwaway `.url` shortcut and removes it again.
    // @R-012
    #[test]
    #[ignore = "tier2: interactive desktop"] // naobi-allow: tier 2 test, D-025
    fn r012_new_start_menu_shortcut_appears() {
        let name = format!("NaobiProbeApp{}", std::process::id());
        let shortcut = start_menu_dirs(std::env::var_os("APPDATA"), None)
            .remove(0)
            .join(format!("{name}.url"));
        let found = || {
            let q = Query { text: name.clone() };
            AppsSource::new(catalog().clone())
                .items(&q)
                .iter()
                .any(|i| i.title == name)
        };
        crate::index_worker::start(vec![make_job(count_scan)]).expect("start the worker");
        assert!(
            until(Duration::from_secs(15), || SCANS.load(Ordering::SeqCst)
                >= 1),
            "the first scan did not finish"
        );
        assert!(!found(), "the probe app exists before it was created");
        std::fs::write(
            &shortcut,
            "[InternetShortcut]
URL=https://example.invalid/
",
        )
        .expect("write the shortcut");
        let seen = until(Duration::from_secs(15), found);
        let _ = std::fs::remove_file(&shortcut);
        assert!(
            seen,
            "the new shortcut was not found after the watcher debounce"
        );
    }

    // @R-012
    #[test]
    fn r012_start_menu_folders_are_the_users_and_the_shared_ones() {
        let dirs = start_menu_dirs(
            Some(r"C:\Users\o\AppData\Roaming".into()),
            Some(r"C:\ProgramData".into()),
        );
        assert_eq!(
            dirs,
            vec![
                PathBuf::from(r"C:\Users\o\AppData\Roaming\Microsoft\Windows\Start Menu\Programs"),
                PathBuf::from(r"C:\ProgramData\Microsoft\Windows\Start Menu\Programs"),
            ]
        );
        assert_eq!(start_menu_dirs(None, None), Vec::<PathBuf>::new());
    }
}
