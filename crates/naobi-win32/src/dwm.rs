//! The Launcher window's OS side: tool-window style, DWM cloak, placement, foreground (ADR-0005).
use std::ffi::c_void;
use windows::core::{w, BOOL, HSTRING};
use windows::Win32::Foundation::{HWND, POINT};
use windows::Win32::Graphics::Dwm::{
    DwmGetWindowAttribute, DwmSetWindowAttribute, DWMWA_CLOAK, DWMWA_CLOAKED,
    DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND,
};
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MonitorFromPoint, MONITORINFO, MONITOR_DEFAULTTONEAREST,
};
use windows::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD};
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_MOUSE, MOUSEINPUT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    FindWindowW, GetCursorPos, GetWindowLongPtrW, SetForegroundWindow, SetWindowLongPtrW,
    SetWindowPos, ShowWindow, GWL_EXSTYLE, HWND_TOPMOST, SWP_NOACTIVATE, SW_HIDE, SW_SHOWNA,
    WS_EX_APPWINDOW, WS_EX_TOOLWINDOW,
};

const LOGICAL_WIDTH: i32 = 750;

fn hwnd(h: isize) -> HWND {
    HWND(h as *mut c_void)
}

/// Finds a top-level window by exact title; the HWND is returned as a plain value.
pub fn find_window(title: &str) -> Option<isize> {
    // SAFETY: the title is a NUL-terminated string that the HSTRING keeps alive for the call.
    unsafe { FindWindowW(None, &HSTRING::from(title)) }
        .ok()
        .map(|h| h.0 as isize)
}

/// The id of the thread that owns window `h`, for tests that check where a window lives.
pub fn window_thread(h: isize) -> u32 {
    // SAFETY: reads the owner thread of a window handle value; the process id is not wanted.
    unsafe { windows::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId(hwnd(h), None) }
}

fn set_cloak(h: isize, on: bool) {
    let v = BOOL::from(on);
    // SAFETY: DWMWA_CLOAK reads one BOOL; `v` outlives the call.
    let _ =
        unsafe { DwmSetWindowAttribute(hwnd(h), DWMWA_CLOAK, &v as *const _ as *const c_void, 4) };
}

/// True while DWM hides the window (our cloak or a virtual desktop switch).
pub fn is_cloaked(h: isize) -> bool {
    let mut v = 0u32;
    // SAFETY: DWMWA_CLOAKED writes one u32 into `v`, which outlives the call.
    let _ = unsafe {
        DwmGetWindowAttribute(hwnd(h), DWMWA_CLOAKED, &mut v as *mut _ as *mut c_void, 4)
    };
    v != 0
}

/// True when the window has `WS_EX_TOOLWINDOW` and not `WS_EX_APPWINDOW`: no taskbar button, not in Alt+Tab.
pub fn is_tool_window(h: isize) -> bool {
    // SAFETY: reads the extended style of a window handle; an invalid handle returns 0.
    let ex = unsafe { GetWindowLongPtrW(hwnd(h), GWL_EXSTYLE) } as u32;
    ex & WS_EX_TOOLWINDOW.0 != 0 && ex & WS_EX_APPWINDOW.0 == 0
}

/// One-time setup of the freshly created window: tool-window style, rounded corners, then cloaked.
pub fn prepare(h: isize) {
    let w = hwnd(h);
    let round = DWMWCP_ROUND;
    // SAFETY: plain style and attribute calls on our own window; `round` outlives the call.
    unsafe {
        // Hide first so the shell drops any taskbar button before the style changes.
        let _ = ShowWindow(w, SW_HIDE);
        let ex = GetWindowLongPtrW(w, GWL_EXSTYLE);
        // winit sets WS_EX_APPWINDOW, which forces a taskbar button even on a tool window.
        let ex = (ex | WS_EX_TOOLWINDOW.0 as isize) & !(WS_EX_APPWINDOW.0 as isize);
        SetWindowLongPtrW(w, GWL_EXSTYLE, ex);
        let _ = DwmSetWindowAttribute(
            w,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            &round as *const _ as *const c_void,
            4,
        );
        let _ = ShowWindow(w, SW_SHOWNA);
    }
    set_cloak(h, true);
}

pub fn hide(h: isize) {
    set_cloak(h, true);
}

