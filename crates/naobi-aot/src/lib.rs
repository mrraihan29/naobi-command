//! Always on top (R-018): toggle the foreground window topmost, and tell Windows how its border
//! looks. The decision logic and the colors are here; Windows is behind `TopmostOps`.
#![forbid(unsafe_code)]

pub mod commands;
pub mod rect;
pub mod topmost;

pub use commands::{AotCommands, Config};
pub use rect::Rect;
pub use topmost::{toggle, Outcome, Target, TopmostOps};

use naobi_core::ui::Theme;

/// How the border of every topmost window looks now. `0xRRGGBB`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Style {
    pub show: bool,
    pub color: u32,
}

/// Oxide on the light theme and stone-focus on the dark one (R-018 v2).
pub const LIGHT: u32 = 0x914A3B;
pub const DARK: u32 = 0xE3BDA9;

/// The border thickness in physical pixels.
pub const THICKNESS: i32 = 2;

/// The color for `setting` (`aot.border_color`: "auto" or "#RRGGBB") under `theme`. Anything that is
/// not a six-digit hex color is "auto".
pub fn border_color(setting: &str, theme: Theme) -> u32 {
    let custom = setting
        .strip_prefix('#')
        .filter(|h| h.len() == 6)
        .and_then(|h| u32::from_str_radix(h, 16).ok());
    custom.unwrap_or(match theme {
        Theme::Paper => LIGHT,
        Theme::Stone => DARK,
    })
}

pub fn style(show: bool, setting: &str, theme: Theme) -> Style {
    Style {
        show,
        color: border_color(setting, theme),
    }
}
