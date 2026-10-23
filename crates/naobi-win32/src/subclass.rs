//! t0 recorder of the keystroke metric (R-006, RK-15). Compiled only with feature `perf`.
//! A window subclass on the Launcher takes QPC at the first `WM_KEYDOWN` or `WM_CHAR` of each key
//! and writes `{"ev":"key","qpc":<ticks>}` to stdout (a pipe to the harness; no socket, no file).
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, Ordering};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::Shell::{DefSubclassProc, SetWindowSubclass};
use windows::Win32::UI::WindowsAndMessaging::{WM_CHAR, WM_KEYDOWN, WM_KEYUP};

/// True between the first KEYDOWN/CHAR of a key and its KEYUP, so one key yields one line.
static DOWN: AtomicBool = AtomicBool::new(false);

unsafe extern "system" fn proc(
    h: HWND,
    m: u32,
    w: WPARAM,
    l: LPARAM,
    _id: usize,
    _data: usize,
) -> LRESULT {
    let t0 = match m {
        WM_KEYDOWN | WM_CHAR if !DOWN.swap(true, Ordering::Relaxed) => Some(crate::platform::qpc()),
        WM_KEYUP => {
            DOWN.store(false, Ordering::Relaxed);
            None
        }
        _ => None,
    };
    // SAFETY: forwards the unchanged message down the subclass chain.
    let r = unsafe { DefSubclassProc(h, m, w, l) };
    // Written after the key was handled so the line does not delay the path being measured.
    if let Some(t) = t0 {
        println!(r#"{{"ev":"key","qpc":{t}}}"#);
    }
    r
}

/// Subclasses the window `hwnd` (a plain value). Must run on the thread that owns the window.
pub fn install(hwnd: isize) -> bool {
    // SAFETY: `proc` matches SUBCLASSPROC; the window belongs to this thread; no reference data is passed.
    unsafe { SetWindowSubclass(HWND(hwnd as *mut c_void), Some(proc), 1, 0) }.as_bool()
}
