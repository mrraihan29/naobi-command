//! Extended offline calculator utilities: WCAG contrast, color mixing, tip/VAT/split,
//! fraction conversion, rounding, ISO week number, and time-left queries.
use crate::format::sig10;
use crate::parse::eval;
use crate::time::{civil_from_days, is_leap_year};

const MS_PER_DAY: i64 = 86_400_000;

/// Evaluates extended offline queries. Returns `None` if `input` does not match any extension.
pub fn evaluate(input: &str, ans: Option<f64>, now_ms: i64) -> Option<String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return None;
    }
    let words: Vec<&str> = trimmed.split_whitespace().collect();
    let lower: Vec<String> = words.iter().map(|w| w.to_ascii_lowercase()).collect();

    if let Some(res) = eval_contrast(&words, &lower) {
        return Some(res);
    }
    if let Some(res) = eval_mix(&words, &lower) {
        return Some(res);
    }
    if let Some(res) = eval_week(&lower, now_ms) {
        return Some(res);
    }
    if let Some(res) = eval_time_left(&lower, now_ms) {
        return Some(res);
    }
    if let Some(res) = eval_rounding(&words, &lower, ans) {
        return Some(res);
    }
    if let Some(res) = eval_fraction(trimmed, &words, &lower, ans) {
        return Some(res);
    }
    if let Some(res) = eval_tip_vat_split(&words, &lower, ans) {
        return Some(res);
    }
    None
}

pub fn parse_srgb_token(tok: &str) -> Option<(u8, u8, u8)> {
    let t = tok.trim();
    if let Some(hex) = t.strip_prefix('#') {
        if hex.len() == 3 && hex.bytes().all(|b| b.is_ascii_hexdigit()) {
            let r = u8::from_str_radix(&hex[0..1], 16).ok()? * 17;
            let g = u8::from_str_radix(&hex[1..2], 16).ok()? * 17;
            let b = u8::from_str_radix(&hex[2..3], 16).ok()? * 17;
            return Some((r, g, b));
        }
        if hex.len() == 6 && hex.bytes().all(|b| b.is_ascii_hexdigit()) {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            return Some((r, g, b));
        }
        return None;
    }
    let lower = t.to_ascii_lowercase();
    let inner = lower.strip_prefix("rgb(")?.strip_suffix(')')?;
    let parts: Vec<&str> = inner.split(',').collect();
    if parts.len() != 3 {
        return None;
    }
    let r: u8 = parts[0].trim().parse().ok()?;
    let g: u8 = parts[1].trim().parse().ok()?;
    let b: u8 = parts[2].trim().parse().ok()?;
    Some((r, g, b))
}

