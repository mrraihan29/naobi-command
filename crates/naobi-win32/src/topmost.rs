//! The Windows side of always on top (R-018, REPORT 5.4): `SetWindowPos(HWND_TOPMOST)`, and a 2 px
//! border that follows each topmost window. The border is one popup window per topmost window whose
//! region is a ring; it is created, moved and removed on the platform thread, where one WinEvent
//! hook per topmost *process* (scoped by process id; the callback acts on destroy, show, hide and
//! location events of the window itself and ignores the rest) keeps it in place. With nothing topmost
//! there is no border window and no hook.
use naobi_aot::{Style, Target, TopmostOps, THICKNESS};
use std::ffi::c_void;
use std::sync::Mutex;
use windows::core::w;
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_EXTENDED_FRAME_BOUNDS};
use windows::Win32::Graphics::Gdi::{
    CombineRgn, CreateRectRgn, CreateSolidBrush, DeleteObject, InvalidateRect, SetWindowRgn,
    HGDIOBJ, RGN_DIFF,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::GetCurrentProcessId;
use windows::Win32::UI::Accessibility::{SetWinEventHook, UnhookWinEvent, HWINEVENTHOOK};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, GetClassNameW, GetForegroundWindow,
    GetWindowLongPtrW, GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId, IsIconic,
    IsWindowVisible, RegisterClassW, SetClassLongPtrW, SetLayeredWindowAttributes, SetWindowPos,
    ShowWindow, EVENT_OBJECT_DESTROY, EVENT_OBJECT_HIDE, EVENT_OBJECT_LOCATIONCHANGE,
    EVENT_OBJECT_SHOW, GCLP_HBRBACKGROUND, GWL_EXSTYLE, HWND_NOTOPMOST, HWND_TOPMOST, LWA_ALPHA,
    OBJID_WINDOW, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SW_HIDE, SW_SHOWNOACTIVATE,
    WINEVENT_OUTOFCONTEXT, WINEVENT_SKIPOWNPROCESS, WNDCLASSW, WS_EX_LAYERED, WS_EX_NOACTIVATE,
    WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
};

unsafe extern "system" fn border_proc(h: HWND, m: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    // SAFETY: forwards the message Windows delivered, unchanged, to the default procedure.
    unsafe { DefWindowProcW(h, m, w, l) }
}

/// One topmost window and the border that follows it.
struct Pin {
    target: isize,
    border: isize,
    pid: u32,
    /// The size the border's ring region was cut for.
    size: (i32, i32),
}

struct State {
    pins: Vec<Pin>,
    /// One hook per process id, as long as a pin of that process exists.
    hooks: Vec<(u32, isize)>,
    style: Style,
    brush: isize,
}

static STATE: Mutex<State> = Mutex::new(State {
    pins: Vec::new(),
    hooks: Vec::new(),
    style: Style {
        show: true,
        color: naobi_aot::LIGHT,
    },
    brush: 0,
});

fn hwnd(h: isize) -> HWND {
    HWND(h as *mut c_void)
}

/// `0xRRGGBB` to the `0x00BBGGRR` Windows wants.
fn colorref(rgb: u32) -> COLORREF {
    COLORREF(((rgb & 0xFF) << 16) | (rgb & 0xFF00) | ((rgb >> 16) & 0xFF))
}

fn lock() -> std::sync::MutexGuard<'static, State> {
    STATE.lock().unwrap_or_else(|e| e.into_inner())
}

/// A fresh brush of the style's color becomes the border class's background; the old one is freed.
fn paint(st: &mut State, any_border: Option<isize>) {
    // SAFETY: creates a brush that this state owns until it is replaced.
    let brush = unsafe { CreateSolidBrush(colorref(st.style.color)) };
    if let Some(b) = any_border {
        // SAFETY: swaps the class background of a window this module created.
        unsafe { SetClassLongPtrW(hwnd(b), GCLP_HBRBACKGROUND, brush.0 as isize) };
    }
    if st.brush != 0 {
        // SAFETY: the old brush is no longer the class background (replaced above) and is freed once.
        let _ = unsafe { DeleteObject(HGDIOBJ(st.brush as *mut c_void)) };
    }
    st.brush = brush.0 as isize;
}

