//! The words of the Awake commands (D-024): `awake on`, `awake off`, `awake 1h`, `awake 45m`,
//! `awake display on`, `awake display off`.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cmd {
    On,
    Off,
    /// Milliseconds.
    For(i64),
    Display(bool),
}

const MINUTE_MS: i64 = 60_000;
/// A day is the longest a timed Awake runs: a typo like `awake 9999h` is not a request.
const MAX_MINUTES: i64 = 24 * 60;

/// The command in `text`, or None when it is not one (the Launcher then shows no Awake command).
pub fn parse(text: &str) -> Option<Cmd> {
    let lower = text.trim().to_ascii_lowercase();
    let mut words = lower.split_whitespace();
    if words.next()? != "awake" {
        return None;
    }
    let (first, second) = (words.next()?, words.next());
    if words.next().is_some() {
        return None;
    }
    match (first, second) {
        ("on", None) => Some(Cmd::On),
        ("off", None) => Some(Cmd::Off),
        ("display", Some("on")) => Some(Cmd::Display(true)),
        ("display", Some("off")) => Some(Cmd::Display(false)),
        (duration, None) => minutes(duration).map(|m| Cmd::For(m * MINUTE_MS)),
        _ => None,
    }
}

/// `30m`, `1h`, `90m`, `2h`: a whole number and one unit, at least a minute and at most a day.
fn minutes(word: &str) -> Option<i64> {
    let (digits, unit) = word.split_at(word.len().checked_sub(1)?);
    if !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let n: i64 = digits.parse().ok()?;
    let m = match unit {
        "m" => n,
        "h" => n.checked_mul(60)?,
        _ => return None,
    };
    (1..=MAX_MINUTES).contains(&m).then_some(m)
}
