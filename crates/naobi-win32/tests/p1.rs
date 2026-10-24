//! Platform thread P1: forwarding (tier 1) and a real P1 started in this process (tier 2).
use std::sync::mpsc;
use std::sync::{Mutex, Once};
use std::time::{Duration, Instant};

use naobi_core::event::AppEvent;
use naobi_core::ids::{HotkeyId, TimerId};
use naobi_core::settings::Settings;
use naobi_win32::{instance, platform};
use std::ffi::c_void;
use windows::core::{w, GUID};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::Shell::{
    Shell_NotifyIconGetRect, Shell_NotifyIconW, NIM_DELETE, NOTIFYICONDATAW, NOTIFYICONIDENTIFIER,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetAncestor, GetDesktopWindow, GetWindowLongW, IsWindowVisible, PostMessageW,
    RegisterWindowMessageW, GA_PARENT, GWL_EXSTYLE, PBT_APMRESUMEAUTOMATIC, PBT_APMRESUMESUSPEND,
    WM_CLIPBOARDUPDATE, WM_CLOSE, WM_DISPLAYCHANGE, WM_HOTKEY, WM_POWERBROADCAST, WM_SETTINGCHANGE,
    WM_TIMER, WS_EX_TOOLWINDOW,
};

#[test]
fn d021_p1_forwards_all_listed_messages() {
    let fwd = |m: u32, w: usize| platform::forward(m, WPARAM(w), LPARAM(0));
    assert_eq!(
        fwd(WM_HOTKEY, 1),
        Some(AppEvent::Hotkey(HotkeyId("center".into())))
    );
    assert!(matches!(
        fwd(WM_CLIPBOARDUPDATE, 0),
        Some(AppEvent::ClipboardChanged(_))
    ));
    assert_eq!(fwd(WM_SETTINGCHANGE, 0), Some(AppEvent::ThemeChanged));
    assert_eq!(
        fwd(WM_POWERBROADCAST, PBT_APMRESUMEAUTOMATIC as usize),
        Some(AppEvent::Resume)
    );
    assert_eq!(
        fwd(WM_POWERBROADCAST, PBT_APMRESUMESUSPEND as usize),
        Some(AppEvent::Resume)
    );
    assert_eq!(
        fwd(WM_POWERBROADCAST, 0x0004),
        None,
        "suspend is not forwarded"
    );
    assert_eq!(fwd(WM_DISPLAYCHANGE, 0), Some(AppEvent::DisplayChanged));
    assert_eq!(fwd(WM_TIMER, 7), Some(AppEvent::Timer(TimerId(7))));
    assert_eq!(fwd(instance::WM_APP_SHOW, 0), Some(AppEvent::ShowRequested));
    assert_eq!(
        fwd(WM_CLOSE, 0),
        None,
        "anything else is left to the default procedure"
    );
}

/// Source scan (D-012): the workspace has no low-level keyboard hook. The needle is split so that
/// this file does not match itself.
#[test]
fn d012_no_low_level_keyboard_hook() {
    let needles = [
        concat!("WH_KEYBOARD", "_LL"),
        concat!("SetWindows", "HookEx"),
    ];
    let crates = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut stack = vec![crates];
    let mut scanned = 0;
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if name != "target" && name != "spikes" {
                    stack.push(path);
                }
            } else if path.extension().is_some_and(|e| e == "rs") {
                scanned += 1;
                let text = std::fs::read_to_string(&path).unwrap_or_default();
                for needle in needles {
                    assert!(!text.contains(needle), "{} uses {needle}", path.display());
                }
            }
        }
    }
    assert!(scanned > 20, "the scan found the workspace sources");
}

// ---- tier 2: one real P1 per test binary, its events recorded ----

static EVENTS: Mutex<Vec<AppEvent>> = Mutex::new(Vec::new());

fn ensure_started() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        assert!(
            platform::start_with(
                &Settings::default(),
                |ev| EVENTS.lock().unwrap_or_else(|e| e.into_inner()).push(ev),
                || {},
            )
            .is_ok(),
            "P1 starts"
        );
    });
}

