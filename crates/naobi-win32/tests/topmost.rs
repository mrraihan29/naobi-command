//! Tier 2: the border follows a topmost window with one hook per process, and with nothing topmost
//! there is no border window and no hook (REPORT 5.4). The helper window belongs to the test process.
#![cfg(feature = "perf")]
use naobi_win32::edit_window::EditWindow;
use naobi_win32::topmost::{counts, pin_now, unpin_now};

// @R-018
#[test]
#[ignore = "tier2: interactive desktop"] // naobi-allow: tier 2 test, D-025
fn d022_no_hook_when_no_topmost_window() {
    let (Some(a), Some(b)) = (EditWindow::open(), EditWindow::open()) else {
        panic!("the helper windows did not open");
    };
    assert_eq!(counts(), (0, 0), "nothing topmost: no border, no hook");
    pin_now(a.hwnd());
    assert_eq!(counts(), (1, 1));
    // A second window of the same process shares the hook.
    pin_now(b.hwnd());
    assert_eq!(counts(), (2, 1));
    // Pinning twice changes nothing.
    pin_now(b.hwnd());
    assert_eq!(counts(), (2, 1));
    unpin_now(a.hwnd());
    assert_eq!(
        counts(),
        (1, 1),
        "the hook stays while a window of its process is followed"
    );
    unpin_now(b.hwnd());
    assert_eq!(counts(), (0, 0), "the last window takes the hook with it");
}
