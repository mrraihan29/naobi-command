//! Offline calendar, Unix epoch, and ISO-8601 conversions (std only, no network calls).

const SECS_PER_DAY: i64 = 86_400;
const MS_PER_SEC: i64 = 1_000;
const MS_PER_DAY: i64 = SECS_PER_DAY * MS_PER_SEC;

/// Evaluate a date, time, timezone, or epoch query.
///
/// Outer `None` means the input does not match a date/time/epoch shape and should fall through
/// to unit conversion or arithmetic; `Some(None)` means it matched a date/time shape that is
/// invalid (e.g. `2026-02-29`) and must not fall through to arithmetic.
pub fn evaluate(input: &str, now_ms: i64) -> Option<Option<String>> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Some(done) = crate::tz::evaluate(trimmed, now_ms) {
        return Some(Some(done));
    }
    if let Some(done) = eval_date_math(trimmed, now_ms) {
        return Some(done);
    }
    eval_epoch(trimmed, now_ms)
}

fn eval_epoch(s: &str, now_ms: i64) -> Option<Option<String>> {
    // Bare 10-digit seconds or 13-digit milliseconds (no leading zero so phone/PIN strings don't match).
    if (s.len() == 10 || s.len() == 13)
        && s.bytes().all(|b| b.is_ascii_digit())
        && !s.starts_with('0')
    {
        let raw: i64 = s.parse().ok()?;
        let ms = if s.len() == 10 {
            raw.checked_mul(MS_PER_SEC)?
        } else {
            raw
        };
        return Some(format_iso_utc(ms));
    }

    let lower = s.to_ascii_lowercase();
    let words: Vec<&str> = s.split_whitespace().collect();
    let lower_words: Vec<String> = words.iter().map(|w| w.to_ascii_lowercase()).collect();

    // `epoch` / `epoch now` / `unix` / `unix now` / `epoch ms` / `unix ms`
    match lower.as_str() {
        "epoch" | "epoch now" | "unix" | "unix now" => {
            return Some(Some(now_ms.div_euclid(MS_PER_SEC).to_string()));
        }
        "epoch ms" | "epoch millis" | "unix ms" | "unix millis" => {
            return Some(Some(now_ms.to_string()));
        }
        _ => {}
    }

    // `epoch <arg>` or `unix <arg>` or `epoch ms <arg>`
    if matches!(
        lower_words.first().map(String::as_str),
        Some("epoch" | "unix")
    ) {
        let (force_ms, rest) = if lower_words.get(1).map(String::as_str) == Some("ms") {
            (true, words.get(2..)?)
        } else {
            (false, words.get(1..)?)
        };
        if rest.is_empty() {
            return Some(None);
        }
        let arg = rest.join(" ");
        if let Ok(n) = arg.parse::<i64>() {
            let ms = if force_ms || n.abs() >= 100_000_000_000 {
                n
            } else {
                match n.checked_mul(MS_PER_SEC) {
                    Some(v) => v,
                    None => return Some(None),
                }
            };
            return Some(format_iso_utc(ms));
        }
        if let Some(ms) = parse_iso_utc_ms(&arg)? {
            let out = if force_ms {
                ms
            } else {
                ms.div_euclid(MS_PER_SEC)
            };
            return Some(Some(out.to_string()));
        }
        return Some(None);
    }

    // `<iso-8601> to|in epoch|unix|ms`
    if lower_words.len() >= 3 {
        let n = lower_words.len();
        if matches!(lower_words[n - 2].as_str(), "to" | "in") {
            let target_ms = match lower_words[n - 1].as_str() {
                "ms" | "millis" => Some(true),
                "epoch" | "unix" | "s" | "sec" | "seconds" => Some(false),
                _ => None,
            };
            if let Some(as_ms) = target_ms {
                let lhs = words[..n - 2].join(" ");
                if let Some(parsed) = parse_iso_utc_ms(&lhs) {
                    return Some(parsed.map(|ms| {
                        if as_ms {
                            ms.to_string()
                        } else {
                            ms.div_euclid(MS_PER_SEC).to_string()
                        }
                    }));
                }
            }
        }
    }

    // Bare ISO-8601 date or datetime
    if let Some(parsed) = parse_iso_utc_ms(s) {
        return Some(parsed.map(|ms| {
            if ms.rem_euclid(MS_PER_SEC) != 0 {
                ms.to_string()
            } else {
                ms.div_euclid(MS_PER_SEC).to_string()
            }
        }));
    }

    None
}

/// Howard Hinnant's proleptic Gregorian days-from-civil (1970-01-01 = 0).
pub(crate) fn days_from_civil(y: i32, m: u8, d: u8) -> Option<i64> {
    if !(1..=12).contains(&m) || d < 1 || d > days_in_month(y, m) {
        return None;
    }
    let y = i64::from(y) - i64::from(m <= 2);
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let m = i64::from(m);
    let d = i64::from(d);
    let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    Some(era * 146_097 + doe - 719_468)
}

