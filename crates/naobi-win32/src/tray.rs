//! Tray icon: three Awake glyphs with a Paused or Notice overlay, rendered at the small-icon size for
//! the current taskbar theme, and the right-click menu of R-022.
use std::ffi::c_void;
use std::sync::atomic::{AtomicIsize, Ordering};
use std::sync::Mutex;

use windows::core::{w, Result, BOOL, HSTRING, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, POINT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    CreateBitmap, CreateDIBSection, DeleteObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB,
    DIB_RGB_COLORS, RGBQUAD,
};
use windows::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD};
use windows::Win32::UI::Shell::{
    Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY,
    NOTIFYICONDATAW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreateIconIndirect, CreatePopupMenu, DestroyIcon, DestroyMenu, GetCursorPos,
    GetSystemMetrics, PostMessageW, SetForegroundWindow, TrackPopupMenu, HICON, ICONINFO,
    MF_CHECKED, MF_GRAYED, MF_SEPARATOR, MF_STRING, SM_CXSMICON, TPM_RETURNCMD, WM_APP, WM_NULL,
};

/// Message the shell posts to the platform window for tray mouse events.
pub const WM_TRAY: u32 = WM_APP + 1;

const ID_OPEN: usize = 1;
const ID_PAUSE: usize = 2;
const ID_AWAKE: usize = 3;
const ID_SETTINGS: usize = 4;
const ID_QUIT: usize = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Awake {
    Off,
    Indefinite,
    Timed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Overlay {
    None,
    Paused,
    Notice,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrayState {
    pub awake: Awake,
    pub overlay: Overlay,
    pub tooltip: String,
}

impl Default for TrayState {
    fn default() -> Self {
        Self {
            awake: Awake::Off,
            overlay: Overlay::None,
            tooltip: "Naobi Command".into(),
        }
    }
}

/// What the Owner picked in the tray menu. Awake status is a disabled line and cannot be picked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuChoice {
    Open,
    Pause,
    Settings,
    Quit,
}

static STATE: Mutex<Option<TrayState>> = Mutex::new(None);
/// The HICON on screen, as a plain value; destroyed when it is replaced.
static ICON: AtomicIsize = AtomicIsize::new(0);

fn current() -> TrayState {
    STATE
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
        .unwrap_or_default()
}

/// Adds the icon (or re-adds it after the taskbar was recreated).
pub fn add(hwnd: HWND) -> Result<()> {
    notify(hwnd, NIM_ADD, &current())
}

/// Replaces the icon, tooltip and glyph state; a no-op on screen if the icon is not there.
pub fn set_state(hwnd: HWND, state: TrayState) {
    *STATE.lock().unwrap_or_else(|e| e.into_inner()) = Some(state.clone());
    let _ = notify(hwnd, NIM_MODIFY, &state);
}

/// Re-renders with the current state, so a taskbar theme change shows in the glyph colour.
pub fn refresh(hwnd: HWND) {
    set_state(hwnd, current());
}

/// Shows or clears the paused overlay and keeps the rest of the state.
pub fn set_paused(hwnd: HWND, paused: bool) {
    let mut s = current();
    s.overlay = match (paused, s.overlay) {
        (true, _) => Overlay::Paused,
        (false, Overlay::Paused) => Overlay::None,
        (false, other) => other,
    };
    set_state(hwnd, s);
}

pub fn remove(hwnd: HWND) {
    let nid = NOTIFYICONDATAW {
        cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: 1,
        ..Default::default()
    };
    // SAFETY: the data struct outlives the call.
    let _ = unsafe { Shell_NotifyIconW(NIM_DELETE, &nid) };
}

fn notify(
    hwnd: HWND,
    action: windows::Win32::UI::Shell::NOTIFY_ICON_MESSAGE,
    state: &TrayState,
) -> Result<()> {
    let icon = render(state, taskbar_is_light())?;
    let mut nid = NOTIFYICONDATAW {
        cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: 1,
        uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP,
        uCallbackMessage: WM_TRAY,
        hIcon: icon,
        ..Default::default()
    };
    for (dst, src) in nid
        .szTip
        .iter_mut()
        .take(127)
        .zip(state.tooltip.encode_utf16())
    {
        *dst = src;
    }
    // SAFETY: the data struct and the icon outlive the call.
    if let Err(e) = unsafe { Shell_NotifyIconW(action, &nid) }.ok() {
        // SAFETY: the new icon was never shown, so nothing else uses it.
        let _ = unsafe { DestroyIcon(icon) };
        return Err(e);
    }
    let old = ICON.swap(icon.0 as isize, Ordering::SeqCst);
    if old != 0 {
        // SAFETY: `old` was stored by an earlier successful call and is no longer on screen.
        let _ = unsafe { DestroyIcon(HICON(old as *mut c_void)) };
    }
    Ok(())
}

/// One row of the tray menu; `menu_entries` is the R-022 list, checked by a test.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MenuEntry {
    Action {
        choice: MenuChoice,
        label: &'static str,
        checked: bool,
    },
    /// A text line that cannot be picked (Awake status).
    Disabled(String),
    Separator,
}

