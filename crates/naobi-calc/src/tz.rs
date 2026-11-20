//! Offline timezone table, US/EU/AU/NZ DST transitions, and natural-language time queries.
use crate::time::{civil_from_days, days_from_civil, days_in_month};
use std::borrow::Cow;

const SECS_PER_DAY: i64 = 86_400;
const MS_PER_SEC: i64 = 1_000;

#[derive(Clone, Copy)]
enum DstRule {
    None,
    Us,
    Eu,
    Au,
    Nz,
}

struct ZoneDef {
    names: &'static [&'static str],
    std_abbr: &'static str,
    dst_abbr: &'static str,
    std_mins: i32,
    dst: DstRule,
}

struct ResolvedZone {
    std_abbr: Cow<'static, str>,
    dst_abbr: Cow<'static, str>,
    std_secs: i64,
    dst: DstRule,
}

const fn z(
    names: &'static [&'static str],
    std_abbr: &'static str,
    dst_abbr: &'static str,
    std_mins: i32,
    dst: DstRule,
) -> ZoneDef {
    ZoneDef {
        names,
        std_abbr,
        dst_abbr,
        std_mins,
        dst,
    }
}

const fn fixed(names: &'static [&'static str], abbr: &'static str, mins: i32) -> ZoneDef {
    z(names, abbr, abbr, mins, DstRule::None)
}

