//! The limits of D-015 and R-009/R-015, and the capture rate limiter.
use std::collections::VecDeque;

pub const MAX_TEXT_BYTES: usize = 1024 * 1024;
pub const MAX_PNG_BYTES: usize = 10_000_000;
/// A raw bitmap over this is refused before any conversion.
pub const MAX_BITMAP_BYTES: usize = 64 * 1024 * 1024;
pub const MAX_FILE_PATHS: usize = 1000;
/// The text index keeps this much of each text entry, and at most `INDEX_MAX_ENTRIES` entries.
pub const PREVIEW_BYTES: usize = 4096;
pub const INDEX_MAX_ENTRIES: usize = 1000;
pub const INDEX_MAX_BYTES: usize = 4_000_000;
/// The decrypted text index is dropped this long after the view closes (T-014).
pub const INDEX_TTL_MS: i64 = 60_000;
pub const CAPTURES_PER_SECOND: usize = 10;

/// Retention and caps. Settings may lower the first two but never raise them (D-015).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    pub retention_days: i64,
    pub max_entries: usize,
    pub store_cap_bytes: i64,
    pub pinned_cap: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            retention_days: 30,
            max_entries: 1000,
            store_cap_bytes: 500 * 1024 * 1024,
            pinned_cap: 100,
        }
    }
}

/// At most `CAPTURES_PER_SECOND` captures in any second. A capture that is refused is retried by the
/// caller after the returned wait, with whatever the clipboard holds then (trailing edge).
#[derive(Debug, Default)]
pub struct Limiter {
    stamps: VecDeque<i64>,
}

impl Limiter {
    /// Ok when a capture may run now (it is counted); otherwise the milliseconds to wait.
    pub fn admit(&mut self, now_ms: i64) -> Result<(), u64> {
        while self.stamps.front().is_some_and(|&t| now_ms - t >= 1000) {
            self.stamps.pop_front();
        }
        if self.stamps.len() < CAPTURES_PER_SECOND {
            self.stamps.push_back(now_ms);
            return Ok(());
        }
        let oldest = self.stamps.front().copied().unwrap_or(now_ms);
        Err((oldest + 1000 - now_ms).max(1) as u64)
    }
}
