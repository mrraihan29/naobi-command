//! Scratchpad provider: multiline Markdown quick-capture view backed by `scratchpad.md`.
use naobi_core::command::{CommandSource, Cx, Outcome, Query};
use naobi_core::effect::Effect;
use naobi_core::ids::{ActionId, ItemId, ViewId};
use naobi_core::item::{Action, Item, ItemKind};
use naobi_core::notice::{Notice, NoticeKind};
use naobi_core::view_source::{Preview, ViewSource};
use std::fs;
use std::io::{self, ErrorKind};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};

pub const VIEW_ID: &str = "scratchpad";
pub const OPEN_CMD_ID: &str = "scratchpad.open";
pub const DOC_ITEM_ID: &str = "scratchpad:doc";
pub const AUTOSAVE_DEBOUNCE_MS: u64 = 500;

/// Persistence port for Scratchpad Markdown content.
pub trait ScratchpadStore: Send + Sync {
    fn load(&self) -> io::Result<String>;
    fn save(&self, content: &str) -> io::Result<()>;
}

/// Disk-backed `scratchpad.md` store using atomic temporary-file replacement.
pub struct FileScratchpadStore {
    path: PathBuf,
}

impl FileScratchpadStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }
}

impl ScratchpadStore for FileScratchpadStore {
    fn load(&self) -> io::Result<String> {
        match fs::read_to_string(&self.path) {
            Ok(text) => Ok(text),
            Err(e) if e.kind() == ErrorKind::NotFound => Ok(String::new()),
            Err(e) => Err(e),
        }
    }

    fn save(&self, content: &str) -> io::Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let tmp = self.path.with_extension("md.tmp");
        if let Err(e) = fs::write(&tmp, content).and_then(|()| fs::rename(&tmp, &self.path)) {
            let _ = fs::remove_file(&tmp);
            return Err(e);
        }
        Ok(())
    }
}

/// Root search command that pushes the Scratchpad view inside the Launcher.
pub struct ScratchpadCommands;

impl CommandSource for ScratchpadCommands {
    fn items(&self, _q: &Query) -> Arc<[Item]> {
        vec![Item {
            id: ItemId(OPEN_CMD_ID.into()),
            title: "Scratchpad".into(),
            subtitle: String::new(),
            keywords: vec![
                "notes".into(),
                "markdown".into(),
                "quick capture".into(),
                "draft".into(),
            ],
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

#[derive(Default)]
struct BufferState {
    text: String,
    dirty: bool,
    generation: u64,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn notice(message: &str) -> Notice {
    Notice {
        subject: "Scratchpad".into(),
        message: message.into(),
        kind: NoticeKind::Error,
    }
}

/// Multiline Scratchpad editor view hosted inside the single Launcher window.
pub struct ScratchpadView {
    store: Arc<dyn ScratchpadStore>,
    state: Mutex<BufferState>,
}

impl ScratchpadView {
    pub fn new(store: Arc<dyn ScratchpadStore>) -> Self {
        Self {
            store,
            state: Mutex::new(BufferState::default()),
        }
    }
}

impl CommandSource for ScratchpadView {
    fn items(&self, _q: &Query) -> Arc<[Item]> {
        vec![Item {
            id: ItemId(DOC_ITEM_ID.into()),
            title: "Scratchpad".into(),
            subtitle: String::new(),
            keywords: Vec::new(),
            alias: None,
            kind: ItemKind::Command,
            primary: ActionId("noop".into()),
            secondary: Some(ActionId("copy_all".into())),
        }]
        .into()
    }

    fn needs_confirm(&self, _id: &ItemId, action: &ActionId) -> bool {
        action.0 == "clear"
    }

    fn actions(&self, _id: &ItemId) -> Vec<Action> {
        let act = |id: &str, label: &str| Action {
            id: ActionId(id.into()),
            label: label.into(),
            shortcut: None,
            enabled: Ok(()),
        };
        vec![
            act("copy_all", "Copy all"),
            act("clear", "Clear scratchpad"),
        ]
    }

    fn run(&self, _id: &ItemId, action: ActionId, cx: &mut Cx) -> Outcome {
        match action.0.as_str() {
            "copy_all" => {
                cx.copy_text(lock(&self.state).text.clone());
                Outcome::Done
            }
            "clear" => {
                let mut s = lock(&self.state);
                match self.store.save("") {
                    Ok(()) => {
                        s.text.clear();
                        s.dirty = false;
                        s.generation += 1;
                        Outcome::Stay {
                            effects: vec![],
                            select: None,
                        }
                    }
                    Err(_) => Outcome::Failed(notice("The scratchpad file could not be cleared.")),
                }
            }
            _ => Outcome::Done,
        }
    }
}

impl ViewSource for ScratchpadView {
    fn view_id(&self) -> ViewId {
        ViewId(VIEW_ID.into())
    }

    fn title(&self) -> &'static str {
        "Scratchpad"
    }

    fn opened_by_hotkey(&self) -> bool {
        true
    }

    fn placeholder(&self) -> &'static str {
        "Write Markdown in Scratchpad..."
    }

    fn preview(&self, _id: &ItemId) -> Option<Preview> {
        None
    }

    fn is_editor(&self) -> bool {
        true
    }

    fn open_editor(&self) -> Result<String, Notice> {
        let mut s = lock(&self.state);
        match self.store.load() {
            Ok(text) => {
                s.text = text.clone();
                s.dirty = false;
                s.generation += 1;
                Ok(text)
            }
            Err(_) => Err(notice("The scratchpad file could not be read.")),
        }
    }

    fn editor_text(&self) -> Option<String> {
        Some(lock(&self.state).text.clone())
    }

    fn set_editor_text(&self, text: String) -> Option<u64> {
        let mut s = lock(&self.state);
        s.text = text;
        s.dirty = true;
        s.generation += 1;
        Some(s.generation)
    }

    fn flush_editor(&self, generation: Option<u64>) -> Result<(), Notice> {
        let mut s = lock(&self.state);
        if !s.dirty {
            return Ok(());
        }
        if let Some(expected) = generation {
            if s.generation != expected {
                return Ok(());
            }
        }
        match self.store.save(&s.text) {
            Ok(()) => {
                s.dirty = false;
                Ok(())
            }
            Err(_) => Err(notice("The scratchpad file could not be saved.")),
        }
    }
}