/// Open, Pause hotkeys (checked while paused), Awake status (disabled), Settings, Quit.
pub fn menu_entries(state: &TrayState) -> Vec<MenuEntry> {
    let awake = match state.awake {
        Awake::Off => "Awake: off",
        Awake::Indefinite => "Awake: on",
        Awake::Timed => "Awake: timed",
    };
    vec![
        MenuEntry::Action {
            choice: MenuChoice::Open,
            label: "Open",
            checked: false,
        },
        MenuEntry::Action {
            choice: MenuChoice::Pause,
            label: "Pause hotkeys",
            checked: state.overlay == Overlay::Paused,
        },
        MenuEntry::Disabled(awake.into()),
        MenuEntry::Separator,
        MenuEntry::Action {
            choice: MenuChoice::Settings,
            label: "Settings",
            checked: false,
        },
        MenuEntry::Action {
            choice: MenuChoice::Quit,
            label: "Quit",
            checked: false,
        },
    ]
}

fn item_id(choice: MenuChoice) -> usize {
    match choice {
        MenuChoice::Open => ID_OPEN,
        MenuChoice::Pause => ID_PAUSE,
        MenuChoice::Settings => ID_SETTINGS,
        MenuChoice::Quit => ID_QUIT,
    }
}

/// Shows the context menu at the cursor and returns the item the Owner picked, if any.
pub fn menu(hwnd: HWND) -> Option<MenuChoice> {
    let entries = menu_entries(&current());
    // SAFETY: the menu is created and destroyed here; TPM_RETURNCMD returns the id and posts nothing.
    unsafe {
        let menu = CreatePopupMenu().ok()?;
        for entry in &entries {
            match entry {
                MenuEntry::Action {
                    choice,
                    label,
                    checked,
                } => {
                    let flags = if *checked {
                        MF_STRING | MF_CHECKED
                    } else {
                        MF_STRING
                    };
                    let text = HSTRING::from(*label);
                    let _ = AppendMenuW(menu, flags, item_id(*choice), &text);
                }
                MenuEntry::Disabled(label) => {
                    let text = HSTRING::from(label.as_str());
                    let _ = AppendMenuW(menu, MF_GRAYED, ID_AWAKE, &text);
                }
                MenuEntry::Separator => {
                    let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
                }
            }
        }
        let mut pt = POINT::default();
        let _ = GetCursorPos(&mut pt);
        // The menu only closes on an outside click if our window is foreground first.
        let _ = SetForegroundWindow(hwnd);
        let chosen = TrackPopupMenu(menu, TPM_RETURNCMD, pt.x, pt.y, None, hwnd, None);
        let _ = PostMessageW(Some(hwnd), WM_NULL, WPARAM(0), LPARAM(0));
        let _ = DestroyMenu(menu);
        match chosen.0 as usize {
            ID_OPEN => Some(MenuChoice::Open),
            ID_PAUSE => Some(MenuChoice::Pause),
            ID_SETTINGS => Some(MenuChoice::Settings),
            ID_QUIT => Some(MenuChoice::Quit),
            _ => None,
        }
    }
}

