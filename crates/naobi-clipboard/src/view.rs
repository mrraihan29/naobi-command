//! The clipboard history as a Launcher view (R-015) and its two root commands. The view is a
//! `ViewSource` over the shared `History`: newest first, search inside the text, and the actions
//! Paste, Copy, Pin and Delete. It only produces data; pasting is an `Effect` the app executes.
use crate::history::{EntryMeta, History, Kind};
use naobi_core::chord::{KeyChord, CTRL, SHIFT};
use naobi_core::command::{CommandSource, Cx, Outcome, Query};
use naobi_core::effect::Effect;
use naobi_core::ids::{ActionId, EntryId, ItemId, ViewId};
use naobi_core::item::{Action, Item, ItemKind};
use naobi_core::notice::{Notice, NoticeKind};
use naobi_core::view_source::{Fact, Preview, ViewSource};
use std::sync::{Arc, Mutex, MutexGuard};

/// The history is read by the Launcher and written by the capture thread.
pub type Shared = Arc<Mutex<History>>;

pub const VIEW_ID: &str = "clipboard";
const ROWS: usize = 50;
const UNREADABLE: &str = "Cannot read this entry";
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
    let line = preview
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("");
    line.chars()
        .take(TITLE_CHARS)
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect()
}

fn fact(label: &str, value: String) -> Fact {
    Fact {
        label: label.to_owned(),
        value,
    }
}

fn counted(n: usize, word: &str) -> String {
    let plural = if n == 1 { "" } else { "s" };
    format!("{n} {word}{plural}")
}

/// The pixel size stored in a PNG's header.
fn png_size(png: &[u8]) -> Option<(u32, u32)> {
    let reader = png::Decoder::new(std::io::Cursor::new(png))
        .read_info()
        .ok()?;
    let info = reader.info();
    Some((info.width, info.height))
}

/// What the preview pane says about an entry (R-027): type, length or size, and copy time.
fn entry_facts(meta: &EntryMeta, payload: &[u8], now: i64) -> Vec<Fact> {
    let text = || String::from_utf8_lossy(payload);
    let (kind, detail) = match meta.kind {
        Kind::Text => (
            "Text",
            fact("Length", counted(text().chars().count(), "character")),
        ),
        Kind::Image => (
            "Image",
            fact(
                "Size",
                png_size(payload).map_or("Unknown".to_owned(), |(w, h)| format!("{w} x {h}")),
            ),
        ),
        Kind::Files => (
            "Files",
            fact("Count", counted(text().lines().count(), "file")),
        ),
    };
    vec![
        fact("Type", kind.to_owned()),
        detail,
        fact("Copied", relative(now, meta.last_copied_at)),
    ]
}

pub struct ClipboardView {
    history: Shared,
}

impl ClipboardView {
    pub fn new(history: Shared) -> Self {
        Self { history }
    }

