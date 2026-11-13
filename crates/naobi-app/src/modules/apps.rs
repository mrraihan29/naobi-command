//! The apps module (NCMD-16): installed apps as Launcher results. The catalog is scanned once at start
//! and again, after the debounce, when a Start Menu folder changes. The scan runs in a short-lived
//! child process (`--list-apps`): it loads about 36 shell and COM libraries that would add some
//! 19 MB to the resident process's working set for good (measured), past the R-006 idle budget.
use super::Modules;
use crate::index_worker::Job;
use naobi_core::notice::{Notice, NoticeKind};
use naobi_launcher::providers::apps::{AppCatalog, AppEntry, AppsSource};
use std::ffi::OsString;
use std::io::{self, Read, Write};
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

/// The argument that makes the exe list the installed apps on stdout and exit.
pub const LIST_APPS_ARG: &str = "--list-apps";
/// Separates the id from the name on a line. No app id or name has one.
const SEP: char = '\t';
/// A scan that takes longer than this is killed: the worker also runs the settings reload.
const SCAN_TIMEOUT: Duration = Duration::from_secs(30);
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

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
    enabled().then(|| make_job(done, list_in_child))
}

fn make_job(done: fn(Option<Notice>), list: fn() -> io::Result<Vec<AppEntry>>) -> Job {
    Job {
        dirs: start_menu_dirs(std::env::var_os("APPDATA"), std::env::var_os("ProgramData"))
            .into_iter()
            .map(|d| (d, true))
            .collect(),
        initial: true,
        // The shell's app list follows the Start Menu by 2 to 4 seconds (measured).
        settle: Some(Duration::from_secs(5)),
        run: Box::new(move || done(scan(list))),
    }
}

fn scan(list: fn() -> io::Result<Vec<AppEntry>>) -> Option<Notice> {
    match list() {
        Ok(apps) => {
            catalog().set(apps);
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

/// The child's output: one `id<SEP>name` line per app. An entry with a tab or a line break in it
/// would break the format, so it is left out.
pub fn write_list(out: &mut impl Write, apps: &[naobi_win32::apps::AppEntry]) -> io::Result<()> {
    let bad = |s: &str| s.contains([SEP, '\n', '\r']);
    let mut out = io::BufWriter::new(out);
    for a in apps.iter().filter(|a| !bad(&a.id) && !bad(&a.name)) {
        writeln!(out, "{}{SEP}{}", a.id, a.name)?;
    }
    out.flush()
}

fn parse_list(text: &str) -> Vec<AppEntry> {
    text.lines()
        .filter_map(|line| line.split_once(SEP))
        .filter(|(id, name)| !id.is_empty() && !name.is_empty())
        .map(|(id, name)| AppEntry {
            id: id.to_owned(),
            name: name.to_owned(),
        })
        .collect()
}

/// Runs the scan in this exe's `--list-apps` child and reads its stdout. Killed after `SCAN_TIMEOUT`.
fn list_in_child() -> io::Result<Vec<AppEntry>> {
    let mut child = Command::new(std::env::current_exe()?)
        .arg(LIST_APPS_ARG)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()?;
    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| io::Error::other("the scan has no stdout"))?;
    // Read on a thread: a pipe holds 4 KB, and the child blocks on a full one.
    let reader = std::thread::spawn(move || {
        let mut text = String::new();
        stdout.read_to_string(&mut text).map(|_| text)
    });
    let end = Instant::now() + SCAN_TIMEOUT;
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if Instant::now() >= end {
            let _ = child.kill();
            let _ = child.wait();
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "the app scan timed out",
            ));
        }
        std::thread::sleep(Duration::from_millis(25));
    };
    let text = reader
        .join()
        .map_err(|_| io::Error::other("the scan reader panicked"))??;
    if status.success() {
        Ok(parse_list(&text))
    } else {
        Err(io::Error::other("the scan process failed"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use naobi_core::command::{CommandSource, Query};
    use std::sync::atomic::{AtomicUsize, Ordering};

    static SCANS: AtomicUsize = AtomicUsize::new(0);

    fn count_scan(_failure: Option<Notice>) {
        SCANS.fetch_add(1, Ordering::SeqCst);
    }

    /// The scan in this process: the unit-test exe cannot be its own `--list-apps` child.
    fn list_here() -> io::Result<Vec<AppEntry>> {
        let apps = naobi_win32::apps::list().map_err(io::Error::other)?;
        let mut text = Vec::new();
        write_list(&mut text, &apps)?;
        Ok(parse_list(&String::from_utf8_lossy(&text)))
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
        crate::index_worker::start(vec![make_job(count_scan, list_here)])
            .expect("start the worker");
        assert!(
            until(Duration::from_secs(15), || SCANS.load(Ordering::SeqCst)
                >= 1),
            "the first scan did not finish"
        );
        assert!(!found(), "the probe app exists before it was created");
        std::fs::write(
            &shortcut,
            "[InternetShortcut]\r\nURL=https://example.invalid/\r\n",
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

    // @R-012
    #[test]
    fn r012_the_scan_child_output_round_trips_and_skips_what_would_break_a_line() {
        let app = |id: &str, name: &str| naobi_win32::apps::AppEntry {
            id: id.into(),
            name: name.into(),
        };
        let mut text = Vec::new();
        write_list(
            &mut text,
            &[
                app(r"{GUID}\calc.exe", "Calculator"),
                app("Pkg_8wekyb3d8bbwe!App", "Name\twith tab"),
                app("https://example.com/", "Line\nbreak"),
                app("Microsoft.WindowsNotepad_8wekyb3d8bbwe!App", "Notepad"),
            ],
        )
        .expect("write");
        let parsed = parse_list(&String::from_utf8_lossy(&text));
        let ids: Vec<_> = parsed.iter().map(|a| a.id.as_str()).collect();
        assert_eq!(
            ids,
            vec![
                r"{GUID}\calc.exe",
                "Microsoft.WindowsNotepad_8wekyb3d8bbwe!App"
            ]
        );
        assert_eq!(parsed[1].name, "Notepad");
        assert!(parse_list("no separator\n\tonly name\nid\t\n").is_empty());
    }
}