/// Howard Hinnant's inverse: Unix day count -> `(year, month, day)`.
pub(crate) fn civil_from_days(z: i64) -> Option<(i32, u8, u8)> {
    let z = z.checked_add(719_468)?;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = mp + if mp < 10 { 3 } else { -9 };
    let year = i32::try_from(y + i64::from(m <= 2)).ok()?;
    Some((year, m as u8, d as u8))
}

pub(crate) fn is_leap_year(y: i32) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

pub(crate) fn days_in_month(y: i32, m: u8) -> u8 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(y) => 29,
        2 => 28,
        _ => 0,
    }
}

fn format_iso_utc(ms: i64) -> Option<String> {
    let day = ms.div_euclid(MS_PER_DAY);
    let rem_ms = ms.rem_euclid(MS_PER_DAY);
    let (y, m, d) = civil_from_days(day)?;
    if !(0..=9999).contains(&y) {
        return None;
    }
    let total_secs = rem_ms / MS_PER_SEC;
    let millis = rem_ms % MS_PER_SEC;
    let h = total_secs / 3600;
    let min = (total_secs % 3600) / 60;
    let sec = total_secs % 60;
    if millis == 0 {
        Some(format!("{y:04}-{m:02}-{d:02}T{h:02}:{min:02}:{sec:02}Z"))
    } else {
        Some(format!(
            "{y:04}-{m:02}-{d:02}T{h:02}:{min:02}:{sec:02}.{millis:03}Z"
        ))
    }
}

/// Outer `None`: does not look like an ISO-8601 date/datetime.
/// `Some(None)`: looks like ISO-8601 (`YYYY-MM-DD...`) but invalid calendar/time values.
fn parse_iso_utc_ms(s: &str) -> Option<Option<i64>> {
    let b = s.as_bytes();
    if b.len() < 10
        || !b[0..4].iter().all(u8::is_ascii_digit)
        || b[4] != b'-'
        || !b[5..7].iter().all(u8::is_ascii_digit)
        || b[7] != b'-'
        || !b[8..10].iter().all(u8::is_ascii_digit)
    {
        return None;
    }
    let y: i32 = s[0..4].parse().ok()?;
    let m: u8 = s[5..7].parse().ok()?;
    let d: u8 = s[8..10].parse().ok()?;
    let Some(days) = days_from_civil(y, m, d) else {
        return Some(None);
    };
    let rest = &s[10..];
    if rest.is_empty() {
        return Some(days.checked_mul(MS_PER_DAY));
    }
    let time_rest = rest
        .strip_prefix('T')
        .or_else(|| rest.strip_prefix('t'))
        .or_else(|| rest.strip_prefix(' '))?;
    Some(parse_iso_time_and_offset(days, time_rest))
}

fn parse_iso_time_and_offset(days: i64, s: &str) -> Option<i64> {
    let b = s.as_bytes();
    if b.len() < 8
        || !b[0..2].iter().all(u8::is_ascii_digit)
        || b[2] != b':'
        || !b[3..5].iter().all(u8::is_ascii_digit)
        || b[5] != b':'
        || !b[6..8].iter().all(u8::is_ascii_digit)
    {
        return None;
    }
    let hh: i64 = s[0..2].parse().ok()?;
    let mm: i64 = s[3..5].parse().ok()?;
    let ss: i64 = s[6..8].parse().ok()?;
    if hh > 23 || mm > 59 || ss > 59 {
        return None;
    }
    let mut rest = &s[8..];
    let mut millis: i64 = 0;
    if let Some(frac_rest) = rest.strip_prefix('.') {
        let digits_len = frac_rest.bytes().take_while(u8::is_ascii_digit).count();
        if digits_len == 0 {
            return None;
        }
        let frac = &frac_rest[..digits_len];
        millis = match digits_len {
            1 => frac.parse::<i64>().ok()? * 100,
            2 => frac.parse::<i64>().ok()? * 10,
            _ => frac[..3].parse::<i64>().ok()?,
        };
        rest = &frac_rest[digits_len..];
    }
    let offset_secs = parse_iso_offset(rest)?;
    let day_ms = days.checked_mul(MS_PER_DAY)?;
    let time_ms = ((hh * 3600 + mm * 60 + ss) * MS_PER_SEC) + millis;
    day_ms
        .checked_add(time_ms)?
        .checked_sub(offset_secs * MS_PER_SEC)
}