/// Pixel size and top-left corner `(x, y, w, h)` for a work area `(left, top, right, bottom)` at `dpi`:
/// 750 logical wide and `height` logical tall, centered horizontally, top edge at 20% of the work
/// area height, but never so low that the bottom edge leaves the work area.
fn placement(work: (i32, i32, i32, i32), dpi: u32, height: i32) -> (i32, i32, i32, i32) {
    let (left, top, right, bottom) = work;
    let scale = |v: i32| v * dpi as i32 / 96;
    let (w, h) = (scale(LOGICAL_WIDTH), scale(height));
    let y = (top + (bottom - top) * 20 / 100).min(bottom - h).max(top);
    (left + (right - left - w) / 2, y, w, h)
}

/// Height of the work area of the monitor under the cursor, in logical px (96 dpi). The Launcher is
/// clamped to 90% of it, so a short screen or a large text scale never pushes the footer off.
pub fn work_height_logical() -> f32 {
    // SAFETY: Win32 calls on plain values; every out-pointer outlives its call.
    unsafe {
        let mut pt = POINT::default();
        let _ = GetCursorPos(&mut pt);
        let mon = MonitorFromPoint(pt, MONITOR_DEFAULTTONEAREST);
        let mut mi = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        let _ = GetMonitorInfoW(mon, &mut mi);
        let (mut dx, mut dy) = (96u32, 96u32);
        let _ = GetDpiForMonitor(mon, MDT_EFFECTIVE_DPI, &mut dx, &mut dy);
        (mi.rcWork.bottom - mi.rcWork.top) as f32 * 96.0 / dx.max(1) as f32
    }
}

/// `(x, y)` moved the least that puts a `w` x `h` window inside `work` `(left, top, right, bottom)`;
/// a window bigger than the work area keeps its left and top edges on it.
fn clamped(work: (i32, i32, i32, i32), (x, y): (i32, i32), w: i32, h: i32) -> (i32, i32) {
    let (left, top, right, bottom) = work;
    (x.min(right - w).max(left), y.min(bottom - h).max(top))
}

/// Where the Owner last dragged the Launcher, as an offset from the top left of the work area it was
/// on. In memory only: a restart opens it at the default place again.
static REMEMBERED: std::sync::Mutex<Option<(i32, i32)>> = std::sync::Mutex::new(None);

/// The remembered spot for a `w` x `h` window on `work`, kept inside it.
fn restored(work: (i32, i32, i32, i32), (ox, oy): (i32, i32), w: i32, h: i32) -> (i32, i32) {
    clamped(work, (work.0 + ox, work.1 + oy), w, h)
}

/// Remembers a dragged position `pos` (physical px) for the next shows.
pub fn remember_position(pos: (i32, i32)) {
    // SAFETY: Win32 calls on plain values; every out-pointer outlives its call.
    let work = unsafe {
        let mut pt = POINT::default();
        let _ = GetCursorPos(&mut pt);
        let mon = MonitorFromPoint(pt, MONITOR_DEFAULTTONEAREST);
        let mut mi = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if !GetMonitorInfoW(mon, &mut mi).as_bool() {
            return;
        }
        mi.rcWork
    };
    *REMEMBERED.lock().unwrap_or_else(|e| e.into_inner()) =
        Some((pos.0 - work.left, pos.1 - work.top));
}

/// Keeps a dragged Launcher at `(x, y)` physical px, `w` x `h`, inside the work area of the monitor
/// under the cursor (the drag handle is under it), so it can never be dragged out of reach.
pub fn clamp_to_work_area(pos: (i32, i32), w: i32, h: i32) -> (i32, i32) {
    // SAFETY: Win32 calls on plain values; every out-pointer outlives its call.
    unsafe {
        let mut pt = POINT::default();
        let _ = GetCursorPos(&mut pt);
        let mon = MonitorFromPoint(pt, MONITOR_DEFAULTTONEAREST);
        let mut mi = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if !GetMonitorInfoW(mon, &mut mi).as_bool() {
            return pos;
        }
        let r = mi.rcWork;
        clamped((r.left, r.top, r.right, r.bottom), pos, w, h)
    }
}

