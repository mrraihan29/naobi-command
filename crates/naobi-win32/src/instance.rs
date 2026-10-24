//! Single instance (ADR-0005, D-013): a named mutex, and a payload-free message to the first
//! instance's platform window, found by its window class name.
use windows::core::{w, Result, PCWSTR};
use windows::Win32::Foundation::{
    CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HANDLE, LPARAM, WPARAM,
};
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::UI::WindowsAndMessaging::{FindWindowW, PostMessageW, WM_APP};

/// Class name of the platform window; the second instance finds the first by it.
pub const CLASS: PCWSTR = w!("NaobiCommandPlatform");
/// Asks the first instance to show the Launcher. No payload, so it cannot do more (T-002).
pub const WM_APP_SHOW: u32 = WM_APP + 2;
const MUTEX: PCWSTR = w!("Local\\NaobiCommand.Instance");

/// Holds the mutex for the life of the process.
pub struct Claim(HANDLE);

impl Drop for Claim {
    fn drop(&mut self) {
        // SAFETY: the handle came from CreateMutexW in `claim` and is closed once, here.
        let _ = unsafe { CloseHandle(self.0) };
    }
}

/// The first instance gets the claim; None means another process already runs.
pub fn claim() -> Result<Option<Claim>> {
    // SAFETY: a named mutex with no security attributes; the name is a static wide string.
    let handle = unsafe { CreateMutexW(None, true, MUTEX)? };
    // SAFETY: reads the calling thread's last error, set by CreateMutexW just above.
    if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        // SAFETY: the handle is ours and is not used again.
        let _ = unsafe { CloseHandle(handle) };
        return Ok(None);
    }
    Ok(Some(Claim(handle)))
}

/// Tells the first instance to show the Launcher. False when no first instance is found.
pub fn show_first() -> bool {
    // SAFETY: static class name; no ownership moves.
    let Ok(hwnd) = (unsafe { FindWindowW(CLASS, None) }) else {
        return false;
    };
    // SAFETY: posts a message with no pointers in it to a window of this session.
    unsafe { PostMessageW(Some(hwnd), WM_APP_SHOW, WPARAM(0), LPARAM(0)) }.is_ok()
}
