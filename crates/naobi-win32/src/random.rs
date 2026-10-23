//! System randomness for the data store salt (D-014). Core stays free of unsafe and of an RNG crate.
use windows::Win32::Security::Cryptography::{BCryptGenRandom, BCRYPT_USE_SYSTEM_PREFERRED_RNG};

/// 16 random bytes from the system-preferred CNG generator.
pub fn random16() -> windows::core::Result<[u8; 16]> {
    let mut buf = [0u8; 16];
    // SAFETY: `buf` is a live 16-byte buffer; a null algorithm handle is allowed with the system RNG flag.
    unsafe { BCryptGenRandom(None, &mut buf, BCRYPT_USE_SYSTEM_PREFERRED_RNG) }.ok()?;
    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn random16_differs_between_calls() {
        let (a, b) = (random16().unwrap(), random16().unwrap());
        assert_ne!(a, [0; 16]);
        assert_ne!(a, b);
    }
}
