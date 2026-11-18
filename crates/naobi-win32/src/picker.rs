//! The color picker's screen side (NCMD-24, R-017, D-023, T-015, T-021): a GDI snapshot of the virtual
//! screen and one borderless topmost overlay per monitor that draws it with a magnifier and the HEX.
//! Everything here runs on the picker thread the app starts for one pick; the platform thread is not
//! involved. The snapshot is memory only and is wiped when it is dropped.
use naobi_core::tokens::{Palette, Rgba};
use naobi_picker::{flip, Rgb, ScreenCapture};
use std::cell::RefCell;
use std::ffi::c_void;
use windows::core::w;
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, BitBlt, CreateCompatibleDC, CreateDIBSection, CreateEllipticRgn, CreateFontW,
    CreatePen, CreateSolidBrush, DeleteDC, DeleteObject, Ellipse, EndPaint, EnumDisplayMonitors,
    GetDC, GetStockObject, InvalidateRect, ReleaseDC, SelectClipRgn, SelectObject, SetBkMode,
    SetStretchBltMode, SetTextColor, SetViewportOrgEx, StretchBlt, TextOutW, BITMAPINFO,
    BITMAPINFOHEADER, CAPTUREBLT, CLIP_DEFAULT_PRECIS, COLORONCOLOR, DEFAULT_CHARSET,
    DEFAULT_QUALITY, DIB_RGB_COLORS, FF_DONTCARE, FW_MEDIUM, HBITMAP, HBRUSH, HDC, HGDIOBJ,
    HMONITOR, NULL_BRUSH, OUT_DEFAULT_PRECIS, PAINTSTRUCT, PS_SOLID, SRCCOPY, TRANSPARENT,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::Input::KeyboardAndMouse::{SetFocus, VK_ESCAPE};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetCursorPos, GetMessageW,
    GetSystemMetrics, KillTimer, LoadCursorW, PostQuitMessage, RegisterClassW, SetForegroundWindow,
    SetTimer, ShowWindow, TranslateMessage, IDC_CROSS, MSG, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN,
    SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN, SW_SHOW, WM_ACTIVATE, WM_CLOSE, WM_DESTROY,
    WM_ERASEBKGND, WM_KEYDOWN, WM_LBUTTONDOWN, WM_MOUSEMOVE, WM_PAINT, WM_RBUTTONDOWN, WM_TIMER,
    WNDCLASSW, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
};

/// The overlays' window title, for tests and assistive tools.
pub const OVERLAY_TITLE: &str = "Naobi Color Picker";

/// Source pixels across the magnifier, and the size of one of them on screen.
const CELLS: i32 = 11;
const CELL: i32 = 12;
/// Gap between the ring and the readout, and the readout's size (type.readout 14/20 plus padding).
const GAP: i32 = 8;
const READOUT_W: i32 = 104;
const READOUT_H: i32 = 28;
/// "Copied #914A3B" stays this long before the overlay closes.
const CONFIRM_MS: u32 = 120;
const CONFIRM_TIMER: usize = 1;

/// A copy of the virtual screen: a memory DC with a 32-bit top-down DIB section selected into it.
pub struct Snapshot {
    dc: HDC,
    bitmap: HBITMAP,
    previous: HGDIOBJ,
    bits: *mut u8,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
}

impl Snapshot {
    /// The color at screen pixel `(x, y)`, or None outside the snapshot.
    fn pixel(&self, x: i32, y: i32) -> Option<Rgb> {
        let (lx, ly) = (x - self.x, y - self.y);
        if lx < 0 || ly < 0 || lx >= self.w || ly >= self.h {
            return None;
        }
        let at = (ly as usize * self.w as usize + lx as usize) * 4;
        // SAFETY: `at + 3` is inside the `w * h * 4` bytes of the DIB section, which lives as long
        // as `self`.
        let px = unsafe { std::slice::from_raw_parts(self.bits.add(at), 4) };
        Some(Rgb {
            r: px[2],
            g: px[1],
            b: px[0],
        })
    }
}

