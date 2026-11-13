//! Reads the clipboard for the history (REPORT 5.4): text, a bitmap or a file list, the privacy
//! formats, and the exe that owns the clipboard. Reads are bounded (T-018) and never log content.
//! The change listener is installed on P1 through `post_to_p1`; P1 only forwards the event, and the
//! read runs on the caller's own worker thread (T-021).
use std::path::Path;

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{CloseHandle, HANDLE, HGLOBAL, HWND};
use windows::Win32::System::DataExchange::{
    AddClipboardFormatListener, CloseClipboard, GetClipboardData, GetClipboardOwner,
    GetClipboardSequenceNumber, IsClipboardFormatAvailable, RegisterClipboardFormatW,
};
use windows::Win32::System::Memory::{GlobalLock, GlobalSize, GlobalUnlock};
use windows::Win32::System::Ole::{CF_DIB, CF_HDROP, CF_UNICODETEXT};
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::Shell::{DragQueryFileW, HDROP};
use windows::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId;

use crate::clipboard_marker::{has_marker, open_clipboard};

/// Text over this many UTF-16 units is cut here; the history then sees "too large" and refuses it.
const MAX_TEXT_UNITS: usize = 1024 * 1024 + 2;
/// A bitmap block is read up to one byte over the history's 64 MiB limit, so an oversize one is
/// recognised without copying all of it.
const MAX_DIB_BYTES: usize = 64 * 1024 * 1024 + 1;
/// One more than the history keeps, so it can tell that the list was cut.
const MAX_PATHS: u32 = 1001;

/// What one change left on the clipboard. The history decides what to keep.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Snapshot {
    pub text: Option<String>,
    pub dib: Option<Vec<u8>>,
    pub files: Vec<String>,
    pub exclude_from_monitor: bool,
    pub can_include_in_history: Option<u32>,
    pub can_upload_to_cloud: Option<u32>,
    /// The write carries the Launcher's private marker.
    pub marker: bool,
    /// The file name of the owning exe, such as `ProtonPass.exe`. Never a path.
    pub owner_exe: Option<String>,
}

/// Starts the change listener on P1. Any thread may call it. False when P1 is not there.
pub fn install_listener() -> bool {
    crate::platform::post_to_p1(Box::new(|| {
        let hwnd = HWND(crate::platform::p1_hwnd() as *mut _);
        // SAFETY: P1's own window, on P1's own thread, as the API requires.
        let _ = unsafe { AddClipboardFormatListener(hwnd) };
    }))
}

/// The clipboard sequence number now.
pub fn sequence() -> u32 {
    // SAFETY: reads a counter; no pointers.
    unsafe { GetClipboardSequenceNumber() }
}

struct Open;

impl Drop for Open {
    fn drop(&mut self) {
        // SAFETY: closes the clipboard this thread opened in `read`.
        let _ = unsafe { CloseClipboard() };
    }
}

fn registered(name: PCWSTR) -> u32 {
    // SAFETY: a static format name; the id is plain data.
    unsafe { RegisterClipboardFormatW(name) }
}

fn available(format: u32) -> bool {
    // SAFETY: a query with a plain format id.
    unsafe { IsClipboardFormatAvailable(format).is_ok() }
}

/// The first `max` bytes of the global block behind `format`, with its full size. The clipboard is open.
fn block(format: u32, max: usize) -> Option<(Vec<u8>, usize)> {
    // SAFETY: the clipboard is open on this thread; the block is locked, read within its own size and
    // `max`, and unlocked before returning. The data is copied, never kept by reference.
    unsafe {
        let handle = GetClipboardData(format).ok()?;
        let mem = HGLOBAL(handle.0);
        let size = GlobalSize(mem);
        let ptr = GlobalLock(mem).cast::<u8>();
        if ptr.is_null() {
            return None;
        }
        let bytes = std::slice::from_raw_parts(ptr, size.min(max)).to_vec();
        let _ = GlobalUnlock(mem);
        Some((bytes, size))
    }
}

fn dword(format: u32) -> Option<u32> {
    let (b, _) = block(format, 4)?;
    Some(u32::from_le_bytes(b.get(..4)?.try_into().ok()?))
}

fn read_text() -> Option<String> {
    let (bytes, _) = block(u32::from(CF_UNICODETEXT.0), MAX_TEXT_UNITS * 2)?;
    let units: Vec<u16> = bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|c| u16::from_le_bytes(*c))
        .take_while(|&u| u != 0)
        .collect();
    Some(String::from_utf16_lossy(&units))
}

fn read_files() -> Vec<String> {
    // SAFETY: the clipboard is open; a CF_HDROP handle is a global block the shell API reads.
    unsafe {
        let Ok(handle) = GetClipboardData(u32::from(CF_HDROP.0)) else {
            return Vec::new();
        };
        let drop = HDROP(handle.0);
        let total = DragQueryFileW(drop, u32::MAX, None);
        (0..total.min(MAX_PATHS))
            .filter_map(|i| {
                let len = DragQueryFileW(drop, i, None) as usize;
                let mut buf = vec![0u16; len + 1];
                let n = DragQueryFileW(drop, i, Some(&mut buf)) as usize;
                String::from_utf16(buf.get(..n)?).ok()
            })
            .collect()
    }
}

/// The file name of the exe that owns the clipboard. Called with the clipboard open.
fn owner_exe() -> Option<String> {
    // SAFETY: plain queries on the owner window and its process; the process handle is closed below.
    unsafe {
        let owner = GetClipboardOwner().ok()?;
        let mut pid = 0u32;
        GetWindowThreadProcessId(owner, Some(&mut pid));
        if pid == 0 {
            return None;
        }
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(
            process,
            PROCESS_NAME_WIN32,
            windows::core::PWSTR(buf.as_mut_ptr()),
            &mut len,
        )
        .is_ok();
        let _ = CloseHandle(HANDLE(process.0));
        if !ok {
            return None;
        }
        let full = String::from_utf16(buf.get(..len as usize)?).ok()?;
        Path::new(&full)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
    }
}

/// Reads what the clipboard holds now, or None when it cannot be opened or holds nothing readable.
/// It may not be called on P1. The caller compares `sequence()` before and after (REPORT 5.4).
pub fn read() -> Option<Snapshot> {
    open_clipboard().ok()?;
    let _open = Open;
    let mut s = Snapshot {
        marker: has_marker(),
        owner_exe: owner_exe(),
        exclude_from_monitor: available(registered(w!(
            "ExcludeClipboardContentFromMonitorProcessing"
        ))),
        ..Snapshot::default()
    };
    let include = registered(w!("CanIncludeInClipboardHistory"));
    let upload = registered(w!("CanUploadToCloudClipboard"));
    if available(include) {
        s.can_include_in_history = dword(include);
    }
    if available(upload) {
        s.can_upload_to_cloud = dword(upload);
    }
    // Files first, then text, then a bitmap: a spreadsheet cell is text and a picture at once.
    if available(u32::from(CF_HDROP.0)) {
        s.files = read_files();
    }
    if s.files.is_empty() && available(u32::from(CF_UNICODETEXT.0)) {
        s.text = read_text();
    }
    if s.files.is_empty() && s.text.is_none() && available(u32::from(CF_DIB.0)) {
        s.dib = block(u32::from(CF_DIB.0), MAX_DIB_BYTES).map(|(b, _)| b);
    }
    Some(s)
}
