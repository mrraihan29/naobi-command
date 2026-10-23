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

/// Evaluate launcher input. `None` means "show no calculator row".
pub fn evaluate(input: &str) -> Option<CalcOutcome> {
    let words: Vec<&str> = input.split_whitespace().collect();
    let n = words.len();
    if n >= 3 && words[n - 2].eq_ignore_ascii_case("to") {
        let base = match words[n - 1].to_ascii_lowercase().as_str() {
            "hex" => Some(16),
            "bin" => Some(2),
            _ => None,
        };
        if let Some(base) = base {
            return to_base(&words[..n - 2].join(" "), base);
        }
    }
    if let Some(done) = convert(&words) {
        return done;
    }
    let (v, ops) = parse::eval(input)?;
    if ops == 0 {
        return None; // bare number
    }
    Some(CalcOutcome {
        text: format::sig10(v.f)?,
        kind: Kind::Number,
    })
}

fn to_base(expr: &str, base: u32) -> Option<CalcOutcome> {
    let (v, _) = parse::eval(expr)?;
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
fn convert(words: &[&str]) -> Option<Option<CalcOutcome>> {
    let n = words.len();
    if n < 3
        || !(words[n - 2].eq_ignore_ascii_case("to") || words[n - 2].eq_ignore_ascii_case("in"))
    {
        return None;
    }
    let lhs = words[..n - 2].concat();
    let split = lhs.find(|c: char| c.is_ascii_alphabetic())?;
    let value: f64 = lhs[..split].parse().ok()?;
    let from = units::lookup(&lhs[split..])?;
    let to = units::lookup(words[n - 1])?;
    if from.cat != to.cat {
        return Some(None);
    }
    let out = format::sig10(units::convert(value, from, to));
    Some(out.map(|num| CalcOutcome {
        text: format!("{num} {}", words[n - 1]),
        kind: Kind::Conversion,
    }))
}
