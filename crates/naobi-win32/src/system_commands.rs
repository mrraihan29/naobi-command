//! The system commands (R-020). Each one calls the Windows API directly: no `shutdown.exe`, so no
//! command line is ever built from a string. The tests never run the destructive ones.
use std::ffi::c_void;
use std::io;

use windows::core::{w, Error, Result, HSTRING};
use windows::Win32::Foundation::{
    CloseHandle, GetLastError, ERROR_NOT_ALL_ASSIGNED, HANDLE, LPARAM, LUID, WPARAM,
};
use windows::Win32::Security::{
    AdjustTokenPrivileges, LookupPrivilegeValueW, LUID_AND_ATTRIBUTES, SE_PRIVILEGE_ENABLED,
    SE_SHUTDOWN_NAME, TOKEN_ADJUST_PRIVILEGES, TOKEN_PRIVILEGES, TOKEN_QUERY,
};
use windows::Win32::System::Power::SetSuspendState;
use windows::Win32::System::Registry::{
    RegGetValueW, RegSetKeyValueW, HKEY_CURRENT_USER, REG_DWORD, RRF_RT_REG_DWORD,
};
use windows::Win32::System::Shutdown::{
    ExitWindowsEx, LockWorkStation, EWX_LOGOFF, EWX_REBOOT, EWX_SHUTDOWN, EXIT_WINDOWS_FLAGS,
    SHUTDOWN_REASON,
};
use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
use windows::Win32::UI::Shell::{
    SHEmptyRecycleBinW, SHERB_NOCONFIRMATION, SHERB_NOPROGRESSUI, SHERB_NOSOUND,
};
use windows::Win32::UI::WindowsAndMessaging::{
    SendMessageTimeoutW, HWND_BROADCAST, SMTO_ABORTIFHUNG, WM_SETTINGCHANGE,
};

/// Where Windows keeps the light or dark choice.
const PERSONALIZE: &str = r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize";
const APPS_VALUE: &str = "AppsUseLightTheme";
const SYSTEM_VALUE: &str = "SystemUsesLightTheme";

pub fn lock() -> io::Result<()> {
    // SAFETY: no arguments; locks the interactive session.
    Ok(unsafe { LockWorkStation() }?)
}

/// Sleep (S3 on this PC). `hibernate` writes the memory to disk and powers off instead.
pub fn suspend(hibernate: bool) -> io::Result<()> {
    // SAFETY: no pointers; not forced, wake events stay enabled.
    if unsafe { SetSuspendState(hibernate, false, false) } {
        Ok(())
    } else {
        Err(Error::from_thread().into())
    }
}

pub fn restart() -> io::Result<()> {
    exit_windows(EWX_REBOOT)
}

pub fn shut_down() -> io::Result<()> {
    exit_windows(EWX_SHUTDOWN)
}

pub fn sign_out() -> io::Result<()> {
    exit_windows(EWX_LOGOFF)
}

/// Not forced: an app that blocks the end of the session still gets to ask, as in the Start menu.
fn exit_windows(how: EXIT_WINDOWS_FLAGS) -> io::Result<()> {
    // Enabling a privilege this user already holds is not elevation; the token stays medium.
    enable_shutdown_privilege()?;
    // SAFETY: no pointers; the reason code 0 is "other".
    Ok(unsafe { ExitWindowsEx(how, SHUTDOWN_REASON(0)) }?)
}

struct Handle(HANDLE);

impl Drop for Handle {
    fn drop(&mut self) {
        // SAFETY: the handle came from a successful OpenProcessToken and is closed once, here.
        let _ = unsafe { CloseHandle(self.0) };
    }
}

