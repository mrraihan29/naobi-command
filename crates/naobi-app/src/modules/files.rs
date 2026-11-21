//! The Files section (NCMD-17, R-013): file rows from the Windows Search index, after the other
//! rows. The query runs in a transient child of this exe (`--search-files`): OLE DB maps tens of
//! megabytes for good, which the resident process must not carry (R-006). One worker thread asks the
//! child, newest query first, drops an answer that takes longer than 500 ms (T-020), and lets the
//! child go when nobody has searched for a while.
use super::Modules;
use naobi_core::settings::Files;
use naobi_launcher::file_index::{resolve_roots, FileIndex, IndexEvent};
use naobi_launcher::providers::files::{
    classify_query, contains_sql, like_sql, merge_paths, rows, FileCommands, FileQuery, LIMIT,
    SPARSE_THRESHOLD,
};
use naobi_win32::dirwatch::{
    is_local_dir, wait_any_changes, DirWatcher, FileAction, FileChange, MultiWake, WakeEvent,
    MAX_WAIT_HANDLES,
};
use naobi_win32::search::{self, SearchError};
use std::io::{BufRead, BufReader, Write};
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

const INDEX_FILE: &str = "file-index.db";

/// The argument that makes the exe answer file queries on stdin and stdout until stdin closes.
pub const SEARCH_FILES_ARG: &str = "--search-files";
/// A slower answer is dropped (T-020), the sync rows never wait for it.
const ANSWER_MS: u64 = 500;
/// The first query of a fresh child also pays for starting it and loading the provider.
const COLD_MS: u64 = 3000;
/// A child that has not answered for this long is stuck: it is killed and started again.
const STUCK: Duration = Duration::from_secs(5);
/// Nobody searched for this long: the child (and its memory) goes away.
const IDLE: Duration = Duration::from_secs(30);
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

fn unavailable() -> &'static Arc<AtomicBool> {
    static FLAG: OnceLock<Arc<AtomicBool>> = OnceLock::new();
    FLAG.get_or_init(|| Arc::new(AtomicBool::new(false)))
}

fn config() -> &'static Mutex<Files> {
    static CFG: OnceLock<Mutex<Files>> = OnceLock::new();
    CFG.get_or_init(|| Mutex::new(Files::default()))
}

static WAKE: OnceLock<Arc<WakeEvent>> = OnceLock::new();

fn indexer_enabled() -> bool {
    !cfg!(feature = "perf") || std::env::var_os("NAOBI_TEST_FILES").is_some()
}

fn index_db_path() -> Option<PathBuf> {
    naobi_core::paths::data_dir().map(|d| d.join(INDEX_FILE))
}

/// Applies updated `[files]` settings; wakes the file-index thread to re-watch and re-sync if changed.
pub fn set_config(files: &Files) {
    let mut guard = config().lock().unwrap_or_else(|e| e.into_inner());
    if *guard != *files {
        *guard = files.clone();
        if let Some(wake) = WAKE.get() {
            wake.signal();
        }
    }
}

/// Starts the background File index scanner and `ReadDirectoryChangesW` watcher.
pub fn start_indexer(files: &Files) {
    set_config(files);
    if !indexer_enabled() {
        return;
    }
    let Some(db_path) = index_db_path() else {
        return;
    };
    let wake = match WakeEvent::new() {
        Ok(w) => Arc::new(w),
        Err(e) => {
            tracing::warn!(
                code = e.code().0,
                "file index wake event failed; local file index is off"
            );
            return;
        }
    };
    let _ = WAKE.set(wake.clone());
    let spawned = std::thread::Builder::new()
        .name("file-index".into())
        .spawn(move || run_indexer(&db_path, &wake));
    if spawned.is_err() {
        tracing::warn!("file index is off: its thread could not start");
    }
}