fn parse_iso_offset(s: &str) -> Option<i64> {
    if s.is_empty() || s.eq_ignore_ascii_case("Z") {
        return Some(0);
    }
    let (sign, rest) = match s.as_bytes().first()? {
        b'+' => (1_i64, &s[1..]),
        b'-' => (-1_i64, &s[1..]),
        _ => return None,
    };
    let (hh_str, mm_str) = if let Some((h, m)) = rest.split_once(':') {
        (h, m)
    } else if rest.len() == 4 {
        (&rest[..2], &rest[2..])
    } else if rest.len() == 2 {
        (rest, "00")
    } else {
        return None;
    };
    if hh_str.len() != 2
        || mm_str.len() != 2
        || !hh_str.bytes().all(|b| b.is_ascii_digit())
        || !mm_str.bytes().all(|b| b.is_ascii_digit())
    {
        return None;
    }
    let hh: i64 = hh_str.parse().ok()?;
    let mm: i64 = mm_str.parse().ok()?;
    if hh > 23 || mm > 59 {
        return None;
    }
    Some(sign * (hh * 3600 + mm * 60))
}

#[derive(Clone, Copy)]
enum YearBias {
    Current,
    Future,
    Past,
}

fn format_days(n: i64) -> String {
    if n.abs() == 1 {
        format!("{n} day")
    } else {
        format!("{n} days")
    }
}

fn format_iso_date(days: i64) -> Option<String> {
    let (y, m, d) = civil_from_days(days)?;
    if !(0..=9999).contains(&y) {
        return None;
    }
    Some(format!("{y:04}-{m:02}-{d:02}"))
}

fn eval_date_math(s: &str, now_ms: i64) -> Option<Option<String>> {
    let today_days = now_ms.div_euclid(MS_PER_DAY);
    let lower = s.to_ascii_lowercase();

    for prefix in ["days until ", "days to ", "until "] {
        if let Some(rest) = lower.strip_prefix(prefix) {
            let target = parse_natural_date(rest.trim(), today_days, YearBias::Future)?;
            return Some(Some(format_days(target.checked_sub(today_days)?)));
        }
    }
    for prefix in ["days since ", "days from "] {
        if let Some(rest) = lower.strip_prefix(prefix) {
            let target = parse_natural_date(rest.trim(), today_days, YearBias::Past)?;
            return Some(Some(format_days(today_days.checked_sub(target)?)));
        }
    }
    if let Some(rest) = lower.strip_prefix("days between ") {
        if let Some((a, b)) = rest.split_once(" and ") {
            let d1 = parse_natural_date(a.trim(), today_days, YearBias::Current)?;
            let d2 = parse_natural_date(b.trim(), today_days, YearBias::Current)?;
            return Some(Some(format_days(d2.checked_sub(d1)?.abs())));
        }
    }

    // `<date> +/- <N> <unit>` or `<date1> - <date2>`
    // Require whitespace around `-` so ISO `YYYY-MM-DD` hyphens are never split.
    let op_split = s.find(" + ").map(|i| (i, 1_i64, 3)).or_else(|| {
        s.find(" - ")
            .map(|i| (i, -1_i64, 3))
            .or_else(|| s.find('+').map(|i| (i, 1_i64, 1)))
    });
    if let Some((idx, sign, sep_len)) = op_split {
        let lhs = s[..idx].trim();
        let rhs = s[idx + sep_len..].trim();
        if let Some(base_days) = parse_natural_date(lhs, today_days, YearBias::Current) {
            if sign < 0 {
                if let Some(rhs_days) = parse_natural_date(rhs, today_days, YearBias::Current) {
                    return Some(Some(format_days(base_days.checked_sub(rhs_days)?)));
                }
            }
            if let Some((amount, unit)) = parse_date_offset(rhs) {
                let delta = amount.checked_mul(sign)?;
                return Some(apply_date_offset(base_days, delta, unit));
            }
        }
    }

    None
}

#[derive(Clone, Copy)]
enum DateUnit {
    Day,
    Week,
    Month,
    Year,
}

fn parse_date_offset(s: &str) -> Option<(i64, DateUnit)> {
    let mut parts = s.split_whitespace();
    let num_str = parts.next()?;
    let unit_str = parts.next()?;
    if parts.next().is_some() || num_str.is_empty() || !num_str.bytes().all(|b| b.is_ascii_digit())
    {
        return None;
    }
    let n: i64 = num_str.parse().ok()?;
    let unit = match unit_str.to_ascii_lowercase().as_str() {
        "d" | "day" | "days" => DateUnit::Day,
        "w" | "wk" | "wks" | "week" | "weeks" => DateUnit::Week,
        "mo" | "mon" | "month" | "months" => DateUnit::Month,
        "y" | "yr" | "yrs" | "year" | "years" => DateUnit::Year,
        _ => return None,
    };
    Some((n, unit))
}

