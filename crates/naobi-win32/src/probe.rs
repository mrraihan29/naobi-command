//! Screen probe and measurement conditions for naobi-perf (R-006, RK-15). Compiled only with feature `perf`.
//! Everything here reads the DWM-composed desktop; nothing touches the Launcher itself.
use std::ffi::c_void;
use windows::core::{w, HSTRING};
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, CreateSolidBrush, DeleteDC, DeleteObject,
    EnumDisplaySettingsW, GetDC, GetDIBits, GetMonitorInfoW, GetPixel, MonitorFromRect, ReleaseDC,
    SelectObject, UpdateWindow, BITMAPINFO, BITMAPINFOHEADER, CAPTUREBLT, DEVMODEW, DIB_RGB_COLORS,
    ENUM_CURRENT_SETTINGS, HBITMAP, HDC, MONITORINFOEXW, MONITOR_DEFAULTTONEAREST, SRCCOPY,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
use windows::Win32::System::Threading::GetSystemTimes;
use windows::Win32::UI::HiDpi::{
    SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    MapVirtualKeyW, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP,
    MAPVK_VK_TO_VSC, VIRTUAL_KEY,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, FindWindowW, GetWindowRect,
    PeekMessageW, RegisterClassW, SetWindowPos, ShowWindow, HWND_TOPMOST, MSG, PM_REMOVE,
    SWP_NOACTIVATE, SW_SHOWNA, WNDCLASSW, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST,
    WS_EX_TRANSPARENT, WS_POPUP,
};

/// Contrasting color of the helper window under the Launcher, as GDI `COLORREF` (#FF00FF).
pub const BACKDROP: u32 = 0x00FF_00FF;

/// `COLORREF` (0x00BBGGRR) of an `#RRGGBB` color.
pub const fn colorref(r: u8, g: u8, b: u8) -> u32 {
    r as u32 | (g as u32) << 8 | (b as u32) << 16
}

/// The harness reads physical pixels of other processes' windows, so it must be per-monitor DPI aware.
pub fn dpi_aware() {
    // SAFETY: plain value argument; failure (already set) is harmless.
    let _ = unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
}

/// Screen rectangle `(x, y, w, h)` of the top-level window titled `title` (valid while it is cloaked).
pub fn window_rect(title: &str) -> Option<(i32, i32, i32, i32)> {
    let mut r = RECT::default();
    // SAFETY: the HSTRING outlives the call; GetWindowRect writes one RECT.
    unsafe {
        let h = FindWindowW(None, &HSTRING::from(title)).ok()?;
        GetWindowRect(h, &mut r).ok()?;
    }
    Some((r.left, r.top, r.right - r.left, r.bottom - r.top))
}

/// Current refresh rate in Hz of the monitor that holds `(x, y, w, h)`; 0 when unknown.
pub fn refresh_hz(rect: (i32, i32, i32, i32)) -> u32 {
    let r = RECT {
        left: rect.0,
        top: rect.1,
        right: rect.0 + rect.2,
        bottom: rect.1 + rect.3,
    };
    let mut mi = MONITORINFOEXW::default();
    mi.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
    let mut dm = DEVMODEW {
        dmSize: std::mem::size_of::<DEVMODEW>() as u16,
        ..Default::default()
    };
    // SAFETY: `mi` and `dm` carry their true sizes and outlive the calls; the device name is NUL-terminated.
    unsafe {
        let mon = MonitorFromRect(&r, MONITOR_DEFAULTTONEAREST);
        if !GetMonitorInfoW(mon, &mut mi as *mut _ as *mut _).as_bool() {
            return 0;
        }
        let name = windows::core::PCWSTR(mi.szDevice.as_ptr());
        if !EnumDisplaySettingsW(name, ENUM_CURRENT_SETTINGS, &mut dm).as_bool() {
            return 0;
        }
    }
    dm.dmDisplayFrequency
}

pub fn on_ac_power() -> bool {
    let mut s = SYSTEM_POWER_STATUS::default();
    // SAFETY: writes one SYSTEM_POWER_STATUS into `s`.
    unsafe { GetSystemPowerStatus(&mut s) }.is_ok() && s.ACLineStatus == 1
}

/// `(busy, total)` system CPU time in 100 ns units since boot, summed over all logical CPUs.
pub fn system_cpu() -> (u64, u64) {
    let (mut idle, mut kernel, mut user) = Default::default();
    // SAFETY: writes three FILETIME values.
    let _ = unsafe { GetSystemTimes(Some(&mut idle), Some(&mut kernel), Some(&mut user)) };
    let t = |f: windows::Win32::Foundation::FILETIME| {
        (u64::from(f.dwHighDateTime) << 32) | u64::from(f.dwLowDateTime)
    };
    // Kernel time already contains idle time.
    let total = t(kernel) + t(user);
    (total.saturating_sub(t(idle)), total)
}

/// FNV-1a over bytes: the "did the pixels change" fingerprint.
pub fn fnv(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325u64, |h, &b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    })
}

/// Reads the composed desktop: single pixels, and a hashed region (DWM-composed `BitBlt` from the screen DC).
pub struct Screen {
    sdc: HDC,
    mdc: HDC,
    bmp: HBITMAP,
    buf: Vec<u8>,
    size: (i32, i32),
}

