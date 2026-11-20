//! Pure color parsing and formatting shared across the Color picker and inline Root search.

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParsedColor {
    pub rgb: Rgb,
    pub format: Format,
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

/// Parses `#RGB`, `#RRGGBB`, `rgb(...)`, `hsl(...)`, or `oklch(...)`.
pub fn parse(input: &str) -> Option<ParsedColor> {
    let s = input.trim();
    if let Some(hex) = s.strip_prefix('#') {
        return parse_hex(hex).map(|rgb| ParsedColor {
            rgb,
            format: Format::Hex,
        });
    }
    if let Some([r, g, b]) = split_func_args(s, "rgb") {
        return parse_rgb([r, g, b]).map(|rgb| ParsedColor {
            rgb,
            format: Format::Rgb,
        });
    }
    if let Some([h, sat, lum]) = split_func_args(s, "hsl") {
        return parse_hsl([h, sat, lum]).map(|rgb| ParsedColor {
            rgb,
            format: Format::Hsl,
        });
    }
    if let Some([l, ch, h]) = split_func_args(s, "oklch") {
        return parse_oklch([l, ch, h]).map(|rgb| ParsedColor {
            rgb,
            format: Format::Oklch,
        });
    }
    None
}

fn split_func_args<'a>(s: &'a str, name: &str) -> Option<[&'a str; 3]> {
    let prefix_len = name.len();
    let head = s.get(..prefix_len)?;
    if !head.eq_ignore_ascii_case(name) {
        return None;
    }
    let rest = s.get(prefix_len..)?;
    let inner = rest.strip_prefix('(')?.strip_suffix(')')?.trim();
    if inner.is_empty() {
        return None;
    }
    let parts = if inner.contains(',') {
        let mut it = inner.split(',').map(str::trim);
        let a = it.next().filter(|p| !p.is_empty())?;
        let b = it.next().filter(|p| !p.is_empty())?;
        let c = it.next().filter(|p| !p.is_empty())?;
        if it.next().is_some() {
            return None;
        }
        [a, b, c]
    } else {
        let mut it = inner.split_whitespace();
        let a = it.next()?;
        let b = it.next()?;
        let c = it.next()?;
        if it.next().is_some() {
            return None;
        }
        [a, b, c]
    };
    Some(parts)
}

fn parse_finite_range(s: &str, min: f64, max: f64) -> Option<f64> {
    let v: f64 = s.parse().ok()?;
    (v.is_finite() && v >= min && v <= max).then_some(v)
}

fn parse_deg(s: &str) -> Option<f64> {
    let num = if s.len() >= 3 && s.as_bytes()[s.len() - 3..].eq_ignore_ascii_case(b"deg") {
        s.get(..s.len() - 3)?
    } else {
        s
    };
    parse_finite_range(num, 0.0, 360.0)
}

fn parse_pct(s: &str) -> Option<f64> {
    parse_finite_range(s.strip_suffix('%')?, 0.0, 100.0)
}

fn parse_rgb(args: [&str; 3]) -> Option<Rgb> {
    let parse_chan = |p: &str| -> Option<u8> {
        if p.is_empty() || !p.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        p.parse().ok()
    };
    Some(Rgb {
        r: parse_chan(args[0])?,
        g: parse_chan(args[1])?,
        b: parse_chan(args[2])?,
    })
}

fn parse_hsl(args: [&str; 3]) -> Option<Rgb> {
    let h = parse_deg(args[0])?;
    let s = parse_pct(args[1])? / 100.0;
    let l = parse_pct(args[2])? / 100.0;
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let hp = (h % 360.0) / 60.0;
    let x = c * (1.0 - (hp % 2.0 - 1.0).abs());
    let (r1, g1, b1) = match hp as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = l - c / 2.0;
    let to_u8 = |v: f64| ((v + m) * 255.0).round().clamp(0.0, 255.0) as u8;
    Some(Rgb {
        r: to_u8(r1),
        g: to_u8(g1),
        b: to_u8(b1),
    })
}

fn parse_oklch(args: [&str; 3]) -> Option<Rgb> {
    let lightness = if args[0].ends_with('%') {
        parse_pct(args[0])? / 100.0
    } else {
        parse_finite_range(args[0], 0.0, 1.0)?
    };
    let chroma = parse_finite_range(args[1], 0.0, 0.5)?;
    let hue = parse_deg(args[2])?;
    let h_rad = hue.to_radians();
    let a = chroma * h_rad.cos();
    let bb = chroma * h_rad.sin();

    let l_ = lightness + 0.396_337_777_4 * a + 0.215_803_757_3 * bb;
    let m_ = lightness - 0.105_561_345_8 * a - 0.063_854_172_8 * bb;
    let s_ = lightness - 0.089_484_177_5 * a - 1.291_485_548_0 * bb;

    let l = l_ * l_ * l_;
    let m = m_ * m_ * m_;
    let s = s_ * s_ * s_;

    let r = 4.076_741_662_1 * l - 3.307_711_591_3 * m + 0.230_969_929_2 * s;
    let g = -1.268_438_004_6 * l + 2.609_757_401_1 * m - 0.341_319_396_5 * s;
    let b = -0.004_196_086_3 * l - 0.703_418_614_7 * m + 1.707_614_701_0 * s;

    Some(Rgb {
        r: unlinear(r),
        g: unlinear(g),
        b: unlinear(b),
    })
}

fn unlinear(v: f64) -> u8 {
    let c = v.clamp(0.0, 1.0);
    let s = if c <= 0.003_130_8 {
        12.92 * c
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    };
    (s * 255.0).round().clamp(0.0, 255.0) as u8
}

fn parse_hex(hex: &str) -> Option<Rgb> {
    if !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    match hex.len() {
        3 => {
            let mut it = hex.as_bytes().iter().map(|&b| {
                let v = u8::from_str_radix(std::str::from_utf8(&[b]).ok()?, 16).ok()?;
                Some(v * 17)
            });
            Some(Rgb {
                r: it.next()??,
                g: it.next()??,
                b: it.next()??,
            })
        }
        6 => Some(Rgb {
            r: u8::from_str_radix(&hex[0..2], 16).ok()?,
            g: u8::from_str_radix(&hex[2..4], 16).ok()?,
            b: u8::from_str_radix(&hex[4..6], 16).ok()?,
        }),
        _ => None,
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
