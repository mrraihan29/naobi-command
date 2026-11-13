//! The Windows side of the paste (D-019, T-008): writing an entry to the clipboard with the private
//! marker, the foreground and elevation checks, and the Ctrl+V keystroke. The order of these steps is
//! decided in `naobi-clipboard::paste`.
use windows::core::w;
use windows::Win32::Foundation::{GlobalFree, HANDLE};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, RegisterClipboardFormatW, SetClipboardData,
};
use windows::Win32::System::Ole::{CF_DIB, CF_HDROP, CF_UNICODETEXT};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, VIRTUAL_KEY, VK_CONTROL,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, GetWindowThreadProcessId, IsWindow,
};

use crate::clipboard_marker::{global, marker_format, open_clipboard};

/// One clipboard format and its bytes.
type Format = (u32, Vec<u8>);

/// Replaces the clipboard with `formats` plus the private marker, in one open.
fn write_formats(formats: &[Format]) -> std::io::Result<()> {
    open_clipboard()?;
    let result = (|| {
        // SAFETY: the clipboard is open on this thread; each block is handed to SetClipboardData,
        // which owns it after a successful call, and freed here after a failed one.
        unsafe {
            EmptyClipboard()?;
            for (format, bytes) in formats
                .iter()
                .cloned()
                .chain(std::iter::once((marker_format(), vec![1u8])))
            {
                let mem = global(&bytes)?;
                if let Err(e) = SetClipboardData(format, Some(HANDLE(mem.0))) {
                    let _ = GlobalFree(Some(mem));
                    return Err(e);
                }
            }
            Ok(())
        }
    })();
    // SAFETY: closes the clipboard opened above.
    let closed = unsafe { CloseClipboard() };
    Ok(result.and(closed)?)
}

fn wide_bytes(s: &str) -> Vec<u8> {
    s.encode_utf16()
        .chain(std::iter::once(0))
        .flat_map(u16::to_le_bytes)
        .collect()
}

/// Text, with the marker.
pub fn write_text(text: &str) -> std::io::Result<()> {
    write_formats(&[(u32::from(CF_UNICODETEXT.0), wide_bytes(text))])
}

/// A picture as the bitmap every program reads and as PNG for the ones that prefer it.
pub fn write_image(png: &[u8], dib: &[u8]) -> std::io::Result<()> {
    // SAFETY: a static format name; the id is plain data.
    let png_format = unsafe { RegisterClipboardFormatW(w!("PNG")) };
    write_formats(&[
        (u32::from(CF_DIB.0), dib.to_vec()),
        (png_format, png.to_vec()),
    ])
}

/// A bitmap only (the previous clipboard coming back).
pub fn write_dib(dib: &[u8]) -> std::io::Result<()> {
    write_formats(&[(u32::from(CF_DIB.0), dib.to_vec())])
}

/// A file list as `CF_HDROP`: a DROPFILES header, then each path and an extra NUL.
pub fn write_files(paths: &[String]) -> std::io::Result<()> {
    let mut data = Vec::new();
    // DROPFILES: pFiles = 20 (header size), pt = (0, 0), fNC = 0, fWide = 1.
    data.extend_from_slice(&20u32.to_le_bytes());
    data.extend_from_slice(&[0u8; 8]);
    data.extend_from_slice(&0u32.to_le_bytes());
    data.extend_from_slice(&1u32.to_le_bytes());
    for p in paths {
        data.extend(wide_bytes(p));
    }
    data.extend_from_slice(&[0, 0]);
    write_formats(&[(u32::from(CF_HDROP.0), data)])
}

/// The foreground window now, as a plain value (0 when there is none).
pub fn foreground_window() -> isize {
    // SAFETY: no arguments; returns a handle value or null.
    unsafe { GetForegroundWindow() }.0 as isize
}

/// True when the window's process runs elevated, or when that cannot be told: this app never sends
/// keys to a window it cannot vouch for (T-008, R-010).
pub fn is_elevated(window: isize) -> bool {
    let hwnd = windows::Win32::Foundation::HWND(window as *mut core::ffi::c_void);
    let mut pid = 0u32;
    // SAFETY: `hwnd` is only a handle value, validated by IsWindow; `pid` receives the process id.
    unsafe {
        if !IsWindow(Some(hwnd)).as_bool() {
            return true;
        }
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
    }
    pid == 0 || crate::token::pid_is_elevated(pid).unwrap_or(true)
}

fn key(vk: VIRTUAL_KEY, up: bool) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                dwFlags: if up {
                    KEYEVENTF_KEYUP
                } else {
                    Default::default()
                },
                ..Default::default()
            },
        },
    }
}

/// Sends Ctrl+V to the foreground window. False when Windows took none of the four events.
pub fn send_ctrl_v() -> bool {
    let v = VIRTUAL_KEY(0x56);
    let seq = [
        key(VK_CONTROL, false),
        key(v, false),
        key(v, true),
        key(VK_CONTROL, true),
    ];
    // SAFETY: SendInput reads `seq`, fully initialised INPUT values, with their true size.
    unsafe { SendInput(&seq, std::mem::size_of::<INPUT>() as i32) == 4 }
}