/// Converts a `DirWatcher` notification batch into `IndexEvent`s, pairing adjacent renames.
pub fn changes_to_events(root: &Path, changes: &[FileChange]) -> Vec<IndexEvent> {
    let mut out = Vec::with_capacity(changes.len());
    let mut i = 0;
    while i < changes.len() {
        match changes[i].action {
            FileAction::Added => out.push(IndexEvent::Added(root.join(&changes[i].rel_path))),
            FileAction::Removed => out.push(IndexEvent::Removed(root.join(&changes[i].rel_path))),
            FileAction::Modified => {}
            FileAction::RenamedOld => {
                if changes
                    .get(i + 1)
                    .is_some_and(|n| n.action == FileAction::RenamedNew)
                {
                    out.push(IndexEvent::Renamed {
                        from: root.join(&changes[i].rel_path),
                        to: root.join(&changes[i + 1].rel_path),
                    });
                    i += 2;
                    continue;
                }
                out.push(IndexEvent::Removed(root.join(&changes[i].rel_path)));
            }
            FileAction::RenamedNew => {
                out.push(IndexEvent::Added(root.join(&changes[i].rel_path)));
            }
        }
        i += 1;
    }
    out
}

fn run_indexer(db_path: &Path, wake: &WakeEvent) {
    if let Some(parent) = db_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let mut idx = match FileIndex::open(db_path) {
        Ok(i) => i,
        Err(e) => {
            tracing::warn!(
                code = ?e.sqlite_error_code(),
                "file index store could not be opened"
            );
            return;
        }
    };
    let max_watchers = MAX_WAIT_HANDLES.saturating_sub(1);

    loop {
        let cfg = config().lock().unwrap_or_else(|e| e.into_inner()).clone();
        let user_profile = std::env::var_os("USERPROFILE").map(PathBuf::from);
        let roots: Vec<PathBuf> = resolve_roots(&cfg.roots, user_profile.as_deref())
            .into_iter()
            .filter(|r| is_local_dir(r))
            .take(max_watchers)
            .collect();

        let mut watchers = Vec::new();
        let mut watched_roots = Vec::new();
        for dir in roots {
            if let Ok(w) = DirWatcher::open_tree_changes(&dir) {
                watchers.push(w);
                watched_roots.push(dir);
            }
        }
        if let Err(e) = idx.sync_roots(&watched_roots, &cfg.ignore) {
            tracing::warn!(
                code = ?e.sqlite_error_code(),
                roots = watched_roots.len(),
                "file index sync failed"
            );
        }

        loop {
            match wait_any_changes(&mut watchers, Some(wake), None) {
                Ok(MultiWake::Signaled) => break,
                Ok(MultiWake::Timeout) => {}
                Ok(MultiWake::Watcher {
                    index,
                    batch: Some(batch),
                }) => {
                    if batch.overflow {
                        let _ = idx.sync_roots(&watched_roots, &cfg.ignore);
                    } else if let Some(root) = watched_roots.get(index) {
                        let events = changes_to_events(root, &batch.changes);
                        let _ = idx.apply_events(&events, &watched_roots, &cfg.ignore);
                    }
                }
                Ok(MultiWake::Watcher { index, batch: None }) => {
                    if index < watchers.len() {
                        watchers.remove(index);
                        watched_roots.remove(index);
                        let _ = idx.sync_roots(&watched_roots, &cfg.ignore);
                    }
                }
                Err(e) => {
                    tracing::warn!(code = e.code().0, "file index watch stopped");
                    return;
                }
            }
        }
    }
}

pub fn register(m: &mut Modules) {
    m.register_source(Box::new(FileCommands::new(unavailable().clone())));
}

/// Asks for the files named like `text`, for the query of `generation`. Returns at once; the answer
/// arrives on the UI thread.
pub fn request(generation: u64, text: &str) {
    static WORKER: OnceLock<Mutex<Sender<(u64, String)>>> = OnceLock::new();
    let tx = WORKER.get_or_init(|| {
        let (tx, rx) = mpsc::channel();
        let spawned = std::thread::Builder::new()
            .name("file-search".into())
            .spawn(move || work(rx, &mut ChildBackend::new(child_command)));
        if spawned.is_err() {
            tracing::warn!("file search is off: its thread could not start");
        }
        Mutex::new(tx)
    });
    let _ = tx
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .send((generation, text.to_owned()));
}

/// What a search can tell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Failure {
    /// Windows Search is not there to ask.
    Unavailable,
    /// No answer in time: the rows are dropped.
    Timeout,
    /// The query failed or the child died.
    Failed,
}

/// Where the answers come from: the child in the app, a fake in the tests.
pub trait Backend {
    fn service_running(&self) -> bool;
    fn query(&mut self, text: &str) -> Result<Vec<String>, Failure>;
    /// Lets go of what a quiet period no longer needs.
    fn stop(&mut self);
    /// True while there is something `stop` would let go of.
    fn running(&self) -> bool;
}

