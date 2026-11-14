//! The Windows side of window management (R-016, RK-11, T-027): the foreground window with its
//! rectangles, the displays with their work areas and DPI, and moving or maximizing a window. The
//! geometry is decided in `naobi-winmgr`.
use naobi_winmgr::{Monitor, Rect, WindowInfo, WindowOps};
use std::ffi::c_void;
use windows::core::BOOL;
use windows::Win32::Foundation::{HWND, LPARAM, RECT};
use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_EXTENDED_FRAME_BOUNDS};
use windows::Win32::Graphics::Gdi::{
    EnumDisplayMonitors, GetMonitorInfoW, MonitorFromWindow, HDC, HMONITOR, MONITORINFO,
    MONITOR_DEFAULTTONEAREST,
};
use windows::Win32::System::Threading::GetCurrentProcessId;
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, GetDpiForWindow, MDT_EFFECTIVE_DPI};
use windows::Win32::UI::WindowsAndMessaging::{
    GetClassNameW, GetForegroundWindow, GetWindowPlacement, GetWindowRect, GetWindowTextLengthW,
    GetWindowTextW, GetWindowThreadProcessId, IsWindowVisible, IsZoomed, SetWindowPos, ShowWindow,
    SWP_NOACTIVATE, SWP_NOZORDER, SW_MAXIMIZE, SW_RESTORE, SW_SHOWMAXIMIZED, WINDOWPLACEMENT,
};

/// The production `WindowOps`.
pub struct Ops;

fn hwnd(h: isize) -> HWND {
    HWND(h as *mut c_void)
}

fn rect(r: RECT) -> Rect {
    Rect::new(r.left, r.top, r.right, r.bottom)
}

/// Every display with its handle value, work area and effective DPI.
fn displays() -> Vec<(isize, Monitor)> {
    unsafe extern "system" fn each(m: HMONITOR, _dc: HDC, _r: *mut RECT, data: LPARAM) -> BOOL {
        // SAFETY: `data` is the address of the `Vec` that `displays` passed and keeps alive for the
        // whole enumeration; the callback runs on the same thread.
        let out = unsafe { &mut *(data.0 as *mut Vec<(isize, Monitor)>) };
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        // SAFETY: `m` is the handle Windows just passed; `info` has its size set.
        if unsafe { GetMonitorInfoW(m, &mut info) }.as_bool() {
            let (mut dx, mut dy) = (96u32, 96u32);
            // SAFETY: writes the two DPI values into the locals above.
            let _ = unsafe { GetDpiForMonitor(m, MDT_EFFECTIVE_DPI, &mut dx, &mut dy) };
            out.push((
                m.0 as isize,
                Monitor {
                    work: rect(info.rcWork),
                    dpi: dx,
                },
            ));
        }
        BOOL(1)
    }
    let mut out: Vec<(isize, Monitor)> = Vec::new();
    // SAFETY: the callback only touches `out`, which outlives the call.
    let _ =
        unsafe { EnumDisplayMonitors(None, None, Some(each), LPARAM(&mut out as *mut _ as isize)) };
    out
}

fn class_of(h: HWND) -> String {
    let mut buf = [0u16; 64];
    // SAFETY: reads the class name into `buf`, which is sized for it.
    let n = unsafe { GetClassNameW(h, &mut buf) }.max(0) as usize;
    String::from_utf16_lossy(&buf[..n.min(buf.len())])
}

fn title_of(h: HWND) -> String {
    // SAFETY: reads the title into a buffer sized from the window's own length.
    unsafe {
        let len = GetWindowTextLengthW(h).max(0) as usize;
        let mut buf = vec![0u16; len + 1];
        let n = GetWindowTextW(h, &mut buf).max(0) as usize;
        String::from_utf16_lossy(&buf[..n.min(len)])
    }
}

