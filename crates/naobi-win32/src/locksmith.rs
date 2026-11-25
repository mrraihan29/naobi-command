//! File Locksmith (Ticket #25): inspects processes holding locks on a path via Restart Manager
//! (`RmStartSession` / `RmRegisterResources` / `RmGetList`) and terminates a chosen PID cleanly.
//! Standard-user sessions can only observe/end processes of the same user integrity level (R-011).
use windows::core::{HSTRING, PCWSTR, PWSTR};
use windows::Win32::Foundation::{
    CloseHandle, ERROR_ACCESS_DENIED, ERROR_MORE_DATA, ERROR_SUCCESS, HANDLE, WIN32_ERROR,
};
use windows::Win32::System::RestartManager::{
    RmEndSession, RmGetList, RmRegisterResources, RmStartSession, CCH_RM_SESSION_KEY, RM_APP_TYPE,
    RM_PROCESS_INFO,
};
use windows::Win32::System::Threading::{
    GetCurrentProcessId, OpenProcess, TerminateProcess, PROCESS_TERMINATE,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LockProcess {
    pub pid: u32,
    pub name: String,
    pub kind: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LockError {
    AccessDenied,
    Failed(u32),
}

struct SessionGuard(u32);

impl Drop for SessionGuard {
    fn drop(&mut self) {
        // SAFETY: `self.0` was obtained from `RmStartSession` and is closed once here.
        unsafe {
            let _ = RmEndSession(self.0);
        }
    }
}

struct HandleGuard(HANDLE);

impl Drop for HandleGuard {
    fn drop(&mut self) {
        // SAFETY: `self.0` is a valid non-null handle owned by this guard.
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

/// Lists processes locking `path` visible to the current user token via Windows Restart Manager.
pub fn list_locks(path: &str) -> Result<Vec<LockProcess>, LockError> {
    let mut session = 0u32;
    let mut key = [0u16; (CCH_RM_SESSION_KEY as usize) + 1];
    // SAFETY: `session` receives the handle; `key` is a writable null-terminated buffer of `CCH_RM_SESSION_KEY + 1` u16s.
    let rc = unsafe { RmStartSession(&mut session, Some(0), PWSTR(key.as_mut_ptr())) };
    if rc != ERROR_SUCCESS {
        return Err(map_code(rc));
    }
    let _guard = SessionGuard(session);

    let wpath = HSTRING::from(path);
    let files = [PCWSTR(wpath.as_ptr())];
    // SAFETY: `session` is open and `wpath` outlives the registration call.
    let rc = unsafe { RmRegisterResources(session, Some(&files), None, None) };
    if rc != ERROR_SUCCESS {
        return Err(map_code(rc));
    }

    let mut needed = 0u32;
    let mut count: u32;
    let mut reason = 0u32;
    let mut buf: Vec<RM_PROCESS_INFO> = Vec::new();
    loop {
        count = buf.len() as u32;
        let ptr = if count > 0 {
            Some(buf.as_mut_ptr())
        } else {
            None
        };
        // SAFETY: `ptr` points to `count` writable `RM_PROCESS_INFO` elements when `Some`, and
        // `needed` / `count` / `reason` are valid stack muts.
        let rc = unsafe { RmGetList(session, &mut needed, &mut count, ptr, &mut reason) };
        if rc == ERROR_SUCCESS {
            buf.truncate(count as usize);
            break;
        }
        if rc == ERROR_MORE_DATA {
            buf.resize_with(needed as usize, Default::default);
            continue;
        }
        return Err(map_code(rc));
    }

    Ok(buf
        .into_iter()
        .map(|info| {
            let pid = info.Process.dwProcessId;
            let raw = &info.strAppName;
            let end = raw.iter().position(|&c| c == 0).unwrap_or(raw.len());
            let parsed = String::from_utf16_lossy(&raw[..end]);
            let name = if parsed.trim().is_empty() {
                format!("PID {pid}")
            } else {
                parsed
            };
            LockProcess {
                pid,
                name,
                kind: app_kind(info.ApplicationType),
            }
        })
        .collect())
}

/// Terminates process `pid` with exit code 1, returning `LockError::AccessDenied` if elevation is required.
pub fn end_task(pid: u32) -> Result<(), LockError> {
    // SAFETY: `GetCurrentProcessId` reads the current process id safely.
    let self_pid = unsafe { GetCurrentProcessId() };
    if pid == 0 || pid == self_pid {
        return Err(LockError::AccessDenied);
    }
    // SAFETY: `pid` is non-zero and not self; `HandleGuard` closes the handle on drop.
    let proc_h = match unsafe { OpenProcess(PROCESS_TERMINATE, false, pid) } {
        Ok(h) => HandleGuard(h),
        Err(e) => {
            if e.code() == ERROR_ACCESS_DENIED.to_hresult() {
                return Err(LockError::AccessDenied);
            }
            return Err(LockError::Failed(e.code().0 as u32));
        }
    };
    // SAFETY: `proc_h.0` is an open handle with `PROCESS_TERMINATE`.
    if let Err(e) = unsafe { TerminateProcess(proc_h.0, 1) } {
        if e.code() == ERROR_ACCESS_DENIED.to_hresult() {
            return Err(LockError::AccessDenied);
        }
        return Err(LockError::Failed(e.code().0 as u32));
    }
    Ok(())
}

fn map_code(code: WIN32_ERROR) -> LockError {
    if code == ERROR_ACCESS_DENIED {
        LockError::AccessDenied
    } else {
        LockError::Failed(code.0)
    }
}

fn app_kind(kind: RM_APP_TYPE) -> &'static str {
    match kind.0 {
        1 => "Desktop app",
        2 => "Service",
        3 => "Explorer",
        4 => "Console",
        5 => "Critical process",
        1000 => "System",
        _ => "Process",
    }
}