/// What the worker found for one request.
#[derive(Debug, PartialEq, Eq)]
pub struct Answer {
    pub paths: Vec<String>,
    /// The state of the "File search unavailable" row; None leaves it as it was.
    pub unavailable: Option<bool>,
}

pub fn answer(backend: &mut dyn Backend, text: &str) -> Answer {
    if !backend.service_running() {
        return Answer {
            paths: Vec::new(),
            unavailable: Some(true),
        };
    }
    match backend.query(text) {
        Ok(paths) => Answer {
            paths,
            unavailable: Some(false),
        },
        Err(Failure::Unavailable) => Answer {
            paths: Vec::new(),
            unavailable: Some(true),
        },
        Err(Failure::Timeout | Failure::Failed) => Answer {
            paths: Vec::new(),
            unavailable: None,
        },
    }
}

/// Routes `text` through `local` File index first, falling back to `backend` when sparse (`< 5`)
/// or explicitly prefixed with `ws:`. Calls `emit_early` immediately when sparse local hits exist
/// before waiting on Windows Search.
pub fn route_answer(
    local: Option<&FileIndex>,
    backend: &mut dyn Backend,
    text: &str,
    mut emit_early: impl FnMut(Vec<String>),
) -> Answer {
    match classify_query(text) {
        None => Answer {
            paths: Vec::new(),
            unavailable: None,
        },
        Some(FileQuery::WindowsSearch(q)) => answer(backend, q),
        Some(FileQuery::Local(q)) => {
            let Some(idx) = local else {
                return answer(backend, q);
            };
            let local_paths = idx.search(q, LIMIT).unwrap_or_default();
            if local_paths.len() >= SPARSE_THRESHOLD || !backend.service_running() {
                return Answer {
                    paths: local_paths,
                    unavailable: Some(false),
                };
            }
            if !local_paths.is_empty() {
                emit_early(local_paths.clone());
            }
            let merged = match backend.query(q) {
                Ok(ws_paths) => merge_paths(local_paths, ws_paths, LIMIT),
                Err(_) => local_paths,
            };
            Answer {
                paths: merged,
                unavailable: Some(false),
            }
        }
    }
}

/// The worker: the newest request wins, a quiet period stops the backend.
fn work(rx: Receiver<(u64, String)>, backend: &mut dyn Backend) {
    let mut reader: Option<FileIndex> = None;
    loop {
        // With nothing running there is nothing to stop: sleep until a request, with no timer.
        let got = if backend.running() {
            rx.recv_timeout(IDLE)
        } else {
            rx.recv().map_err(|_| RecvTimeoutError::Disconnected)
        };
        let mut next = match got {
            Ok(r) => r,
            Err(RecvTimeoutError::Timeout) => {
                backend.stop();
                continue;
            }
            Err(RecvTimeoutError::Disconnected) => return,
        };
        // Typing is faster than searching: only the newest query is worth asking.
        while let Ok(newer) = rx.try_recv() {
            next = newer;
        }
        let (generation, text) = next;
        if indexer_enabled() && reader.is_none() {
            reader = index_db_path().and_then(|p| FileIndex::open_reader(&p).ok());
        }
        let found = route_answer(reader.as_ref(), backend, &text, |early_paths| {
            unavailable().store(false, Ordering::Relaxed);
            let items = rows(&early_paths);
            crate::to_modules(move |c, h| c.on_file_results(generation, items, h));
        });
        if let Some(flag) = found.unavailable {
            unavailable().store(flag, Ordering::Relaxed);
        }
        let items = rows(&found.paths);
        crate::to_modules(move |c, h| c.on_file_results(generation, items, h));
    }
}

fn child_command() -> std::io::Result<Command> {
    let mut command = Command::new(std::env::current_exe()?);
    command
        .arg(SEARCH_FILES_ARG)
        .creation_flags(CREATE_NO_WINDOW);
    Ok(command)
}

/// A running child: its stdin, the lines it answers with, and when it was last asked.
struct Running {
    child: Child,
    stdin: ChildStdin,
    lines: Receiver<String>,
    /// Set while a request has not been answered completely.
    asked_at: Option<Instant>,
}

/// The persistent child, started on the first query.
pub struct ChildBackend {
    command: fn() -> std::io::Result<Command>,
    running: Option<Running>,
    next_id: u64,
    answer_ms: u64,
    cold_ms: u64,
}

