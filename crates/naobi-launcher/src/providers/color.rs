//! Inline color format conversion in Root search (`#hex`, `rgb(...)`, `hsl(...)`, `oklch(...)`).
use naobi_core::color::{format, parse, Format, Rgb};
use naobi_core::command::{CommandSource, Cx, Outcome, Query};
use naobi_core::ids::{ActionId, ItemId};
use naobi_core::item::{Action, Item, ItemKind};
use std::sync::Arc;

const ID_PREFIX: &str = "color-inline:";

pub struct ColorSource;

impl CommandSource for ColorSource {
    fn items(&self, q: &Query) -> Arc<[Item]> {
        let Some(parsed) = parse(&q.text) else {
            return Arc::from(Vec::new());
        };
        let primary_fmt = match parsed.format {
            Format::Hex => Format::Rgb,
            Format::Rgb | Format::Hsl | Format::Oklch => Format::Hex,
        };
        let secondary_fmt = Format::ALL
            .into_iter()
            .find(|&f| f != primary_fmt && f != parsed.format);
        let title = format(parsed.rgb, primary_fmt);
        let subtitle = Format::ALL
            .into_iter()
            .filter(|&f| f != primary_fmt)
            .map(|f| format(parsed.rgb, f))
            .collect::<Vec<_>>()
            .join(" · ");
        vec![Item {
            id: ItemId(format!("{ID_PREFIX}{:06X}", parsed.rgb.packed())),
            title,
            subtitle,
            keywords: Vec::new(),
            // The query is this row's alias, so ranking always keeps it and puts it first.
            alias: Some(q.text.clone()),
            kind: ItemKind::Color,
            primary: ActionId(primary_fmt.id().into()),
            secondary: secondary_fmt.map(|f| ActionId(f.id().into())),
        }]
        .into()
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

    fn run(&self, id: &ItemId, action: ActionId, cx: &mut Cx) -> Outcome {
        let Some(hex) = id.0.strip_prefix(ID_PREFIX) else {
            return Outcome::Done;
        };
        let Ok(packed) = u32::from_str_radix(hex, 16) else {
            return Outcome::Done;
        };
        if let Some(f) = Format::of_id(&action.0) {
            cx.copy_text(format(Rgb::from_packed(packed), f));
        }
        Outcome::Done
    }
}