fn rel_luminance((r, g, b): (u8, u8, u8)) -> f64 {
    let chan = |v: u8| {
        let s = f64::from(v) / 255.0;
        if s <= 0.03928 {
            s / 12.92
        } else {
            ((s + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * chan(r) + 0.7152 * chan(g) + 0.0722 * chan(b)
}

fn wcag_contrast(a: (u8, u8, u8), b: (u8, u8, u8)) -> f64 {
    let l1 = rel_luminance(a);
    let l2 = rel_luminance(b);
    let (hi, lo) = if l1 >= l2 { (l1, l2) } else { (l2, l1) };
    (hi + 0.05) / (lo + 0.05)
}

/// Joins `rgb(r, g, b)` tokens split across spaces (e.g. `rgb(255, 0, 0)`).
fn collect_color_args(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut depth = 0usize;
    for ch in s.chars() {
        match ch {
            '(' => {
                depth += 1;
                cur.push(ch);
            }
            ')' => {
                depth = depth.saturating_sub(1);
                cur.push(ch);
            }
            c if c.is_whitespace() && depth == 0 => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
            c => cur.push(c),
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

fn eval_contrast(words: &[&str], lower: &[String]) -> Option<String> {
    if lower.first().map(String::as_str) != Some("contrast") {
        return None;
    }
    let rest = words.get(1..)?.join(" ");
    let args = collect_color_args(&rest);
    if args.len() != 2 {
        return None;
    }
    let c1 = parse_srgb_token(&args[0])?;
    let c2 = parse_srgb_token(&args[1])?;
    let ratio = wcag_contrast(c1, c2);
    let level = if ratio >= 7.0 {
        "AAA / AA"
    } else if ratio >= 4.5 {
        "AA"
    } else if ratio >= 3.0 {
        "AA Large"
    } else {
        "Fail"
    };
    Some(format!("{ratio:.2}:1 ({level})"))
}

pub fn mix_colors(input: &str) -> Option<(u8, u8, u8)> {
    let trimmed = input.trim();
    let (head, rest) = trimmed.split_once(char::is_whitespace)?;
    if !head.eq_ignore_ascii_case("mix") {
        return None;
    }
    let args = collect_color_args(rest);
    if !(2..=3).contains(&args.len()) {
        return None;
    }
    let c1 = parse_srgb_token(&args[0])?;
    let c2 = parse_srgb_token(&args[1])?;
    let ratio = if let Some(r) = args.get(2) {
        if let Some(pct) = r.strip_suffix('%') {
            pct.trim().parse::<f64>().ok()? / 100.0
        } else {
            r.parse::<f64>().ok()?
        }
    } else {
        0.5
    };
    if !ratio.is_finite() || !(0.0..=1.0).contains(&ratio) {
        return None;
    }
    let blend = |a: u8, b: u8| -> u8 {
        ((f64::from(a) * (1.0 - ratio) + f64::from(b) * ratio)
            .round()
            .clamp(0.0, 255.0)) as u8
    };
    Some((blend(c1.0, c2.0), blend(c1.1, c2.1), blend(c1.2, c2.2)))
}

fn eval_mix(words: &[&str], lower: &[String]) -> Option<String> {
    if lower.first().map(String::as_str) != Some("mix") {
        return None;
    }
    let (r, g, b) = mix_colors(&words.join(" "))?;
    Some(format!("#{r:02X}{g:02X}{b:02X}"))
}

fn eval_week(lower: &[String], now_ms: i64) -> Option<String> {
    if (lower.len() == 1 && lower[0] == "week")
        || (lower.len() == 2 && lower[0] == "week" && lower[1] == "number")
    {
        let day = now_ms.div_euclid(MS_PER_DAY);
        let (y, m, d) = civil_from_days(day)?;
        let wk = iso_week(y, m, d)?;
        return Some(format!("Week {wk}"));
    }
    if lower.len() == 2 && lower[0] == "week" {
        let parts: Vec<&str> = lower[1].split('-').collect();
        if parts.len() == 3 {
            let y: i32 = parts[0].parse().ok()?;
            let m: u8 = parts[1].parse().ok()?;
            let d: u8 = parts[2].parse().ok()?;
            let wk = iso_week(y, m, d)?;
            return Some(format!("Week {wk}"));
        }
    }
    None
}

fn day_of_year(y: i32, m: u8, d: u8) -> Option<i32> {
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    let cum = [0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334];
    let leap = i32::from(m > 2 && is_leap_year(y));
    Some(cum[(m - 1) as usize] + i32::from(d) + leap)
}

fn iso_week(y: i32, m: u8, d: u8) -> Option<u32> {
    let doy = day_of_year(y, m, d)?;
    // Sakamoto/Tomohiko day-of-week: 1=Mon..7=Sun
    let dow = {
        let t = [0, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4];
        let y_adj = y - i32::from(m < 3);
        let raw = (y_adj + y_adj / 4 - y_adj / 100 + y_adj / 400 + t[(m - 1) as usize] + i32::from(d))
            .rem_euclid(7);
        if raw == 0 {
            7
        } else {
            raw
        }
    };
    let w = (doy - dow + 10) / 7;
    if w < 1 {
        let prev = y - 1;
        iso_week(prev, 12, 31)
    } else if w == 53 {
        let dec31_dow = {
            let t = [0, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4];
            let raw = (y + y / 4 - y / 100 + y / 400 + t[11] + 31).rem_euclid(7);
            if raw == 0 {
                7
            } else {
                raw
            }
        };
        if dec31_dow < 4 {
            Some(1)
        } else {
            Some(53)
        }
    } else {
        Some(w as u32)
    }
}

fn eval_time_left(lower: &[String], now_ms: i64) -> Option<String> {
    let joined = lower.join(" ");
    if matches!(joined.as_str(), "time left" | "time left today") {
        let rem_secs = (MS_PER_DAY - now_ms.rem_euclid(MS_PER_DAY)) / 1000;
        let hours = rem_secs / 3600;
        let mins = (rem_secs % 3600) / 60;
        return Some(format!("{hours}h {mins}m left today"));
    }
    if matches!(
        joined.as_str(),
        "days left in year" | "time left in year" | "days left"
    ) {
        let day = now_ms.div_euclid(MS_PER_DAY);
        let (y, m, d) = civil_from_days(day)?;
        let doy = day_of_year(y, m, d)?;
        let total = if is_leap_year(y) { 366 } else { 365 };
        return Some(format!("{} days", total - doy));
    }
    None
}

fn eval_rounding(words: &[&str], lower: &[String], ans: Option<f64>) -> Option<String> {
    let first = lower.first().map(String::as_str)?;
    if !matches!(first, "round" | "ceil" | "floor") || words.len() < 2 {
        return None;
    }
    if first == "round" && lower.len() >= 4 {
        let n = lower.len();
        if lower[n - 3] == "to" && matches!(lower[n - 1].as_str(), "dp" | "decimal" | "decimals") {
            let places: usize = lower[n - 2].parse().ok()?;
            if places <= 10 {
                let (v, _) = eval(&words[1..n - 3].join(" "), ans)?;
                let factor = 10f64.powi(places as i32);
                let rounded = (v.f * factor).round() / factor;
                return Some(format!("{rounded:.places$}"));
            }
        }
    }
    let (v, _) = eval(&words[1..].join(" "), ans)?;
    let out = match first {
        "ceil" => v.f.ceil(),
        "floor" => v.f.floor(),
        _ => v.f.round(),
    };
    sig10(out)
}

fn eval_fraction(_raw: &str, words: &[&str], lower: &[String], ans: Option<f64>) -> Option<String> {
    let n = lower.len();
    if n >= 3 && matches!(lower[n - 2].as_str(), "to" | "in") && lower[n - 1] == "fraction" {
        let (v, _) = eval(&words[..n - 2].join(" "), ans)?;
        return float_to_fraction(v.f);
    }
    None
}

fn float_to_fraction(x: f64) -> Option<String> {
    if !x.is_finite() {
        return None;
    }
    let sign = if x < 0.0 { "-" } else { "" };
    let val = x.abs();
    let max_den = 10_000i64;
    let mut best_num = val.round() as i64;
    let mut best_den = 1i64;
    let mut best_err = (val - best_num as f64).abs();
    for den in 1..=max_den {
        let num = (val * den as f64).round() as i64;
        let err = (val - num as f64 / den as f64).abs();
        if err < best_err - 1e-12 {
            best_err = err;
            best_num = num;
            best_den = den;
            if err < 1e-9 {
                break;
            }
        }
    }
    let g = gcd(best_num, best_den);
    let (num, den) = (best_num / g, best_den / g);
    if den == 1 {
        Some(format!("{sign}{num}"))
    } else {
        Some(format!("{sign}{num}/{den}"))
    }
}

fn gcd(mut a: i64, mut b: i64) -> i64 {
    while b != 0 {
        let r = a % b;
        a = b;
        b = r;
    }
    a.max(1)
}

fn parse_pct(s: &str) -> Option<f64> {
    let raw = s.strip_suffix('%')?;
    let p = raw.parse::<f64>().ok()?;
    p.is_finite().then_some(p)
}

fn eval_tip_vat_split(words: &[&str], lower: &[String], ans: Option<f64>) -> Option<String> {
    match lower.first().map(String::as_str)? {
        "tip" | "vat" => {
            // `tip 15% on 80` / `vat 11% on 100` / `tip 15% of 80` / `tip 80 15%`
            if words.len() == 4 && matches!(lower[2].as_str(), "on" | "of") {
                let pct = parse_pct(words[1])?;
                let (amt, _) = eval(words[3], ans)?;
                let extra = amt.f * pct / 100.0;
                if lower[2] == "of" {
                    return sig10(extra);
                }
                let total = sig10(amt.f + extra)?;
                let tip_s = sig10(extra)?;
                return Some(format!("{total} (+{tip_s})"));
            }
            if words.len() == 3 {
                if let (Some(pct), Some((amt, _))) =
                    (parse_pct(words[2]), eval(words[1], ans))
                {
                    let extra = amt.f * pct / 100.0;
                    let total = sig10(amt.f + extra)?;
                    let tip_s = sig10(extra)?;
                    return Some(format!("{total} (+{tip_s})"));
                }
            }
            None
        }
        "split" => {
            // `split 120 / 4` or `split 120 between 4` or `split 120 4`
            let (amt_s, people_s) = if words.len() == 4
                && matches!(lower[2].as_str(), "/" | "by" | "between" | "among")
            {
                (words[1], words[3])
            } else if words.len() == 3 {
                (words[1], words[2])
            } else {
                return None;
            };
            let (amt, _) = eval(amt_s, ans)?;
            let (people, _) = eval(people_s, ans)?;
            if people.f <= 0.0 {
                return None;
            }
            let share = sig10(amt.f / people.f)?;
            Some(format!("{share} each"))
        }
        _ => None,
    }
}