impl ChildBackend {
    pub fn new(command: fn() -> std::io::Result<Command>) -> Self {
        Self {
            command,
            running: None,
            next_id: 0,
            answer_ms: ANSWER_MS,
            cold_ms: COLD_MS,
        }
    }

    fn start(&self) -> std::io::Result<Running> {
        let mut child = (self.command)()?
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        super::keep_in_job(&child);
        let (Some(stdin), Some(stdout)) = (child.stdin.take(), child.stdout.take()) else {
            let _ = child.kill();
            return Err(std::io::Error::other("the search child has no pipes"));
        };
        let (tx, lines) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                if tx.send(line).is_err() {
                    break;
                }
            }
        });
        Ok(Running {
            child,
            stdin,
            lines,
            asked_at: None,
        })
    }

    fn kill(&mut self) {
        if let Some(mut r) = self.running.take() {
            let _ = r.child.kill();
            let _ = r.child.wait();
        }
    }
}

impl Backend for ChildBackend {
    fn service_running(&self) -> bool {
        search::service_running()
    }

    fn query(&mut self, text: &str) -> Result<Vec<String>, Failure> {
        // A child that has been silent too long is stuck.
        if self
            .running
            .as_ref()
            .and_then(|r| r.asked_at)
            .is_some_and(|t| t.elapsed() > STUCK)
        {
            self.kill();
        }
        let fresh = self.running.is_none();
        if fresh {
            self.running = Some(self.start().map_err(|_| Failure::Failed)?);
        }
        self.next_id += 1;
        let id = self.next_id;
        let Some(r) = self.running.as_mut() else {
            return Err(Failure::Failed);
        };
        // The query is one line: a control character would split it.
        let line: String = text
            .chars()
            .map(|c| if c.is_control() { ' ' } else { c })
            .collect();
        if writeln!(r.stdin, "{id}\t{line}")
            .and_then(|()| r.stdin.flush())
            .is_err()
        {
            self.kill();
            return Err(Failure::Failed);
        }
        r.asked_at.get_or_insert_with(Instant::now);
        let budget = if fresh { self.cold_ms } else { self.answer_ms };
        let end = Instant::now() + Duration::from_millis(budget);
        let mut paths = Vec::new();
        loop {
            let left = end.saturating_duration_since(Instant::now());
            match r.lines.recv_timeout(left) {
                Ok(l) => {
                    let Some((got, rest)) = l.split_once('\t') else {
                        continue;
                    };
                    // An answer to an older request that finished late.
                    if got.parse::<u64>() != Ok(id) {
                        continue;
                    }
                    match rest {
                        "" => {
                            r.asked_at = None;
                            return Ok(paths);
                        }
                        "!" => {
                            r.asked_at = None;
                            return Err(Failure::Unavailable);
                        }
                        "?" => {
                            r.asked_at = None;
                            return Err(Failure::Failed);
                        }
                        path => paths.push(path.to_owned()),
                    }
                }
                // The child stays: its answer is dropped, and it may be quick the next time.
                Err(RecvTimeoutError::Timeout) => return Err(Failure::Timeout),
                Err(RecvTimeoutError::Disconnected) => {
                    self.kill();
                    return Err(Failure::Failed);
                }
            }
        }
    }

    fn stop(&mut self) {
        self.kill();
    }

    fn running(&self) -> bool {
        self.running.is_some()
    }
}

impl Drop for ChildBackend {
    fn drop(&mut self) {
        self.kill();
    }
}

/// The child: reads `id<TAB>text` lines from stdin and answers each with `id<TAB>path` lines and a
/// last `id<TAB>` line (`id<TAB>!` first when Windows Search is not there, `id<TAB>?` when the
/// query failed). Ends when stdin closes,
/// which is when the app stops asking or exits. Nothing of the text is logged (T-010).
pub fn search_files() -> anyhow::Result<()> {
    let stdin = std::io::stdin();
    let mut out = std::io::stdout().lock();
    for line in stdin.lock().lines() {
        let line = line?;
        let Some((id, text)) = line.split_once('\t') else {
            continue;
        };
        match run_query(text) {
            Ok(paths) => {
                for p in paths {
                    // A name with a line break or another control character would forge a line.
                    if !p.is_empty() && p != "!" && p != "?" && !p.chars().any(char::is_control) {
                        writeln!(out, "{id}\t{p}")?;
                    }
                }
            }
            Err(SearchError::Unavailable) => writeln!(out, "{id}\t!")?,
            Err(SearchError::Failed) => writeln!(out, "{id}\t?")?,
        }
        writeln!(out, "{id}\t")?;
        out.flush()?;
    }
    Ok(())
}

