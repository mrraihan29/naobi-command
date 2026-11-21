//! Tier 2 (interactive desktop): `EnumWindows` window-switcher smoke check.
use naobi_win32::{dwm, window_switcher};
use std::ffi::c_void;
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId;

// @R-011
#[test]
#[ignore = "tier2: interactive desktop"] // naobi-allow: tier 2 test, D-025
fn r011_window_switcher_enumerates_non_cloaked_non_self_windows() {
    let entries = window_switcher::list();
    let own_pid = std::process::id();
    for entry in &entries {
        assert!(!entry.title.trim().is_empty());
        assert!(
            !dwm::is_cloaked(entry.hwnd),
            "cloaked window leaked: {}",
            entry.hwnd
        );
        assert!(
            !dwm::is_tool_window(entry.hwnd),
            "tool window leaked: {}",
            entry.hwnd
        );
        let mut pid = 0u32;
        // SAFETY: `entry.hwnd` was returned by `EnumWindows`; `pid` receives its owning process id.
        unsafe {
            GetWindowThreadProcessId(HWND(entry.hwnd as *mut c_void), Some(&mut pid));
        }
        assert_ne!(pid, own_pid, "own process window leaked: {}", entry.hwnd);
    }
}
