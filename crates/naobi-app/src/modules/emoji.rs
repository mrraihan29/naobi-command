//! Emoji & Symbol picker module wiring (`naobi-launcher::providers::emoji`).
use super::Modules;
use naobi_core::ids::{HotkeyId, ViewId};
use naobi_launcher::providers::emoji::{EmojiSource, EmojiView, VIEW_ID};

pub const HOTKEY: &str = "emoji";

pub fn register(m: &mut Modules) {
    m.register_source(Box::new(EmojiSource));
    m.register_view(Box::new(EmojiView));
    m.subscribe(
        HotkeyId(HOTKEY.into()),
        Box::new(|_, cx| cx.open_view(ViewId(VIEW_ID.into()))),
    );
}