impl Drop for Snapshot {
    fn drop(&mut self) {
        // SAFETY: the bytes are the DIB section's own `w * h * 4`; the GDI objects were made in
        // `capture` and are released here exactly once, the bitmap after it is deselected.
        unsafe {
            std::ptr::write_bytes(self.bits, 0, self.w as usize * self.h as usize * 4);
            SelectObject(self.dc, self.previous);
            let _ = DeleteObject(self.bitmap.into());
            let _ = DeleteDC(self.dc);
        }
    }
}

/// The GDI snapshot, as the port the picker crate defines.
pub struct ScreenGrab;

impl ScreenCapture for ScreenGrab {
    type Frame = Snapshot;

    fn capture(&mut self) -> Option<Snapshot> {
        // The Launcher hides by cloak, which DWM applies at its next frame: wait for it, so the
        // snapshot never shows the window the command came from.
        // SAFETY: no arguments.
        let _ = unsafe { windows::Win32::Graphics::Dwm::DwmFlush() };
        std::thread::sleep(std::time::Duration::from_millis(60));
        // SAFETY: plain metric queries.
        let (x, y, w, h) = unsafe {
            (
                GetSystemMetrics(SM_XVIRTUALSCREEN),
                GetSystemMetrics(SM_YVIRTUALSCREEN),
                GetSystemMetrics(SM_CXVIRTUALSCREEN),
                GetSystemMetrics(SM_CYVIRTUALSCREEN),
            )
        };
        if w <= 0 || h <= 0 {
            return None;
        }
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w,
                biHeight: -h,
                biPlanes: 1,
                biBitCount: 32,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits: *mut c_void = std::ptr::null_mut();
        // SAFETY: `info` and `bits` live for the calls; the screen DC is released before the end and
        // the memory DC and DIB section become the Snapshot, which frees them on drop.
        unsafe {
            let screen = GetDC(None);
            let dc = CreateCompatibleDC(Some(screen));
            let bitmap = CreateDIBSection(Some(dc), &info, DIB_RGB_COLORS, &mut bits, None, 0);
            let Ok(bitmap) = bitmap else {
                let _ = DeleteDC(dc);
                ReleaseDC(None, screen);
                return None;
            };
            let previous = SelectObject(dc, bitmap.into());
            let copied = BitBlt(dc, 0, 0, w, h, Some(screen), x, y, SRCCOPY | CAPTUREBLT);
            ReleaseDC(None, screen);
            let snap = Snapshot {
                dc,
                bitmap,
                previous,
                bits: bits.cast(),
                x,
                y,
                w,
                h,
            };
            // A failed copy is dropped (and wiped) here.
            copied.ok().map(|()| snap)
        }
    }
}

struct Win {
    hwnd: isize,
    /// The monitor's rectangle in screen pixels.
    rect: RECT,
}

struct State {
    snap: *const Snapshot,
    pal: Palette,
    wins: Vec<Win>,
    cursor: POINT,
    picked: Option<Rgb>,
    confirming: bool,
}

thread_local! {
    /// The pick in progress on this thread. Borrowed briefly and never across a call that can send
    /// a message back to the window procedure.
    static PICK: RefCell<Option<State>> = const { RefCell::new(None) };
}

fn colorref(c: Rgba) -> COLORREF {
    COLORREF(u32::from(c.r) | u32::from(c.g) << 8 | u32::from(c.b) << 16)
}

unsafe extern "system" fn monitor_rect(
    _: HMONITOR,
    _: HDC,
    r: *mut RECT,
    out: LPARAM,
) -> windows::core::BOOL {
    // SAFETY: `out` is the address of the Vec `monitors` passed by `overlay`, alive during the
    // enumeration; `r` is the monitor rectangle Windows hands in.
    unsafe {
        let list = &mut *(out.0 as *mut Vec<RECT>);
        list.push(*r);
    }
    true.into()
}

