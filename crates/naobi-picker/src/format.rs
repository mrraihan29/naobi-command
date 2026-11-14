//! One color in four notations. HEX is what a pick copies; the others are copy actions in the
//! Color History view.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Hex,
    Rgb,
    Hsl,
    Oklch,
}

impl Format {
    pub const ALL: [Format; 4] = [Format::Hex, Format::Rgb, Format::Hsl, Format::Oklch];

    pub fn id(self) -> &'static str {
        match self {
            Format::Hex => "hex",
            Format::Rgb => "rgb",
            Format::Hsl => "hsl",
            Format::Oklch => "oklch",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Format::Hex => "Copy as HEX",
            Format::Rgb => "Copy as RGB",
            Format::Hsl => "Copy as HSL",
            Format::Oklch => "Copy as OKLCH",
        }
    }

    pub fn of_id(id: &str) -> Option<Format> {
        Self::ALL.into_iter().find(|f| f.id() == id)
    }
}

impl Rgb {
    pub fn hex(self) -> String {
        format(self, Format::Hex)
    }

    /// `0xRRGGBB`, for the row's swatch.
    pub fn packed(self) -> u32 {
        u32::from_be_bytes([0, self.r, self.g, self.b])
    }

    pub fn from_packed(v: u32) -> Rgb {
        let [_, r, g, b] = v.to_be_bytes();
        Rgb { r, g, b }
    }
}

pub fn format(c: Rgb, f: Format) -> String {
    match f {
        Format::Hex => format!("#{:02X}{:02X}{:02X}", c.r, c.g, c.b),
        Format::Rgb => format!("rgb({}, {}, {})", c.r, c.g, c.b),
        Format::Hsl => {
            let (h, s, l) = hsl(c);
            format!("hsl({h}, {s}%, {l}%)")
        }
        Format::Oklch => {
            let (l, ch, h) = oklch(c);
            format!("oklch({:.1}% {ch:.3} {h:.1})", l * 100.0)
        }
    }
}

fn hsl(c: Rgb) -> (u32, u32, u32) {
    let (r, g, b) = (
        f64::from(c.r) / 255.0,
        f64::from(c.g) / 255.0,
        f64::from(c.b) / 255.0,
    );
    let (max, min) = (r.max(g).max(b), r.min(g).min(b));
    let l = (max + min) / 2.0;
    let d = max - min;
    if d == 0.0 {
        return (0, 0, (l * 100.0).round() as u32);
    }
    let s = d / (1.0 - (2.0 * l - 1.0).abs());
    let h = if max == r {
        ((g - b) / d).rem_euclid(6.0)
    } else if max == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    } * 60.0;
    (
        (h.round() as u32) % 360,
        (s * 100.0).round() as u32,
        (l * 100.0).round() as u32,
    )
}

fn linear(v: u8) -> f64 {
    let v = f64::from(v) / 255.0;
    if v <= 0.04045 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}

/// Lightness 0..1, chroma, hue in degrees (0 for a gray, whose hue means nothing).
fn oklch(c: Rgb) -> (f64, f64, f64) {
    let (r, g, b) = (linear(c.r), linear(c.g), linear(c.b));
    let l = (0.412_221_470_8 * r + 0.536_332_536_3 * g + 0.051_445_992_9 * b).cbrt();
    let m = (0.211_903_498_2 * r + 0.680_699_545_1 * g + 0.107_396_956_6 * b).cbrt();
    let s = (0.088_302_461_9 * r + 0.281_718_837_6 * g + 0.629_978_700_5 * b).cbrt();
    let lightness = 0.210_454_255_3 * l + 0.793_617_785_0 * m - 0.004_072_046_8 * s;
    let a = 1.977_998_495_1 * l - 2.428_592_205_0 * m + 0.450_593_709_9 * s;
    let bb = 0.025_904_037_1 * l + 0.782_771_766_2 * m - 0.808_675_766_0 * s;
    let chroma = a.hypot(bb);
    let hue = if chroma < 1e-4 {
        0.0
    } else {
        bb.atan2(a).to_degrees().rem_euclid(360.0)
    };
    (lightness, chroma, hue)
}