    fn row(h: &History, m: EntryMeta, now: i64) -> Option<Item> {
        // An entry that no longer decrypts stays in the list, so it can be deleted.
        let title = match h.preview(m.id) {
            Ok(Some(p)) => first_line(&p),
            Ok(None) => return None,
            Err(_) => UNREADABLE.to_owned(),
        };
        let mut subtitle = relative(now, m.last_copied_at);
        if m.pinned {
            subtitle.push_str(" · Pinned");
        }
        if let Some(pos) = h.queue().position(EntryId(m.id)) {
            subtitle.push_str(&format!(" · Queue #{}", pos + 1));
        }
        Some(Item {
            id: ItemId(format!("clip:{}", m.id)),
            title,
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
        let h = lock(&self.history);
        let meta = entry_id(id).and_then(|n| h.get(n).ok().flatten());
        let pinned = meta.as_ref().is_some_and(|m| m.pinned);
        let is_text = meta.as_ref().is_some_and(|m| m.kind == Kind::Text);
        let has_queued = !h.queue().is_empty();
        drop(h);
        let action = |id: &str, label: &str, shortcut: Option<KeyChord>| Action {
            id: ActionId(id.into()),
            label: label.into(),
            shortcut,
            enabled: Ok(()),
        };
        vec![
            action("paste", "Paste", None),
            action("copy", "Copy", None),
            Action {
                id: ActionId("paste_plain".into()),
                label: "Paste as Plain Text".into(),
                shortcut: Some(KeyChord {
                    mods: CTRL | SHIFT,
                    vk: 0x0D,
                }),
                enabled: if is_text {
                    Ok(())
                } else {
                    Err("Only text entries can be pasted as plain text.".into())
                },
            },
            action("queue_add", "Add to paste queue", None),
            Action {
                id: ActionId("queue_next".into()),
                label: "Paste next in queue".into(),
                shortcut: None,
                enabled: if has_queued {
                    Ok(())
                } else {
                    Err("Paste queue is empty.".into())
                },
            },
            Action {
                id: ActionId("queue_clear".into()),
                label: "Clear paste queue".into(),
                shortcut: None,
                enabled: if has_queued {
                    Ok(())
                } else {
                    Err("Paste queue is empty.".into())
                },
            },
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
            "paste_plain" => {
                if h.get(n)
                    .ok()
                    .flatten()
                    .is_some_and(|m| m.kind == Kind::Text)
                {
                    Outcome::Effects(vec![Effect::Paste(EntryId(n))])
                } else {
                    Outcome::Done
                }
            }
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
            "queue_add" => {
                if h.get(n).ok().flatten().is_some() {
                    h.queue_mut().push(EntryId(n));
                    Outcome::Stay {
                        effects: vec![],
                        select: Some(id.clone()),
                    }
                } else {
                    Outcome::Done
                }
            }
            "queue_next" => match h.pop_next_queued() {
                Some(next) => Outcome::Effects(vec![Effect::Paste(next)]),
                None => failed("The paste queue is empty."),
            },
            "queue_clear" => {
                h.queue_mut().clear();
                Outcome::Stay {
                    effects: vec![],
                    select: Some(id.clone()),
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

    fn title(&self) -> &'static str {
        "Clipboard"
    }

    fn opened_by_hotkey(&self) -> bool {
        true
    }

    fn ranked(&self) -> bool {
        false
    }

    fn placeholder(&self) -> &'static str {
        "Search clipboard history..."
    }

    fn empty_title(&self) -> &'static str {
        "Nothing copied yet"
    }

    fn preview(&self, id: &ItemId) -> Option<Preview> {
        self.pane(id).map(|(preview, _)| preview)
    }

    fn pane(&self, id: &ItemId) -> Option<(Preview, Vec<Fact>)> {
        let n = entry_id(id)?;
        let h = lock(&self.history);
        let meta = h.get(n).ok().flatten()?;
        let payload = match h.payload(n) {
            Ok(p) => p?,
            Err(_) => return Some((Preview::Text(UNREADABLE.to_owned()), Vec::new())),
        };
        let facts = entry_facts(&meta, &payload, h.now_s());
        let preview = match meta.kind {
            Kind::Text => Preview::Text(
                String::from_utf8_lossy(&payload)
                    .chars()
                    .take(PREVIEW_CHARS)
                    .collect(),
            ),
            Kind::Files => Preview::Files(
                String::from_utf8_lossy(&payload)
                    .lines()
                    .map(str::to_owned)
                    .collect(),
            ),
            Kind::Image => decode_png(&payload).unwrap_or_else(|| {
                Preview::Text(
                    h.preview(n)
                        .ok()
                        .flatten()
                        .map(|p| p.to_string())
                        .unwrap_or_default(),
                )
            }),
        };
        Some((preview, facts))
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
        let command = |id: &str, title: &str, subtitle: &str, keywords: &[&str]| Item {
            id: ItemId(id.into()),
            title: title.into(),
            subtitle: subtitle.into(),
            keywords: keywords.iter().map(|k| (*k).into()).collect(),
            alias: None,
            kind: ItemKind::Command,
            primary: ActionId("run".into()),
            secondary: None,
        };
        vec![
            command(
                OPEN,
                "Clipboard History",
                "",
                &["paste history", "copy history"],
            ),
            command(
                CLEAR,
                "Clear Clipboard History",
                "Press Enter twice to confirm",
                &["delete history"],
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

#[cfg(test)]
mod tests {
    use super::*;

    // @R-027
    #[test]
    fn r027_an_image_entry_states_its_pixel_size() {
        let mut png_bytes = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut png_bytes, 3, 2);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().expect("header");
            writer.write_image_data(&[0; 24]).expect("pixels");
        }
        let meta = EntryMeta {
            id: 1,
            kind: Kind::Image,
            last_copied_at: 100,
            pinned: false,
            source_exe: None,
            byte_size: png_bytes.len() as i64,
        };
        let facts = entry_facts(&meta, &png_bytes, 160);
        assert_eq!(facts[0], fact("Type", "Image".into()));
        assert_eq!(facts[1], fact("Size", "3 x 2".into()));
        assert_eq!(facts[2], fact("Copied", "1 min ago".into()));
    }
}