fn monitors() -> Vec<RECT> {
    let mut list: Vec<RECT> = Vec::new();
    // SAFETY: the callback only pushes into `list`, which outlives the call.
    unsafe {
        let _ = EnumDisplayMonitors(
            None,
            None,
            Some(monitor_rect),
            LPARAM(&mut list as *mut _ as isize),
        );
    }
    list
}

/// Shows the overlays over `snap` and runs their message loop until a click, Esc, a right click or
/// a lost focus. Returns the picked color. Blocks the calling (picker) thread.
pub fn overlay(snap: &Snapshot, palette: &Palette) -> Option<Rgb> {
    let rects = monitors();
    if rects.is_empty() {
        return None;
    }
    let pal = *palette;
    PICK.with(|p| {
        *p.borrow_mut() = Some(State {
            snap,
            pal,
            wins: Vec::new(),
            cursor: POINT::default(),
            picked: None,
            confirming: false,
        });
    });
    let made = create_windows(&rects);
    let picked = if made {
        // SAFETY: the loop belongs to this thread, which created every window it dispatches for.
        unsafe {
            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).0 > 0 {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        PICK.with(|p| p.borrow().as_ref().and_then(|s| s.picked))
    } else {
        None
    };
    let wins: Vec<isize> = PICK.with(|p| {
        p.borrow_mut()
            .take()
            .map(|s| s.wins.iter().map(|w| w.hwnd).collect())
            .unwrap_or_default()
    });
    for h in wins {
        // SAFETY: a window this thread created.
        unsafe {
            let _ = DestroyWindow(HWND(h as *mut c_void));
        }
    }
    picked
}

fn create_windows(rects: &[RECT]) -> bool {
    // SAFETY: a module handle query, a class registration with static names and window creation on
    // this thread; the windows own no data (their state is this thread's `PICK`).
    unsafe {
        let Ok(instance) = GetModuleHandleW(None) else {
            return false;
        };
        let class = WNDCLASSW {
            lpfnWndProc: Some(proc),
            hInstance: instance.into(),
            lpszClassName: w!("NaobiColorPicker"),
            hCursor: LoadCursorW(None, IDC_CROSS).unwrap_or_default(),
            hbrBackground: HBRUSH::default(),
            ..Default::default()
        };
        // A second registration (the next pick) fails harmlessly: the class already exists.
        RegisterClassW(&class);
        let mut first = None;
        for r in rects {
            let Ok(h) = CreateWindowExW(
                WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
                w!("NaobiColorPicker"),
                w!("Naobi Color Picker"),
                WS_POPUP,
                r.left,
                r.top,
                r.right - r.left,
                r.bottom - r.top,
                None,
                None,
                Some(instance.into()),
                None,
            ) else {
                continue;
            };
            PICK.with(|p| {
                if let Some(s) = p.borrow_mut().as_mut() {
                    s.wins.push(Win {
                        hwnd: h.0 as isize,
                        rect: *r,
                    });
                }
            });
            first.get_or_insert(h);
        }
        let mut at = POINT::default();
        let _ = GetCursorPos(&mut at);
        PICK.with(|p| {
            if let Some(s) = p.borrow_mut().as_mut() {
                s.cursor = at;
            }
        });
        let Some(first) = first else { return false };
        let wins: Vec<isize> = PICK.with(|p| {
            p.borrow()
                .as_ref()
                .map(|s| s.wins.iter().map(|w| w.hwnd).collect())
                .unwrap_or_default()
        });
        for h in &wins {
            let _ = ShowWindow(HWND(*h as *mut c_void), SW_SHOW);
        }
        // The overlay under the cursor takes the keyboard so Esc cancels.
        let under = PICK.with(|p| {
            p.borrow().as_ref().and_then(|s| {
                s.wins
                    .iter()
                    .find(|w| contains(&w.rect, at))
                    .map(|w| w.hwnd)
            })
        });
        let focus = under.map_or(first, |h| HWND(h as *mut c_void));
        let _ = SetForegroundWindow(focus);
        let _ = SetFocus(Some(focus));
        true
    }
}

fn contains(r: &RECT, p: POINT) -> bool {
    p.x >= r.left && p.x < r.right && p.y >= r.top && p.y < r.bottom
}

fn cancel() {
    // SAFETY: ends this thread's message loop.
    unsafe { PostQuitMessage(0) };
}

/// Repaints the area the magnifier and the readout cover around `at`, on every overlay.
fn invalidate_around(at: POINT) {
    let wins: Vec<(isize, RECT)> = PICK.with(|p| {
        p.borrow()
            .as_ref()
            .map(|s| s.wins.iter().map(|w| (w.hwnd, w.rect)).collect())
            .unwrap_or_default()
    });
    for (h, r) in wins {
        // SAFETY: a window of this thread.
        let dpi = unsafe { GetDpiForWindow(HWND(h as *mut c_void)) }.max(96);
        let reach = ((f64::from((CELLS * CELL) / 2 + GAP + READOUT_W) * f64::from(dpi) / 96.0)
            .ceil() as i32)
            + 16;
        let area = RECT {
            left: at.x - r.left - reach,
            top: at.y - r.top - reach,
            right: at.x - r.left + reach,
            bottom: at.y - r.top + reach,
        };
        // SAFETY: a window of this thread and a rectangle that lives for the call.
        unsafe {
            let _ = InvalidateRect(Some(HWND(h as *mut c_void)), Some(&area), false);
        }
    }
}

unsafe extern "system" fn proc(h: HWND, m: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match m {
        WM_ERASEBKGND => return LRESULT(1),
        WM_PAINT => {
            paint(h);
            return LRESULT(0);
        }
        WM_MOUSEMOVE => {
            let mut now = POINT::default();
            // SAFETY: writes one POINT.
            let _ = unsafe { GetCursorPos(&mut now) };
            let before = PICK.with(|p| {
                let mut p = p.borrow_mut();
                let s = p.as_mut()?;
                (!s.confirming).then(|| std::mem::replace(&mut s.cursor, now))
            });
            if let Some(before) = before {
                invalidate_around(before);
                invalidate_around(now);
            }
            return LRESULT(0);
        }
        WM_LBUTTONDOWN => {
            click(h);
            return LRESULT(0);
        }
        WM_CLOSE | WM_DESTROY | WM_RBUTTONDOWN => {
            cancel();
            return LRESULT(0);
        }
        WM_KEYDOWN if wp.0 == usize::from(VK_ESCAPE.0) => {
            cancel();
            return LRESULT(0);
        }
        WM_ACTIVATE if (wp.0 & 0xFFFF) == 0 => {
            // Focus went somewhere that is not one of the overlays: the pick is over.
            let other = lp.0;
            let ours = PICK
                .try_with(|p| {
                    p.try_borrow()
                        .ok()
                        .and_then(|p| p.as_ref().map(|s| s.wins.iter().any(|w| w.hwnd == other)))
                })
                .ok()
                .flatten();
            if ours == Some(false) {
                cancel();
            }
            return LRESULT(0);
        }
        WM_TIMER if wp.0 == CONFIRM_TIMER => {
            // SAFETY: the timer of this window.
            let _ = unsafe { KillTimer(Some(h), CONFIRM_TIMER) };
            cancel();
            return LRESULT(0);
        }
        _ => {}
    }
    // SAFETY: forwards the message Windows delivered, unchanged.
    unsafe { DefWindowProcW(h, m, wp, lp) }
}

/// The click: the pixel under the cursor is the pick; "Copied #HEX" shows for a moment, then the
/// overlay closes.
fn click(h: HWND) {
    let mut now = POINT::default();
    // SAFETY: writes one POINT.
    let _ = unsafe { GetCursorPos(&mut now) };
    let hit = PICK.with(|p| {
        let mut p = p.borrow_mut();
        let s = p.as_mut()?;
        if s.confirming {
            return None;
        }
        // SAFETY: `snap` is the Snapshot `overlay` borrows for the whole loop.
        let color = unsafe { (*s.snap).pixel(now.x, now.y) }?;
        s.cursor = now;
        s.picked = Some(color);
        s.confirming = true;
        Some(())
    });
    if hit.is_some() {
        invalidate_around(now);
        // SAFETY: a timer of this window, fired on this thread.
        unsafe { SetTimer(Some(h), CONFIRM_TIMER, CONFIRM_MS, None) };
    }
}

fn paint(h: HWND) {
    let mut ps = PAINTSTRUCT::default();
    // SAFETY: the paint cycle of a window of this thread: BeginPaint and EndPaint pair up, and the
    // drawing in between sends no message to this window, so the shared borrow is never in the way.
    unsafe {
        let dc = BeginPaint(h, &mut ps);
        PICK.with(|p| {
            if let Ok(state) = p.try_borrow() {
                if let Some(s) = state.as_ref() {
                    if let Some(win) = s.wins.iter().find(|w| w.hwnd == h.0 as isize) {
                        draw(dc, h, s, win, ps.rcPaint);
                    }
                }
            }
        });
        let _ = EndPaint(h, &ps);
    }
}

/// Draws the area `area` (client coordinates) off screen and copies it over in one go.
///
/// # Safety
/// `dc` is the paint DC of `h`; `s.snap` points at the live snapshot.
unsafe fn draw(dc: HDC, h: HWND, s: &State, win: &Win, area: RECT) {
    // SAFETY: see the function's contract; every GDI object made here is deleted before returning.
    unsafe {
        let snap = &*s.snap;
        let (aw, ah) = (area.right - area.left, area.bottom - area.top);
        if aw <= 0 || ah <= 0 {
            return;
        }
        let buf = CreateCompatibleDC(Some(dc));
        let bmp = windows::Win32::Graphics::Gdi::CreateCompatibleBitmap(dc, aw, ah);
        let old = SelectObject(buf, bmp.into());
        // The frozen screen under the area.
        let _ = BitBlt(
            buf,
            0,
            0,
            aw,
            ah,
            Some(snap.dc),
            win.rect.left - snap.x + area.left,
            win.rect.top - snap.y + area.top,
            SRCCOPY,
        );
        // From here on, client coordinates.
        let _ = SetViewportOrgEx(buf, -area.left, -area.top, None);
        let k = f64::from(GetDpiForWindow(h).max(96)) / 96.0;
        let at = POINT {
            x: s.cursor.x - win.rect.left,
            y: s.cursor.y - win.rect.top,
        };
        let size = (
            win.rect.right - win.rect.left,
            win.rect.bottom - win.rect.top,
        );
        magnifier(buf, snap, s, at, (area.left, area.top), size, k);
        let _ = SetViewportOrgEx(buf, 0, 0, None);
        let _ = BitBlt(dc, area.left, area.top, aw, ah, Some(buf), 0, 0, SRCCOPY);
        SelectObject(buf, old);
        let _ = DeleteObject(bmp.into());
        let _ = DeleteDC(buf);
    }
}

/// The ring around the cursor with the zoomed pixels, and the HEX box beside it.
///
/// # Safety
/// `buf` is a valid memory DC; `snap` is live.
unsafe fn magnifier(
    buf: HDC,
    snap: &Snapshot,
    s: &State,
    at: POINT,
    origin: (i32, i32),
    size: (i32, i32),
    k: f64,
) {
    let sc = |v: i32| (f64::from(v) * k).round() as i32;
    let cell = sc(CELL);
    let radius = cell * CELLS / 2;
    // SAFETY: see the function's contract; objects made here are deleted at the end.
    unsafe {
        // Zoomed pixels, cut to the ring.
        // A clip region is in device units: the viewport origin does not move it.
        let ring = CreateEllipticRgn(
            at.x - radius - origin.0,
            at.y - radius - origin.1,
            at.x + radius - origin.0,
            at.y + radius - origin.1,
        );
        SelectClipRgn(buf, Some(ring));
        SetStretchBltMode(buf, COLORONCOLOR);
        let (gx, gy) = (s.cursor.x, s.cursor.y);
        let half = CELLS / 2;
        let _ = StretchBlt(
            buf,
            at.x - radius,
            at.y - radius,
            radius * 2,
            radius * 2,
            Some(snap.dc),
            gx - snap.x - half,
            gy - snap.y - half,
            CELLS,
            CELLS,
            SRCCOPY,
        );
        SelectClipRgn(buf, None);
        let _ = DeleteObject(ring.into());
        // The ring (1 px line.hair) and the center cell, outlined twice so it shows on any color.
        let hair = CreatePen(PS_SOLID, 1, colorref(s.pal.hair));
        let old_pen = SelectObject(buf, hair.into());
        let old_brush = SelectObject(buf, GetStockObject(NULL_BRUSH));
        let _ = Ellipse(
            buf,
            at.x - radius,
            at.y - radius,
            at.x + radius,
            at.y + radius,
        );
        for (inset, c) in [(0, s.pal.text_primary), (1, s.pal.window)] {
            let pen = CreatePen(PS_SOLID, 1, colorref(c));
            SelectObject(buf, pen.into());
            let _ = windows::Win32::Graphics::Gdi::Rectangle(
                buf,
                at.x - cell / 2 + inset,
                at.y - cell / 2 + inset,
                at.x + cell / 2 + 1 - inset,
                at.y + cell / 2 + 1 - inset,
            );
            SelectObject(buf, hair.into());
            let _ = DeleteObject(pen.into());
        }
        // The readout on surface.well, flipped to the other side near the screen edge.
        let (rw, rh) = (sc(READOUT_W), sc(READOUT_H));
        let x = flip(at.x, rw, size.0, radius + sc(GAP));
        let y = (at.y - rh / 2).clamp(0, (size.1 - rh).max(0));
        let well = CreateSolidBrush(colorref(s.pal.well));
        SelectObject(buf, well.into());
        let _ = windows::Win32::Graphics::Gdi::Rectangle(buf, x, y, x + rw, y + rh);
        SelectObject(buf, old_brush);
        SelectObject(buf, old_pen);
        let _ = DeleteObject(well.into());
        let _ = DeleteObject(hair.into());
        // The text: the HEX under the cursor, or the confirmation after the click.
        let color = if s.confirming {
            s.picked
        } else {
            snap.pixel(gx, gy)
        };
        if let Some(c) = color {
            let text = if s.confirming {
                format!("Copied {}", c.hex())
            } else {
                c.hex()
            };
            // type.readout is JetBrains Mono, which only the Launcher bundles; Consolas is the
            // closest face every Windows 11 has.
            let font = CreateFontW(
                -sc(14),
                0,
                0,
                0,
                FW_MEDIUM.0 as i32,
                0,
                0,
                0,
                DEFAULT_CHARSET,
                OUT_DEFAULT_PRECIS,
                CLIP_DEFAULT_PRECIS,
                DEFAULT_QUALITY,
                FF_DONTCARE.0 as u32,
                w!("Consolas"),
            );
            let old_font = SelectObject(buf, font.into());
            SetBkMode(buf, TRANSPARENT);
            SetTextColor(buf, colorref(s.pal.text_primary));
            let wide: Vec<u16> = text.encode_utf16().collect();
            let _ = TextOutW(buf, x + sc(10), y + sc(4), &wide);
            SelectObject(buf, old_font);
            let _ = DeleteObject(font.into());
        }
    }
}