/// The ranked query first; when it finds nothing, the words anywhere in the name.
fn run_query(text: &str) -> Result<Vec<String>, SearchError> {
    let Some(sql) = contains_sql(text, LIMIT) else {
        return Ok(Vec::new());
    };
    // Search rejects some punctuation inside a CONTAINS phrase ("1=1*"): that is not a failure of
    // the search, the substring query below takes it.
    let found = match search::query(&sql) {
        Ok(found) if !found.is_empty() => return Ok(found),
        Ok(found) => found,
        Err(SearchError::Unavailable) => return Err(SearchError::Unavailable),
        Err(SearchError::Failed) => Vec::new(),
    };
    match like_sql(text, LIMIT) {
        Some(sql) => search::query(&sql),
        None => Ok(found),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fake {
        service: bool,
        result: Result<Vec<String>, Failure>,
        asked: Vec<String>,
    }

    impl Backend for Fake {
        fn service_running(&self) -> bool {
            self.service
        }
        fn query(&mut self, text: &str) -> Result<Vec<String>, Failure> {
            self.asked.push(text.to_owned());
            self.result.clone()
        }
        fn stop(&mut self) {}
        fn running(&self) -> bool {
            false
        }
    }

    fn fake(service: bool, result: Result<Vec<String>, Failure>) -> Fake {
        Fake {
            service,
            result,
            asked: Vec::new(),
        }
    }

    // @R-013
    #[test]
    fn r013_search_disabled_shows_unavailable_row() {
        // The service is stopped: no query is made, the row shows.
        let mut stopped = fake(false, Ok(vec!["C:\\a".into()]));
        let a = answer(&mut stopped, "a");
        assert_eq!((a.paths.len(), a.unavailable), (0, Some(true)));
        assert!(stopped.asked.is_empty());
        // The child says the provider is not there.
        let a = answer(&mut fake(true, Err(Failure::Unavailable)), "a");
        assert_eq!((a.paths.len(), a.unavailable), (0, Some(true)));
        // An answer clears the row.
        let a = answer(&mut fake(true, Ok(vec!["C:\\a.txt".into()])), "a");
        assert_eq!(
            (a.paths, a.unavailable),
            (vec!["C:\\a.txt".to_owned()], Some(false))
        );
        // A slow or failed query drops the rows and leaves the row as it was.
        for failure in [Failure::Timeout, Failure::Failed] {
            let a = answer(&mut fake(true, Err(failure)), "a");
            assert_eq!((a.paths.len(), a.unavailable), (0, None));
        }
    }

    /// A child that reads and never answers: PowerShell sleeping.
    fn hanging() -> std::io::Result<Command> {
        let mut c = Command::new("powershell");
        c.args(["-NoProfile", "-Command", "Start-Sleep -Seconds 30"]);
        Ok(c)
    }

    // @R-012
    #[test]
    fn r012_file_provider_times_out_without_blocking() {
        let mut backend = ChildBackend::new(hanging);
        backend.cold_ms = ANSWER_MS;
        let started = Instant::now();
        let out = backend.query("note");
        let took = started.elapsed();
        assert_eq!(out, Err(Failure::Timeout));
        assert!(
            took >= Duration::from_millis(ANSWER_MS)
                && took < Duration::from_millis(ANSWER_MS + 1500),
            "{took:?}"
        );
        // The worker does not wait for a hanging backend: the next query times out on its own clock.
        let again = Instant::now();
        assert_eq!(backend.query("note2"), Err(Failure::Timeout));
        assert!(again.elapsed() < Duration::from_millis(ANSWER_MS + 1500));
        backend.stop();
        assert!(backend.running.is_none(), "the child is gone after stop");
    }

    /// A child that answers like the real one, for two requests: a path and the end line.
    fn answering() -> std::io::Result<Command> {
        let mut c = Command::new("powershell");
        c.args([
            "-NoProfile",
            "-Command",
            "while ($l = [Console]::In.ReadLine()) { $id = $l.Split(\"`t\")[0]; \
             if ($id -eq '2') { [Console]::Out.WriteLine(\"1`tC:\\stale.txt\"); } \
             [Console]::Out.WriteLine(\"$id`tC:\\found $id.txt\"); [Console]::Out.WriteLine(\"$id`t\"); \
             [Console]::Out.Flush() }",
        ]);
        Ok(c)
    }

    // @R-013
    #[test]
    fn r013_the_child_protocol_round_trips_and_skips_late_answers() {
        let mut backend = ChildBackend::new(answering);
        // The first answer may take a PowerShell start; give it room.
        backend.answer_ms = 10_000;
        backend.cold_ms = 10_000;
        assert_eq!(backend.query("one"), Ok(vec!["C:\\found 1.txt".to_owned()]));
        // An answer that belongs to request 1 arrives during request 2: it is skipped.
        assert_eq!(backend.query("two"), Ok(vec!["C:\\found 2.txt".to_owned()]));
        backend.stop();
    }

    // @R-013
    #[test]
    fn r013_a_dead_child_is_a_failure_and_the_next_query_starts_a_new_one() {
        fn dying() -> std::io::Result<Command> {
            let mut c = Command::new("powershell");
            c.args(["-NoProfile", "-Command", "exit 0"]);
            Ok(c)
        }
        let mut backend = ChildBackend::new(dying);
        backend.answer_ms = 10_000;
        backend.cold_ms = 10_000;
        assert_eq!(backend.query("x"), Err(Failure::Failed));
        assert!(backend.running.is_none());
        assert_eq!(backend.query("y"), Err(Failure::Failed));
    }

    fn seeded_index(names: &[&str]) -> (std::path::PathBuf, naobi_launcher::file_index::FileIndex) {
        static SEQ: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let id = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("naobi-app-files-{}-{id}", std::process::id(),));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("mkdir");
        for name in names {
            std::fs::write(dir.join(name), "x").expect("write");
        }
        let mut idx = naobi_launcher::file_index::FileIndex::open_in_memory().expect("idx");
        idx.sync_roots(std::slice::from_ref(&dir), &[])
            .expect("sync");
        (dir, idx)
    }

    // @R-013
    #[test]
    fn r013_local_index_serves_without_spawning_child_when_enough_hits() {
        let (dir, idx) = seeded_index(&[
            "note0.txt",
            "note1.txt",
            "note2.txt",
            "note3.txt",
            "note4.txt",
            "note5.txt",
        ]);
        let mut backend = fake(true, Ok(vec![r"C:\ws\other.txt".into()]));
        let mut early = Vec::new();
        let a = route_answer(Some(&idx), &mut backend, "note", |p| early = p);
        assert_eq!(a.paths.len(), 6);
        assert_eq!(a.unavailable, Some(false));
        assert!(backend.asked.is_empty(), "Windows Search was not queried");
        assert!(early.is_empty(), "no duplicate emit when >= threshold");
        let _ = std::fs::remove_dir_all(&dir);
    }

    // @R-013
    #[test]
    fn r013_sparse_local_hits_fall_back_and_merge_with_windows_search() {
        let (dir, idx) = seeded_index(&["alpha1.txt", "alpha2.txt"]);
        let local1 = dir.join("alpha1.txt").to_string_lossy().into_owned();
        let local2 = dir.join("alpha2.txt").to_string_lossy().into_owned();
        let mut backend = fake(true, Ok(vec![local1.clone(), r"C:\ws\alpha3.txt".into()]));
        let mut early = Vec::new();
        let a = route_answer(Some(&idx), &mut backend, "alpha", |p| early = p);
        assert_eq!(early, vec![local1.clone(), local2.clone()]);
        assert_eq!(backend.asked, vec!["alpha"]);
        assert_eq!(
            a.paths,
            vec![local1.clone(), local2.clone(), r"C:\ws\alpha3.txt".into()]
        );
        assert_eq!(a.unavailable, Some(false));

        // When Windows Search is stopped, sparse local hits remain available without Unavailable row.
        let mut stopped = fake(false, Ok(vec![]));
        let a = route_answer(Some(&idx), &mut stopped, "alpha", |_| {});
        assert_eq!(a.paths, vec![local1, local2]);
        assert_eq!(a.unavailable, Some(false));

        // Without local index and stopped Windows Search, Unavailable row shows.
        let a = route_answer(None, &mut stopped, "alpha", |_| {});
        assert!(a.paths.is_empty());
        assert_eq!(a.unavailable, Some(true));
        let _ = std::fs::remove_dir_all(&dir);
    }

    // @R-013
    #[test]
    fn r013_ws_prefix_bypasses_local_index_and_strips_prefix() {
        let (dir, idx) = seeded_index(&[
            "ws0.txt", "ws1.txt", "ws2.txt", "ws3.txt", "ws4.txt", "ws5.txt",
        ]);
        let mut backend = fake(true, Ok(vec![r"C:\System\ws_hit.txt".into()]));
        let a = route_answer(Some(&idx), &mut backend, "ws:ws", |_| {});
        assert_eq!(backend.asked, vec!["ws"]);
        assert_eq!(a.paths, vec![r"C:\System\ws_hit.txt"]);
        assert_eq!(a.unavailable, Some(false));

        let mut blank = fake(true, Ok(vec![]));
        let a = route_answer(Some(&idx), &mut blank, "ws:   ", |_| {});
        assert!(blank.asked.is_empty());
        assert!(a.paths.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    // @R-013
    #[test]
    fn r013_changes_to_events_pairs_renames_and_skips_modified() {
        use naobi_launcher::file_index::IndexEvent;
        use naobi_win32::dirwatch::{FileAction, FileChange};
        use std::path::Path;

        let root = Path::new(r"C:\Users\me\Docs");
        let events = changes_to_events(
            root,
            &[
                FileChange {
                    action: FileAction::RenamedOld,
                    rel_path: "a.txt".into(),
                },
                FileChange {
                    action: FileAction::RenamedNew,
                    rel_path: "b.txt".into(),
                },
                FileChange {
                    action: FileAction::Added,
                    rel_path: "c.txt".into(),
                },
                FileChange {
                    action: FileAction::Modified,
                    rel_path: "d.txt".into(),
                },
                FileChange {
                    action: FileAction::Removed,
                    rel_path: "e.txt".into(),
                },
                FileChange {
                    action: FileAction::RenamedOld,
                    rel_path: "orphan_old.txt".into(),
                },
                FileChange {
                    action: FileAction::RenamedNew,
                    rel_path: "orphan_new.txt".into(),
                },
            ],
        );
        assert_eq!(
            events,
            vec![
                IndexEvent::Renamed {
                    from: root.join("a.txt"),
                    to: root.join("b.txt"),
                },
                IndexEvent::Added(root.join("c.txt")),
                IndexEvent::Removed(root.join("e.txt")),
                IndexEvent::Renamed {
                    from: root.join("orphan_old.txt"),
                    to: root.join("orphan_new.txt"),
                },
            ]
        );
    }

    #[test]
    fn d020_file_index_errors_never_log_paths() {
        use std::sync::{Arc, Mutex};
        #[derive(Clone, Default)]
        struct Buf(Arc<Mutex<Vec<u8>>>);
        impl std::io::Write for Buf {
            fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
                self.0
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .extend_from_slice(buf);
                Ok(buf.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }

        let dir = std::env::temp_dir().join(format!(
            "naobi-files-log-{}-zq-file-path-9914",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("mkdir");
        let blocker = dir.join("zq-blocker-file");
        std::fs::write(&blocker, "not a directory").expect("write blocker");
        let bad_db = blocker.join("zq-secret-db-9914.db");
        let wake = WakeEvent::new().expect("wake");

        let buf = Buf::default();
        let sink = buf.clone();
        let sub = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::WARN)
            .with_target(false)
            .with_writer(move || sink.clone())
            .finish();
        tracing::subscriber::with_default(sub, || {
            tracing::callsite::rebuild_interest_cache();
            run_indexer(&bad_db, &wake);
        });

        let text =
            String::from_utf8_lossy(&buf.0.lock().unwrap_or_else(|e| e.into_inner())).into_owned();
        let _ = std::fs::remove_dir_all(&dir);
        assert!(
            text.contains("file index store could not be opened"),
            "{text}"
        );
        assert!(text.contains("code="), "{text}");
        assert!(!text.contains("zq-file-path-9914"), "leaked dir: {text}");
        assert!(!text.contains("zq-blocker-file"), "leaked file: {text}");
        assert!(!text.contains("zq-secret-db-9914"), "leaked db: {text}");
    }
}
