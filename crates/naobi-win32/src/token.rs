//! Token checks shared by the window, topmost and paste items (R-010).
use std::ffi::c_void;
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::Security::{GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY};
use windows::Win32::System::Threading::{
    OpenProcess, OpenProcessToken, PROCESS_QUERY_LIMITED_INFORMATION,
};

struct Owned(HANDLE);

impl Drop for Owned {
    fn drop(&mut self) {
        // SAFETY: the handle came from a successful Open* call and is closed once, here.
        let _ = unsafe { CloseHandle(self.0) };
    }
}

/// True when the process behind `process` runs with an elevated token.
pub fn process_is_elevated(process: HANDLE) -> windows::core::Result<bool> {
    let mut token = HANDLE::default();
    // SAFETY: `token` receives a new handle that `Owned` closes.
    unsafe { OpenProcessToken(process, TOKEN_QUERY, &mut token)? };
    let token = Owned(token);
    let mut e = TOKEN_ELEVATION::default();
    let mut len = 0u32;
    // SAFETY: TokenElevation writes one TOKEN_ELEVATION into `e`; the size passed is its size.
    unsafe {
        GetTokenInformation(
            token.0,
            TokenElevation,
            Some(&mut e as *mut _ as *mut c_void),
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut len,
        )?
    };
    Ok(e.TokenIsElevated != 0)
}

/// Same check for a process id (the handle is opened with query-limited rights and closed here).
pub fn pid_is_elevated(pid: u32) -> windows::core::Result<bool> {
    // SAFETY: OpenProcess returns a new handle that `Owned` closes.
    let p = Owned(unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid)? });
    process_is_elevated(p.0)
}
