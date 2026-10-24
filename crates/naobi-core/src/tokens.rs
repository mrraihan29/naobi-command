//! Naobi design tokens (DESIGN_SYSTEM.md, R-007) as plain data. The WCAG test and the Slint theme
//! both read these values, so the numbers have one source. Framework types never appear here.

/// An sRGB color with alpha (255 = opaque).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Rgba {
    pub const fn rgb(hex: u32) -> Self {
        Self::with_alpha(hex, 255)
    }

    pub const fn with_alpha(hex: u32, a: u8) -> Self {
        Self {
            r: (hex >> 16) as u8,
            g: (hex >> 8) as u8,
            b: hex as u8,
            a,
        }
    }
}

/// One theme. `subtitle_selected` is the subtitle color on a selected row: paper steps to the
/// secondary ink, stone keeps the muted tone (the stone palette has no secondary tier).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Palette {
    pub window: Rgba,
    pub raised: Rgba,
    pub selected: Rgba,
    pub well: Rgba,
    pub text_primary: Rgba,
    pub text_secondary: Rgba,
    pub subtitle_selected: Rgba,
    pub text_muted: Rgba,
    pub accent: Rgba,
    pub focus: Rgba,
    /// Armed-command bar and glyph. Stone has no danger tint, so it is the primary text color.
    pub danger: Rgba,
    pub success: Rgba,
    pub hair: Rgba,
    /// Chip and notice outline. Stone value from the approved mockup (`T.stone.ls`, #5A5249).
    pub strong: Rgba,
}

/// The light theme.
pub const PAPER: Palette = Palette {
    window: Rgba::rgb(0xFAF8F2),
    raised: Rgba::rgb(0xF3F0E8),
    selected: Rgba::rgb(0xE8E6E0),
    well: Rgba::rgb(0xF3F0E8),
    text_primary: Rgba::rgb(0x171714),
    text_secondary: Rgba::rgb(0x393732),
    subtitle_selected: Rgba::rgb(0x393732),
    text_muted: Rgba::rgb(0x6D6A63),
    accent: Rgba::rgb(0x914A3B),
    focus: Rgba::rgb(0x275D9A),
    danger: Rgba::rgb(0xA63E32),
    success: Rgba::rgb(0x4E6A51),
    hair: Rgba::rgb(0xD9D3C7),
    strong: Rgba::rgb(0xC7C1B6),
};

/// The dark theme. Muted text is on-stone at 58% alpha, composited over each surface.
pub const STONE: Palette = Palette {
    window: Rgba::rgb(0x2A2622),
    raised: Rgba::rgb(0x36312C),
    selected: Rgba::rgb(0x3A3632),
    well: Rgba::rgb(0x0D0B09),
    text_primary: Rgba::rgb(0xF3F0E8),
    text_secondary: Rgba::rgb(0xF3F0E8),
    subtitle_selected: Rgba::with_alpha(0xF3F0E8, 148),
    text_muted: Rgba::with_alpha(0xF3F0E8, 148),
    accent: Rgba::rgb(0xE3BDA9),
    focus: Rgba::rgb(0xE3BDA9),
    danger: Rgba::rgb(0xF3F0E8),
    success: Rgba::rgb(0xF3F0E8),
    hair: Rgba::rgb(0x4A433A),
    strong: Rgba::rgb(0x5A5249),
};
