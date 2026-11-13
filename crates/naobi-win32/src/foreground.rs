//! Focus return (D-013): the window that had focus before the Launcher opened gets it back when the
//! Launcher closes without launching anything.
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, IsWindow, SetForegroundWindow};

/// The current foreground window as a plain value, or None when there is none. `own` is the
/// Launcher's own window, which is never remembered.
pub fn remember(own: isize) -> Option<isize> {
    // SAFETY: GetForegroundWindow has no arguments and returns a handle value or null.
    let h = unsafe { GetForegroundWindow() };
    (!h.0.is_null() && h.0 as isize != own).then_some(h.0 as isize)
}

/// Gives focus back to `hwnd` if that window still exists. A refusal by Windows is ignored: the
/// Owner then clicks where they want to be.
pub fn restore(hwnd: isize) {
    let h = HWND(hwnd as *mut core::ffi::c_void);
    // SAFETY: `h` is only a handle value; IsWindow validates it, and SetForegroundWindow takes the
    // same value. Neither dereferences it.
    unsafe {
        if IsWindow(Some(h)).as_bool() {
            let _ = SetForegroundWindow(h);
        }
    }
}
