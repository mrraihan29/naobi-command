//! The Files section (NCMD-17, R-013): file rows from the Windows Search index, after the other
//! rows. The query runs in a transient child of this exe (`--search-files`): OLE DB maps tens of
//! megabytes for good, which the resident process must not carry (R-006). One worker thread asks the
//! child, newest query first, drops an answer that takes longer than 500 ms (T-020), and lets the
//! child go when nobody has searched for a while.
use super::Modules;
use naobi_launcher::providers::files::{contains_sql, like_sql, rows, FileCommands, LIMIT};
use naobi_win32::search::{self, SearchError};
use std::io::{BufRead, BufReader, Write};
use std::os::windows::process::CommandExt;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

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

/// The worker: the newest request wins, a quiet period stops the backend.
fn work(rx: Receiver<(u64, String)>, backend: &mut dyn Backend) {
    loop {
        let mut next = match rx.recv_timeout(IDLE) {
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
        let found = answer(backend, &text);
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
}

impl Drop for ChildBackend {
    fn drop(&mut self) {
        self.kill();
    }
}

/// The child: reads `id<TAB>text` lines from stdin and answers each with `id<TAB>path` lines and a
/// last `id<TAB>` line (`id<TAB>!` first when Windows Search is not there). Ends when stdin closes,
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
                    writeln!(out, "{id}\t{p}")?;
                }
            }
            Err(SearchError::Unavailable) => writeln!(out, "{id}\t!")?,
            Err(SearchError::Failed) => {}
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
    let found = search::query(&sql)?;
    if !found.is_empty() {
        return Ok(found);
    }
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
}
