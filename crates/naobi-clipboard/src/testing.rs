//! Test doubles for the ports. Public so the integration tests and the app's tests share them.
use crate::ports::{Cipher, CipherError, Clock};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;
use zeroize::Zeroizing;

const TAG: &[u8] = b"ENC1:";

/// Not encryption: it prefixes a tag, so a test can see that a blob went through the cipher and that
/// plaintext never reached a column directly.
pub struct TaggedCipher;

impl Cipher for TaggedCipher {
    fn encrypt(&self, plain: &[u8]) -> Result<Vec<u8>, CipherError> {
        Ok([TAG, plain].concat())
    }

    fn decrypt(&self, blob: &[u8]) -> Result<Zeroizing<Vec<u8>>, CipherError> {
        blob.strip_prefix(TAG)
            .map(|p| Zeroizing::new(p.to_vec()))
            .ok_or(CipherError)
    }
}

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