/// Taskbar theme: `SystemUsesLightTheme` (1 when unreadable, which is the Windows default).
fn taskbar_is_light() -> bool {
    let mut v = 1u32;
    let mut size = 4u32;
    // SAFETY: RRF_RT_REG_DWORD writes at most 4 bytes into `v`, and `size` says so.
    let _ = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize"),
            w!("SystemUsesLightTheme"),
            RRF_RT_REG_DWORD,
            None,
            Some(&mut v as *mut _ as *mut c_void),
            Some(&mut size),
        )
    };
    v != 0
}

/// The glyph as BGRA pixels, `size` x `size`, top row first. Pure, so tests check the shapes.
pub fn glyph(size: u32, awake: Awake, overlay: Overlay, light_taskbar: bool) -> Vec<u8> {
    let n = size as f32;
    let c = n / 2.0;
    let r = c - 1.0;
    let half_ring = 0.75;
    let fg = if light_taskbar {
        [0x20u8, 0x20, 0x20]
    } else {
        [0xF0u8, 0xF0, 0xF0]
    };
    let b = n * 0.26; // badge radius
    let (bcx, bcy) = (n - b, n - b);
    let mut px = Vec::with_capacity((size * size * 4) as usize);
    for y in 0..size {
        for x in 0..size {
            let (dx, dy) = (x as f32 + 0.5 - c, y as f32 + 0.5 - c);
            let d = (dx * dx + dy * dy).sqrt();
            let ring = (d - r).abs() <= half_ring;
            let on = match awake {
                Awake::Off => ring,
                Awake::Indefinite => d <= r,
                Awake::Timed => ring || (d <= r - 2.0 && dx >= 0.0 && dy <= 0.0),
            };
            let (ax, ay) = (x as f32 + 0.5, y as f32 + 0.5);
            let in_badge = match overlay {
                Overlay::None => false,
                _ => (ax - bcx).powi(2) + (ay - bcy).powi(2) <= b * b,
            };
            let hole = overlay == Overlay::Paused
                && in_badge
                && ((ax - bcx).abs() >= b * 0.15 && (ax - bcx).abs() <= b * 0.5)
                && (ay - bcy).abs() <= b * 0.5;
            let alpha = if hole {
                0
            } else if on || in_badge {
                255
            } else {
                0
            };
            px.extend_from_slice(&[fg[2], fg[1], fg[0], alpha]);
        }
    }
    px
}

