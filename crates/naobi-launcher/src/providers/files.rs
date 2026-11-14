//! File results from the Windows Search index (R-013, REPORT 5.4 "Files"). The search itself is the
//! app's (a child process, because OLE DB must stay out of the resident one); this module owns what
//! is pure: the Search SQL built from user text (T-031), the rows, and the "unavailable" state.
use naobi_core::command::{CommandSource, Cx, Outcome, Query};
use naobi_core::effect::{Effect, LaunchTarget};
use naobi_core::ids::{ActionId, ItemId};
use naobi_core::item::{Action, Item, ItemKind};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Rows asked for (the engine shows at most this many too).
pub const LIMIT: usize = 30;
/// Longer text is cut: a name search never needs more.
const MAX_TEXT_CHARS: usize = 100;
/// The status row while Windows Search cannot answer.
pub const UNAVAILABLE: &str = "File search unavailable";

/// The words of `text` that a query can use: control characters, double quotes and `*` are dropped
/// (a quote cannot be escaped inside a CONTAINS phrase, and `*` is the prefix wildcard), single
/// quotes are doubled for the SQL literal.
fn terms(text: &str) -> Vec<String> {
    text.chars()
        .take(MAX_TEXT_CHARS)
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect::<String>()
        .split_whitespace()
        .map(|w| {
            w.chars()
                .filter(|c| !matches!(c, '"' | '*'))
                .collect::<String>()
        })
        .filter(|w| !w.is_empty())
        .map(|w| w.replace('\'', "''"))
        .collect()
}

/// `%`, `_` and `[` are wildcards in a Search LIKE; each goes in brackets to match itself.
fn like_escape(term: &str) -> String {
    let mut out = String::with_capacity(term.len());
    for c in term.chars() {
        match c {
            '%' | '_' | '[' => {
                out.push('[');
                out.push(c);
                out.push(']');
            }
            c => out.push(c),
        }
    }
    out
}

/// The two query skeletons. Windows Search has no parameter binding, so the escaped words are
/// placed into the skeleton (T-031 covers the escaping); the user text goes in last, after the
/// number, so it is never searched for a placeholder.
const CONTAINS_QUERY: &str = "SELECT TOP {top} System.ItemPathDisplay FROM SystemIndex WHERE CONTAINS(System.FileName, '{terms}') AND scope='file:' ORDER BY System.Search.Rank DESC";
const LIKE_QUERY: &str =
    "SELECT TOP {top} System.ItemPathDisplay FROM SystemIndex WHERE {likes} AND scope='file:'";

/// The ranked query: every word is a quoted CONTAINS prefix term on the file name. `None` when the
/// text has no usable word. The scope clause is part of the skeleton, so it always follows the text.
pub fn contains_sql(text: &str, limit: usize) -> Option<String> {
    let words = terms(text);
    if words.is_empty() {
        return None;
    }
    let phrase: Vec<String> = words.iter().map(|w| format!("\"{w}*\"")).collect();
    Some(
        CONTAINS_QUERY
            .replace("{top}", &limit.to_string())
            .replace("{terms}", &phrase.join(" AND ")),
    )
}

/// The fallback when the ranked query finds nothing: the words anywhere in the file name.
pub fn like_sql(text: &str, limit: usize) -> Option<String> {
    let words = terms(text);
    if words.is_empty() {
        return None;
    }
    let likes: Vec<String> = words
        .iter()
        .map(|w| format!("System.FileName LIKE '%{}%'", like_escape(w)))
        .collect();
    Some(
        LIKE_QUERY
            .replace("{top}", &limit.to_string())
            .replace("{likes}", &likes.join(" AND ")),
    )
}

/// A row for each path: the file name as the title and the full path as the subtitle, so a file
/// named like an app is never taken for the app (T-003).
pub fn rows(paths: &[String]) -> Vec<Item> {
    paths
        .iter()
        .take(LIMIT)
        .map(|p| Item {
            id: ItemId(format!("file:{p}")),
            title: p
                .rsplit(['\\', '/'])
                .find(|s| !s.is_empty())
                .unwrap_or(p)
                .to_owned(),
            subtitle: p.clone(),
            keywords: Vec::new(),
            alias: None,
            kind: ItemKind::File,
            primary: ActionId("open".into()),
            secondary: Some(ActionId("folder".into())),
        })
        .collect()
}

/// The folder of a path, for "Open Containing Folder".
fn parent(path: &str) -> Option<&str> {
    let end = path.rfind(['\\', '/'])?;
    let dir = &path[..end];
    // `C:` alone is the drive's current directory; `C:\` is its root.
    Some(if dir.ends_with(':') {
        &path[..=end]
    } else {
        dir
    })
}

/// The root source that owns the file rows' actions, and the "unavailable" state. It lists nothing
/// itself: its rows arrive from the search, through `Launcher::file_results`.
pub struct FileCommands {
    unavailable: Arc<AtomicBool>,
}

impl FileCommands {
    pub fn new(unavailable: Arc<AtomicBool>) -> Self {
        Self { unavailable }
    }
}

impl CommandSource for FileCommands {
    fn items(&self, _q: &Query) -> Arc<[Item]> {
        Arc::new([])
    }

    fn takes_files(&self) -> bool {
        true
    }

    fn pending(&self) -> Option<&'static str> {
        self.unavailable
            .load(Ordering::Relaxed)
            .then_some(UNAVAILABLE)
    }

    fn actions(&self, _id: &ItemId) -> Vec<Action> {
        let action = |id: &str, label: &str| Action {
            id: ActionId(id.into()),
            label: label.into(),
            shortcut: None,
            enabled: Ok(()),
        };
        vec![
            action("open", "Open"),
            action("folder", "Open Containing Folder"),
        ]
    }

    fn run(&self, id: &ItemId, action: ActionId, _cx: &mut Cx) -> Outcome {
        let Some(path) = id.0.strip_prefix("file:") else {
            return Outcome::Done;
        };
        let target = match action.0.as_str() {
            "open" => Some(path),
            "folder" => parent(path),
            _ => None,
        };
        match target {
            Some(t) => Outcome::Effects(vec![
                Effect::Launch(LaunchTarget::ShellItem(t.to_owned())),
                Effect::Hide,
            ]),
            None => Outcome::Done,
        }
    }
}
