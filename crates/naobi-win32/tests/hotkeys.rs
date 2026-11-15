//! Tier 2 (interactive desktop): the real P1 registers the chords. Probing a chord from this thread
//! tells whether P1 holds it: a free chord registers, a held one does not.
use std::time::{Duration, Instant};

use naobi_core::settings::Settings;
use naobi_win32::platform;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    RegisterHotKey, UnregisterHotKey, MOD_CONTROL, MOD_NOREPEAT, VK_SPACE,
};

const PROBE_ID: i32 = 0x7F00;

/// True when Ctrl+Space is free system-wide.
fn launcher_free() -> bool {
    // SAFETY: registers a probe id on this thread's queue and releases it at once; no pointers.
    unsafe {
        let free = RegisterHotKey(
            None,
            PROBE_ID,
            MOD_CONTROL | MOD_NOREPEAT,
            VK_SPACE.0 as u32,
        )
        .is_ok();
        if free {
            let _ = UnregisterHotKey(None, PROBE_ID);
        }
        free
    }
}

fn wait_until(mut cond: impl FnMut() -> bool) -> bool {
    let end = Instant::now() + Duration::from_secs(3);
    while Instant::now() < end {
        if cond() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    cond()
}

// @R-022
#[test]
#[ignore = "tier2: interactive desktop"]
fn r022_pause_hotkeys_unregisters_all() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        assert!(
            platform::start_with(&Settings::default(), |_| {}, || {}).is_ok(),
            "P1 starts"
        );
    });
    assert!(
        wait_until(|| !launcher_free()),
        "P1 holds Ctrl+Space once started"
    );
    assert!(platform::set_hotkeys_paused(true));
    assert!(wait_until(launcher_free), "Pause released every chord");
    assert!(platform::set_hotkeys_paused(false));
    assert!(
        wait_until(|| !launcher_free()),
        "resume registers the chords again"
    );
}
