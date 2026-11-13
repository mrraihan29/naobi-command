//! The clipboard history as a Launcher view (R-015) and its two root commands. The view is a
//! `ViewSource` over the shared `History`: newest first, search inside the text, and the actions
//! Paste, Copy, Pin and Delete. It only produces data; pasting is an `Effect` the app executes.
use crate::history::{EntryMeta, History, Kind};
use naobi_core::chord::{KeyChord, CTRL};
use naobi_core::command::{CommandSource, Cx, Outcome, Query};
use naobi_core::effect::Effect;
use naobi_core::ids::{ActionId, EntryId, ItemId, ViewId};
use naobi_core::item::{Action, Item, ItemKind};
use naobi_core::notice::{Notice, NoticeKind};
use naobi_core::view_source::{Preview, ViewSource};
use std::sync::{Arc, Mutex, MutexGuard};

/// The history is read by the Launcher and written by the capture thread.
pub type Shared = Arc<Mutex<History>>;

pub const VIEW_ID: &str = "clipboard";
const ROWS: usize = 50;
const TITLE_CHARS: usize = 100;
/// The preview pane shows at most this many characters of a text entry.
const PREVIEW_CHARS: usize = 20_000;
/// A picture is decoded for the pane only up to this many pixels.
const PREVIEW_PIXELS: u64 = 2_000_000;

pub fn share(history: History) -> Shared {
    Arc::new(Mutex::new(history))
}

fn lock(h: &Shared) -> MutexGuard<'_, History> {
    h.lock().unwrap_or_else(|e| e.into_inner())
}

fn failed(message: &str) -> Outcome {
    Outcome::Failed(Notice {
        subject: "Clipboard".into(),
        message: message.into(),
        kind: NoticeKind::Error,
    })
}

fn entry_id(id: &ItemId) -> Option<i64> {
    id.0.strip_prefix("clip:")?.parse().ok()
}

/// "just now", "5 min ago", "3 h ago", "2 d ago".
pub fn relative(now: i64, then: i64) -> String {
    let secs = (now - then).max(0);
    match secs {
        0..=59 => "just now".to_owned(),
        60..=3599 => format!("{} min ago", secs / 60),
        3600..=86_399 => format!("{} h ago", secs / 3600),
        _ => format!("{} d ago", secs / 86_400),
    }
}

fn first_line(preview: &str) -> String {
    let line = preview.lines().next().unwrap_or("").trim();
    line.chars()
        .take(TITLE_CHARS)
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect()
}

pub struct ClipboardView {
    history: Shared,
}

impl ClipboardView {
    pub fn new(history: Shared) -> Self {
        Self { history }
    }

    fn row(h: &History, m: EntryMeta, now: i64) -> Option<Item> {
        let preview = h.preview(m.id).ok().flatten()?;
        let mut subtitle = relative(now, m.last_copied_at);
        if m.pinned {
            subtitle.push_str(" · Pinned");
        }
        Some(Item {
            id: ItemId(format!("clip:{}", m.id)),
            title: first_line(&preview),
            subtitle,
            keywords: Vec::new(),
            alias: None,
            kind: ItemKind::ClipboardEntry,
            primary: ActionId("paste".into()),
            secondary: Some(ActionId("copy".into())),
        })
    }
}

impl CommandSource for ClipboardView {
    fn items(&self, q: &Query) -> Arc<[Item]> {
        let mut h = lock(&self.history);
        let now = h.now_s();
        let text = q.text.trim();
        let metas: Vec<EntryMeta> = if text.is_empty() {
            h.list(ROWS).unwrap_or_default()
        } else {
            let ids = h.search(text).unwrap_or_default();
            ids.into_iter()
                .take(ROWS)
                .filter_map(|id| h.get(id).ok().flatten())
                .collect()
        };
        metas
            .into_iter()
            .filter_map(|m| Self::row(&h, m, now))
            .collect()
    }

    fn actions(&self, id: &ItemId) -> Vec<Action> {
        let pinned = entry_id(id)
            .and_then(|n| lock(&self.history).get(n).ok().flatten())
            .is_some_and(|m| m.pinned);
        let action = |id: &str, label: &str, shortcut: Option<KeyChord>| Action {
            id: ActionId(id.into()),
            label: label.into(),
            shortcut,
            enabled: Ok(()),
        };
        vec![
            action("paste", "Paste", None),
            action("copy", "Copy", None),
            action(
                "pin",
                if pinned { "Unpin" } else { "Pin" },
                Some(KeyChord {
                    mods: CTRL,
                    vk: 0xBE,
                }),
            ),
            action(
                "delete",
                "Delete",
                Some(KeyChord {
                    mods: CTRL,
                    vk: 0x58,
                }),
            ),
        ]
    }