#[rustfmt::skip]
static ZONES: &[ZoneDef] = &[
    fixed(&["utc"], "UTC", 0),
    fixed(&["gmt"], "GMT", 0),
    fixed(&["bst"], "BST", 60),
    z(&["london", "uk", "britain", "england", "edinburgh", "dublin", "lisbon"], "GMT", "BST", 0, DstRule::Eu),

    // Americas
    fixed(&["est"], "EST", -300),
    fixed(&["edt"], "EDT", -240),
    z(&["et", "eastern", "new york", "nyc", "ny", "boston", "miami", "atlanta", "washington", "dc", "philadelphia", "toronto", "montreal", "ottawa"], "EST", "EDT", -300, DstRule::Us),

    fixed(&["cst"], "CST", -360),
    fixed(&["cdt"], "CDT", -300),
    z(&["ct", "central", "chicago", "dallas", "houston", "austin", "minneapolis"], "CST", "CDT", -360, DstRule::Us),
    fixed(&["mexico city"], "CST", -360),

    fixed(&["mst"], "MST", -420),
    fixed(&["mdt"], "MDT", -360),
    z(&["mt", "mountain", "denver", "salt lake city", "calgary"], "MST", "MDT", -420, DstRule::Us),
    fixed(&["phoenix", "arizona"], "MST", -420),

    fixed(&["pst"], "PST", -480),
    fixed(&["pdt"], "PDT", -420),
    z(&["pt", "pacific", "los angeles", "la", "san francisco", "sf", "seattle", "san diego", "portland", "las vegas", "vancouver"], "PST", "PDT", -480, DstRule::Us),

    fixed(&["akst"], "AKST", -540),
    fixed(&["akdt"], "AKDT", -480),
    z(&["akt", "anchorage", "alaska"], "AKST", "AKDT", -540, DstRule::Us),
    fixed(&["hst", "honolulu", "hawaii"], "HST", -600),
    fixed(&["brt", "sao paulo", "rio", "rio de janeiro", "brasilia"], "BRT", -180),
    fixed(&["art", "buenos aires"], "ART", -180),

    // Europe / Africa / Middle East
    fixed(&["wet"], "WET", 0),
    fixed(&["west"], "WEST", 60),
    fixed(&["cet"], "CET", 60),
    fixed(&["cest"], "CEST", 120),
    z(&["berlin", "paris", "amsterdam", "rome", "madrid", "barcelona", "munich", "frankfurt", "zurich", "geneva", "vienna", "brussels", "prague", "warsaw", "stockholm", "oslo", "copenhagen", "milan"], "CET", "CEST", 60, DstRule::Eu),
    fixed(&["eet"], "EET", 120),
    fixed(&["eest"], "EEST", 180),
    z(&["athens", "helsinki", "bucharest", "sofia"], "EET", "EEST", 120, DstRule::Eu),
    fixed(&["sast", "johannesburg", "cape town"], "SAST", 120),
    fixed(&["wat", "lagos"], "WAT", 60),
    fixed(&["eat", "nairobi"], "EAT", 180),
    fixed(&["msk", "moscow"], "MSK", 180),
    fixed(&["trt", "istanbul", "ankara", "turkey"], "TRT", 180),
    fixed(&["gst", "dubai", "abu dhabi", "uae"], "GST", 240),

    // Asia / Indonesia
    fixed(&["pkt", "karachi", "lahore", "islamabad"], "PKT", 300),
    fixed(&["ist", "india", "mumbai", "delhi", "new delhi", "bangalore", "bengaluru", "chennai", "hyderabad", "kolkata"], "IST", 330),
    fixed(&["npt", "kathmandu", "nepal"], "NPT", 345),
    fixed(&["bdt", "dhaka", "bangladesh"], "BDT", 360),
    fixed(&["wib", "jakarta", "bandung", "surabaya", "medan", "yogyakarta", "semarang"], "WIB", 420),
    fixed(&["ict", "bangkok", "hanoi", "ho chi minh", "saigon", "phnom penh", "vientiane"], "ICT", 420),
    fixed(&["wita", "bali", "denpasar", "makassar", "balikpapan"], "WITA", 480),
    fixed(&["wit", "jayapura", "ambon", "papua"], "WIT", 540),
    fixed(&["sgt", "singapore", "sg"], "SGT", 480),
    fixed(&["myt", "kuala lumpur", "kl", "malaysia"], "MYT", 480),
    fixed(&["pht", "manila", "philippines"], "PHT", 480),
    fixed(&["hkt", "hong kong", "hk"], "HKT", 480),
    fixed(&["bjt", "beijing", "shanghai", "shenzhen", "guangzhou", "taipei", "china", "taiwan"], "CST", 480),
    fixed(&["jst", "tokyo", "osaka", "kyoto", "japan"], "JST", 540),
    fixed(&["kst", "seoul", "busan", "korea"], "KST", 540),

    // Oceania
    fixed(&["awst", "perth"], "AWST", 480),
    fixed(&["acst"], "ACST", 570),
    fixed(&["acdt"], "ACDT", 630),
    z(&["adelaide"], "ACST", "ACDT", 570, DstRule::Au),
    fixed(&["aest"], "AEST", 600),
    fixed(&["aedt"], "AEDT", 660),
    fixed(&["brisbane"], "AEST", 600),
    z(&["sydney", "melbourne", "canberra", "hobart"], "AEST", "AEDT", 600, DstRule::Au),
    fixed(&["nzst"], "NZST", 720),
    fixed(&["nzdt"], "NZDT", 780),
    z(&["auckland", "wellington", "nz"], "NZST", "NZDT", 720, DstRule::Nz),
];

pub(crate) fn evaluate(s: &str, now_ms: i64) -> Option<String> {
    let lower = s.trim().to_ascii_lowercase();
    let now_secs = now_ms.div_euclid(MS_PER_SEC);

    // `time in <zone>`, `current time in <zone>`, `now in <zone>`
    for prefix in ["current time in ", "time in ", "now in "] {
        if let Some(zone_str) = lower.strip_prefix(prefix) {
            let zone = lookup_zone(zone_str.trim())?;
            return format_instant_in_zone(now_secs, &zone);
        }
    }

    // `<zone> time`
    if let Some(zone_str) = lower.strip_suffix(" time") {
        let zone = lookup_zone(zone_str.trim())?;
        return format_instant_in_zone(now_secs, &zone);
    }

    // `<time> <src_zone> in|to <dst_zone>`
    let words: Vec<&str> = lower.split_whitespace().collect();
    if words.len() >= 4 {
        let sep_idx = words.iter().rposition(|&w| w == "in" || w == "to")?;
        if sep_idx >= 2 && sep_idx + 1 < words.len() {
            let dst_str = words[sep_idx + 1..].join(" ");
            if let Some(dst) = lookup_zone(&dst_str) {
                if let Some(out) = eval_conversion(&words[..sep_idx], &dst, now_secs) {
                    return Some(out);
                }
            }
        }
    }

    None
}

