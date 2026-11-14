//! Icons for Launcher rows (NCMD-16, REPORT 3.6): an app's own icon from the shell, and a file's icon
//! from its extension alone. Both load shell libraries that never leave the process, so they run
//! only in the app's transient `--icons` child. A file is never opened: the extension icon asks the
//! shell for "naobi.<ext>" with `SHGFI_USEFILEATTRIBUTES`, so no disk or network access happens, and
//! no `.url`, `.lnk` or `.library-ms` icon location is ever read (T-030).
use naobi_core::ui::Icon;
use windows::core::{HSTRING, PCWSTR};
use windows::Win32::Foundation::SIZE;
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, DeleteDC, DeleteObject, GetDIBits, GetObjectW, BITMAP, BITMAPINFO,
    BITMAPINFOHEADER, DIB_RGB_COLORS, HBITMAP, HGDIOBJ,
};
use windows::Win32::Storage::FileSystem::{GetDriveTypeW, FILE_ATTRIBUTE_NORMAL};
use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED};
use windows::Win32::UI::Shell::{
    IShellItemImageFactory, SHCreateItemFromParsingName, SHGetFileInfoW, SHFILEINFOW, SHGFI_ICON,
    SHGFI_LARGEICON, SHGFI_USEFILEATTRIBUTES, SIIGBF_ICONONLY,
};
use windows::Win32::UI::WindowsAndMessaging::{DestroyIcon, GetIconInfo, ICONINFO};

/// `GetDriveTypeW` for a local fixed disk.
const DRIVE_FIXED: u32 = 3;

/// The size asked of the shell for an app, twice the 22 px the row shows so it stays sharp at 200%.
const APP_PX: i32 = 44;

/// Initialises COM for this thread, for `f`.
pub fn with_com<T>(f: impl FnOnce() -> T) -> T {
    // SAFETY: COM is initialised and released on this thread around `f`.
    let com = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED).is_ok() };
    let out = f();
    if com {
        // SAFETY: pairs the successful CoInitializeEx above.
        unsafe { CoUninitialize() };
    }
    out
}

/// True when `id` may be handed to the shell for an icon: a package or program name, a
/// `{known folder}\x.exe` id, or a path on a local fixed drive. A path on a network, removable or
/// missing drive is refused (opening it would reach the network or stall, T-030), and so is a
/// shortcut or internet shortcut, whose own icon location is never to be followed.
pub fn icon_id_is_local(id: &str) -> bool {
    let lower = id.to_ascii_lowercase();
    if lower.ends_with(".lnk") || lower.ends_with(".url") || lower.ends_with(".library-ms") {
        return false;
    }
    let b = id.as_bytes();
    if b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':' {
        let root = HSTRING::from(format!("{}:\\", char::from(b[0]).to_ascii_uppercase()));
        // SAFETY: a plain query on a drive root string that lives for the call.
        return unsafe { GetDriveTypeW(&root) } == DRIVE_FIXED;
    }
    true
}

/// The icon of the app whose `shell:AppsFolder\<id>` entry is `id`. Call it inside `with_com`.
pub fn app_icon(id: &str) -> Option<Icon> {
    if !icon_id_is_local(id) {
        return None;
    }
    let path = HSTRING::from(format!(r"shell:AppsFolder\{id}"));
    // SAFETY: the item and the bitmap it makes are owned here; the bitmap is deleted by
    // `bitmap_pixels`' caller below.
    unsafe {
        let item: IShellItemImageFactory = SHCreateItemFromParsingName(&path, None).ok()?;
        let hbm = item
            .GetImage(
                SIZE {
                    cx: APP_PX,
                    cy: APP_PX,
                },
                SIIGBF_ICONONLY,
            )
            .ok()?;
        let pixels = bitmap_pixels(hbm);
        let _ = DeleteObject(HGDIOBJ(hbm.0));
        pixels
    }
}

