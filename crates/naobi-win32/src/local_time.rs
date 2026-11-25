//! Wall-clock local date and time from Windows (`GetLocalTime` / `GetSystemTime`).
use windows::Win32::System::SystemInformation::{GetLocalTime, GetSystemTime};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalTime {
    pub year: u16,
    pub month: u16,
    pub day: u16,
    pub hour: u16,
    pub minute: u16,
    pub second: u16,
}

/// Reads the current local system date and time.
pub fn now() -> LocalTime {
    // SAFETY: `GetLocalTime` takes no pointers and returns a plain `SYSTEMTIME` value.
    let st = unsafe { GetLocalTime() };
    LocalTime {
        year: st.wYear,
        month: st.wMonth,
        day: st.wDay,
        hour: st.wHour,
        minute: st.wMinute,
        second: st.wSecond,
    }
}

/// Reads the current local time alongside the active UTC offset in minutes.
pub fn now_with_offset() -> (LocalTime, i32) {
    // SAFETY: `GetSystemTime` returns a plain `SYSTEMTIME` value.
    let utc = unsafe { GetSystemTime() };
    // SAFETY: `GetLocalTime` returns a plain `SYSTEMTIME` value.
    let loc = unsafe { GetLocalTime() };
    let to_mins = |y: u16, m: u16, d: u16, h: u16, min: u16| -> i64 {
        let y = i64::from(y) - i64::from(m <= 2);
        let era = y / 400;
        let yoe = y - era * 400;
        let m = i64::from(m);
        let d = i64::from(d);
        let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
        let days = era * 146_097 + yoe * 365 + yoe / 4 - yoe / 100 + doy;
        days * 1440 + i64::from(h) * 60 + i64::from(min)
    };
    let offset = (to_mins(loc.wYear, loc.wMonth, loc.wDay, loc.wHour, loc.wMinute)
        - to_mins(utc.wYear, utc.wMonth, utc.wDay, utc.wHour, utc.wMinute)) as i32;
    (
        LocalTime {
            year: loc.wYear,
            month: loc.wMonth,
            day: loc.wDay,
            hour: loc.wHour,
            minute: loc.wMinute,
            second: loc.wSecond,
        },
        offset,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_time_returns_valid_calendar_fields() {
        let t = now();
        assert!((2020..=2100).contains(&t.year));
        assert!((1..=12).contains(&t.month));
        assert!((1..=31).contains(&t.day));
        assert!(t.hour < 24);
        assert!(t.minute < 60);
        assert!(t.second < 60);
    }
}
