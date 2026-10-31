//! Armed commands (R-020, GLOSSARY): the first Enter arms a system command, the second runs it.
//! Any other key, a selection change or 5 s disarms it.
use naobi_core::ids::{ActionId, ItemId};

pub const ARM_MS: i64 = 5_000;

#[derive(Debug)]
struct Pending {
    item: ItemId,
    action: ActionId,
    until_ms: i64,
}

#[derive(Debug, Default)]
pub struct Armed {
    pending: Option<Pending>,
}

impl Armed {
    /// The item armed and still inside its window, if any.
    pub fn armed_item(&self, now_ms: i64) -> Option<&ItemId> {
        match &self.pending {
            Some(p) if now_ms < p.until_ms => Some(&p.item),
            _ => None,
        }
    }

    /// True when `action` on `item` was armed and is still in time. The arming is used up either
    /// way, so a second confirm needs a fresh arm.
    pub fn confirm(&mut self, item: &ItemId, action: &ActionId, now_ms: i64) -> bool {
        let ok = match &self.pending {
            Some(p) => now_ms < p.until_ms && p.item == *item && p.action == *action,
            None => false,
        };
        self.pending = None;
        ok
    }

    pub fn arm(&mut self, item: ItemId, action: ActionId, now_ms: i64) {
        self.pending = Some(Pending {
            item,
            action,
            until_ms: now_ms + ARM_MS,
        });
    }

    pub fn disarm(&mut self) {
        self.pending = None;
    }
}
