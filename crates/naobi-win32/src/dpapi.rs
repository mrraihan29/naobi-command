//! DPAPI for the current user (ADR-0006, D-014): one blob at a time, no UI, with a fixed entropy
//! string so a blob cannot be opened by an unrelated program that merely calls CryptUnprotectData.
//! This protects against other accounts and offline copies, not against code running as the user.
use std::io;

use windows::Win32::Foundation::{LocalFree, HLOCAL};
use windows::Win32::Security::Cryptography::{
    CryptProtectData, CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
};

const ENTROPY: &[u8] = b"naobi-command.clipboard.v1";

fn blob(data: &[u8]) -> CRYPT_INTEGER_BLOB {
    CRYPT_INTEGER_BLOB {
        cbData: data.len() as u32,
        // The API reads through this pointer and never writes it.
        pbData: data.as_ptr().cast_mut(),
    }
}

/// Copies the output block into a Vec, wipes the block, and frees it.
fn take(out: CRYPT_INTEGER_BLOB, wipe: bool) -> Vec<u8> {
    if out.pbData.is_null() {
        return Vec::new();
    }
    // SAFETY: the API returned a LocalAlloc block of `cbData` bytes at `pbData`; it is read once,
    // optionally overwritten with volatile writes (so the compiler cannot drop them), then freed once.
    unsafe {
        let v = std::slice::from_raw_parts(out.pbData, out.cbData as usize).to_vec();
        if wipe {
            for i in 0..out.cbData as usize {
                std::ptr::write_volatile(out.pbData.add(i), 0);
            }
        }
        let _ = LocalFree(Some(HLOCAL(out.pbData.cast())));
        v
    }
}

/// Encrypts `plain` for the current user.
pub fn protect(plain: &[u8]) -> io::Result<Vec<u8>> {
    let (input, entropy) = (blob(plain), blob(ENTROPY));
    let mut out = CRYPT_INTEGER_BLOB::default();
    // SAFETY: the blobs describe slices that outlive the call; `out` receives a LocalAlloc block.
    unsafe {
        CryptProtectData(
            &input,
            None,
            Some(&entropy),
            None,
            None,
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut out,
        )?
    };
    Ok(take(out, false))
}

/// Decrypts a blob made by `protect`. The OS buffer that held the plaintext is wiped before it is
/// freed; the returned Vec is the caller's to wipe.
pub fn unprotect(blob_in: &[u8]) -> io::Result<Vec<u8>> {
    let (input, entropy) = (blob(blob_in), blob(ENTROPY));
    let mut out = CRYPT_INTEGER_BLOB::default();
    // SAFETY: as in `protect`.
    unsafe {
        CryptUnprotectData(
            &input,
            None,
            Some(&entropy),
            None,
            None,
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut out,
        )?
    };
    Ok(take(out, true))
}
