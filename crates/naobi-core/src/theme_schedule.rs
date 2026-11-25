//! Pure theme schedule evaluation and solar sunrise/sunset math (no OS calls, no location API).
use crate::settings::ThemeSchedule;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScheduleTime {
    pub year: i32,
    pub month: u32,
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
    pub second: u32,
    pub utc_offset_minutes: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScheduleDecision {
    pub want_dark: bool,
    pub next_delay_ms: u64,
    pub boundary_id: i64,
}

#[derive(Debug, Default)]
pub struct ScheduleState {
    last_boundary: Option<i64>,
}

impl ScheduleState {
    pub fn reset(&mut self) {
        self.last_boundary = None;
    }

    /// Evaluates `schedule` at `now`. Returns `(apply_target, next_delay_ms)` where `apply_target`
    /// is `Some((want_dark, include_system))` only when entering a new window (so manual toggles
    /// are preserved until the next scheduled transition).
    pub fn step(
        &mut self,
        schedule: &ThemeSchedule,
        now: ScheduleTime,
    ) -> Option<(Option<(bool, bool)>, u64)> {
        let dec = evaluate(schedule, now)?;
        let apply = if self.last_boundary == Some(dec.boundary_id) {
            None
        } else {
            self.last_boundary = Some(dec.boundary_id);
            Some((dec.want_dark, schedule.target == "both"))
        };
        Some((apply, dec.next_delay_ms))
    }
}

/// Parses `"HH:MM"` into minutes since midnight (`0..1440`).
pub fn parse_hhmm(s: &str) -> Option<u32> {
    let (hh, mm) = s.trim().split_once(':')?;
    if hh.len() != 2 || mm.len() != 2 {
        return None;
    }
    let h: u32 = hh.parse().ok()?;
    let m: u32 = mm.parse().ok()?;
    (h < 24 && m < 60).then_some(h * 60 + m)
}

/// Evaluates `schedule` at `now`, returning whether dark mode is active, ms until next boundary,
/// and a monotonic boundary identifier.
pub fn evaluate(schedule: &ThemeSchedule, now: ScheduleTime) -> Option<ScheduleDecision> {
    if !schedule.enabled {
        return None;
    }
    let (light_min, dark_min) = match schedule.mode.as_str() {
        "fixed" => (parse_hhmm(&schedule.light_at)?, parse_hhmm(&schedule.dark_at)?),
        "solar" => sunrise_sunset_local_minutes(
            now.year,
            now.month,
            now.day,
            schedule.latitude,
            schedule.longitude,
            now.utc_offset_minutes,
            schedule.offset_minutes,
        )?,
        _ => return None,
    };
    if light_min == dark_min {
        return None;
    }

    let now_sec = (now.hour * 3600 + now.minute * 60 + now.second) as i64;
    let light_sec = (light_min * 60) as i64;
    let dark_sec = (dark_min * 60) as i64;
    let day = days_from_civil(now.year, now.month, now.day);

    let (want_dark, next_sec_in_day, boundary_day) = if light_sec < dark_sec {
        if now_sec < light_sec {
            (true, light_sec, day - 1)
        } else if now_sec < dark_sec {
            (false, dark_sec, day)
        } else {
            (true, 86_400 + light_sec, day)
        }
    } else {
        // Inverted (dark_at before light_at, e.g. dark 02:00, light 14:00)
        if now_sec < dark_sec {
            (false, dark_sec, day - 1)
        } else if now_sec < light_sec {
            (true, light_sec, day)
        } else {
            (false, 86_400 + dark_sec, day)
        }
    };

    let delay_secs = (next_sec_in_day - now_sec).max(1) as u64;
    Some(ScheduleDecision {
        want_dark,
        next_delay_ms: delay_secs * 1000,
        boundary_id: (boundary_day << 1) | i64::from(want_dark),
    })
}

/// Computes local `(sunrise_min, sunset_min)` in `0..1440` using standard NOAA solar geometry.
/// Returns `None` at polar latitudes without sunrise/sunset on that date.
pub fn sunrise_sunset_local_minutes(
    year: i32,
    month: u32,
    day: u32,
    lat_deg: f64,
    lon_deg: f64,
    utc_offset_minutes: i32,
    offset_minutes: i32,
) -> Option<(u32, u32)> {
    let doy = day_of_year(year, month, day)? as f64;
    let days_in_year = if is_leap(year) { 366.0 } else { 365.0 };
    // Approximate solar noon fractional year
    let gamma = 2.0 * std::f64::consts::PI / days_in_year * (doy - 1.0);
    let eqtime = 229.18
        * (0.000075
            + 0.001868 * gamma.cos()
            - 0.032077 * gamma.sin()
            - 0.014615 * (2.0 * gamma).cos()
            - 0.040849 * (2.0 * gamma).sin());
    let decl = 0.006918
        - 0.399912 * gamma.cos()
        + 0.070257 * gamma.sin()
        - 0.006758 * (2.0 * gamma).cos()
        + 0.000907 * (2.0 * gamma).sin()
        - 0.002697 * (3.0 * gamma).cos()
        + 0.00148 * (3.0 * gamma).sin();
    let lat = lat_deg.to_radians();
    let zenith = 90.833_f64.to_radians();
    let cos_ha = (zenith.cos() / (lat.cos() * decl.cos())) - (lat.tan() * decl.tan());
    if !(-1.0..=1.0).contains(&cos_ha) {
        return None;
    }
    let ha_deg = cos_ha.acos().to_degrees();
    let sunrise_utc = 720.0 - 4.0 * (lon_deg + ha_deg) - eqtime;
    let sunset_utc = 720.0 - 4.0 * (lon_deg - ha_deg) - eqtime;
    let shift = f64::from(utc_offset_minutes + offset_minutes);
    let wrap = |m: f64| -> u32 {
        let r = m.round() as i32;
        r.rem_euclid(1440) as u32
    };
    Some((wrap(sunrise_utc + shift), wrap(sunset_utc + shift)))
}

fn is_leap(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn day_of_year(year: i32, month: u32, day: u32) -> Option<u32> {
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    let cum = [0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334];
    let leap_extra = u32::from(month > 2 && is_leap(year));
    Some(cum[(month - 1) as usize] + day + leap_extra)
}

/// Howard Hinnant's civil-from-days inverse: days since 1970-01-01.
fn days_from_civil(y: i32, m: u32, d: u32) -> i64 {
    let y = i64::from(y) - i64::from(m <= 2);
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let m = i64::from(m);
    let d = i64::from(d);
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixed() -> ThemeSchedule {
        ThemeSchedule {
            enabled: true,
            mode: "fixed".into(),
            light_at: "07:00".into(),
            dark_at: "19:00".into(),
            latitude: 0.0,
            longitude: 0.0,
            offset_minutes: 0,
            target: "apps".into(),
        }
    }

    #[test]
    fn fixed_schedule_evaluates_day_night_and_preserves_manual_override() {
        let sch = fixed();
        let mut st = ScheduleState::default();

        // 12:00 -> light, 7h until 19:00
        let (apply, delay) = st
            .step(
                &sch,
                ScheduleTime {
                    year: 2026,
                    month: 11,
                    day: 25,
                    hour: 12,
                    minute: 0,
                    second: 0,
                    utc_offset_minutes: 420,
                },
            )
            .unwrap();
        assert_eq!(apply, Some((false, false)));
        assert_eq!(delay, 7 * 3600 * 1000);

        // Same window later (manual override happened at 15:00) -> does NOT overwrite!
        let (apply, delay) = st
            .step(
                &sch,
                ScheduleTime {
                    year: 2026,
                    month: 11,
                    day: 25,
                    hour: 15,
                    minute: 0,
                    second: 0,
                    utc_offset_minutes: 420,
                },
            )
            .unwrap();
        assert_eq!(apply, None);
        assert_eq!(delay, 4 * 3600 * 1000);

        // Next boundary (19:00) -> switches to dark and arms 12h until 07:00 next day.
        let (apply, delay) = st
            .step(
                &sch,
                ScheduleTime {
                    year: 2026,
                    month: 11,
                    day: 25,
                    hour: 19,
                    minute: 0,
                    second: 0,
                    utc_offset_minutes: 420,
                },
            )
            .unwrap();
        assert_eq!(apply, Some((true, false)));
        assert_eq!(delay, 12 * 3600 * 1000);
    }

    #[test]
    fn solar_sunrise_sunset_matches_known_jakarta_times() {
        // Jakarta (-6.2, 106.8, UTC+7) on Nov 25: sunrise ~05:27, sunset ~17:50 local.
        let (rise, set) = sunrise_sunset_local_minutes(2026, 11, 25, -6.2, 106.8, 420, 0).unwrap();
        assert!((5 * 60 + 15..=5 * 60 + 45).contains(&rise), "rise={rise}");
        assert!((17 * 60 + 35..=18 * 60 + 5).contains(&set), "set={set}");

        // Polar winter returns None gracefully.
        assert_eq!(
            sunrise_sunset_local_minutes(2026, 12, 21, 78.2, 15.6, 60, 0),
            None
        );
    }
}