fn eval_conversion(lhs: &[&str], dst: &ResolvedZone, now_secs: i64) -> Option<String> {
    // Support both `5pm est` (`lhs[0]` is time) and `5 pm est` (`lhs[0..2]` is time).
    for time_words in [1usize, 2] {
        if lhs.len() <= time_words {
            continue;
        }
        let zone_str = lhs[time_words..].join(" ");
        let Some(src) = lookup_zone(&zone_str) else {
            continue;
        };
        let time_str = lhs[..time_words].join(" ");
        let (utc_secs, src_day) = if time_str == "now" {
            let (src_off, _) = src.offset_at_utc(now_secs)?;
            (
                now_secs,
                now_secs.checked_add(src_off)?.div_euclid(SECS_PER_DAY),
            )
        } else {
            let secs_of_day = parse_time_of_day(&time_str)?;
            let (cur_src_off, _) = src.offset_at_utc(now_secs)?;
            let src_day = now_secs.checked_add(cur_src_off)?.div_euclid(SECS_PER_DAY);
            let local_secs = src_day
                .checked_mul(SECS_PER_DAY)?
                .checked_add(secs_of_day)?;
            let utc_secs = src.local_to_utc(local_secs)?;
            (utc_secs, src_day)
        };

        let (dst_off, dst_abbr) = dst.offset_at_utc(utc_secs)?;
        let dst_local = utc_secs.checked_add(dst_off)?;
        let dst_day = dst_local.div_euclid(SECS_PER_DAY);
        let rem = dst_local.rem_euclid(SECS_PER_DAY);
        let hh = rem / 3600;
        let mm = (rem % 3600) / 60;
        let day_diff = dst_day - src_day;
        let utc_tag = format_utc_offset(dst_off);
        return Some(if day_diff == 0 {
            format!("{hh:02}:{mm:02} {dst_abbr} ({utc_tag})")
        } else {
            format!("{hh:02}:{mm:02} {dst_abbr} ({day_diff:+}d, {utc_tag})")
        });
    }
    None
}

fn format_instant_in_zone(utc_secs: i64, zone: &ResolvedZone) -> Option<String> {
    let (off, abbr) = zone.offset_at_utc(utc_secs)?;
    let local = utc_secs.checked_add(off)?;
    let day = local.div_euclid(SECS_PER_DAY);
    let rem = local.rem_euclid(SECS_PER_DAY);
    let (y, m, d) = civil_from_days(day)?;
    let hh = rem / 3600;
    let mm = (rem % 3600) / 60;
    let utc_tag = format_utc_offset(off);
    Some(format!(
        "{y:04}-{m:02}-{d:02} {hh:02}:{mm:02} {abbr} ({utc_tag})"
    ))
}

fn format_utc_offset(offset_secs: i64) -> String {
    let sign = if offset_secs < 0 { '-' } else { '+' };
    let abs_mins = offset_secs.abs() / 60;
    let h = abs_mins / 60;
    let m = abs_mins % 60;
    if m == 0 {
        format!("UTC{sign}{h}")
    } else {
        format!("UTC{sign}{h}:{m:02}")
    }
}

