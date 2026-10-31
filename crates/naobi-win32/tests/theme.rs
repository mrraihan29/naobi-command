//! Tier 2 (interactive desktop): reads the real Windows setting. It changes nothing; flipping the
//! Owner's light or dark mode is a manual step of the Gate 3 checklist.
use naobi_core::theme::ThemeSource;
use naobi_core::ui::Theme;
use naobi_win32::{dwm, theme::SystemTheme};

// @R-007
#[test]
#[ignore = "tier2: interactive desktop"] // naobi-allow: tier 2 test, D-025
fn r007_system_theme_reads_apps_use_light_theme() {
    let want = if dwm::system_uses_dark_theme() {
        Theme::Stone
    } else {
        Theme::Paper
    };
    assert_eq!(SystemTheme.current(), want);
}
