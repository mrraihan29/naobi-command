//! Whether a font family is installed (R-030). A Theme names families only; font files are never opened
//! on its behalf.
use windows::Win32::Foundation::LPARAM;
use windows::Win32::Graphics::Gdi::{
    EnumFontFamiliesExW, GetDC, ReleaseDC, DEFAULT_CHARSET, LOGFONTW, TEXTMETRICW,
};

/// Windows keeps face names in 32 UTF-16 units, the terminator included.
const MAX_FACE_UNITS: usize = 31;

unsafe extern "system" fn found_one(
    _font: *const LOGFONTW,
    _metrics: *const TEXTMETRICW,
    _kind: u32,
    found: LPARAM,
) -> i32 {
    // SAFETY: `found` is the address of the bool `is_installed` keeps alive during the enumeration.
    unsafe { *(found.0 as *mut bool) = true };
    0
}

/// True when Windows has a font family with this exact name.
pub fn is_installed(family: &str) -> bool {
    let units: Vec<u16> = family.encode_utf16().collect();
    if units.is_empty() || units.len() > MAX_FACE_UNITS {
        return false;
    }
    let mut request = LOGFONTW {
        lfCharSet: DEFAULT_CHARSET,
        ..Default::default()
    };
    request.lfFaceName[..units.len()].copy_from_slice(&units);
    let mut found = false;
    // SAFETY: the screen DC is released below; `request` and `found` outlive the synchronous call.
    unsafe {
        let dc = GetDC(None);
        EnumFontFamiliesExW(
            dc,
            &request,
            Some(found_one),
            LPARAM(&mut found as *mut bool as isize),
            0,
        );
        ReleaseDC(None, dc);
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    // @R-030
    #[test]
    #[ignore = "tier2: interactive desktop"] // naobi-allow: reads the real Windows font list
    fn r030_installed_fonts_are_found_and_unknown_ones_are_not() {
        assert!(is_installed("Segoe UI"));
        assert!(!is_installed("No Such Font Family 12345"));
        assert!(!is_installed(""));
    }
}
