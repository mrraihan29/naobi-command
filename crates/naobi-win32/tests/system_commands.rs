//! Tier 2 (interactive desktop): the one system command that is safe to run for real, dark mode. It
//! flips Windows between light and dark and the guard puts both stored values back, even if an
//! assert fails. Lock, sleep, restart, shut down, sign out and empty recycle bin are never run by a
//! test.
use naobi_win32::system_commands::{
    apps_use_light_theme, restore_theme, system_uses_light_theme, toggle_dark_mode,
};

/// Both raw values, written back as they were (they can differ: dark system, light apps).
struct Restore(Option<u32>, Option<u32>);

impl Drop for Restore {
    fn drop(&mut self) {
        restore_theme(self.0, self.1);
    }
}

// @R-020
#[test]
#[ignore = "tier2: interactive desktop"] // naobi-allow: tier 2 test, D-025
fn r020_toggle_dark_mode_flips_the_windows_app_theme() {
    let before = apps_use_light_theme();
    let _restore = Restore(before, system_uses_light_theme());
    toggle_dark_mode().expect("toggle");
    assert_ne!(
        apps_use_light_theme(),
        before,
        "the app theme did not change"
    );
}