fn lookup_zone(name: &str) -> Option<ResolvedZone> {
    let clean = name.trim();
    for def in ZONES {
        if def.names.contains(&clean) {
            return Some(ResolvedZone {
                std_abbr: Cow::Borrowed(def.std_abbr),
                dst_abbr: Cow::Borrowed(def.dst_abbr),
                std_secs: i64::from(def.std_mins) * 60,
                dst: def.dst,
            });
        }
    }
    parse_utc_offset_zone(clean)
}

fn parse_utc_offset_zone(s: &str) -> Option<ResolvedZone> {
    let rest = s.strip_prefix("utc").or_else(|| s.strip_prefix("gmt"))?;
    if rest.is_empty() {
        return None;
    }
    let (sign, mag) = match rest.as_bytes().first()? {
        b'+' => (1_i64, &rest[1..]),
        b'-' => (-1_i64, &rest[1..]),
        _ => return None,
    };
    let (h_str, m_str) = mag.split_once(':').unwrap_or((mag, "0"));
    if h_str.is_empty() || h_str.len() > 2 || m_str.len() > 2 {
        return None;
    }
    let h: i64 = h_str.parse().ok()?;
    let m: i64 = m_str.parse().ok()?;
    if h > 14 || m > 59 {
        return None;
    }
    let secs = sign * (h * 3600 + m * 60);
    let label = format_utc_offset(secs);
    Some(ResolvedZone {
        std_abbr: Cow::Owned(label.clone()),
        dst_abbr: Cow::Owned(label),
        std_secs: secs,
        dst: DstRule::None,
    })
}

impl ResolvedZone {
    fn offset_at_utc(&self, utc_secs: i64) -> Option<(i64, &str)> {
        if matches!(self.dst, DstRule::None) {
            return Some((self.std_secs, &self.std_abbr));
        }
        let is_dst = is_dst_active(utc_secs, self.std_secs, self.dst)?;
        if is_dst {
            Some((self.std_secs + 3600, &self.dst_abbr))
        } else {
            Some((self.std_secs, &self.std_abbr))
        }
    }

    fn local_to_utc(&self, local_secs: i64) -> Option<i64> {
        if matches!(self.dst, DstRule::None) {
            return local_secs.checked_sub(self.std_secs);
        }
        let std_cand = local_secs.checked_sub(self.std_secs)?;
        if let Some((off, _)) = self.offset_at_utc(std_cand) {
            if off == self.std_secs {
                return Some(std_cand);
            }
        }
        let dst_cand = local_secs.checked_sub(self.std_secs + 3600)?;
        if let Some((off, _)) = self.offset_at_utc(dst_cand) {
            if off == self.std_secs + 3600 {
                return Some(dst_cand);
            }
        }
        Some(std_cand)
    }
}

fn is_dst_active(utc_secs: i64, std_secs: i64, rule: DstRule) -> Option<bool> {
    let day = utc_secs.checked_add(std_secs)?.div_euclid(SECS_PER_DAY);
    let (year, _, _) = civil_from_days(day)?;
    match rule {
        DstRule::None => Some(false),
        DstRule::Us => {
            // 2nd Sunday of March 02:00 local std -> 1st Sunday of November 02:00 local dst
            let start_day = nth_weekday_of_month(year, 3, 0, 2)?;
            let end_day = nth_weekday_of_month(year, 11, 0, 1)?;
            let start = start_day * SECS_PER_DAY + 2 * 3600 - std_secs;
            let end = end_day * SECS_PER_DAY + 2 * 3600 - (std_secs + 3600);
            Some(utc_secs >= start && utc_secs < end)
        }
        DstRule::Eu => {
            // Last Sunday of March 01:00 UTC -> Last Sunday of October 01:00 UTC
            let start_day = last_weekday_of_month(year, 3, 0)?;
            let end_day = last_weekday_of_month(year, 10, 0)?;
            let start = start_day * SECS_PER_DAY + 3600;
            let end = end_day * SECS_PER_DAY + 3600;
            Some(utc_secs >= start && utc_secs < end)
        }
        DstRule::Au => {
            // 1st Sunday of October 02:00 local std -> 1st Sunday of April 03:00 local dst
            let end_day = nth_weekday_of_month(year, 4, 0, 1)?;
            let start_day = nth_weekday_of_month(year, 10, 0, 1)?;
            let end = end_day * SECS_PER_DAY + 3 * 3600 - (std_secs + 3600);
            let start = start_day * SECS_PER_DAY + 2 * 3600 - std_secs;
            Some(utc_secs < end || utc_secs >= start)
        }
        DstRule::Nz => {
            // Last Sunday of September 02:00 local std -> 1st Sunday of April 03:00 local dst
            let end_day = nth_weekday_of_month(year, 4, 0, 1)?;
            let start_day = last_weekday_of_month(year, 9, 0)?;
            let end = end_day * SECS_PER_DAY + 3 * 3600 - (std_secs + 3600);
            let start = start_day * SECS_PER_DAY + 2 * 3600 - std_secs;
            Some(utc_secs < end || utc_secs >= start)
        }
    }
}

