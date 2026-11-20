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

    /// The color as `0xRRGGBB`, without its alpha.
    pub const fn to_rgb(self) -> u32 {
        ((self.r as u32) << 16) | ((self.g as u32) << 8) | self.b as u32
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

/// A dark built-in theme: green-grey surfaces, cream ink, amber accent, teal focus (D-046).
pub const TOKYO_HAZE: Palette = Palette {
    window: Rgba::rgb(0x1B221F),
    raised: Rgba::rgb(0x2A312D),
    selected: Rgba::rgb(0x333B37),
    well: Rgba::rgb(0x0D0F0E),
    text_primary: Rgba::rgb(0xF0E6B8),
    text_secondary: Rgba::rgb(0xD6CDA4),
    subtitle_selected: Rgba::rgb(0xD6CDA4),
    text_muted: Rgba::rgb(0x9DB0A6),
    accent: Rgba::rgb(0xE8A33D),
    focus: Rgba::rgb(0x7CC9B3),
    danger: Rgba::rgb(0xF89A8B),
    success: Rgba::rgb(0x93BD82),
    hair: Rgba::rgb(0x3B4641),
    strong: Rgba::rgb(0x505D57),
};

/// A dark built-in theme: near-black surfaces, cream ink, orange accent, red focus (D-046).
pub const NEON_NIGHT: Palette = Palette {
    window: Rgba::rgb(0x0D0F0E),
    raised: Rgba::rgb(0x171A18),
    selected: Rgba::rgb(0x222623),
    well: Rgba::rgb(0x050605),
    text_primary: Rgba::rgb(0xF0E6B8),
    text_secondary: Rgba::rgb(0xD6CDA4),
    subtitle_selected: Rgba::rgb(0xD6CDA4),
    text_muted: Rgba::rgb(0xA3A08B),
    accent: Rgba::rgb(0xD9772B),
    focus: Rgba::rgb(0xF0634F),
    danger: Rgba::rgb(0xFF7B6B),
    success: Rgba::rgb(0x8FC47A),
    hair: Rgba::rgb(0x2B302D),
    strong: Rgba::rgb(0x434944),
};

fn luminance(c: Rgba) -> f64 {
    let lin = |v: u8| {
        let s = f64::from(v) / 255.0;
        if s <= 0.03928 {
            s / 12.92
        } else {
            ((s + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * lin(c.r) + 0.7152 * lin(c.g) + 0.0722 * lin(c.b)
}

/// The WCAG contrast ratio of two opaque colors.
pub fn contrast(a: Rgba, b: Rgba) -> f64 {
    let (x, y) = (luminance(a), luminance(b));
    (x.max(y) + 0.05) / (x.min(y) + 0.05)
}

/// Composites `fg` (with its alpha) over an opaque `bg`.
pub fn over(fg: Rgba, bg: Rgba) -> Rgba {
    let mix = |f: u8, b: u8| {
        let a = f64::from(fg.a) / 255.0;
        (f64::from(f) * a + f64::from(b) * (1.0 - a)).round() as u8
    };
    Rgba {
        r: mix(fg.r, bg.r),
        g: mix(fg.g, bg.g),
        b: mix(fg.b, bg.b),
        a: 255,
    }
}

/// (name, foreground, background, minimum ratio). Text needs 4.5:1, UI components and the accent or
/// focus marks 3:1 (R-007).
fn contrast_pairs(p: &Palette) -> [(&'static str, Rgba, Rgba, f64); 17] {
    const TEXT: f64 = 4.5;
    const UI: f64 = 3.0;
    [
        ("primary on window", p.text_primary, p.window, TEXT),
        ("primary on raised", p.text_primary, p.raised, TEXT),
        ("primary on selected", p.text_primary, p.selected, TEXT),
        ("primary on well", p.text_primary, p.well, TEXT),
        ("secondary on window", p.text_secondary, p.window, TEXT),
        // Subtitles, kinds, key caps and action reasons on a selected row all use this pair. Muted
        // ink is not listed here: paper's fails on the selected wash by design.
        (
            "subtitle on selected",
            p.subtitle_selected,
            p.selected,
            TEXT,
        ),
        ("muted on window", p.text_muted, p.window, TEXT),
        ("muted on raised", p.text_muted, p.raised, TEXT),
        ("muted on well", p.text_muted, p.well, TEXT),
        ("danger on window", p.danger, p.window, TEXT),
        ("danger on selected", p.danger, p.selected, TEXT),
        ("success on window", p.success, p.window, TEXT),
        ("accent on window", p.accent, p.window, UI),
        ("accent on selected", p.accent, p.selected, UI),
        ("focus on window", p.focus, p.window, UI),
        ("focus on raised", p.focus, p.raised, UI),
        ("focus on selected", p.focus, p.selected, UI),
    ]
}

/// One color pair below its WCAG 2.2 AA minimum.
#[derive(Debug, Clone, PartialEq)]
pub struct ContrastFailure {
    pub pair: &'static str,
    pub ratio: f64,
    pub minimum: f64,
}

/// The pairs of `p` below WCAG 2.2 AA, in table order.
pub fn contrast_failures(p: &Palette) -> Vec<ContrastFailure> {
    contrast_pairs(p)
        .into_iter()
        .filter_map(|(pair, fg, bg, minimum)| {
            let ratio = contrast(over(fg, bg), bg);
            (ratio < minimum).then_some(ContrastFailure {
                pair,
                ratio,
                minimum,
            })
        })
        .collect()
}