fn enable_shutdown_privilege() -> Result<()> {
    let mut token = HANDLE::default();
    // SAFETY: `token` receives a new handle that `Handle` closes.
    unsafe {
        OpenProcessToken(
            GetCurrentProcess(),
            TOKEN_ADJUST_PRIVILEGES | TOKEN_QUERY,
            &mut token,
        )?
    };
    let token = Handle(token);
    let mut luid = LUID::default();
    // SAFETY: looks up a well-known privilege name; `luid` receives the id.
    unsafe { LookupPrivilegeValueW(None, SE_SHUTDOWN_NAME, &mut luid)? };
    let privileges = TOKEN_PRIVILEGES {
        PrivilegeCount: 1,
        Privileges: [LUID_AND_ATTRIBUTES {
            Luid: luid,
            Attributes: SE_PRIVILEGE_ENABLED,
        }],
    };
    // SAFETY: `privileges` is one fully initialised entry and outlives the call.
    unsafe { AdjustTokenPrivileges(token.0, false, Some(&privileges), 0, None, None)? };
    // The call succeeds even when the privilege is not held; only the last error says so.
    // SAFETY: reads this thread's last error code.
    if unsafe { GetLastError() } == ERROR_NOT_ALL_ASSIGNED {
        return Err(Error::from(ERROR_NOT_ALL_ASSIGNED));
    }
    Ok(())
}

pub fn empty_recycle_bin() -> io::Result<()> {
    // SAFETY: no window, no path (every drive); the flags silence the dialogs, which the second
    // Enter replaced.
    Ok(unsafe {
        SHEmptyRecycleBinW(
            None,
            None,
            SHERB_NOCONFIRMATION | SHERB_NOPROGRESSUI | SHERB_NOSOUND,
        )
    }?)
}

/// The next `AppsUseLightTheme` for the current one: light (1) becomes dark (0), anything else light.
pub fn toggled(light: Option<u32>) -> u32 {
    u32::from(light == Some(0))
}

/// Flips Windows between light and dark for apps and the system, then tells every window.
pub fn toggle_dark_mode() -> io::Result<()> {
    let next = toggled(read_dword(APPS_VALUE));
    write_dword(APPS_VALUE, next)?;
    write_dword(SYSTEM_VALUE, next)?;
    // SAFETY: the broadcast string is a static, NUL-terminated wide literal; the call waits at most
    // half a second per window and skips hung ones.
    unsafe {
        SendMessageTimeoutW(
            HWND_BROADCAST,
            WM_SETTINGCHANGE,
            WPARAM(0),
            LPARAM(w!("ImmersiveColorSet").as_ptr() as isize),
            SMTO_ABORTIFHUNG,
            500,
            None,
        );
    }
    Ok(())
}

/// `AppsUseLightTheme` as stored, or None when the value is missing.
pub fn apps_use_light_theme() -> Option<u32> {
    read_dword(APPS_VALUE)
}

fn read_dword(name: &str) -> Option<u32> {
    let (key, name) = (HSTRING::from(PERSONALIZE), HSTRING::from(name));
    let mut data = 0u32;
    let mut len = 4u32;
    // SAFETY: a DWORD query writes at most 4 bytes into `data`, which lives for the call.
    let err = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            &key,
            &name,
            RRF_RT_REG_DWORD,
            None,
            Some(&mut data as *mut u32 as *mut c_void),
            Some(&mut len),
        )
    };
    err.is_ok().then_some(data)
}

fn write_dword(name: &str, value: u32) -> Result<()> {
    let (key, name) = (HSTRING::from(PERSONALIZE), HSTRING::from(name));
    // SAFETY: the data pointer covers the 4 bytes of `value`, which outlives the call.
    unsafe {
        RegSetKeyValueW(
            HKEY_CURRENT_USER,
            &key,
            &name,
            REG_DWORD.0,
            Some(&value as *const u32 as *const c_void),
            4,
        )
        .ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // @R-020
    #[test]
    fn r020_toggle_dark_mode_flips_app_theme() {
        assert_eq!(toggled(Some(1)), 0, "light becomes dark");
        assert_eq!(toggled(Some(0)), 1, "dark becomes light");
        assert_eq!(toggled(None), 0, "a missing value is the default, light");
    }
}