/// Renders the glyph for `state` into a new HICON at the small-icon size.
fn render(state: &TrayState, light_taskbar: bool) -> Result<HICON> {
    // SAFETY: a system metric query with no pointers.
    let size = unsafe { GetSystemMetrics(SM_CXSMICON) }.max(16) as u32;
    let pixels = glyph(size, state.awake, state.overlay, light_taskbar);
    let header = BITMAPINFOHEADER {
        biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
        biWidth: size as i32,
        biHeight: -(size as i32), // top-down rows
        biPlanes: 1,
        biBitCount: 32,
        biCompression: BI_RGB.0,
        ..Default::default()
    };
    let info = BITMAPINFO {
        bmiHeader: header,
        bmiColors: [RGBQUAD::default()],
    };
    let mut bits: *mut c_void = std::ptr::null_mut();
    // SAFETY: `info` lives for the call; `bits` receives the section's pixel pointer, valid until
    // DeleteObject; we copy exactly `pixels.len()` bytes into it, the section's size.
    unsafe {
        let color = CreateDIBSection(None, &info, DIB_RGB_COLORS, &mut bits, None, 0)?;
        std::ptr::copy_nonoverlapping(pixels.as_ptr(), bits.cast::<u8>(), pixels.len());
        // An all-zero mask: the per-pixel alpha decides what shows.
        let mask = CreateBitmap(size as i32, size as i32, 1, 1, None);
        let icon_info = ICONINFO {
            fIcon: BOOL(1),
            xHotspot: 0,
            yHotspot: 0,
            hbmMask: mask,
            hbmColor: color,
        };
        let icon = CreateIconIndirect(&icon_info);
        let _ = DeleteObject(color.into());
        let _ = DeleteObject(mask.into());
        icon
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn alpha(px: &[u8], size: u32, x: u32, y: u32) -> u8 {
        px[((y * size + x) * 4 + 3) as usize]
    }

    #[test]
    fn off_is_an_outline_and_indefinite_is_filled() {
        let off = glyph(16, Awake::Off, Overlay::None, true);
        let on = glyph(16, Awake::Indefinite, Overlay::None, true);
        assert_eq!(alpha(&off, 16, 8, 8), 0, "centre of the outline is empty");
        assert_eq!(alpha(&off, 16, 8, 1), 255, "the outline is drawn");
        assert_eq!(
            alpha(&on, 16, 8, 8),
            255,
            "centre of the filled glyph is set"
        );
        assert_eq!(alpha(&on, 16, 0, 0), 0, "corners stay transparent");
    }

    #[test]
    fn timed_fills_the_top_right_quarter_and_leaves_the_rest_empty() {
        let timed = glyph(16, Awake::Timed, Overlay::None, true);
        assert_eq!(alpha(&timed, 16, 12, 6), 255, "top-right quarter is filled");
        assert_eq!(alpha(&timed, 16, 5, 11), 0, "bottom-left is empty");
    }

    #[test]
    fn paused_overlay_adds_a_badge_in_the_bottom_right() {
        let none = glyph(16, Awake::Off, Overlay::None, true);
        let paused = glyph(16, Awake::Off, Overlay::Paused, true);
        assert_eq!(alpha(&none, 16, 14, 14), 0);
        assert_eq!(alpha(&paused, 16, 14, 14), 255);
    }

    // @R-022
    #[test]
    fn r022_tray_menu_has_open_pause_awake_status_settings_quit() {
        let state = TrayState {
            awake: Awake::Timed,
            overlay: Overlay::Paused,
            ..TrayState::default()
        };
        assert_eq!(
            menu_entries(&state),
            vec![
                MenuEntry::Action {
                    choice: MenuChoice::Open,
                    label: "Open",
                    checked: false
                },
                MenuEntry::Action {
                    choice: MenuChoice::Pause,
                    label: "Pause hotkeys",
                    checked: true
                },
                MenuEntry::Disabled("Awake: timed".into()),
                MenuEntry::Separator,
                MenuEntry::Action {
                    choice: MenuChoice::Settings,
                    label: "Settings",
                    checked: false
                },
                MenuEntry::Action {
                    choice: MenuChoice::Quit,
                    label: "Quit",
                    checked: false
                },
            ]
        );
    }

    #[test]
    fn every_awake_and_overlay_state_renders_at_16_and_24_px() {
        for size in [16u32, 24] {
            for awake in [Awake::Off, Awake::Indefinite, Awake::Timed] {
                for overlay in [Overlay::None, Overlay::Paused, Overlay::Notice] {
                    for light in [true, false] {
                        let px = glyph(size, awake, overlay, light);
                        assert_eq!(px.len(), (size * size * 4) as usize);
                        assert!(
                            px.iter().skip(3).step_by(4).any(|&a| a == 255),
                            "{size} {awake:?} {overlay:?}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn taskbar_theme_picks_the_glyph_colour() {
        let light = glyph(16, Awake::Indefinite, Overlay::None, true);
        let dark = glyph(16, Awake::Indefinite, Overlay::None, false);
        assert_eq!(
            &light[(8 * 16 + 8) * 4..(8 * 16 + 8) * 4 + 3],
            &[0x20, 0x20, 0x20]
        );
        assert_eq!(
            &dark[(8 * 16 + 8) * 4..(8 * 16 + 8) * 4 + 3],
            &[0xF0, 0xF0, 0xF0]
        );
    }
}
