//! Always on top (R-018): toggle the foreground window topmost, and tell Windows how its border
//! looks. The decision logic and the colors are here; Windows is behind `TopmostOps`.
#![forbid(unsafe_code)]

pub mod commands;
pub mod rect;
pub mod topmost;

pub use commands::{AotCommands, Config};
pub use rect::Rect;
pub use topmost::{toggle, Outcome, Target, TopmostOps};

/// How the border of every topmost window looks now. `0xRRGGBB`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Style {
    pub show: bool,
    pub color: u32,
}

/// The accents of the Built-in themes: oxide on paper and stone-focus on stone (R-018).
pub const LIGHT: u32 = 0x914A3B;
pub const DARK: u32 = 0xE3BDA9;

/// The border thickness in physical pixels.
pub const THICKNESS: i32 = 2;

/// The color for `setting` (`aot.border_color`: "auto" or "#RRGGBB"). "auto", and anything that is
/// not a six-digit hex color, is `accent`, the active Theme's accent (R-018 v3).
pub fn border_color(setting: &str, accent: u32) -> u32 {
    let custom = setting
        .strip_prefix('#')
        .filter(|h| h.len() == 6)
        .and_then(|h| u32::from_str_radix(h, 16).ok());
    custom.unwrap_or(accent)
}

pub fn style(show: bool, setting: &str, accent: u32) -> Style {
    Style {
        show,
        color: border_color(setting, accent),
    }
}