/// The icon the shell shows for files that end in `.ext` (any other text, or none, gives the plain
/// document icon). Nothing on disk is read.
pub fn ext_icon(ext: &str) -> Option<Icon> {
    let name = HSTRING::from(if ext.is_empty() {
        "naobi".to_owned()
    } else {
        format!("naobi.{ext}")
    });
    let mut info = SHFILEINFOW::default();
    // SAFETY: `name` and `info` live for the call; USEFILEATTRIBUTES makes the shell use the given
    // attributes and the name only, so it never touches a file. The icon it returns is destroyed
    // before returning, after its bitmaps are copied out.
    unsafe {
        let got = SHGetFileInfoW(
            PCWSTR(name.as_ptr()),
            FILE_ATTRIBUTE_NORMAL,
            Some(&mut info),
            std::mem::size_of::<SHFILEINFOW>() as u32,
            SHGFI_USEFILEATTRIBUTES | SHGFI_ICON | SHGFI_LARGEICON,
        );
        if got == 0 || info.hIcon.is_invalid() {
            return None;
        }
        let mut parts = ICONINFO::default();
        let icon = if GetIconInfo(info.hIcon, &mut parts).is_ok() {
            let color = bitmap_pixels(parts.hbmColor);
            let mask = bitmap_pixels(parts.hbmMask);
            let _ = DeleteObject(HGDIOBJ(parts.hbmColor.0));
            let _ = DeleteObject(HGDIOBJ(parts.hbmMask.0));
            color.map(|mut c| {
                // An icon without an alpha channel: its mask says where it is opaque.
                if c.rgba.as_chunks::<4>().0.iter().all(|p| p[3] == 0) {
                    if let Some(m) = mask.filter(|m| m.rgba.len() == c.rgba.len()) {
                        for (p, q) in c
                            .rgba
                            .as_chunks_mut::<4>()
                            .0
                            .iter_mut()
                            .zip(m.rgba.as_chunks::<4>().0)
                        {
                            if q[0] == 0 {
                                p[3] = 255;
                            } else {
                                // Premultiplied: a transparent pixel has no color.
                                *p = [0, 0, 0, 0];
                            }
                        }
                    }
                }
                c
            })
        } else {
            None
        };
        let _ = DestroyIcon(info.hIcon);
        icon
    }
}

/// The pixels of a bitmap as premultiplied RGBA, top row first. `None` for a null or empty bitmap.
///
/// # Safety
/// `hbm` is null or a bitmap handle that stays valid for the call.
unsafe fn bitmap_pixels(hbm: HBITMAP) -> Option<Icon> {
    if hbm.is_invalid() {
        return None;
    }
    // SAFETY: see the function's contract; the memory DC is deleted before returning.
    unsafe {
        let mut bm = BITMAP::default();
        if GetObjectW(
            HGDIOBJ(hbm.0),
            std::mem::size_of::<BITMAP>() as i32,
            Some(&mut bm as *mut BITMAP as *mut core::ffi::c_void),
        ) == 0
        {
            return None;
        }
        let (w, h) = (bm.bmWidth, bm.bmHeight);
        if w <= 0 || h <= 0 || w > 512 || h > 512 {
            return None;
        }
        let mut info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w,
                // Negative: top row first.
                biHeight: -h,
                biPlanes: 1,
                biBitCount: 32,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bgra = vec![0u8; w as usize * h as usize * 4];
        let dc = CreateCompatibleDC(None);
        let lines = GetDIBits(
            dc,
            hbm,
            0,
            h as u32,
            Some(bgra.as_mut_ptr().cast()),
            &mut info,
            DIB_RGB_COLORS,
        );
        let _ = DeleteDC(dc);
        if lines != h {
            return None;
        }
        for p in bgra.as_chunks_mut::<4>().0 {
            p.swap(0, 2);
        }
        Some(Icon {
            w: w as u32,
            h: h as u32,
            rgba: bgra,
        })
    }
}
