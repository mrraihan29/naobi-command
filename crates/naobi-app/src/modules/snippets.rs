//! Snippets module wiring (`naobi-launcher::providers::snippets` + `snippets.toml` hot-reload).
//! Never logs snippet names, aliases, text, or clipboard payloads (D-020).
use super::Modules;
use naobi_core::ids::{HotkeyId, ViewId};
use naobi_core::notice::{Notice, NoticeKind};
use naobi_core::settings::MAX_FILE_BYTES;
use naobi_core::snippet::{Snippet, SnippetsError, SnippetsFile, FILE_NAME};
use naobi_launcher::providers::snippets::{
    LocalDateTime, SnippetEnv, SnippetSet, SnippetSource, SnippetsCommands, SnippetsView, VIEW_ID,
};
use naobi_win32::{clipboard_marker, local_time, random};
use std::fs::File;
use std::io::{ErrorKind, Read};
use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};

pub const HOTKEY: &str = "snippets";
pub const NOTICE_SUBJECT: &str = FILE_NAME;

static SET: OnceLock<SnippetSet> = OnceLock::new();
static LAST: Mutex<Vec<Snippet>> = Mutex::new(Vec::new());

fn snippet_set() -> &'static SnippetSet {
    SET.get_or_init(SnippetSet::default)
}

struct WinSnippetEnv;

impl SnippetEnv for WinSnippetEnv {
    fn clipboard_text(&self) -> Option<String> {
        clipboard_marker::read_text()
    }

    fn random16(&self) -> Option<[u8; 16]> {
        random::random16().ok()
    }

    fn local_now(&self) -> LocalDateTime {
        let t = local_time::now();
        LocalDateTime {
            year: t.year,
            month: t.month,
            day: t.day,
            hour: t.hour,
            minute: t.minute,
            second: t.second,
        }
    }
}

pub fn register(m: &mut Modules) {
    let set = snippet_set().clone();
    let env: Arc<dyn SnippetEnv> = Arc::new(WinSnippetEnv);
    m.register_source(Box::new(SnippetsCommands));
    m.register_source(Box::new(SnippetSource::new(set.clone(), env.clone())));
    m.register_view(Box::new(SnippetsView::new(set, env)));
    m.subscribe(
        HotkeyId(HOTKEY.into()),
        Box::new(|_, cx| cx.open_view(ViewId(VIEW_ID.into()))),
    );
}

fn notice(line: Option<usize>, message: &str, kind: NoticeKind) -> Notice {
    let text = match line {
        Some(n) => format!("{FILE_NAME} line {n}: {message}"),
        None => format!("{FILE_NAME}: {message}"),
    };
    Notice {
        subject: NOTICE_SUBJECT.into(),
        message: text,
        kind,
    }
}

enum ReadSnippets {
    Missing,
    Unreadable,
    Text(String),
}

fn read_capped(path: &Path) -> ReadSnippets {
    let file = match File::open(path) {
        Ok(f) => f,
        Err(e) if e.kind() == ErrorKind::NotFound => return ReadSnippets::Missing,
        Err(_) => return ReadSnippets::Unreadable,
    };
    let mut bytes = Vec::new();
    if file
        .take(MAX_FILE_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .is_err()
    {
        return ReadSnippets::Unreadable;
    }
    ReadSnippets::Text(String::from_utf8_lossy(&bytes).into_owned())
}

/// Reloads `snippets.toml` from `path` keeping `last` on parse/oversize/transient read error.
pub fn reload_path(path: &Path, last: &[Snippet]) -> (Vec<Snippet>, Vec<Notice>) {
    let text = match read_capped(path) {
        ReadSnippets::Missing => return (Vec::new(), Vec::new()),
        ReadSnippets::Unreadable => return (last.to_vec(), Vec::new()),
        ReadSnippets::Text(t) => t,
    };
    match SnippetsFile::parse(&text) {
        Ok(parsed) => {
            let notices = parsed
                .reports
                .iter()
                .map(|r| notice(r.line, &r.message, NoticeKind::Warning))
                .collect();
            (parsed.snippets, notices)
        }
        Err(e) => {
            let line = match &e {
                SnippetsError::Syntax { line, .. } => *line,
                SnippetsError::TooLarge => None,
            };
            (
                last.to_vec(),
                vec![notice(line, &e.to_string(), NoticeKind::Error)],
            )
        }
    }
}

/// Reloads from `path`, updates the live `SnippetSet`, and returns any Notices to raise.
/// Logs only counts (D-020).
pub fn reload_and_apply(path: &Path) -> Vec<Notice> {
    let mut last = LAST.lock().unwrap_or_else(|e| e.into_inner());
    let (next, notices) = reload_path(path, &last);
    *last = next.clone();
    tracing::info!(count = next.len(), "snippets loaded");
    snippet_set().set(next);
    notices
}

/// Immediately reloads `snippets.toml` from `settings_dir()` (e.g. after Save as Snippet).
pub fn reload_now() {
    if let Some(dir) = naobi_core::paths::settings_dir() {
        let _ = reload_and_apply(&dir.join(FILE_NAME));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    struct Dir(PathBuf);

    impl Dir {
        fn new(name: &str) -> Self {
            let p =
                std::env::temp_dir().join(format!("naobi-mod-snip-{name}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&p);
            fs::create_dir_all(&p).expect("dir");
            Dir(p)
        }
    }

    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn hot_reload_applies_valid_keeps_last_on_error_and_clears_when_removed() {
        let dir = Dir::new("reload");
        let path = dir.0.join(FILE_NAME);

        // Missing file -> empty, no notice.
        let (s0, n0) = reload_path(&path, &[]);
        assert!(s0.is_empty() && n0.is_empty());

        // Valid edit applies.
        fs::write(
            &path,
            "[[snippets]]\nname = \"Hi\"\nalias = \"hi\"\ntext = \"Hello\"\n",
        )
        .expect("write");
        let (s1, n1) = reload_path(&path, &s0);
        assert_eq!(s1.len(), 1);
        assert!(n1.is_empty());

        // Invalid edit keeps last valid and raises line-numbered Error notice.
        fs::write(&path, "[[snippets]]\nname = \"Hi\"\nbroken = = 1\n").expect("write");
        let (s2, n2) = reload_path(&path, &s1);
        assert_eq!(s2, s1);
        assert_eq!(n2.len(), 1);
        assert_eq!(n2[0].kind, NoticeKind::Error);
        assert!(n2[0].message.starts_with("snippets.toml line 3:"));

        // Oversize file keeps last valid and raises Error notice.
        fs::write(&path, "#".repeat(MAX_FILE_BYTES + 1)).expect("write oversize");
        let (s_over, n_over) = reload_path(&path, &s1);
        assert_eq!(s_over, s1);
        assert_eq!(n_over.len(), 1);
        assert_eq!(n_over[0].kind, NoticeKind::Error);
        assert!(n_over[0].message.contains("256 KiB"));

        // Removing the file clears snippets cleanly.
        fs::remove_file(&path).expect("remove");
        let (s3, n3) = reload_path(&path, &s2);
        assert!(s3.is_empty() && n3.is_empty());
    }
}