    fn run(&self, id: &ItemId, action: ActionId, _cx: &mut Cx) -> Outcome {
        let Some(n) = entry_id(id) else {
            return Outcome::Done;
        };
        let mut h = lock(&self.history);
        match action.0.as_str() {
            "paste" => Outcome::Effects(vec![Effect::Paste(EntryId(n))]),
            "copy" => Outcome::Effects(vec![Effect::CopyEntry(EntryId(n))]),
            "pin" => {
                let pinned = h.get(n).ok().flatten().is_some_and(|m| m.pinned);
                let r = if pinned { h.unpin(n) } else { h.pin(n) };
                match r {
                    Ok(()) => Outcome::Refresh,
                    Err(crate::Error::PinLimit) => failed("At most 100 entries can be pinned."),
                    Err(_) => failed("The history could not be changed."),
                }
            }
            "delete" => match h.delete(n) {
                Ok(()) => Outcome::Refresh,
                Err(_) => failed("The history could not be changed."),
            },
            _ => Outcome::Done,
        }
    }
}

impl ViewSource for ClipboardView {
    fn view_id(&self) -> ViewId {
        ViewId(VIEW_ID.into())
    }

    fn opened_by_hotkey(&self) -> bool {
        true
    }

    fn ranked(&self) -> bool {
        false
    }

    fn empty_title(&self) -> &'static str {
        "Nothing copied yet"
    }

    fn preview(&self, id: &ItemId) -> Option<Preview> {
        let n = entry_id(id)?;
        let h = lock(&self.history);
        let meta = h.get(n).ok().flatten()?;
        let payload = h.payload(n).ok().flatten()?;
        match meta.kind {
            Kind::Text => Some(Preview::Text(
                String::from_utf8_lossy(&payload)
                    .chars()
                    .take(PREVIEW_CHARS)
                    .collect(),
            )),
            Kind::Files => Some(Preview::Files(
                String::from_utf8_lossy(&payload)
                    .lines()
                    .map(str::to_owned)
                    .collect(),
            )),
            Kind::Image => Some(decode_png(&payload).unwrap_or_else(|| {
                Preview::Text(
                    h.preview(n)
                        .ok()
                        .flatten()
                        .map(|p| p.to_string())
                        .unwrap_or_default(),
                )
            })),
        }
    }
}

/// The picture for the pane as RGBA, or None when it is too big or not what this build wrote.
fn decode_png(png: &[u8]) -> Option<Preview> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(png));
    decoder.set_transformations(png::Transformations::EXPAND);
    let mut reader = decoder.read_info().ok()?;
    let info = reader.info();
    let (w, h) = (info.width, info.height);
    if u64::from(w) * u64::from(h) > PREVIEW_PIXELS {
        return None;
    }
    let mut buf = vec![0; reader.output_buffer_size()?];
    let frame = reader.next_frame(&mut buf).ok()?;
    let rgba = match frame.color_type {
        png::ColorType::Rgba => buf[..frame.buffer_size()].to_vec(),
        png::ColorType::Rgb => buf[..frame.buffer_size()]
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect(),
        _ => return None,
    };
    Some(Preview::Rgba { w, h, rgba })
}

/// The two root commands: open the history, and clear it (a second Enter confirms, since it deletes
/// pinned entries too).
pub struct ClipboardCommands {
    history: Shared,
}

impl ClipboardCommands {
    pub fn new(history: Shared) -> Self {
        Self { history }
    }
}

const OPEN: &str = "clipboard.open";
const CLEAR: &str = "clipboard.clear";

impl CommandSource for ClipboardCommands {
    fn items(&self, _q: &Query) -> Arc<[Item]> {
        let command = |id: &str, title: &str, subtitle: &str| Item {
            id: ItemId(id.into()),
            title: title.into(),
            subtitle: subtitle.into(),
            keywords: Vec::new(),
            alias: None,
            kind: ItemKind::Command,
            primary: ActionId("run".into()),
            secondary: None,
        };
        vec![
            command(OPEN, "Clipboard History", ""),
            command(
                CLEAR,
                "Clear Clipboard History",
                "Press Enter twice to confirm",
            ),
        ]
        .into()
    }

    fn armed(&self) -> Vec<ItemId> {
        vec![ItemId(CLEAR.into())]
    }

    fn actions(&self, id: &ItemId) -> Vec<Action> {
        let label = if id.0 == CLEAR { "Clear" } else { "Open" };
        vec![Action {
            id: ActionId("run".into()),
            label: label.into(),
            shortcut: None,
            enabled: Ok(()),
        }]
    }

    fn run(&self, id: &ItemId, _action: ActionId, _cx: &mut Cx) -> Outcome {
        match id.0.as_str() {
            OPEN => Outcome::Effects(vec![Effect::PushView(ViewId(VIEW_ID.into()))]),
            CLEAR => match lock(&self.history).clear() {
                Ok(_) => Outcome::Done,
                Err(_) => failed("The history could not be cleared."),
            },
            _ => Outcome::Done,
        }
    }
}
