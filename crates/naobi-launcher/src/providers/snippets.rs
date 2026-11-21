//! Manual searchable Snippets (Issue #12): Root search, `snippets` View, and placeholder
//! expansion (`{date}`, `{time}`, `{uuid}`, `{clipboard}`) at activation time.
//! Never logs snippet content or clipboard payloads (D-020).
use crate::providers::dev::format_uuid_v4;
use naobi_core::command::{CommandSource, Cx, Outcome, Query};
use naobi_core::effect::Effect;
use naobi_core::ids::{ActionId, ItemId, ViewId};
use naobi_core::item::{Action, Item, ItemKind};
use naobi_core::notice::{Notice, NoticeKind};
use naobi_core::snippet::Snippet;
use naobi_core::view_source::{Fact, Preview, ViewSource};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

pub const ID_PREFIX: &str = "snip:";
pub const VIEW_ID: &str = "snippets";
pub const OPEN_CMD_ID: &str = "snip.open";

const PREVIEW_CHARS: usize = 100;

/// Local wall-clock date and time for `{date}` and `{time}` expansion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalDateTime {
    pub year: u16,
    pub month: u16,
    pub day: u16,
    pub hour: u16,
    pub minute: u16,
    pub second: u16,
}

/// Host capabilities needed when activating a Snippet.
pub trait SnippetEnv: Send + Sync {
    fn clipboard_text(&self) -> Option<String>;
    fn random16(&self) -> Option<[u8; 16]>;
    fn local_now(&self) -> LocalDateTime;
}

#[derive(Debug, Clone)]
struct IndexedSnippet {
    id: ItemId,
    snippet: Snippet,
}

/// Thread-safe live snippet catalog updated on startup and `snippets.toml` hot-reload.
#[derive(Clone, Default)]
pub struct SnippetSet(Arc<RwLock<Arc<[IndexedSnippet]>>>);

impl SnippetSet {
    pub fn set(&self, snippets: Vec<Snippet>) {
        let mut counts: HashMap<String, usize> = HashMap::new();
        let indexed: Arc<[IndexedSnippet]> = snippets
            .into_iter()
            .map(|snippet| {
                let count = counts.entry(snippet.name.clone()).or_insert(0);
                *count += 1;
                let id = if *count == 1 {
                    format!("{ID_PREFIX}{}", snippet.name)
                } else {
                    format!("{ID_PREFIX}{}#{count}", snippet.name)
                };
                IndexedSnippet {
                    id: ItemId(id),
                    snippet,
                }
            })
            .collect();
        *self.0.write().unwrap_or_else(|e| e.into_inner()) = indexed;
    }

    fn snapshot(&self) -> Arc<[IndexedSnippet]> {
        Arc::clone(&self.0.read().unwrap_or_else(|e| e.into_inner()))
    }

    fn find(&self, id: &ItemId) -> Option<Snippet> {
        self.snapshot()
            .iter()
            .find(|e| &e.id == id)
            .map(|e| e.snippet.clone())
    }
}

fn single_line_preview(text: &str) -> String {
    let line = text
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("");
    line.chars()
        .take(PREVIEW_CHARS)
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect()
}

fn to_item(entry: &IndexedSnippet) -> Item {
    let s = &entry.snippet;
    let preview = single_line_preview(&s.text);
    let subtitle = match &s.alias {
        Some(alias) if preview.is_empty() => alias.clone(),
        Some(alias) => format!("{alias} · {preview}"),
        None => preview,
    };
    let keywords = s.alias.iter().cloned().collect();
    Item {
        id: entry.id.clone(),
        title: s.name.clone(),
        subtitle,
        keywords,
        alias: s.alias.clone(),
        kind: ItemKind::Snippet,
        primary: ActionId("paste".into()),
        secondary: Some(ActionId("copy".into())),
    }
}

fn snippet_actions() -> Vec<Action> {
    vec![
        Action {
            id: ActionId("paste".into()),
            label: "Paste".into(),
            shortcut: None,
            enabled: Ok(()),
        },
        Action {
            id: ActionId("copy".into()),
            label: "Copy".into(),
            shortcut: None,
            enabled: Ok(()),
        },
    ]
}

/// Expands `{date}`, `{time}`, `{uuid}`, and `{clipboard}` in a single pass.
/// Clipboard content is never re-scanned for placeholders; unknown `{...}` stays literal.
pub fn expand_placeholders(template: &str, env: &dyn SnippetEnv) -> Result<String, Notice> {
    let mut out = String::with_capacity(template.len());
    let mut now: Option<LocalDateTime> = None;
    let mut clipboard: Option<String> = None;
    let mut rest = template;

    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after_open = &rest[open..];
        let Some(close) = after_open.find('}') else {
            out.push_str(after_open);
            rest = "";
            break;
        };
        let token = &after_open[..=close];
        match token {
            "{date}" => {
                let dt = *now.get_or_insert_with(|| env.local_now());
                out.push_str(&format!("{:04}-{:02}-{:02}", dt.year, dt.month, dt.day));
            }
            "{time}" => {
                let dt = *now.get_or_insert_with(|| env.local_now());
                out.push_str(&format!("{:02}:{:02}:{:02}", dt.hour, dt.minute, dt.second));
            }
            "{uuid}" => {
                let bytes = env.random16().ok_or_else(|| Notice {
                    subject: "Snippets".into(),
                    message: "Could not read system randomness.".into(),
                    kind: NoticeKind::Error,
                })?;
                out.push_str(&format_uuid_v4(bytes));
            }
            "{clipboard}" => {
                let clip =
                    clipboard.get_or_insert_with(|| env.clipboard_text().unwrap_or_default());
                out.push_str(clip);
            }
            _ => out.push_str(token),
        }
        rest = &after_open[close + 1..];
    }
    out.push_str(rest);
    Ok(out)
}

