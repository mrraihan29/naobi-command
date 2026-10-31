//! The OS side of `ThemeSource`: the app theme from `AppsUseLightTheme` and reduced motion from the
//! client-area animation setting. The change message itself arrives on P1 as `AppEvent::ThemeChanged`.
use std::ffi::c_void;

use naobi_core::theme::ThemeSource;
use naobi_core::ui::Theme;
use windows::core::BOOL;
use windows::Win32::UI::WindowsAndMessaging::{
    SystemParametersInfoW, SPI_GETCLIENTAREAANIMATION, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS,
};

/// Windows' own light or dark setting, and its "Animation effects" switch.
pub struct SystemTheme;

impl ThemeSource for SystemTheme {
    fn current(&self) -> Theme {
        if crate::dwm::system_uses_dark_theme() {
            Theme::Stone
        } else {
            Theme::Paper
        }
    }

    fn reduced_motion(&self) -> bool {
        !animations_on()
    }
}

/// `SPI_GETCLIENTAREAANIMATION`. An unreadable value counts as on, the Windows default.
fn animations_on() -> bool {
    let mut on = BOOL(1);
    // SAFETY: the query writes one BOOL (4 bytes) into `on`, which lives for the call.
    let ok = unsafe {
        SystemParametersInfoW(
            SPI_GETCLIENTAREAANIMATION,
            0,
            Some(&mut on as *mut BOOL as *mut c_void),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
    }
    .is_ok();
    !ok || on.as_bool()
}
