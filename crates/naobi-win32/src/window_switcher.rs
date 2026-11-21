//! Win32 implementation for the Window switcher: `EnumWindows` + `DWMWA_CLOAKED` filtering,
//! plus Focus, Minimize, Close (`WM_CLOSE`), and Force Quit (`TerminateProcess`).
use crate::process::{exe_name, Owned};
use std::ffi::c_void;
use windows::core::BOOL;
use windows::Win32::Foundation::{ERROR_ACCESS_DENIED, HWND, LPARAM, WPARAM};
use windows::Win32::System::Threading::{
    GetCurrentProcessId, OpenProcess, TerminateProcess, PROCESS_TERMINATE,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetClassNameW, GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId,
    IsIconic, IsWindow, IsWindowVisible, PostMessageW, SetForegroundWindow, ShowWindowAsync,
    SW_MINIMIZE, SW_RESTORE, WM_CLOSE,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowEntry {
    pub hwnd: isize,
    pub title: String,
    pub process_name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionError {
    /// Target runs at higher integrity (Windows UIPI blocks control, R-011).
    Elevated,
    /// Windows refused the operation or the window is no longer valid.
    Refused,
}

fn hwnd_of(h: isize) -> HWND {
    HWND(h as *mut c_void)
}

fn class_of(h: HWND) -> String {
    let mut buf = [0u16; 64];
    // SAFETY: reads the class name into `buf`, sized for it.
    let n = unsafe { GetClassNameW(h, &mut buf) }.max(0) as usize;
    String::from_utf16_lossy(&buf[..n.min(buf.len())])
}

fn title_of(h: HWND) -> String {
    // SAFETY: reads the title into a buffer sized from the window's reported text length.
    unsafe {
        let len = GetWindowTextLengthW(h).max(0) as usize;
        if len == 0 {
            return String::new();
        }
        let mut buf = vec![0u16; len + 1];
        let n = GetWindowTextW(h, &mut buf).max(0) as usize;
        String::from_utf16_lossy(&buf[..n.min(len)])
    }
}

fn is_shell_class(class: &str) -> bool {
    matches!(
        class,
        "Progman" | "WorkerW" | "Shell_TrayWnd" | "Shell_SecondaryTrayWnd"
    )
}

fn is_candidate(
    visible: bool,
    pid: u32,
    own_pid: u32,
    tool: bool,
    cloaked: bool,
    class_name: &str,
    title: &str,
) -> bool {
    visible
        && pid != 0
        && pid != own_pid
        && !tool
        && !cloaked
        && !is_shell_class(class_name)
        && !title.trim().is_empty()
}

struct EnumState {
    own_pid: u32,
    out: Vec<WindowEntry>,
}

unsafe extern "system" fn enum_proc(h: HWND, lparam: LPARAM) -> BOOL {
    // SAFETY: `lparam` points to the stack-allocated `EnumState` inside `list()`, alive for the duration of `EnumWindows`.
    let state = unsafe { &mut *(lparam.0 as *mut EnumState) };
    // SAFETY: plain read-only window queries on a handle passed by EnumWindows.
    let visible = unsafe { IsWindowVisible(h) }.as_bool();
    if !visible {
        return BOOL(1);
    }
    let mut pid = 0u32;
    // SAFETY: `pid` receives the owning process id of `h`.
    unsafe { GetWindowThreadProcessId(h, Some(&mut pid)) };
    let raw = h.0 as isize;
    if pid == 0
        || pid == state.own_pid
        || crate::dwm::is_tool_window(raw)
        || crate::dwm::is_cloaked(raw)
    {
        return BOOL(1);
    }
    let class = class_of(h);
    if is_shell_class(&class) {
        return BOOL(1);
    }
    let title = title_of(h);
    if !is_candidate(visible, pid, state.own_pid, false, false, &class, &title) {
        return BOOL(1);
    }
    state.out.push(WindowEntry {
        hwnd: raw,
        title,
        process_name: exe_name(pid).unwrap_or_default(),
    });
    BOOL(1)
}

/// Enumerates visible, non-cloaked top-level Open windows, excluding Naobi's own windows.
pub fn list() -> Vec<WindowEntry> {
    // SAFETY: GetCurrentProcessId takes no arguments and cannot fail.
    let own_pid = unsafe { GetCurrentProcessId() };
    let mut state = EnumState {
        own_pid,
        out: Vec::new(),
    };
    let ptr = &mut state as *mut EnumState as isize;
    // SAFETY: `enum_proc` borrows `state` exclusively during the synchronous EnumWindows walk.
    let _ = unsafe { EnumWindows(Some(enum_proc), LPARAM(ptr)) };
    state.out
}

pub fn focus(hwnd: isize) -> Result<(), ActionError> {
    let h = hwnd_of(hwnd);
    // SAFETY: `h` is validated by IsWindow before IsIconic / ShowWindowAsync / SetForegroundWindow.
    unsafe {
        if !IsWindow(Some(h)).as_bool() {
            return Err(ActionError::Refused);
        }
        if IsIconic(h).as_bool() {
            let _ = ShowWindowAsync(h, SW_RESTORE);
        }
        if !SetForegroundWindow(h).as_bool() {
            return Err(ActionError::Refused);
        }
    }
    Ok(())
}

fn ensure_target_window(hwnd: isize) -> Result<HWND, ActionError> {
    let h = hwnd_of(hwnd);
    // SAFETY: `IsWindow` validates `h` as a live window handle before checking elevation.
    if !unsafe { IsWindow(Some(h)) }.as_bool() {
        return Err(ActionError::Refused);
    }
    if crate::paste::is_elevated(hwnd) {
        return Err(ActionError::Elevated);
    }
    Ok(h)
}

pub fn minimize(hwnd: isize) -> Result<(), ActionError> {
    let h = ensure_target_window(hwnd)?;
    // SAFETY: `h` was validated as a live non-elevated window by `ensure_target_window`.
    if !unsafe { ShowWindowAsync(h, SW_MINIMIZE) }.as_bool() {
        return Err(ActionError::Refused);
    }
    Ok(())
}

pub fn close(hwnd: isize) -> Result<(), ActionError> {
    let h = ensure_target_window(hwnd)?;
    // SAFETY: `h` was validated as a live non-elevated window by `ensure_target_window`.
    if let Err(e) = unsafe { PostMessageW(Some(h), WM_CLOSE, WPARAM(0), LPARAM(0)) } {
        if e.code() == ERROR_ACCESS_DENIED.to_hresult() {
            return Err(ActionError::Elevated);
        }
        return Err(ActionError::Refused);
    }
    Ok(())
}

pub fn force_quit(hwnd: isize) -> Result<(), ActionError> {
    let h = ensure_target_window(hwnd)?;
    let mut pid = 0u32;
    // SAFETY: `h` was validated by `ensure_target_window`; `pid` receives its owning process id.
    unsafe {
        GetWindowThreadProcessId(h, Some(&mut pid));
        if pid == 0 || pid == GetCurrentProcessId() {
            return Err(ActionError::Refused);
        }
    }
    // SAFETY: `pid` is a non-zero non-self process id; `Owned` closes the handle once.
    let process = match unsafe { OpenProcess(PROCESS_TERMINATE, false, pid) } {
        Ok(p) => Owned(p),
        Err(e) => {
            if e.code() == ERROR_ACCESS_DENIED.to_hresult() {
                return Err(ActionError::Elevated);
            }
            return Err(ActionError::Refused);
        }
    };
    // SAFETY: `process.0` is a valid handle opened with PROCESS_TERMINATE.
    unsafe { TerminateProcess(process.0, 1) }.map_err(|_| ActionError::Refused)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filter_rejects_invisible_cloaked_tool_shell_empty_and_own_windows() {
        assert!(is_candidate(true, 42, 1, false, false, "Notepad", "Doc"));
        assert!(!is_candidate(false, 42, 1, false, false, "Notepad", "Doc"));
        assert!(!is_candidate(true, 1, 1, false, false, "Notepad", "Doc"));
        assert!(!is_candidate(true, 0, 1, false, false, "Notepad", "Doc"));
        assert!(!is_candidate(true, 42, 1, true, false, "Notepad", "Doc"));
        assert!(!is_candidate(true, 42, 1, false, true, "Notepad", "Doc"));
        assert!(!is_candidate(
            true,
            42,
            1,
            false,
            false,
            "Progman",
            "Program Manager"
        ));
        assert!(!is_candidate(
            true,
            42,
            1,
            false,
            false,
            "Shell_TrayWnd",
            "Taskbar"
        ));
        assert!(!is_candidate(true, 42, 1, false, false, "Notepad", "   "));
    }

    #[test]
    fn list_excludes_current_process_and_actions_reject_invalid_hwnd() {
        // SAFETY: GetCurrentProcessId takes no arguments.
        let own_pid = unsafe { GetCurrentProcessId() };
        for w in list() {
            assert!(!w.title.trim().is_empty());
            let mut pid = 0u32;
            // SAFETY: `w.hwnd` came from EnumWindows.
            unsafe { GetWindowThreadProcessId(hwnd_of(w.hwnd), Some(&mut pid)) };
            assert_ne!(pid, own_pid);
        }
        assert_eq!(focus(0), Err(ActionError::Refused));
        assert_eq!(minimize(0), Err(ActionError::Refused));
        assert_eq!(close(0), Err(ActionError::Refused));
        assert_eq!(force_quit(0), Err(ActionError::Refused));
    }
}