/// Weekday: 0 = Sun, 1 = Mon, ..., 6 = Sat (1970-01-01 was Thursday = 4).
fn weekday_from_days(z: i64) -> u8 {
    (z + 4).rem_euclid(7) as u8
}

fn nth_weekday_of_month(year: i32, month: u8, weekday: u8, nth: u8) -> Option<i64> {
    let first = days_from_civil(year, month, 1)?;
    let first_wd = weekday_from_days(first);
    let offset = (i64::from(weekday) - i64::from(first_wd)).rem_euclid(7);
    let day = 1 + offset + i64::from(nth - 1) * 7;
    let dim = i64::from(days_in_month(year, month));
    (day <= dim).then_some(first + day - 1)
}

fn last_weekday_of_month(year: i32, month: u8, weekday: u8) -> Option<i64> {
    let dim = days_in_month(year, month);
    let last = days_from_civil(year, month, dim)?;
    let last_wd = weekday_from_days(last);
    let back = (i64::from(last_wd) - i64::from(weekday)).rem_euclid(7);
    Some(last - back)
}

/// Parses `5pm`, `5:30pm`, `5:30 pm`, `12am`, `17:00`, `17:00:00`. Rejects bare numbers like `5`.
fn parse_time_of_day(s: &str) -> Option<i64> {
    let compact: String = s.chars().filter(|c| !c.is_ascii_whitespace()).collect();
    let (core, am_pm) = if let Some(c) = compact.strip_suffix("am") {
        (c, Some(false))
    } else if let Some(c) = compact.strip_suffix("pm") {
        (c, Some(true))
    } else {
        (compact.as_str(), None)
    };

    let parse_u = |t: &str| -> Option<i64> {
        (!t.is_empty() && t.bytes().all(|b| b.is_ascii_digit()))
            .then(|| t.parse().ok())
            .flatten()
    };

    let (hh, mm, ss) = if let Some((h_s, rest)) = core.split_once(':') {
        let h = parse_u(h_s)?;
        let (m_s, s_s) = rest.split_once(':').unwrap_or((rest, "00"));
        if m_s.len() != 2 || s_s.len() != 2 {
            return None;
        }
        (h, parse_u(m_s)?, parse_u(s_s)?)
    } else if am_pm.is_some() {
        (parse_u(core)?, 0, 0)
    } else {
        return None;
    };

    if !(0..=59).contains(&mm) || !(0..=59).contains(&ss) {
        return None;
    }

    let hh24 = match am_pm {
        Some(is_pm) => {
            if !(1..=12).contains(&hh) {
                return None;
            }
            match (hh, is_pm) {
                (12, false) => 0,
                (12, true) => 12,
                (h, false) => h,
                (h, true) => h + 12,
            }
        }
        None => {
            if !(0..=23).contains(&hh) {
                return None;
            }
            hh
        }
    };

    Some(hh24 * 3600 + mm * 60 + ss)
}
