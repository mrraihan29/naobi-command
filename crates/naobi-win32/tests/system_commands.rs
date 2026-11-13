//! Tier 2 (interactive desktop): the one system command that is safe to run for real, dark mode. It
//! flips Windows between light and dark and the guard puts it back, even if an assert fails. Lock,
//! sleep, restart, shut down, sign out and empty recycle bin are never run by a test.
use naobi_win32::system_commands::{apps_use_light_theme, toggle_dark_mode};

struct Restore(Option<u32>);

impl Drop for Restore {
    fn drop(&mut self) {
        if apps_use_light_theme() != self.0 {
            let _ = toggle_dark_mode();
        }
    }
}

// @R-020
#[test]
#[ignore = "tier2: interactive desktop"] // naobi-allow: tier 2 test, D-025
fn r020_toggle_dark_mode_flips_the_windows_app_theme() {
    let before = apps_use_light_theme();
    let _restore = Restore(before);
    toggle_dark_mode().expect("toggle");
    let flipped = apps_use_light_theme();
    assert_ne!(flipped, before, "the app theme did not change");
    toggle_dark_mode().expect("toggle back");
    assert_eq!(
        apps_use_light_theme(),
        before.or(Some(1)),
        "the theme is not back"
    );
}
