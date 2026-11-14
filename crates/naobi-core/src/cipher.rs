//! The cipher port: shared by the clipboard and the run history, which both store DPAPI blobs (D-014).
use zeroize::Zeroizing;

/// Encrypts one blob at a time. Production is DPAPI for the current user; tests use a tagged identity.
pub trait Cipher: Send {
    fn encrypt(&self, plain: &[u8]) -> Result<Vec<u8>, CipherError>;
    /// The plaintext comes back in a buffer that is zeroed when dropped (T-014).
    fn decrypt(&self, blob: &[u8]) -> Result<Zeroizing<Vec<u8>>, CipherError>;
}

/// A cipher failure carries no detail: the data is the secret.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("the cipher could not process the data")]
pub struct CipherError;

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
