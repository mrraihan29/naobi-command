//! Pure calculator and unit-conversion seam (R-014, D-017). Std only, no adapters.

mod format;
mod parse;
mod units;

/// What kind of result `CalcOutcome::text` holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Number,
    Conversion,
    Base,
}

/// A displayable result; `text` is the string Enter copies.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CalcOutcome {
    pub text: String,
    pub kind: Kind,
}

/// Options controlling calculator evaluation (`ans` chaining and CSS `rem` base pixel size).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EvalOptions {
    pub ans: Option<f64>,
    pub rem_base_px: f64,
}

impl Default for EvalOptions {
    fn default() -> Self {
        Self {
            ans: None,
            rem_base_px: 16.0,
        }
    }
}

/// Evaluate launcher input with default options. `None` means "show no calculator row".
pub fn evaluate(input: &str) -> Option<CalcOutcome> {
    evaluate_with(input, EvalOptions::default())
}

/// Evaluate launcher input with `opts`. `None` means "show no calculator row".
pub fn evaluate_with(input: &str, opts: EvalOptions) -> Option<CalcOutcome> {
    let words: Vec<&str> = input.split_whitespace().collect();
    let n = words.len();
    if n >= 3 && words[n - 2].eq_ignore_ascii_case("to") {
        let base = match words[n - 1].to_ascii_lowercase().as_str() {
            "hex" => Some(16),
            "bin" => Some(2),
            _ => None,
        };
        if let Some(base) = base {
            return to_base(&words[..n - 2].join(" "), base, opts.ans);
        }
    }
    if let Some(done) = convert(&words, opts) {
        return done;
    }
    let (v, ops) = parse::eval(input, opts.ans)?;
    if ops == 0 {
        return None; // bare number
    }
    Some(CalcOutcome {
        text: format::sig10(v.f)?,
        kind: Kind::Number,
    })
}

/// Extracts a finite numeric value from a stored `CalcOutcome::text` for `ans` chaining.
pub fn parse_ans(result: &str) -> Option<f64> {
    let tok = result.split_whitespace().next()?;
    let (neg, unsigned) = match tok.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, tok),
    };
    let val = if let Some(hex) = unsigned
        .strip_prefix("0x")
        .or_else(|| unsigned.strip_prefix("0X"))
    {
        let mag = i128::from_str_radix(hex, 16).ok()? as f64;
        if neg {
            -mag
        } else {
            mag
        }
    } else if let Some(bin) = unsigned
        .strip_prefix("0b")
        .or_else(|| unsigned.strip_prefix("0B"))
    {
        let mag = i128::from_str_radix(bin, 2).ok()? as f64;
        if neg {
            -mag
        } else {
            mag
        }
    } else {
        tok.parse::<f64>().ok()?
    };
    val.is_finite().then_some(val)
}

fn to_base(expr: &str, base: u32, ans: Option<f64>) -> Option<CalcOutcome> {
    let (v, _) = parse::eval(expr, ans)?;
    // Exact i128 path; fall back to f64 only for integers below 2^53.
    let i = v.i.or_else(|| {
        (v.f.fract() == 0.0 && v.f.abs() < 9_007_199_254_740_992.0).then_some(v.f as i128)
    })?;
    let (sign, mag) = (if i < 0 { "-" } else { "" }, i.unsigned_abs());
    let text = if base == 16 {
        format!("{sign}0x{mag:X}")
    } else {
        format!("{sign}0b{mag:b}")
    };
    Some(CalcOutcome {
        text,
        kind: Kind::Base,
    })
}

/// `<number> <unit> to|in <unit>`. Outer `None`: not a conversion shape (fall through);
/// `Some(None)`: a conversion that shows nothing (cross-category, overflow).
fn convert(words: &[&str], opts: EvalOptions) -> Option<Option<CalcOutcome>> {
    let n = words.len();
    if n < 3
        || !(words[n - 2].eq_ignore_ascii_case("to") || words[n - 2].eq_ignore_ascii_case("in"))
    {
        return None;
    }
    let lhs = words[..n - 2].concat();
    let split = lhs.find(|c: char| c.is_ascii_alphabetic())?;
    let (value, from_unit) =
        if split == 0 && lhs.get(..3).is_some_and(|p| p.eq_ignore_ascii_case("ans")) {
            (opts.ans.filter(|v| v.is_finite())?, &lhs[3..])
        } else {
            (lhs[..split].parse().ok()?, &lhs[split..])
        };
    let converted = if let Some(css) = convert_css(value, from_unit, words[n - 1], opts.rem_base_px)
    {
        match css {
            Some(v) => v,
            None => return Some(None),
        }
    } else {
        let from = units::lookup(from_unit)?;
        let to = units::lookup(words[n - 1])?;
        if from.cat != to.cat {
            return Some(None);
        }
        units::convert(value, from, to)
    };
    let out = format::sig10(converted);
    Some(out.map(|num| CalcOutcome {
        text: format!("{num} {}", words[n - 1]),
        kind: Kind::Conversion,
    }))
}

fn convert_css(value: f64, from: &str, to: &str, rem_base_px: f64) -> Option<Option<f64>> {
    let is_css = |u: &str| u.eq_ignore_ascii_case("px") || u.eq_ignore_ascii_case("rem");
    if !is_css(from) {
        return None;
    }
    if !is_css(to) || !rem_base_px.is_finite() || rem_base_px <= 0.0 {
        return Some(None);
    }
    let out = match (
        from.to_ascii_lowercase().as_str(),
        to.to_ascii_lowercase().as_str(),
    ) {
        ("px", "rem") => value / rem_base_px,
        ("rem", "px") => value * rem_base_px,
        _ => value,
    };
    Some(Some(out))
}
