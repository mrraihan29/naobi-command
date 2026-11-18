//! The Color History view and the two root commands. Rows are a swatch and the HEX; the action panel
//! copies the color in any of the four notations. Only data and effects: the app copies.
use crate::format::{format, Format};
use crate::history::{ColorHistory, Pick};
use naobi_core::command::{CommandSource, Cx, Outcome, Query};
use naobi_core::effect::Effect;
use naobi_core::ids::{ActionId, HotkeyId, ItemId, ViewId};
use naobi_core::item::{Action, Item, ItemKind};
use naobi_core::view_source::{Preview, ViewSource};
use std::sync::{Arc, Mutex, MutexGuard};

/// Written by the picker thread, read by the Launcher.
pub type Shared = Arc<Mutex<ColorHistory>>;

pub const VIEW_ID: &str = "colors";
/// The hotkey id of the picker, also what the "Pick Color" command runs.
pub const HOTKEY: &str = "color_picker";

fn lock(h: &Shared) -> MutexGuard<'_, ColorHistory> {
    h.lock().unwrap_or_else(|e| e.into_inner())
}

fn pick_id(id: &ItemId) -> Option<i64> {
    id.0.strip_prefix("color:")?.parse().ok()
}

pub struct ColorHistoryView {
    history: Shared,
}

impl ColorHistoryView {
    pub fn new(history: Shared) -> Self {
        Self { history }
    }

    fn find(&self, id: &ItemId) -> Option<Pick> {
        let n = pick_id(id)?;
        lock(&self.history)
            .list()
            .ok()?
            .into_iter()
            .find(|p| p.id == n)
    }
}

impl CommandSource for ColorHistoryView {
    fn items(&self, q: &Query) -> Arc<[Item]> {
        let want = q.text.trim().trim_start_matches('#').to_uppercase();
        let picks = lock(&self.history).list().unwrap_or_default();
        picks
            .into_iter()
            .filter(|p| p.rgb.hex()[1..].contains(&want))
            .map(|p| Item {
                id: ItemId(format!("color:{}", p.id)),
                title: p.rgb.hex(),
                subtitle: format(p.rgb, Format::Rgb),
                keywords: Vec::new(),
                alias: None,
                kind: ItemKind::Color,
                primary: ActionId(Format::Hex.id().into()),
                secondary: Some(ActionId(Format::Rgb.id().into())),
            })
            .collect()
    }

    fn actions(&self, _id: &ItemId) -> Vec<Action> {
        Format::ALL
            .into_iter()
            .map(|f| Action {
                id: ActionId(f.id().into()),
                label: f.label().into(),
                shortcut: None,
                enabled: Ok(()),
            })
            .collect()
    }

    fn run(&self, id: &ItemId, action: ActionId, _cx: &mut Cx) -> Outcome {
        match (self.find(id), Format::of_id(&action.0)) {
            (Some(p), Some(f)) => Outcome::Effects(vec![Effect::CopyText(format(p.rgb, f))]),
            _ => Outcome::Done,
        }
    }
}

impl ViewSource for ColorHistoryView {
    fn view_id(&self) -> ViewId {
        ViewId(VIEW_ID.into())
    }

    fn title(&self) -> &'static str {
        "Color history"
    }

    fn opened_by_hotkey(&self) -> bool {
        false
    }

    fn ranked(&self) -> bool {
        false
    }

    fn placeholder(&self) -> &'static str {
        "Search colors..."
    }

    fn empty_title(&self) -> &'static str {
        "No colors picked yet"
    }

    fn preview(&self, id: &ItemId) -> Option<Preview> {
        let p = self.find(id)?;
        let lines: Vec<String> = Format::ALL.into_iter().map(|f| format(p.rgb, f)).collect();
        Some(Preview::Text(lines.join("\n")))
    }
}

const PICK: &str = "picker.pick";
const HISTORY: &str = "picker.history";

pub struct PickerCommands;

impl CommandSource for PickerCommands {
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
                PICK,
                "Pick Color",
                "Click any pixel to copy its HEX",
                &["color picker", "eyedropper", "hex"],
            ),
            command(HISTORY, "Color History", "", &["recent colors"]),
        ]
        .into()
    }

    fn actions(&self, _id: &ItemId) -> Vec<Action> {
        vec![Action {
            id: ActionId("run".into()),
            label: "Run".into(),
            shortcut: None,
            enabled: Ok(()),
        }]
    }

    fn run(&self, id: &ItemId, _action: ActionId, _cx: &mut Cx) -> Outcome {
        match id.0.as_str() {
            // The same handler as the hotkey, once the Launcher is out of the picture.
            PICK => Outcome::Effects(vec![Effect::RunHotkey(HotkeyId(HOTKEY.into()))]),
            HISTORY => Outcome::Effects(vec![Effect::PushView(ViewId(VIEW_ID.into()))]),
            _ => Outcome::Done,
        }
    }
}
