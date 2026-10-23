//! Platform thread (P1): a message-only window that owns the hotkey and the tray icon.
//! NCMD-12 replaces the window by a hidden top-level tool window so broadcasts arrive.
use std::sync::{mpsc, OnceLock};
use windows::core::{w, Error, Result};
use windows::Win32::Foundation::{E_FAIL, HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Performance::{QueryPerformanceCounter, QueryPerformanceFrequency};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    RegisterHotKey, MOD_ALT, MOD_NOREPEAT, VK_SPACE,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, RegisterClassW, HWND_MESSAGE,
    MSG, WINDOW_EX_STYLE, WINDOW_STYLE, WM_CONTEXTMENU, WM_HOTKEY, WM_RBUTTONUP, WNDCLASSW,
};

use crate::tray;

struct Callbacks {
    on_hotkey: Box<dyn Fn() + Send + Sync>,
    on_quit: Box<dyn Fn() + Send + Sync>,
}

static CALLBACKS: OnceLock<Callbacks> = OnceLock::new();

/// Monotonic timestamp (QueryPerformanceCounter ticks), the clock of the readiness lines.
pub fn qpc() -> i64 {
    let mut t = 0i64;
    // SAFETY: writes one i64 into `t`.
    let _ = unsafe { QueryPerformanceCounter(&mut t) };
    t
}

/// Milliseconds in `ticks` QueryPerformanceCounter ticks.
pub fn qpc_to_ms(ticks: i64) -> f64 {
    let mut f = 0i64;
    // SAFETY: writes one i64 into `f`.
    let _ = unsafe { QueryPerformanceFrequency(&mut f) };
    ticks as f64 * 1000.0 / f.max(1) as f64
}

/// Starts P1 and returns once Alt+Space is registered and the tray icon is shown ("tray-ready").
/// `on_hotkey` runs on P1 for every Alt+Space; `on_quit` runs on P1 after the Owner chose Quit.
pub fn start(
    on_hotkey: impl Fn() + Send + Sync + 'static,
    on_quit: impl Fn() + Send + Sync + 'static,
) -> Result<()> {
    let callbacks = Callbacks {
        on_hotkey: Box::new(on_hotkey),
        on_quit: Box::new(on_quit),
    };
    if CALLBACKS.set(callbacks).is_err() {
        return Err(Error::from(E_FAIL));
    }
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || match init() {
        Ok(()) => {
            let _ = tx.send(Ok(()));
            message_loop();
        }
        Err(e) => {
            let _ = tx.send(Err(e));
        }
    });
    rx.recv().unwrap_or_else(|_| Err(Error::from(E_FAIL)))
}

fn init() -> Result<()> {
    // SAFETY: registers a class and creates a message-only window on this thread; the wide strings are static.
    unsafe {
        let instance: HINSTANCE = GetModuleHandleW(None)?.into();
        let class = WNDCLASSW {
            lpfnWndProc: Some(wndproc),
            hInstance: instance,
            lpszClassName: w!("NaobiCommandPlatform"),
            ..Default::default()
        };
        RegisterClassW(&class);
        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            w!("NaobiCommandPlatform"),
            w!(""),
            WINDOW_STYLE(0),
            0,
            0,
            0,
            0,
            Some(HWND_MESSAGE),
            None,
            Some(instance),
            None,
        )?;
        RegisterHotKey(Some(hwnd), 1, MOD_ALT | MOD_NOREPEAT, VK_SPACE.0 as u32)?;
        tray::add(hwnd)
    }
}

fn message_loop() {
    let mut msg = MSG::default();
    // SAFETY: standard message pump on the thread that created the window.
    while unsafe { GetMessageW(&mut msg, None, 0, 0) }.0 > 0 {
        // SAFETY: `msg` was filled by GetMessageW.
        unsafe { DispatchMessageW(&msg) };
    }
}

unsafe extern "system" fn wndproc(h: HWND, m: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    match m {
        WM_HOTKEY => {
            if let Some(c) = CALLBACKS.get() {
                (c.on_hotkey)();
            }
            LRESULT(0)
        }
        tray::WM_TRAY => {
            let event = (l.0 as u32) & 0xFFFF;
            if (event == WM_RBUTTONUP || event == WM_CONTEXTMENU) && tray::menu_chose_quit(h) {
                tray::remove(h);
                if let Some(c) = CALLBACKS.get() {
                    (c.on_quit)();
                }
            }
            LRESULT(0)
        }
        // SAFETY: forwards the unchanged message to the default procedure.
        _ => unsafe { DefWindowProcW(h, m, w, l) },
    }
}
