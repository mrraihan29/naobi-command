//! The icon cache (NCMD-16, REPORT 3.6): the last 100 icons, least recently used out first, and the
//! rule for what may be asked for. Extraction is the app's (a child process); keys carry no path.
use naobi_core::ui::Icon;
use std::collections::VecDeque;
use std::sync::Arc;

/// R-006 / REPORT 3.6: at most this many icons stay in memory.
pub const CAPACITY: usize = 100;

#[derive(Default)]
pub struct IconCache {
    /// Most recently used first. A hundred entries is a short scan, which beats a second index.
    entries: VecDeque<(String, Arc<Icon>)>,
}

impl IconCache {
    /// The icon for `key`, now the most recently used.
    pub fn get(&mut self, key: &str) -> Option<Arc<Icon>> {
        let at = self.entries.iter().position(|(k, _)| k == key)?;
        let entry = self.entries.remove(at)?;
        let icon = entry.1.clone();
        self.entries.push_front(entry);
        Some(icon)
    }

    /// Adds or replaces an icon; the least recently used one goes when the cache is full.
    pub fn insert(&mut self, key: String, icon: Icon) {
        self.entries.retain(|(k, _)| *k != key);
        self.entries.push_front((key, Arc::new(icon)));
        self.entries.truncate(CAPACITY);
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// True for a key the icon child may be asked for. An app id that is a UNC or URL path is refused,
/// so no icon request can reach a network share (T-030); an extension is letters and digits only.
pub fn askable(key: &str) -> bool {
    if let Some(ext) = key.strip_prefix("e:") {
        return ext.len() <= 16 && ext.bytes().all(|b| b.is_ascii_alphanumeric());
    }
    let Some(id) = key.strip_prefix("a:") else {
        return false;
    };
    !id.is_empty()
        && id.len() <= 512
        && !id.starts_with(['\\', '/'])
        && !id.contains("//")
        && !id.contains(":\\\\")
        && !id.contains("\\\\")
        && !id.chars().any(char::is_control)
}
