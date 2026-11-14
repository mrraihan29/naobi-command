//! The `>` run source (R-024, D-038): a query starting with `>` is a command line. The root lists
//! the typed line first, then the run history lines that contain it, and Enter runs one. Item ids
//! and icon keys never hold command text; the line lives in this source (typed) or the history.
use crate::run_history::RunHistory;
use naobi_core::command::{CommandSource, Cx, Outcome, Query};
use naobi_core::effect::LaunchTarget;
use naobi_core::ids::{ActionId, ItemId};
use naobi_core::item::{Action, Item, ItemKind};
use naobi_core::notice::{Notice, NoticeKind};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use zeroize::Zeroizing;

pub const TYPED_ID: &str = "run:line";
/// Followed by the history row id, digits only.
pub const HISTORY_PREFIX: &str = "run:h:";
pub const MAX_LINE_CHARS: usize = 8000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Program {
    pub path: String,
    pub gui: bool,
}

/// The program the first token names (PATH, then App Paths). Called on Enter only.
pub trait ProgramLookup: Send + Sync {
    fn find(&self, token: &str) -> Option<Program>;
}

/// `"C:\a b\x.exe" -y` -> (`C:\a b\x.exe`, `-y`); `notepad a.txt` -> (`notepad`, `a.txt`). An
/// unclosed quote takes the rest as the token. The arguments are kept as typed.
pub fn split_first(line: &str) -> (&str, &str) {
    let line = line.trim_start();
    if let Some(rest) = line.strip_prefix('"') {
        return match rest.find('"') {
            Some(i) => (&rest[..i], rest[i + 1..].trim_start()),
            None => (rest, ""),
        };
    }
    match line.find(char::is_whitespace) {
        Some(i) => (&line[..i], line[i..].trim_start()),
        None => (line, ""),
    }
}

pub struct RunSource {
    history: Option<Mutex<RunHistory>>,
    lookup: Box<dyn ProgramLookup>,
    now_ms: Box<dyn Fn() -> i64 + Send + Sync>,
    typed: Mutex<Zeroizing<String>>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

fn action(id: &str, label: &str) -> Action {
    Action {
        id: ActionId(id.into()),
        label: label.into(),
        shortcut: None,
        enabled: Ok(()),
    }
}

fn row(id: String, line: &str, subtitle: String) -> Item {
    Item {
        id: ItemId(id),
        title: line.to_owned(),
        subtitle,
        keywords: Vec::new(),
        alias: None,
        kind: ItemKind::CommandLine,
        primary: ActionId("run".into()),
        secondary: None,
    }
}

fn notice(subject: &str, message: &str, kind: NoticeKind) -> Notice {
    Notice {
        subject: subject.into(),
        message: message.into(),
        kind,
    }
}

impl RunSource {
    /// `history` is None when the store could not be opened: lines still run, with a Warning Notice.
    pub fn new(
        history: Option<RunHistory>,
        lookup: Box<dyn ProgramLookup>,
        now_ms: Box<dyn Fn() -> i64 + Send + Sync>,
    ) -> Self {
        Self {
            history: history.map(Mutex::new),
            lookup,
            now_ms,
            typed: Mutex::new(Zeroizing::new(String::new())),
        }
    }

    /// The command line a row stands for.
    fn line_of(&self, id: &ItemId) -> Option<Zeroizing<String>> {
        if id.0 == TYPED_ID {
            return Some(lock(&self.typed).clone());
        }
        let n: i64 = id.0.strip_prefix(HISTORY_PREFIX)?.parse().ok()?;
        let mut h = lock(self.history.as_ref()?);
        h.lines().iter().find(|l| l.id == n).map(|l| l.line.clone())
    }
}

impl CommandSource for RunSource {
    fn claims(&self, q: &Query) -> bool {
        q.text.starts_with('>')
    }

    fn items(&self, q: &Query) -> Arc<[Item]> {
        let Some(text) = q.text.strip_prefix('>') else {
            // Not a run query: nothing decrypted stays in memory.
            if let Some(h) = &self.history {
                lock(h).forget();
            }
            lock(&self.typed).clear();
            return Arc::from(Vec::new());
        };
        let line = text.trim();
        *lock(&self.typed) = Zeroizing::new(line.to_owned());
        let mut rows = Vec::new();
        if !line.is_empty() {
            rows.push(row(TYPED_ID.into(), line, "Run command line".into()));
        }
        if let Some(h) = &self.history {
            let needle = line.to_lowercase();
            let mut h = lock(h);
            for l in h.lines() {
                if !line.is_empty()
                    && (l.line.as_str() == line || !l.line.to_lowercase().contains(&needle))
                {
                    continue;
                }
                let subtitle = match &l.program {
                    Some(p) => format!("History · {p}"),
                    None => "History".to_owned(),
                };
                rows.push(row(format!("{HISTORY_PREFIX}{}", l.id), &l.line, subtitle));
            }
        }
        rows.into()
    }

    fn actions(&self, id: &ItemId) -> Vec<Action> {
        let mut a = vec![action("run", "Run")];
        if id.0.starts_with(HISTORY_PREFIX) {
            a.push(action("delete", "Delete from Run History"));
        }
        a
    }

    fn run(&self, id: &ItemId, action: ActionId, cx: &mut Cx) -> Outcome {
        if action.0 == "delete" {
            let done =
                id.0.strip_prefix(HISTORY_PREFIX)
                    .and_then(|n| n.parse::<i64>().ok())
                    .zip(self.history.as_ref())
                    .map(|(n, h)| lock(h).delete(n).is_ok());
            return match done {
                Some(true) => Outcome::Refresh,
                _ => Outcome::Failed(notice(
                    "Run history",
                    "This command line was not deleted.",
                    NoticeKind::Error,
                )),
            };
        }
        let Some(line) = self.line_of(id).filter(|l| !l.is_empty()) else {
            return Outcome::Done;
        };
        if line.chars().count() > MAX_LINE_CHARS {
            return Outcome::Failed(notice(
                "Run",
                "That command line is too long.",
                NoticeKind::Error,
            ));
        }
        let (token, args) = split_first(&line);
        let found = if token.is_empty() {
            None
        } else {
            self.lookup.find(token)
        };
        let target = match &found {
            Some(p) if p.gui => LaunchTarget::Program {
                path: p.path.clone(),
                args: args.to_owned(),
            },
            _ => LaunchTarget::Console(line.to_string()),
        };
        let now = (self.now_ms)();
        let saved = self.history.as_ref().is_some_and(|h| {
            lock(h)
                .record(&line, found.as_ref().map(|p| p.path.as_str()), now)
                .is_ok()
        });
        cx.launch(target);
        if saved {
            Outcome::Done
        } else {
            Outcome::Effects(vec![naobi_core::effect::Effect::Notice(notice(
                "Run history",
                "This command line was not saved.",
                NoticeKind::Warning,
            ))])
        }
    }
}