fn run_snippet(set: &SnippetSet, env: &dyn SnippetEnv, id: &ItemId, action: &ActionId) -> Outcome {
    let Some(snippet) = set.find(id) else {
        return Outcome::Failed(Notice {
            subject: "Snippets".into(),
            message: "That snippet is no longer defined.".into(),
            kind: NoticeKind::Error,
        });
    };
    let expanded = match expand_placeholders(&snippet.text, env) {
        Ok(text) => text,
        Err(notice) => return Outcome::Failed(notice),
    };
    match action.0.as_str() {
        "copy" => Outcome::Effects(vec![Effect::CopyText(expanded)]),
        _ => Outcome::Effects(vec![Effect::PasteText(expanded)]),
    }
}

/// Exposes all Snippets in Root search (matched by name or Alias).
pub struct SnippetSource {
    set: SnippetSet,
    env: Arc<dyn SnippetEnv>,
}

impl SnippetSource {
    pub fn new(set: SnippetSet, env: Arc<dyn SnippetEnv>) -> Self {
        Self { set, env }
    }
}

impl CommandSource for SnippetSource {
    fn items(&self, _q: &Query) -> Arc<[Item]> {
        self.set.snapshot().iter().map(to_item).collect()
    }

    fn actions(&self, _id: &ItemId) -> Vec<Action> {
        snippet_actions()
    }

    fn run(&self, id: &ItemId, action: ActionId, _cx: &mut Cx) -> Outcome {
        run_snippet(&self.set, &*self.env, id, &action)
    }
}

/// Root command to open the `snippets` View.
pub struct SnippetsCommands;

impl CommandSource for SnippetsCommands {
    fn items(&self, _q: &Query) -> Arc<[Item]> {
        vec![Item {
            id: ItemId(OPEN_CMD_ID.into()),
            title: "Snippets".into(),
            subtitle: String::new(),
            keywords: vec!["snippet".into(), "snippets".into(), "text expansion".into()],
            alias: None,
            kind: ItemKind::Command,
            primary: ActionId("run".into()),
            secondary: None,
        }]
        .into()
    }

    fn actions(&self, _id: &ItemId) -> Vec<Action> {
        vec![Action {
            id: ActionId("run".into()),
            label: "Open".into(),
            shortcut: None,
            enabled: Ok(()),
        }]
    }

    fn run(&self, id: &ItemId, _action: ActionId, _cx: &mut Cx) -> Outcome {
        if id.0 == OPEN_CMD_ID {
            Outcome::Effects(vec![Effect::PushView(ViewId(VIEW_ID.into()))])
        } else {
            Outcome::Done
        }
    }
}

/// Dedicated `snippets` View preserving `snippets.toml` order and previewing snippet text.
pub struct SnippetsView {
    set: SnippetSet,
    env: Arc<dyn SnippetEnv>,
}

impl SnippetsView {
    pub fn new(set: SnippetSet, env: Arc<dyn SnippetEnv>) -> Self {
        Self { set, env }
    }
}

impl CommandSource for SnippetsView {
    fn items(&self, q: &Query) -> Arc<[Item]> {
        let needle = q.text.trim().to_lowercase();
        let snap = self.set.snapshot();
        if needle.is_empty() {
            return snap.iter().map(to_item).collect();
        }
        let mut exact = Vec::new();
        let mut rest = Vec::new();
        for entry in snap.iter() {
            let s = &entry.snippet;
            let alias_exact = s
                .alias
                .as_deref()
                .is_some_and(|a| a.eq_ignore_ascii_case(&needle));
            if alias_exact {
                exact.push(to_item(entry));
            } else if s.name.to_lowercase().contains(&needle)
                || s.alias
                    .as_deref()
                    .is_some_and(|a| a.to_lowercase().contains(&needle))
                || s.text.to_lowercase().contains(&needle)
            {
                rest.push(to_item(entry));
            }
        }
        exact.extend(rest);
        exact.into()
    }

    fn actions(&self, _id: &ItemId) -> Vec<Action> {
        snippet_actions()
    }

    fn run(&self, id: &ItemId, action: ActionId, _cx: &mut Cx) -> Outcome {
        run_snippet(&self.set, &*self.env, id, &action)
    }
}

impl ViewSource for SnippetsView {
    fn view_id(&self) -> ViewId {
        ViewId(VIEW_ID.into())
    }

    fn title(&self) -> &'static str {
        "Snippets"
    }

    fn opened_by_hotkey(&self) -> bool {
        true
    }

    fn ranked(&self) -> bool {
        false
    }

    fn placeholder(&self) -> &'static str {
        "Search snippets..."
    }

    fn empty_title(&self) -> &'static str {
        "No snippets yet"
    }

    fn preview(&self, id: &ItemId) -> Option<Preview> {
        self.pane(id).map(|(p, _)| p)
    }

    fn pane(&self, id: &ItemId) -> Option<(Preview, Vec<Fact>)> {
        let snippet = self.set.find(id)?;
        let chars = snippet.text.chars().count();
        let mut facts = Vec::new();
        if let Some(alias) = snippet.alias {
            facts.push(Fact {
                label: "Alias".into(),
                value: alias,
            });
        }
        facts.push(Fact {
            label: "Length".into(),
            value: if chars == 1 {
                "1 character".into()
            } else {
                format!("{chars} characters")
            },
        });
        Some((Preview::Text(snippet.text), facts))
    }
}
