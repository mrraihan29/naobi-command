//! Start with Windows (D-007): the HKCU Run value holds the quoted exe path, or the value is removed.
use std::ffi::c_void;
use std::path::Path;

use windows::core::{Error, Result, HSTRING};
use windows::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS, WIN32_ERROR};
use windows::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegDeleteTreeW, RegDeleteValueW, RegGetValueW, RegOpenKeyExW,
    RegSetValueExW, HKEY, HKEY_CURRENT_USER, KEY_SET_VALUE, REG_OPTION_NON_VOLATILE, REG_SZ,
    RRF_RT_REG_SZ,
};

/// The Run key the Owner's autostart setting writes to.
pub const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
/// The value name under that key.
pub const VALUE_NAME: &str = "NaobiCommand";

/// Writes the quoted `exe` path (`enabled`) or removes the value, under HKCU `key_path`.
pub fn set_enabled(key_path: &str, exe: &Path, enabled: bool) -> Result<()> {
    let key = HSTRING::from(key_path);
    let name = HSTRING::from(VALUE_NAME);
    if !enabled {
        let mut hkey = HKEY::default();
        // SAFETY: out-pointer to a local HKEY that is closed below.
        let err =
            unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, &key, Some(0), KEY_SET_VALUE, &mut hkey) };
        if err == ERROR_FILE_NOT_FOUND {
            return Ok(());
        }
        check(err)?;
        // SAFETY: `hkey` is open and the value name outlives the call.
        let err = unsafe { RegDeleteValueW(hkey, &name) };
        // SAFETY: `hkey` was opened above and is closed once.
        let _ = unsafe { RegCloseKey(hkey) };
        return if err == ERROR_FILE_NOT_FOUND {
            Ok(())
        } else {
            check(err)
        };
    }
    let quoted = format!("\"{}\"", exe.display());
    let wide: Vec<u16> = quoted.encode_utf16().chain(std::iter::once(0)).collect();
    // SAFETY: `wide` outlives the call; the byte view covers exactly its elements.
    let bytes = unsafe { std::slice::from_raw_parts(wide.as_ptr().cast::<u8>(), wide.len() * 2) };
    let mut hkey = HKEY::default();
    // SAFETY: out-pointer to a local HKEY that is closed below.
    let err = unsafe {
        RegCreateKeyExW(
            HKEY_CURRENT_USER,
            &key,
            Some(0),
            None,
            REG_OPTION_NON_VOLATILE,
            KEY_SET_VALUE,
            None,
            &mut hkey,
            None,
        )
    };
    check(err)?;
    // SAFETY: `hkey` is open and the data is a NUL-terminated UTF-16 string.
    let err = unsafe { RegSetValueExW(hkey, &name, Some(0), REG_SZ, Some(bytes)) };
    // SAFETY: `hkey` was opened above and is closed once.
    let _ = unsafe { RegCloseKey(hkey) };
    check(err)
}

/// The value as stored, or None when it is absent.
pub fn value(key_path: &str) -> Option<String> {
    let key = HSTRING::from(key_path);
    let name = HSTRING::from(VALUE_NAME);
    let mut buf = vec![0u16; 1024];
    let mut len = (buf.len() * 2) as u32;
    // SAFETY: `buf` holds `len` bytes, and RRF_RT_REG_SZ writes no more than that.
    let err = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            &key,
            &name,
            RRF_RT_REG_SZ,
            None,
            Some(buf.as_mut_ptr().cast::<c_void>()),
            Some(&mut len),
        )
    };
    if err != ERROR_SUCCESS {
        return None;
    }
    let chars = (len as usize / 2).saturating_sub(1);
    String::from_utf16(&buf[..chars]).ok()
}

/// Deletes the HKCU key tree at `key_path`. Tests use it to remove their throwaway keys; production
/// never calls it. A missing key is not an error.
pub fn remove_key_tree(key_path: &str) {
    // Only throwaway test keys may be deleted: an empty or foreign path must never reach the registry.
    assert!(
        key_path.starts_with(r"Software\NaobiCommandTest\"),
        "remove_key_tree is for the throwaway test keys only"
    );
    let key = HSTRING::from(key_path);
    // SAFETY: deletes only the named HKCU subtree, which the assert above limits to the test keys.
    let _ = unsafe { RegDeleteTreeW(HKEY_CURRENT_USER, &key) };
}

fn check(err: WIN32_ERROR) -> Result<()> {
    if err == ERROR_SUCCESS {
        Ok(())
    } else {
        Err(Error::from_hresult(err.to_hresult()))
    }
}