fn apply_date_offset(base_days: i64, delta: i64, unit: DateUnit) -> Option<String> {
    match unit {
        DateUnit::Day => format_iso_date(base_days.checked_add(delta)?),
        DateUnit::Week => format_iso_date(base_days.checked_add(delta.checked_mul(7)?)?),
        DateUnit::Month => add_months(base_days, delta),
        DateUnit::Year => add_months(base_days, delta.checked_mul(12)?),
    }
}

fn add_months(base_days: i64, delta_months: i64) -> Option<String> {
    let (y, m, d) = civil_from_days(base_days)?;
    let total = i64::from(y)
        .checked_mul(12)?
        .checked_add(i64::from(m - 1))?
        .checked_add(delta_months)?;
    let new_y = i32::try_from(total.div_euclid(12)).ok()?;
    let new_m = (total.rem_euclid(12) + 1) as u8;
    let clamped_d = d.min(days_in_month(new_y, new_m));
    format_iso_date(days_from_civil(new_y, new_m, clamped_d)?)
}

fn parse_natural_date(s: &str, today_days: i64, bias: YearBias) -> Option<i64> {
    let lower = s.trim().to_ascii_lowercase();
    match lower.as_str() {
        "today" | "now" => return Some(today_days),
        "tomorrow" | "tmrw" => return today_days.checked_add(1),
        "yesterday" => return today_days.checked_sub(1),
        _ => {}
    }

    // Strict `YYYY-MM-DD`
    if lower.len() == 10 {
        let b = lower.as_bytes();
        if b[0..4].iter().all(u8::is_ascii_digit)
            && b[4] == b'-'
            && b[5..7].iter().all(u8::is_ascii_digit)
            && b[7] == b'-'
            && b[8..10].iter().all(u8::is_ascii_digit)
        {
            let y: i32 = lower[0..4].parse().ok()?;
            let m: u8 = lower[5..7].parse().ok()?;
            let d: u8 = lower[8..10].parse().ok()?;
            return days_from_civil(y, m, d);
        }
    }

    let tokens: Vec<&str> = lower
        .split(|c: char| c.is_ascii_whitespace() || c == ',')
        .filter(|t| !t.is_empty())
        .collect();
    let (cur_year, _, _) = civil_from_days(today_days)?;

    let (day, month, explicit_year): (u8, u8, Option<i32>) = match tokens.as_slice() {
        [a, b] => {
            if let (Some(d), Some(m)) = (parse_day_token(a), parse_month_name(b)) {
                (d, m, None)
            } else if let (Some(m), Some(d)) = (parse_month_name(a), parse_day_token(b)) {
                (d, m, None)
            } else {
                return None;
            }
        }
        [a, b, c] => {
            let y: i32 = c.parse().ok()?;
            if !(0..=9999).contains(&y) {
                return None;
            }
            if let (Some(d), Some(m)) = (parse_day_token(a), parse_month_name(b)) {
                (d, m, Some(y))
            } else if let (Some(m), Some(d)) = (parse_month_name(a), parse_day_token(b)) {
                (d, m, Some(y))
            } else {
                return None;
            }
        }
        _ => return None,
    };

    if let Some(y) = explicit_year {
        return days_from_civil(y, month, day);
    }

    match bias {
        YearBias::Current => days_from_civil(cur_year, month, day),
        YearBias::Future => (cur_year..=cur_year + 8).find_map(|y| {
            let d = days_from_civil(y, month, day)?;
            (d >= today_days).then_some(d)
        }),
        YearBias::Past => (cur_year - 8..=cur_year).rev().find_map(|y| {
            let d = days_from_civil(y, month, day)?;
            (d <= today_days).then_some(d)
        }),
    }
}

fn parse_day_token(s: &str) -> Option<u8> {
    let digits = s
        .strip_suffix("st")
        .or_else(|| s.strip_suffix("nd"))
        .or_else(|| s.strip_suffix("rd"))
        .or_else(|| s.strip_suffix("th"))
        .unwrap_or(s);
    if digits.is_empty() || digits.len() > 2 || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let d: u8 = digits.parse().ok()?;
    (1..=31).contains(&d).then_some(d)
}

fn parse_month_name(s: &str) -> Option<u8> {
    match s {
        "jan" | "january" => Some(1),
        "feb" | "february" => Some(2),
        "mar" | "march" => Some(3),
        "apr" | "april" => Some(4),
        "may" => Some(5),
        "jun" | "june" => Some(6),
        "jul" | "july" => Some(7),
        "aug" | "august" => Some(8),
        "sep" | "sept" | "september" => Some(9),
        "oct" | "october" => Some(10),
        "nov" | "november" => Some(11),
        "dec" | "december" => Some(12),
        _ => None,
    }
}