impl Screen {
    /// `size` is the largest region `region_hash` will read.
    pub fn new(size: (i32, i32)) -> Self {
        // SAFETY: GDI object creation on the screen DC; every object is released in Drop.
        unsafe {
            let sdc = GetDC(None);
            let mdc = CreateCompatibleDC(Some(sdc));
            let bmp = CreateCompatibleBitmap(sdc, size.0, size.1);
            SelectObject(mdc, bmp.into());
            Screen {
                sdc,
                mdc,
                bmp,
                buf: vec![0; (size.0 * size.1 * 4) as usize],
                size,
            }
        }
    }

    /// `COLORREF` of one composed pixel (0xFFFFFFFF when off screen).
    pub fn pixel(&self, x: i32, y: i32) -> u32 {
        // SAFETY: reads one pixel from a DC owned by self.
        unsafe { GetPixel(self.sdc, x, y) }.0
    }

    /// Hash of the `size` region whose top-left corner is `(x, y)`.
    pub fn region_hash(&mut self, x: i32, y: i32) -> u64 {
        let mut bi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: self.size.0,
                biHeight: -self.size.1,
                biPlanes: 1,
                biBitCount: 32,
                ..Default::default()
            },
            ..Default::default()
        };
        // SAFETY: the DCs and bitmap belong to self; `buf` holds width x height x 4 bytes.
        unsafe {
            let _ = BitBlt(
                self.mdc,
                0,
                0,
                self.size.0,
                self.size.1,
                Some(self.sdc),
                x,
                y,
                SRCCOPY | CAPTUREBLT,
            );
            GetDIBits(
                self.mdc,
                self.bmp,
                0,
                self.size.1 as u32,
                Some(self.buf.as_mut_ptr() as *mut c_void),
                &mut bi,
                DIB_RGB_COLORS,
            );
        }
        fnv(&self.buf)
    }
}

impl Drop for Screen {
    fn drop(&mut self) {
        // SAFETY: releases the objects created in `new`, once.
        unsafe {
            let _ = DeleteObject(self.bmp.into());
            let _ = DeleteDC(self.mdc);
            ReleaseDC(None, self.sdc);
        }
    }
}

unsafe extern "system" fn backdrop_proc(h: HWND, m: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    // SAFETY: forwards the unchanged message; the class brush paints the background.
    unsafe { DefWindowProcW(h, m, w, l) }
}

/// A solid #FF00FF topmost window under the Launcher rectangle: the contrasting backdrop of every run.
pub struct Backdrop(HWND);

impl Backdrop {
    pub fn new(rect: (i32, i32, i32, i32)) -> windows::core::Result<Self> {
        // SAFETY: registers a class and creates one window on this thread; strings are static.
        unsafe {
            let inst = GetModuleHandleW(None)?.into();
            RegisterClassW(&WNDCLASSW {
                lpfnWndProc: Some(backdrop_proc),
                hInstance: inst,
                lpszClassName: w!("NaobiPerfBackdrop"),
                hbrBackground: CreateSolidBrush(COLORREF(BACKDROP)),
                ..Default::default()
            });
            let h = CreateWindowExW(
                WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TRANSPARENT,
                w!("NaobiPerfBackdrop"),
                w!(""),
                WS_POPUP,
                rect.0,
                rect.1,
                rect.2,
                rect.3,
                None,
                None,
                Some(inst),
                None,
            )?;
            let _ = SetWindowPos(
                h,
                Some(HWND_TOPMOST),
                rect.0,
                rect.1,
                rect.2,
                rect.3,
                SWP_NOACTIVATE,
            );
            let _ = ShowWindow(h, SW_SHOWNA);
            let _ = UpdateWindow(h);
            Ok(Backdrop(h))
        }
    }

    /// Dispatches pending messages so the window stays painted.
    pub fn pump(&self) {
        let mut msg = MSG::default();
        // SAFETY: standard non-blocking pump on the thread that created the window.
        unsafe {
            while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                DispatchMessageW(&msg);
            }
        }
    }
}

impl Drop for Backdrop {
    fn drop(&mut self) {
        // SAFETY: destroys the window created in `new`, on its own thread.
        let _ = unsafe { DestroyWindow(self.0) };
    }
}

/// Presses and releases one key (the Launcher has the foreground). Sent with its scan code so winit sees a real key.
pub fn press(vk: u16) {
    let vk = VIRTUAL_KEY(vk);
    // SAFETY: plain value arguments.
    let scan = unsafe { MapVirtualKeyW(u32::from(vk.0), MAPVK_VK_TO_VSC) } as u16;
    let ev = |up: bool| INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                wScan: scan,
                dwFlags: if up {
                    KEYEVENTF_KEYUP
                } else {
                    Default::default()
                },
                ..Default::default()
            },
        },
    };
    let seq = [ev(false), ev(true)];
    // SAFETY: SendInput reads `seq`, fully initialised INPUT values, with their true size.
    unsafe { SendInput(&seq, std::mem::size_of::<INPUT>() as i32) };
}
