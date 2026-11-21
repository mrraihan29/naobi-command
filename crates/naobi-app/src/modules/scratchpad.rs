//! Scratchpad module wiring (`naobi-launcher::providers::scratchpad`).
use super::Modules;
use naobi_core::ids::{HotkeyId, ViewId};
use naobi_launcher::providers::scratchpad::{
    FileScratchpadStore, ScratchpadCommands, ScratchpadStore, ScratchpadView, VIEW_ID,
};
use std::io;
use std::sync::Arc;

pub const HOTKEY: &str = "scratchpad";

struct UnavailableStore;

impl ScratchpadStore for UnavailableStore {
    fn load(&self) -> io::Result<String> {
        Err(io::Error::other("settings directory unavailable"))
    }

    fn save(&self, _content: &str) -> io::Result<()> {
        Err(io::Error::other("settings directory unavailable"))
    }
}

pub fn register(m: &mut Modules) {
    let store: Arc<dyn ScratchpadStore> = match naobi_core::paths::settings_dir() {
        Some(dir) => Arc::new(FileScratchpadStore::new(dir.join("scratchpad.md"))),
        None => Arc::new(UnavailableStore),
    };
    m.register_source(Box::new(ScratchpadCommands));
    m.register_view(Box::new(ScratchpadView::new(store)));
    m.subscribe(
        HotkeyId(HOTKEY.into()),
        Box::new(|_, cx| cx.open_view(ViewId(VIEW_ID.into()))),
    );
}
