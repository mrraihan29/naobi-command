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

/// A light built-in theme: warm rice-paper surfaces, ink-black text, indigo accent, jade focus (D-046).
pub const WASHI: Palette = Palette {
    window: Rgba::rgb(0xF5F0DA),
    raised: Rgba::rgb(0xECE6CC),
    selected: Rgba::rgb(0xE0D8B8),
    well: Rgba::rgb(0xECE6CC),
    text_primary: Rgba::rgb(0x1A1A20),
    text_secondary: Rgba::rgb(0x3A3A44),
    subtitle_selected: Rgba::rgb(0x3A3A44),
    text_muted: Rgba::rgb(0x5C5850),
    accent: Rgba::rgb(0x3B4BA0),
    focus: Rgba::rgb(0x1F6B66),
    danger: Rgba::rgb(0x982F25),
    success: Rgba::rgb(0x44634A),
    hair: Rgba::rgb(0xD8D0B0),
    strong: Rgba::rgb(0xC6BC98),
};

/// A light built-in theme: pale blush surfaces, deep plum text, sakura-red accent, violet focus (D-046).
pub const SAKURA_DAWN: Palette = Palette {
    window: Rgba::rgb(0xFBE9ED),
    raised: Rgba::rgb(0xF4DAE0),
    selected: Rgba::rgb(0xE8C7D1),
    well: Rgba::rgb(0xF4DAE0),
    text_primary: Rgba::rgb(0x3A1830),
    text_secondary: Rgba::rgb(0x5A2E4E),
    subtitle_selected: Rgba::rgb(0x5A2E4E),
    text_muted: Rgba::rgb(0x7A4F6C),
    accent: Rgba::rgb(0xB5254F),
    focus: Rgba::rgb(0x5B4BA8),
    danger: Rgba::rgb(0x8E1B1A),
    success: Rgba::rgb(0x34704A),
    hair: Rgba::rgb(0xE8CDD3),
    strong: Rgba::rgb(0xD8B8C1),
};

/// A light built-in theme: green-tinted cream surfaces, deep green text, tea-green accent, blue focus (D-046).
pub const MATCHA: Palette = Palette {
    window: Rgba::rgb(0xDDEBC9),
    raised: Rgba::rgb(0xD1E2B9),
    selected: Rgba::rgb(0xC2D8A4),
    well: Rgba::rgb(0xD1E2B9),
    text_primary: Rgba::rgb(0x17301C),
    text_secondary: Rgba::rgb(0x2F4A33),
    subtitle_selected: Rgba::rgb(0x2F4A33),
    text_muted: Rgba::rgb(0x445C43),
    accent: Rgba::rgb(0x3F7226),
    focus: Rgba::rgb(0x2F6F8F),
    danger: Rgba::rgb(0x8A2820),
    success: Rgba::rgb(0x2A6A36),
    hair: Rgba::rgb(0xC2D4A8),
    strong: Rgba::rgb(0xAEC38E),
};

/// A light built-in theme: warm sand surfaces, dark brown text, terracotta accent, deep blue focus (D-046).
pub const DUNE: Palette = Palette {
    window: Rgba::rgb(0xF0E1C8),
    raised: Rgba::rgb(0xE4D1B0),
    selected: Rgba::rgb(0xDDC8A5),
    well: Rgba::rgb(0xE4D1B0),
    text_primary: Rgba::rgb(0x33241A),
    text_secondary: Rgba::rgb(0x4F3A2B),
    subtitle_selected: Rgba::rgb(0x4F3A2B),
    text_muted: Rgba::rgb(0x5A4632),
    accent: Rgba::rgb(0xAA4219),
    focus: Rgba::rgb(0x235A75),
    danger: Rgba::rgb(0x7F2218),
    success: Rgba::rgb(0x435E32),
    hair: Rgba::rgb(0xD2BE9A),
    strong: Rgba::rgb(0xBFA880),
};

/// A dark built-in theme: deep navy surfaces, blue-white text, cyan accent, periwinkle focus (D-046).
pub const MIDNIGHT_INK: Palette = Palette {
    window: Rgba::rgb(0x0D1A33),
    raised: Rgba::rgb(0x172844),
    selected: Rgba::rgb(0x20365A),
    well: Rgba::rgb(0x070D1A),
    text_primary: Rgba::rgb(0xE4EEFF),
    text_secondary: Rgba::rgb(0xB9C9E6),
    subtitle_selected: Rgba::rgb(0xB9C9E6),
    text_muted: Rgba::rgb(0x8CA0C2),
    accent: Rgba::rgb(0x3CCBE0),
    focus: Rgba::rgb(0x9DB8FF),
    danger: Rgba::rgb(0xFF8A80),
    success: Rgba::rgb(0x7AD6A0),
    hair: Rgba::rgb(0x2A3A58),
    strong: Rgba::rgb(0x40557A),
};

/// A dark built-in theme: dusk-purple surfaces, cream text, gold accent, pale-teal focus (D-046).
pub const KYOTO_DUSK: Palette = Palette {
    window: Rgba::rgb(0x231733),
    raised: Rgba::rgb(0x32244A),
    selected: Rgba::rgb(0x403060),
    well: Rgba::rgb(0x120D1F),
    text_primary: Rgba::rgb(0xF5EBD6),
    text_secondary: Rgba::rgb(0xD9CDB4),
    subtitle_selected: Rgba::rgb(0xD9CDB4),
    text_muted: Rgba::rgb(0xA99FBC),
    accent: Rgba::rgb(0xEDCB5E),
    focus: Rgba::rgb(0x8CC8D8),
    danger: Rgba::rgb(0xF79A90),
    success: Rgba::rgb(0x9CCB8A),
    hair: Rgba::rgb(0x3E3258),
    strong: Rgba::rgb(0x56487A),
};

/// A dark built-in theme: very dark teal surfaces, pale-aqua text, coral accent, sand-yellow focus (D-046).
pub const DEEP_SEA: Palette = Palette {
    window: Rgba::rgb(0x0A2226),
    raised: Rgba::rgb(0x12353B),
    selected: Rgba::rgb(0x1B484E),
    well: Rgba::rgb(0x051216),
    text_primary: Rgba::rgb(0xD6F2F0),
    text_secondary: Rgba::rgb(0xAFD6D4),
    subtitle_selected: Rgba::rgb(0xAFD6D4),
    text_muted: Rgba::rgb(0x7FA9A8),
    accent: Rgba::rgb(0xFF7F66),
    focus: Rgba::rgb(0xE8D07A),
    danger: Rgba::rgb(0xFF9DA5),
    success: Rgba::rgb(0x7ED6A0),
    hair: Rgba::rgb(0x1F4A52),
    strong: Rgba::rgb(0x2F6670),
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
