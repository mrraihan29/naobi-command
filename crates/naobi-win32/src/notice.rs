//! Notice balloons on the tray icon (R-008, D-018). P1 owns the icon, so the request is posted there.
//! Do Not Disturb can hide a balloon; the banner and the tray tooltip are the reliable channel.
use std::ffi::c_void;

use naobi_core::notice::{Notice, NoticeKind};
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Shell::{
    Shell_NotifyIconW, NIF_INFO, NIIF_ERROR, NIIF_INFO, NIIF_WARNING, NIM_MODIFY, NOTIFYICONDATAW,
    NOTIFY_ICON_INFOTIP_FLAGS,
};

use crate::platform;

const TITLE: &str = "Naobi Command";

/// Asks P1 to show `notice` as a balloon on the tray icon. Any thread may call it; false when P1 is
/// not up yet.
pub fn balloon(notice: &Notice) -> bool {
    let flags = match notice.kind {
        NoticeKind::Info => NIIF_INFO,
        NoticeKind::Warning => NIIF_WARNING,
        NoticeKind::Error => NIIF_ERROR,
    };
    let body = notice.message.clone();
    platform::post_to_p1(Box::new(move || show(&body, flags)))
}

/// Runs on P1: modifies the icon that P1 added, with the balloon in it.
fn show(body: &str, flags: NOTIFY_ICON_INFOTIP_FLAGS) {
    let mut nid = NOTIFYICONDATAW {
        cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: HWND(platform::p1_hwnd() as *mut c_void),
        uID: 1,
        uFlags: NIF_INFO,
        dwInfoFlags: flags,
        ..Default::default()
    };
    copy_utf16(&mut nid.szInfoTitle, TITLE);
    copy_utf16(&mut nid.szInfo, body);
    // SAFETY: the data struct lives for the call, and its window is P1, on whose thread this runs.
    let _ = unsafe { Shell_NotifyIconW(NIM_MODIFY, &nid) };
}

/// Writes `src` as UTF-16, cut so the last element stays zero (the terminator).
fn copy_utf16(dst: &mut [u16], src: &str) {
    let room = dst.len() - 1;
    for (d, s) in dst.iter_mut().take(room).zip(src.encode_utf16()) {
        *d = s;
    }
}
