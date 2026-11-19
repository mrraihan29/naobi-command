//! t0 recorder of the keystroke metric (R-006, RK-15). Compiled only with feature `perf`.
//! A window subclass on the Launcher takes QPC at the first `WM_KEYDOWN` of each key (the one whose
//! previous-state bit is clear, so auto-repeat adds nothing; `WM_CHAR` always follows it) and writes `{"ev":"key","qpc":<ticks>}` to stdout (a pipe to the harness; no socket, no file).
use std::ffi::c_void;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::Shell::{DefSubclassProc, SetWindowSubclass};
use windows::Win32::UI::WindowsAndMessaging::WM_KEYDOWN;

/// Bit 30 of a `WM_KEYDOWN` lParam: the key was already down (an auto-repeat).
const WAS_DOWN: isize = 1 << 30;

unsafe extern "system" fn proc(
    h: HWND,
    m: u32,
    w: WPARAM,
    l: LPARAM,
    _id: usize,
    _data: usize,
) -> LRESULT {
    // A flag held between KEYDOWN and KEYUP stuck when the show hotkey's KEYDOWN reached the window
    // without its KEYUP, and the next key then wrote no line.
    let t0 = (m == WM_KEYDOWN && l.0 & WAS_DOWN == 0).then(crate::platform::qpc);
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
