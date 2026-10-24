//! Writes text with a private clipboard format next to it, so our own copies can be told apart
//! (copy effects, paste and picker use this, D-019). Never logs the content.
use std::sync::OnceLock;
use std::time::Duration;

use windows::core::{w, Result};
use windows::Win32::Foundation::{GlobalFree, HANDLE, HGLOBAL};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard,
    RegisterClipboardFormatW, SetClipboardData,
};
use windows::Win32::System::Memory::{
    GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock, GMEM_MOVEABLE,
};
use windows::Win32::System::Ole::CF_UNICODETEXT;

/// Opens the clipboard, retrying three times at 10 ms: another process may hold it for a moment.
fn open_clipboard() -> Result<()> {
    let mut attempt = 0;
    loop {
        // SAFETY: opens the clipboard for this thread; the caller closes it.
        match unsafe { OpenClipboard(None) } {
            Ok(()) => return Ok(()),
            Err(e) if attempt == 2 => return Err(e),
            Err(_) => {
                attempt += 1;
                std::thread::sleep(Duration::from_millis(10));
            }
        }
    }
}

/// The private format's id, registered once per process.
pub fn marker_format() -> u32 {
    static FORMAT: OnceLock<u32> = OnceLock::new();
    *FORMAT.get_or_init(|| {
        // SAFETY: a static format name; the returned id is plain data.
        unsafe { RegisterClipboardFormatW(w!("NaobiCommand.Marker")) }
    })
}

/// Copies `bytes` into a new movable global block. The clipboard owns the block after SetClipboardData.
fn global(bytes: &[u8]) -> Result<HGLOBAL> {
    // SAFETY: allocates a movable block of `bytes.len()` bytes and fills it under lock.
    unsafe {
        let mem = GlobalAlloc(GMEM_MOVEABLE, bytes.len())?;
        let dst = GlobalLock(mem).cast::<u8>();
        if dst.is_null() {
            let _ = GlobalFree(Some(mem));
            return Err(windows::core::Error::from_thread());
        }
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), dst, bytes.len());
        let _ = GlobalUnlock(mem);
        Ok(mem)
    }
}

/// Puts `text` on the clipboard as CF_UNICODETEXT plus the private marker format.
pub fn write_text(text: &str) -> Result<()> {
    let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    // SAFETY: `wide` is read as raw bytes of its own length.
    let wide_bytes =
        unsafe { std::slice::from_raw_parts(wide.as_ptr().cast::<u8>(), wide.len() * 2) };
    // SAFETY: the clipboard is opened and closed on this thread; the handles passed to
    // SetClipboardData belong to the clipboard after a successful call.
    unsafe {
        open_clipboard()?;
        let result = (|| {
            EmptyClipboard()?;
            let text_mem = global(wide_bytes)?;
            if let Err(e) = SetClipboardData(u32::from(CF_UNICODETEXT.0), Some(HANDLE(text_mem.0)))
            {
                let _ = GlobalFree(Some(text_mem));
                return Err(e);
            }
            let marker = global(&[1u8])?;
            if let Err(e) = SetClipboardData(marker_format(), Some(HANDLE(marker.0))) {
                let _ = GlobalFree(Some(marker));
                return Err(e);
            }
            Ok(())
        })();
        // A failed close must not hide the write's own error.
        let closed = CloseClipboard();
        result.and(closed)
    }
}

/// True when the clipboard holds the private marker format.
pub fn has_marker() -> bool {
    // SAFETY: a query with a plain format id; no pointers.
    unsafe { IsClipboardFormatAvailable(marker_format()).is_ok() }
}

/// The CF_UNICODETEXT content, or None when there is none.
pub fn read_text() -> Option<String> {
    // SAFETY: the clipboard is opened and closed here; the locked block is read only within its size.
    unsafe {
        open_clipboard().ok()?;
        let text = (|| {
            let handle = GetClipboardData(u32::from(CF_UNICODETEXT.0)).ok()?;
            let mem = HGLOBAL(handle.0);
            let ptr = GlobalLock(mem).cast::<u16>();
            if ptr.is_null() {
                return None;
            }
            // Another process wrote this block: never read past its size, even without a NUL.
            let max = GlobalSize(mem) / 2;
            let mut len = 0usize;
            while len < max && *ptr.add(len) != 0 {
                len += 1;
            }
            let s = String::from_utf16(std::slice::from_raw_parts(ptr, len)).ok();
            let _ = GlobalUnlock(mem);
            s
        })();
        let _ = CloseClipboard();
        text
    }
}