fn create_border(st: &mut State) -> Option<isize> {
    static CLASS: std::sync::Once = std::sync::Once::new();
    // SAFETY: a module handle query and a class registration with a static name; Windows owns the class.
    let instance = unsafe { GetModuleHandleW(None) }.ok()?;
    CLASS.call_once(|| {
        let class = WNDCLASSW {
            lpfnWndProc: Some(border_proc),
            hInstance: instance.into(),
            lpszClassName: w!("NaobiTopmostBorder"),
            ..Default::default()
        };
        // SAFETY: `class` is fully initialised and outlives the call.
        unsafe { RegisterClassW(&class) };
    });
    // SAFETY: creates a click-through tool popup on this (the platform) thread; it owns no data.
    let h = unsafe {
        CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_LAYERED | WS_EX_TRANSPARENT,
            w!("NaobiTopmostBorder"),
            w!(""),
            WS_POPUP,
            0,
            0,
            0,
            0,
            None,
            None,
            Some(instance.into()),
            None,
        )
    }
    .ok()?;
    // Layered and transparent together: clicks on the ring reach the window under it.
    // SAFETY: sets full opacity on the window created above.
    let _ = unsafe { SetLayeredWindowAttributes(h, COLORREF(0), 255, LWA_ALPHA) };
    paint(st, Some(h.0 as isize));
    Some(h.0 as isize)
}

/// The visible frame of `target`, or None while it is minimized, hidden or gone.
fn frame_of(target: isize) -> Option<RECT> {
    let h = hwnd(target);
    let mut r = RECT::default();
    // SAFETY: plain queries on a window handle; the frame bounds go into a local.
    unsafe {
        // Minimized, hidden (to the tray, say) or cloaked (another virtual desktop): no border.
        if IsIconic(h).as_bool() || !IsWindowVisible(h).as_bool() || crate::dwm::is_cloaked(target)
        {
            return None;
        }
        DwmGetWindowAttribute(
            h,
            DWMWA_EXTENDED_FRAME_BOUNDS,
            &mut r as *mut _ as *mut c_void,
            std::mem::size_of::<RECT>() as u32,
        )
        .ok()?;
    }
    (r.right > r.left && r.bottom > r.top).then_some(r)
}

/// Puts the pin's border around its window, or hides it.
fn place(pin: &mut Pin, show: bool) {
    let b = hwnd(pin.border);
    let frame = if show { frame_of(pin.target) } else { None };
    let Some(f) = frame else {
        // SAFETY: hides a window this module created.
        let _ = unsafe { ShowWindow(b, SW_HIDE) };
        return;
    };
    let (x, y) = (f.left - THICKNESS, f.top - THICKNESS);
    let (w, h) = (
        f.right - f.left + 2 * THICKNESS,
        f.bottom - f.top + 2 * THICKNESS,
    );
    // SAFETY: regions are created here and handed to the window, which owns the outer one; the
    // inner one is freed. The border is a window of this thread.
    unsafe {
        if pin.size != (w, h) {
            let ring = CreateRectRgn(0, 0, w, h);
            let hole = CreateRectRgn(THICKNESS, THICKNESS, w - THICKNESS, h - THICKNESS);
            CombineRgn(Some(ring), Some(ring), Some(hole), RGN_DIFF);
            let _ = DeleteObject(HGDIOBJ(hole.0));
            SetWindowRgn(b, Some(ring), true);
            pin.size = (w, h);
        }
        let _ = SetWindowPos(b, Some(HWND_TOPMOST), x, y, w, h, SWP_NOACTIVATE);
        let _ = ShowWindow(b, SW_SHOWNOACTIVATE);
    }
}

unsafe extern "system" fn on_event(
    _hook: HWINEVENTHOOK,
    event: u32,
    window: HWND,
    id_object: i32,
    _id_child: i32,
    _thread: u32,
    _time: u32,
) {
    if id_object != OBJID_WINDOW.0 {
        return;
    }
    // Callbacks arrive between our own calls on this thread; if the state is in use, skip one move.
    let Ok(mut st) = STATE.try_lock() else { return };
    let target = window.0 as isize;
    if event == EVENT_OBJECT_DESTROY {
        remove(&mut st, target);
    } else if event == EVENT_OBJECT_LOCATIONCHANGE
        || event == EVENT_OBJECT_SHOW
        || event == EVENT_OBJECT_HIDE
    {
        let show = st.style.show;
        if let Some(pin) = st.pins.iter_mut().find(|p| p.target == target) {
            place(pin, show);
        }
    }
}

