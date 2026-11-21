//! Wall-clock local date and time from Windows (`GetLocalTime`).
use windows::Win32::System::SystemInformation::GetLocalTime;

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