/// Moves the cloaked window to the monitor under the cursor, `height` logical px tall (the UI's own
/// frame height), uncloaks it and takes the foreground.
pub fn show(h: isize, height: f32) {
    let w = hwnd(h);
    // SAFETY: Win32 calls on plain values and our own window; every out-pointer outlives its call.
    unsafe {
        let mut pt = POINT::default();
        let _ = GetCursorPos(&mut pt);
        let mon = MonitorFromPoint(pt, MONITOR_DEFAULTTONEAREST);
        let mut mi = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        let _ = GetMonitorInfoW(mon, &mut mi);
        let (mut dx, mut dy) = (96u32, 96u32);
        let _ = GetDpiForMonitor(mon, MDT_EFFECTIVE_DPI, &mut dx, &mut dy);
        let r = mi.rcWork;
        let work = (r.left, r.top, r.right, r.bottom);
        let (mut x, mut y, pw, ph) = placement(work, dx, height.round() as i32);
        if let Some(off) = *REMEMBERED.lock().unwrap_or_else(|e| e.into_inner()) {
            (x, y) = restored(work, off, pw, ph);
        }
        let _ = SetWindowPos(w, Some(HWND_TOPMOST), x, y, pw, ph, SWP_NOACTIVATE);
        set_cloak(h, false);
        // Foreground trick (ADR-0005): an empty mouse event, then SetForegroundWindow.
        let empty = INPUT {
            r#type: INPUT_MOUSE,
            Anonymous: INPUT_0 {
                mi: MOUSEINPUT::default(),
            },
        };
        SendInput(&[empty], std::mem::size_of::<INPUT>() as i32);
        let _ = SetForegroundWindow(w);
    }
}

/// True when the user chose dark app mode in Windows (`stone` theme), read once at startup.
pub fn system_uses_dark_theme() -> bool {
    let mut v = 1u32;
    let mut size = 4u32;
    // SAFETY: RRF_RT_REG_DWORD writes at most 4 bytes into `v`; `size` says so.
    let err = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize"),
            w!("AppsUseLightTheme"),
            RRF_RT_REG_DWORD,
            None,
            Some(&mut v as *mut _ as *mut c_void),
            Some(&mut size),
        )
    };
    err.0 == 0 && v == 0
}

#[cfg(test)]
mod tests {
    use super::{clamped, placement, restored};

    #[test]
    fn launcher_sits_at_a_fifth_of_the_work_area_and_scales_with_dpi() {
        // 1920 x 1040 work area, 100%: 750 x 475 centered, top at 208.
        assert_eq!(placement((0, 0, 1920, 1040), 96, 475), (585, 208, 750, 475));
        // Second monitor at x = 1920, 125% DPI: 937 x 593 (integer division).
        assert_eq!(
            placement((1920, 0, 3840, 1080), 120, 475),
            (1920 + (1920 - 937) / 2, 216, 937, 593)
        );
        // A tall window (text scale 1.25, 594 logical) is lifted so its bottom stays on screen.
        assert_eq!(placement((0, 0, 1920, 1040), 96, 594), (585, 208, 750, 594));
        assert_eq!(placement((0, 0, 1920, 600), 96, 540), (585, 60, 750, 540));
        // Taller than the work area: the top edge is the work area's top.
        assert_eq!(placement((0, 0, 1920, 500), 96, 540).1, 0);
    }

    #[test]
    fn a_remembered_position_is_kept_per_work_area_and_clamped() {
        // Dragged to (100, 50) from the top left of the first monitor.
        assert_eq!(restored((0, 0, 1920, 1040), (100, 50), 750, 475), (100, 50));
        // The same offset on a monitor to the right.
        assert_eq!(
            restored((1920, 0, 3840, 1040), (100, 50), 750, 475),
            (2020, 50)
        );
        // A smaller work area pulls it back inside.
        assert_eq!(restored((0, 0, 800, 600), (100, 400), 750, 475), (50, 125));
    }

    #[test]
    fn a_dragged_window_stays_inside_the_work_area() {
        let work = (0, 0, 1920, 1040);
        assert_eq!(clamped(work, (585, 208), 750, 475), (585, 208));
        assert_eq!(clamped(work, (-300, -40), 750, 475), (0, 0));
        assert_eq!(clamped(work, (1800, 900), 750, 475), (1170, 565));
        // A window wider than the work area keeps its left and top edges.
        assert_eq!(clamped((0, 0, 500, 400), (90, 90), 750, 475), (0, 0));
        // A second monitor to the right.
        assert_eq!(
            clamped((1920, 0, 3840, 1040), (1500, 10), 750, 475),
            (1920, 10)
        );
    }
}
