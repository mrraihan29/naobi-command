//! Content-free logs (D-020): tracing to size-rotated files at level info. Callers pass ids, kinds and
//! counts only; never a clipboard entry, a query, a file name or a settings value.
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use naobi_core::notice::NoticeKind;
use tracing_subscriber::fmt::MakeWriter;

const LOG_NAME: &str = "naobi-command.log";
/// D-020: at most 5 files of 1 MB each.
const MAX_BYTES: u64 = 1024 * 1024;
const KEEP: usize = 5;

/// Appends to `dir/name`. Before a write would take the file past `max`, the file moves to `name.1`
/// (and the older ones up one), so at most `keep` files exist.
pub struct SizeRotating {
    dir: PathBuf,
    name: String,
    file: Option<File>,
    len: u64,
    max: u64,
    keep: usize,
}

impl SizeRotating {
    pub fn open(dir: &Path, name: &str, max: u64, keep: usize) -> io::Result<Self> {
        fs::create_dir_all(dir)?;
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(dir.join(name))?;
        let len = file.metadata()?.len();
        Ok(Self {
            dir: dir.to_path_buf(),
            name: name.into(),
            file: Some(file),
            len,
            max,
            keep,
        })
    }

    /// Slot 0 is the live file, slot `i` is `name.i`.
    fn path(&self, slot: usize) -> PathBuf {
        match slot {
            0 => self.dir.join(&self.name),
            i => self.dir.join(format!("{}.{i}", self.name)),
        }
    }

    fn rotate(&mut self) -> io::Result<()> {
        // The live file is closed first: Windows does not rename an open file.
        self.file = None;
        let _ = fs::remove_file(self.path(self.keep - 1));
        for slot in (1..self.keep - 1).rev() {
            let _ = fs::rename(self.path(slot), self.path(slot + 1));
        }
        fs::rename(self.path(0), self.path(1))?;
        self.file = Some(
            OpenOptions::new()
                .create(true)
                .append(true)
                .open(self.path(0))?,
        );
        self.len = 0;
        Ok(())
    }
}

impl Write for SizeRotating {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let full = self.len > 0 && self.len + buf.len() as u64 > self.max;
        if self.file.is_none() || full {
            self.rotate()?;
        }
        let file = self
            .file
            .as_mut()
            .ok_or_else(|| io::Error::other("no log file"))?;
        let n = file.write(buf)?;
        self.len += n as u64;
        Ok(n)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.file.as_mut().map_or(Ok(()), Write::flush)
    }
}

/// The rotating file shared by every thread that logs.
#[derive(Clone)]
struct Shared(Arc<Mutex<SizeRotating>>);

impl Write for Shared {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).flush()
    }
}

fn subscriber<W>(writer: W) -> impl tracing::Subscriber + Send + Sync
where
    W: for<'w> MakeWriter<'w> + Send + Sync + 'static,
{
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_target(false)
        .with_writer(writer)
        .finish()
}

/// Starts the log in `dir` for this process. Best effort: when it fails the app runs without a log.
pub fn init(dir: &Path) -> io::Result<()> {
    let file = Shared(Arc::new(Mutex::new(SizeRotating::open(
        dir, LOG_NAME, MAX_BYTES, KEEP,
    )?)));
    tracing::subscriber::set_global_default(subscriber(move || file.clone()))
        .map_err(|_| io::Error::other("a log subscriber is already set"))
}

/// A hotkey fired. `name` is its settings name, such as "launcher".
pub fn hotkey(name: &str) {
    tracing::info!(hotkey = name, "hotkey pressed");
}

/// A system command ran (T-009): its id only. The log line carries the time.
pub fn system_command(id: &str) {
    tracing::info!(command = id, "system command ran");
}

/// The clipboard retention removed entries (T-010): counts and the reason, never content.
pub fn clipboard_purge(expired: usize, excess: usize, evicted: usize) {
    tracing::info!(
        expired,
        excess,
        evicted,
        "clipboard purge: retention, entry count and size cap"
    );
}

/// A Notice was raised. Its kind only: the message can echo settings text.
pub fn notice(kind: NoticeKind) {
    tracing::info!(kind = ?kind, "notice raised");
}