fn wait_for(mut cond: impl FnMut() -> bool) -> bool {
    let end = Instant::now() + Duration::from_secs(3);
    while Instant::now() < end {
        if cond() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    cond()
}

fn saw(ev: &AppEvent) -> bool {
    EVENTS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .contains(ev)
}

#[test]
#[ignore = "tier2: interactive desktop"]
fn d021_p1_receives_broadcast_setting_change() {
    ensure_started();
    // SAFETY: broadcasts a message with no pointers; the timeout bounds the wait.
    unsafe {
        use windows::Win32::UI::WindowsAndMessaging::{
            SendMessageTimeoutW, HWND_BROADCAST, SMTO_ABORTIFHUNG,
        };
        let _ = SendMessageTimeoutW(
            HWND_BROADCAST,
            WM_SETTINGCHANGE,
            WPARAM(0),
            LPARAM(0),
            SMTO_ABORTIFHUNG,
            1000,
            None,
        );
    }
    assert!(
        wait_for(|| saw(&AppEvent::ThemeChanged)),
        "ThemeChanged never arrived"
    );
}

#[test]
#[ignore = "tier2: interactive desktop"]
fn d021_post_to_p1_runs_on_platform_thread() {
    ensure_started();
    let (tx, rx) = mpsc::channel();
    assert!(platform::post_to_p1(Box::new(move || {
        let _ = tx.send(std::thread::current().id());
    })));
    let ran_on = rx
        .recv_timeout(Duration::from_secs(3))
        .expect("the posted job ran");
    assert_ne!(
        ran_on,
        std::thread::current().id(),
        "the job ran on P1, not the test thread"
    );
}

fn tray_icon_present(hwnd: HWND) -> bool {
    let id = NOTIFYICONIDENTIFIER {
        cbSize: std::mem::size_of::<NOTIFYICONIDENTIFIER>() as u32,
        hWnd: hwnd,
        uID: 1,
        guidItem: GUID::zeroed(),
    };
    // SAFETY: `id` lives for the call; the query only reads the shell's icon table.
    unsafe { Shell_NotifyIconGetRect(&id) }.is_ok()
}

#[test]
#[ignore = "tier2: interactive desktop"]
fn d021_tray_icon_readded_on_taskbar_created() {
    ensure_started();
    let hwnd = HWND(platform::p1_hwnd() as *mut c_void);
    // Explorer restarting drops the icon; simulate that by removing it from our window.
    let nid = NOTIFYICONDATAW {
        cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: 1,
        ..Default::default()
    };
    // SAFETY: removes the icon that P1 added under this window and id.
    let _ = unsafe { Shell_NotifyIconW(NIM_DELETE, &nid) };
    assert!(!tray_icon_present(hwnd), "the icon was removed");

    // SAFETY: the registered message name has no pointers.
    let taskbar_created = unsafe { RegisterWindowMessageW(w!("TaskbarCreated")) };
    // SAFETY: posts a message with no pointers to P1's window.
    unsafe { PostMessageW(Some(hwnd), taskbar_created, WPARAM(0), LPARAM(0)) }
        .expect("post TaskbarCreated to P1");
    assert!(
        wait_for(|| tray_icon_present(hwnd)),
        "P1 re-added the icon on TaskbarCreated"
    );
}

#[test]
#[ignore = "tier2: interactive desktop"]
fn d021_p1_window_is_a_hidden_tool_window_under_the_desktop() {
    ensure_started();
    let hwnd = HWND(platform::p1_hwnd() as *mut c_void);
    // SAFETY: reads the extended style and visibility of our own window; no pointers.
    let (ex_style, visible, parent) = unsafe {
        (
            GetWindowLongW(hwnd, GWL_EXSTYLE) as u32,
            IsWindowVisible(hwnd).as_bool(),
            GetAncestor(hwnd, GA_PARENT),
        )
    };
    assert!(ex_style & WS_EX_TOOLWINDOW.0 != 0, "tool window style");
    assert!(!visible, "never shown");
    // SAFETY: returns the desktop window handle; no pointers.
    let desktop = unsafe { GetDesktopWindow() };
    assert_eq!(
        parent, desktop,
        "top-level under the desktop, not message-only"
    );
}
