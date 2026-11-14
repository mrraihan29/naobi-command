//! Test doubles for the ports. Public so the integration tests and the app's tests share them.
use crate::ports::Clock;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;

pub use naobi_core::cipher::TaggedCipher;

/// A clock a test moves by hand. Clones share the time.
#[derive(Clone, Default)]
pub struct FakeClock(Arc<AtomicI64>);

impl FakeClock {
    pub fn at(ms: i64) -> Self {
        Self(Arc::new(AtomicI64::new(ms)))
    }

    pub fn advance(&self, ms: i64) {
        self.0.fetch_add(ms, Ordering::SeqCst);
    }
}

impl Clock for FakeClock {
    fn now_ms(&self) -> i64 {
        self.0.load(Ordering::SeqCst)
    }
}