/// The window in front that is not ours, waiting a moment after the Launcher hid for focus to land.
fn front_window() -> Option<HWND> {
    for _ in 0..16 {
        // SAFETY: no arguments; returns a handle value or null.
        let h = unsafe { GetForegroundWindow() };
        if !h.0.is_null() {
            let mut pid = 0u32;
            // SAFETY: `pid` receives the owning process id of a handle just returned by Windows.
            unsafe { GetWindowThreadProcessId(h, Some(&mut pid)) };
            // SAFETY: no arguments.
            if pid != unsafe { GetCurrentProcessId() } {
                return Some(h);
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    None
}

/// The desktop and the taskbar are in front when nothing else is, and are not for moving.
fn movable(h: HWND) -> bool {
    // SAFETY: a plain query on a window handle.
    let visible = unsafe { IsWindowVisible(h) }.as_bool();
    let shell = matches!(
        class_of(h).as_str(),
        "Progman" | "WorkerW" | "Shell_TrayWnd" | "Shell_SecondaryTrayWnd"
    );
    visible && !shell && !crate::dwm::is_tool_window(h.0 as isize)
}

impl WindowOps for Ops {
    fn foreground(&mut self) -> Option<WindowInfo> {
        let h = front_window().filter(|h| movable(*h))?;
        let mut current = RECT::default();
        let mut frame = RECT::default();
        let mut placement = WINDOWPLACEMENT {
            length: std::mem::size_of::<WINDOWPLACEMENT>() as u32,
            ..Default::default()
        };
        // SAFETY: each call writes one fixed-size structure that lives in this frame.
        unsafe {
            GetWindowRect(h, &mut current).ok()?;
            let _ = DwmGetWindowAttribute(
                h,
                DWMWA_EXTENDED_FRAME_BOUNDS,
                &mut frame as *mut _ as *mut c_void,
                std::mem::size_of::<RECT>() as u32,
            );
            let _ = GetWindowPlacement(h, &mut placement);
        }
        // No frame bounds (some windows): no invisible border.
        if frame.right <= frame.left {
            frame = current;
        }
        let maximized = placement.showCmd == SW_SHOWMAXIMIZED.0 as u32;
        // SAFETY: a plain query; the handle value is matched against the enumerated displays.
        let on = unsafe { MonitorFromWindow(h, MONITOR_DEFAULTTONEAREST) };
        let monitor = displays()
            .iter()
            .position(|(m, _)| *m == on.0 as isize)
            .unwrap_or(0);
        Some(WindowInfo {
            hwnd: h.0 as isize,
            title: title_of(h),
            outer: if maximized {
                rect(placement.rcNormalPosition)
            } else {
                rect(current)
            },
            border: Rect::new(
                frame.left - current.left,
                frame.top - current.top,
                current.right - frame.right,
                current.bottom - frame.bottom,
            ),
            monitor,
            // Fails closed: a process that cannot be inspected counts as elevated (T-027).
            elevated: crate::paste::is_elevated(h.0 as isize),
            maximized,
        })
    }

    fn monitors(&mut self) -> Vec<Monitor> {
        displays().into_iter().map(|(_, m)| m).collect()
    }

    fn move_to(&mut self, window: isize, outer: Rect, dpi: u32) -> bool {
        let h = hwnd(window);
        let place = || {
            // SAFETY: moves a window handle without activating it; a refusal is an Err.
            unsafe {
                SetWindowPos(
                    h,
                    None,
                    outer.left,
                    outer.top,
                    outer.width(),
                    outer.height(),
                    SWP_NOZORDER | SWP_NOACTIVATE,
                )
            }
            .is_ok()
        };
        // SAFETY: plain queries and a restore on a window handle.
        unsafe {
            if IsZoomed(h).as_bool() {
                let _ = ShowWindow(h, SW_RESTORE);
            }
        }
        if !place() {
            return false;
        }
        // A window that crossed to a display with another DPI is resized by Windows after the move;
        // a second pass puts it where it was asked to be (RK-11).
        let mut now = RECT::default();
        // SAFETY: reads the rectangle into a local.
        let (changed, window_dpi) = unsafe {
            (
                GetWindowRect(h, &mut now).is_ok() && rect(now) != outer,
                GetDpiForWindow(h),
            )
        };
        if changed || window_dpi != dpi {
            return place();
        }
        true
    }

    fn unmaximize(&mut self, window: isize) -> bool {
        // SAFETY: restores a window handle; the return value is the previous visibility.
        let _ = unsafe { ShowWindow(hwnd(window), SW_RESTORE) };
        true
    }

    fn maximize(&mut self, window: isize) -> bool {
        // SAFETY: shows a window handle maximized; the return value is the previous visibility.
        let _ = unsafe { ShowWindow(hwnd(window), SW_MAXIMIZE) };
        true
    }
}
