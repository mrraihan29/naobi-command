//! T-025: DLLs loaded later come only from System32 and WinSxS, never from the exe's folder.
use windows::Win32::System::LibraryLoader::{
    SetDefaultDllDirectories, LOAD_LIBRARY_SEARCH_SYSTEM32,
};

/// Call first in `main`.
pub fn harden() {
    // SAFETY: a process-wide setting with one flag and no pointers.
    let _ = unsafe { SetDefaultDllDirectories(LOAD_LIBRARY_SEARCH_SYSTEM32) };
}
