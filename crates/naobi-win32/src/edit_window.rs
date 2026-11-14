//! A top-level window with an edit control, for tier 2 tests that paste into "another app". It runs on
//! its own thread with its own message loop, as a real program's window would. Compiled only with
//! feature `perf`.
use std::sync::mpsc;
use std::thread::JoinHandle;

use windows::core::w;
use windows::Win32::Foundation::{HWND, LPARAM, RECT, WPARAM};
use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_EXTENDED_FRAME_BOUNDS};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_MOUSE, MOUSEINPUT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DispatchMessageW, GetMessageW, GetWindowRect, GetWindowTextLengthW,
    GetWindowTextW, PostThreadMessageW, SetForegroundWindow, TranslateMessage, MSG,
    WINDOW_EX_STYLE, WINDOW_STYLE, WM_QUIT, WS_OVERLAPPEDWINDOW, WS_VISIBLE,
};

/// `ES_MULTILINE`: not part of the typed window styles.
const ES_MULTILINE: u32 = 0x0004;

/// Makes this (test) process per-monitor DPI aware like the app, so the rectangles it reads are the
/// physical ones the app works with. Call it before the first window.
pub fn dpi_aware() {
    // SAFETY: a plain call with a constant; it fails harmlessly when the process already has one.
    let _ = unsafe {
        windows::Win32::UI::HiDpi::SetProcessDpiAwarenessContext(
            windows::Win32::UI::HiDpi::DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
        )
    };
}

pub struct EditWindow {
    hwnd: isize,
    thread_id: u32,
    thread: Option<JoinHandle<()>>,
}

impl EditWindow {
    pub fn open() -> Option<Self> {
        let (tx, rx) = mpsc::channel();
        let thread = std::thread::spawn(move || {
            // SAFETY: a standard EDIT window created and pumped on this one thread; the message
            // structure lives for the loop.
            unsafe {
                let hwnd = CreateWindowExW(
                    WINDOW_EX_STYLE(0),
                    w!("EDIT"),
                    w!("naobi test paste target"),
                    WS_OVERLAPPEDWINDOW | WS_VISIBLE | WINDOW_STYLE(ES_MULTILINE),
                    200,
                    200,
                    500,
                    300,
                    None,
                    None,
                    None,
                    None,
                )
                .ok()
                .map(|h| h.0 as isize);
                let _ = tx.send((hwnd, GetCurrentThreadId()));
                let mut msg = MSG::default();
                while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            }
        });
        let (hwnd, thread_id) = rx.recv().ok()?;
        Some(Self {
            hwnd: hwnd?,
            thread_id,
            thread: Some(thread),
        })
    }

    pub fn hwnd(&self) -> isize {
        self.hwnd
    }

    fn handle(&self) -> HWND {
        HWND(self.hwnd as *mut core::ffi::c_void)
    }

    /// Takes the foreground (an empty mouse event first, so Windows allows it).
    pub fn focus(&self) {
        let empty = INPUT {
            r#type: INPUT_MOUSE,
            Anonymous: INPUT_0 {
                mi: MOUSEINPUT::default(),
            },
        };
        // SAFETY: plain values; the window belongs to this process.
        unsafe {
            SendInput(&[empty], std::mem::size_of::<INPUT>() as i32);
            let _ = SetForegroundWindow(self.handle());
        }
    }

    /// The window rectangle and the visible frame (without the invisible resize border), in physical
    /// pixels.
    pub fn rects(&self) -> (naobi_winmgr::Rect, naobi_winmgr::Rect) {
        let (mut outer, mut frame) = (RECT::default(), RECT::default());
        // SAFETY: each call writes one RECT into a local of this frame.
        unsafe {
            let _ = GetWindowRect(self.handle(), &mut outer);
            let _ = DwmGetWindowAttribute(
                self.handle(),
                DWMWA_EXTENDED_FRAME_BOUNDS,
                &mut frame as *mut _ as *mut core::ffi::c_void,
                std::mem::size_of::<RECT>() as u32,
            );
        }
        let r = |r: RECT| naobi_winmgr::Rect::new(r.left, r.top, r.right, r.bottom);
        (r(outer), r(frame))
    }

    /// What the edit control holds now.
    pub fn text(&self) -> String {
        // SAFETY: reads the control's text into a buffer sized from its own length.
        unsafe {
            let len = GetWindowTextLengthW(self.handle()).max(0) as usize;
            let mut buf = vec![0u16; len + 1];
            let n = GetWindowTextW(self.handle(), &mut buf).max(0) as usize;
            String::from_utf16_lossy(&buf[..n.min(len)])
        }
    }
}

impl Drop for EditWindow {
    fn drop(&mut self) {
        // SAFETY: asks the window's own thread to end its message loop.
        let _ = unsafe { PostThreadMessageW(self.thread_id, WM_QUIT, WPARAM(0), LPARAM(0)) };
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}
