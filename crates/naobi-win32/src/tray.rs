//! Tray icon with a Quit item only (NCMD-12 later owns the full menu).
use windows::core::{w, Result};
use windows::Win32::Foundation::{HWND, POINT};
use windows::Win32::UI::Shell::{
    Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NOTIFYICONDATAW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, DestroyMenu, GetCursorPos, LoadIconW, PostMessageW,
    SetForegroundWindow, TrackPopupMenu, IDI_APPLICATION, MF_STRING, TPM_RETURNCMD, WM_NULL,
};

/// Message the shell posts to the platform window for tray mouse events.
pub const WM_TRAY: u32 = 0x8000 + 1; // WM_APP + 1
const QUIT: usize = 1;

fn data(hwnd: HWND) -> NOTIFYICONDATAW {
    NOTIFYICONDATAW {
        cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: 1,
        ..Default::default()
    }
}

pub fn add(hwnd: HWND) -> Result<()> {
    let mut nid = data(hwnd);
    nid.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
    nid.uCallbackMessage = WM_TRAY;
    for (dst, src) in nid.szTip.iter_mut().zip("Naobi Command".encode_utf16()) {
        *dst = src;
    }
    // SAFETY: loads a stock icon owned by the system; the data struct outlives the call.
    unsafe {
        nid.hIcon = LoadIconW(None, IDI_APPLICATION)?;
        Shell_NotifyIconW(NIM_ADD, &nid).ok()
    }
}

pub fn remove(hwnd: HWND) {
    // SAFETY: the data struct outlives the call.
    let _ = unsafe { Shell_NotifyIconW(NIM_DELETE, &data(hwnd)) };
}

/// Shows the context menu at the cursor; true when the Owner chose Quit.
pub fn menu_chose_quit(hwnd: HWND) -> bool {
    // SAFETY: a menu created here is destroyed here; TPM_RETURNCMD returns the chosen id, nothing is posted.
    unsafe {
        let Ok(menu) = CreatePopupMenu() else {
            return false;
        };
        let _ = AppendMenuW(menu, MF_STRING, QUIT, w!("Quit"));
        let mut pt = POINT::default();
        let _ = GetCursorPos(&mut pt);
        // The menu only closes on an outside click if our window is foreground first.
        let _ = SetForegroundWindow(hwnd);
        let chosen = TrackPopupMenu(menu, TPM_RETURNCMD, pt.x, pt.y, None, hwnd, None);
        let _ = PostMessageW(Some(hwnd), WM_NULL, Default::default(), Default::default());
        let _ = DestroyMenu(menu);
        chosen.0 as usize == QUIT
    }
}