fn add(st: &mut State, target: isize) {
    if st.pins.iter().any(|p| p.target == target) {
        return;
    }
    let mut pid = 0u32;
    // SAFETY: `pid` receives the process id of a window handle.
    unsafe { GetWindowThreadProcessId(hwnd(target), Some(&mut pid)) };
    let Some(border) = create_border(st) else {
        return;
    };
    let mut pin = Pin {
        target,
        border,
        pid,
        size: (0, 0),
    };
    place(&mut pin, st.style.show);
    st.pins.push(pin);
    if pid != 0 && !st.hooks.iter().any(|(p, _)| *p == pid) {
        // SAFETY: an out-of-context hook scoped to this process id, for the object events from
        // destroy to location change (the callback ignores all but the ones it names); the callback
        // is a plain function and the hook is removed in `remove`.
        let hook = unsafe {
            SetWinEventHook(
                EVENT_OBJECT_DESTROY,
                EVENT_OBJECT_LOCATIONCHANGE,
                None,
                Some(on_event),
                pid,
                0,
                WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
            )
        };
        // A refused hook leaves the border where it is: it just does not follow.
        if !hook.0.is_null() {
            st.hooks.push((pid, hook.0 as isize));
        }
    }
}

fn remove(st: &mut State, target: isize) {
    let Some(i) = st.pins.iter().position(|p| p.target == target) else {
        return;
    };
    let pin = st.pins.remove(i);
    // SAFETY: destroys a window this module created, on the thread that created it.
    let _ = unsafe { DestroyWindow(hwnd(pin.border)) };
    // The last pin of a process takes its hook with it: with nothing topmost there is no hook.
    if !st.pins.iter().any(|p| p.pid == pin.pid) {
        if let Some(j) = st.hooks.iter().position(|(p, _)| *p == pin.pid) {
            let (_, hook) = st.hooks.remove(j);
            // SAFETY: the handle came from SetWinEventHook and is unhooked once.
            let _ = unsafe { UnhookWinEvent(HWINEVENTHOOK(hook as *mut c_void)) };
        }
    }
}

fn apply_style(style: Style) {
    let mut st = lock();
    st.style = style;
    let first = st.pins.first().map(|p| p.border);
    paint(&mut st, first);
    for pin in &mut st.pins {
        place(pin, style.show);
        // SAFETY: repaints a window this module created.
        let _ = unsafe { InvalidateRect(Some(hwnd(pin.border)), None, true) };
    }
}

/// Starts following `window` with a border, on the calling thread: the platform thread in the app.
pub fn pin_now(window: isize) {
    add(&mut lock(), window);
}

/// Stops following `window` and removes its border (and its process's hook with the last pin).
pub fn unpin_now(window: isize) {
    remove(&mut lock(), window);
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

/// The desktop and the taskbar are in front when nothing else is, and are not for toggling.
fn movable(h: HWND) -> bool {
    // SAFETY: a plain query on a window handle.
    let visible = unsafe { IsWindowVisible(h) }.as_bool();
    let shell = matches!(
        class_of(h).as_str(),
        "Progman" | "WorkerW" | "Shell_TrayWnd" | "Shell_SecondaryTrayWnd"
    );
    visible && !shell && !crate::dwm::is_tool_window(h.0 as isize)
}

/// The number of topmost windows being followed and of hooks installed.
pub fn counts() -> (usize, usize) {
    let st = lock();
    (st.pins.len(), st.hooks.len())
}

/// The production `TopmostOps`.
pub struct Ops;

impl TopmostOps for Ops {
    fn foreground(&mut self) -> Option<Target> {
        let h = front_window().filter(|h| movable(*h))?;
        Some(Target {
            hwnd: h.0 as isize,
            title: title_of(h),
            // Fails closed: a process that cannot be inspected counts as elevated (T-027).
            elevated: crate::paste::is_elevated(h.0 as isize),
        })
    }

    fn is_topmost(&mut self, window: isize) -> bool {
        // SAFETY: reads the extended style of a window handle; an invalid handle gives 0.
        let ex = unsafe { GetWindowLongPtrW(hwnd(window), GWL_EXSTYLE) } as u32;
        ex & WS_EX_TOPMOST.0 != 0
    }

    fn set_topmost(&mut self, window: isize, on: bool) -> bool {
        // SAFETY: changes only the z-order band of a window handle; no move, size or activation.
        unsafe {
            SetWindowPos(
                hwnd(window),
                Some(if on { HWND_TOPMOST } else { HWND_NOTOPMOST }),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            )
        }
        .is_ok()
    }

    fn follow(&mut self, window: isize, on: bool) {
        // The border and the hook belong to the platform thread (a WinEvent hook needs the message
        // loop of the thread that installs it).
        crate::platform::post_to_p1(Box::new(move || {
            if on {
                pin_now(window);
            } else {
                unpin_now(window);
            }
        }));
    }

    fn set_style(&mut self, style: Style) {
        // Before the platform thread is up there is nothing to repaint: keep the look for later.
        if !crate::platform::post_to_p1(Box::new(move || apply_style(style))) {
            lock().style = style;
        }
    }
}