/// A command ran and produced `count` rows.
// naobi-allow: the launcher commands (NCMD-15) are the first callers.
#[cfg_attr(not(test), allow(dead_code))]
pub fn command(id: &str, count: usize) {
    tracing::info!(command = id, count, "command ran");
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A log buffer that a test can read back.
    #[derive(Clone, Default)]
    struct Buf(Arc<Mutex<Vec<u8>>>);

    impl Write for Buf {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.0
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    /// A folder for one test, emptied first. Removed by the caller at the end.
    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("naobi-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    /// Runs `f` with a log that records into the returned text.
    fn capture(f: impl FnOnce()) -> String {
        let buf = Buf::default();
        let sink = buf.clone();
        tracing::subscriber::with_default(subscriber(move || sink.clone()), || {
            // Another test thread may have hit a callsite first with no subscriber and cached it as
            // disabled; rebuild so this subscriber sees every event.
            tracing::callsite::rebuild_interest_cache();
            f();
        });
        let bytes = buf.0.lock().unwrap_or_else(|e| e.into_inner()).clone();
        String::from_utf8_lossy(&bytes).into_owned()
    }

    #[test]
    fn d020_logs_contain_no_content() {
        // Markers stand in for a query, a clipboard entry and a file name.
        const MARKERS: [&str; 3] = ["zq-query-7731", "zq-clip-7731", "zq-file-7731"];
        let text = capture(|| {
            let mut center = crate::notices::NoticeCenter::default();
            center.raise(naobi_core::notice::Notice {
                subject: "launcher".into(),
                message: format!("{} {} {}", MARKERS[0], MARKERS[1], MARKERS[2]),
                kind: NoticeKind::Warning,
            });
            hotkey("launcher");
            notice(NoticeKind::Warning);
        });
        assert!(text.contains("notice raised"), "{text}");
        assert!(text.contains("hotkey pressed"), "{text}");
        for marker in MARKERS {
            assert!(!text.contains(marker), "log leaked {marker}: {text}");
        }
    }

    // @R-020
    #[test]
    fn t009_system_command_log_has_id_and_time_only() {
        let text = capture(|| system_command("sys.restart"));
        assert!(text.contains("sys.restart"), "{text}");
        assert!(text.contains("system command ran"), "{text}");
        // The line starts with the timestamp tracing adds.
        assert!(text.trim_start().starts_with("20"), "{text}");
    }

    #[test]
    fn d020_dev_tools_never_log_clipboard_payloads() {
        use naobi_core::command::{CommandSource, Cx, Query};
        use naobi_core::ids::ActionId;
        use naobi_launcher::providers::dev::{DevEnv, DevSource};

        struct FakeDevEnv;
        impl DevEnv for FakeDevEnv {
            fn clipboard_text(&self) -> Option<String> {
                Some("zq-clip-dev-9182".to_owned())
            }
            fn random16(&self) -> Option<[u8; 16]> {
                Some([0x42; 16])
            }
            fn now_ms(&self) -> i64 {
                1_700_000_000_000
            }
        }

        let text = capture(|| {
            let src = DevSource::new(Box::new(FakeDevEnv));
            for cmd in ["b64", "b64d", "url", "urld", "sha256", "json", "wc"] {
                let items = src.items(&Query { text: cmd.into() });
                for item in items.iter() {
                    let mut cx = Cx::default();
                    let _ = src.run(&item.id, ActionId("copy".into()), &mut cx);
                }
            }
        });
        assert!(
            !text.contains("zq-clip-dev-9182"),
            "log leaked clipboard payload: {text}"
        );
        assert!(text.is_empty(), "dev tools must emit zero logs: {text}");
    }

    #[test]
    fn d020_log_has_command_id_no_content() {
        let text = capture(|| command("clip.paste", 3));
        assert!(text.contains("clip.paste"), "{text}");
        assert!(text.contains("count=3"), "{text}");
    }

    #[test]
    fn d020_log_rotation_keeps_five_files_of_1mb() {
        let dir = temp("rotation");
        let mut log = SizeRotating::open(&dir, "t.log", MAX_BYTES, KEEP).expect("open");
        let line = [b'x'; 1000];
        for _ in 0..8000 {
            log.write_all(&line).expect("write");
        }
        drop(log);
        let mut files: Vec<PathBuf> = fs::read_dir(&dir)
            .expect("read dir")
            .filter_map(|e| e.ok().map(|e| e.path()))
            .collect();
        files.sort();
        assert_eq!(files.len(), KEEP, "{files:?}");
        for f in &files {
            let len = fs::metadata(f).expect("stat").len();
            assert!(len <= MAX_BYTES, "{} is {len} bytes", f.display());
        }
        let _ = fs::remove_dir_all(&dir);
    }
}
