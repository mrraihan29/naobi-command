//! The state machine. It holds the mode and the display choice and tells the port what to do; the
//! clock is passed in, so a test moves time by hand.
const MINUTE_MS: i64 = 60_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Off,
    Indefinite,
    /// Until this Unix time in milliseconds.
    Timed(i64),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Off,
    Indefinite,
    Timed,
}

/// What the tray shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct View {
    pub kind: Kind,
    pub tooltip: String,
}

/// Windows, as Awake needs it. The production one is in `naobi-app` over `naobi-win32`.
pub trait AwakePort {
    /// Holds (or releases) the power request "Naobi Command Awake": the system stays awake, and the
    /// display too when `display` is set.
    fn power(&mut self, on: bool, display: bool);
    /// Arms the expiry timer for that many milliseconds and the 60 s tooltip refresh, or stops both.
    fn timers(&mut self, expiry_ms: Option<u64>);
    fn tray(&mut self, view: View);
}

pub struct Awake {
    mode: Mode,
    display: bool,
}

impl Default for Awake {
    fn default() -> Self {
        Self {
            mode: Mode::Off,
            display: false,
        }
    }
}

/// "Naobi Command Awake: 1 h 5 min left" (rounded up to the minute).
fn left(ms: i64) -> String {
    let minutes = (ms.max(0) + MINUTE_MS - 1) / MINUTE_MS;
    match (minutes / 60, minutes % 60) {
        (0, m) => format!("{m} min left"),
        (h, 0) => format!("{h} h left"),
        (h, m) => format!("{h} h {m} min left"),
    }
}

impl Awake {
    pub fn mode(&self) -> Mode {
        self.mode
    }

    pub fn keeps_display(&self) -> bool {
        self.display
    }

    /// Milliseconds left in Timed mode.
    pub fn remaining(&self, now_ms: i64) -> Option<i64> {
        match self.mode {
            Mode::Timed(until) => Some((until - now_ms).max(0)),
            _ => None,
        }
    }

    /// The tray as it should look now.
    pub fn view(&self, now_ms: i64) -> View {
        let (kind, text) = match self.mode {
            Mode::Off => (Kind::Off, "Naobi Command".to_owned()),
            Mode::Indefinite => (
                Kind::Indefinite,
                "Naobi Command Awake: until turned off".to_owned(),
            ),
            Mode::Timed(until) => (
                Kind::Timed,
                format!("Naobi Command Awake: {}", left(until - now_ms)),
            ),
        };
        View {
            kind,
            tooltip: text,
        }
    }

    /// Tells the port the state: power request, timers and tray. Safe to repeat.
    fn apply(&self, port: &mut dyn AwakePort, now_ms: i64) {
        let on = self.mode != Mode::Off;
        port.power(on, on && self.display);
        port.timers(self.remaining(now_ms).map(|ms| ms as u64));
        port.tray(self.view(now_ms));
    }

    pub fn off(&mut self, port: &mut dyn AwakePort, now_ms: i64) {
        self.mode = Mode::Off;
        self.apply(port, now_ms);
    }

    pub fn indefinite(&mut self, port: &mut dyn AwakePort, now_ms: i64) {
        self.mode = Mode::Indefinite;
        self.apply(port, now_ms);
    }

    pub fn timed(&mut self, ms: i64, port: &mut dyn AwakePort, now_ms: i64) {
        self.mode = Mode::Timed(now_ms + ms);
        self.apply(port, now_ms);
    }

    /// Keeping the display on or letting it sleep, in whatever mode (it matters while Awake is on).
    pub fn set_display(&mut self, on: bool, port: &mut dyn AwakePort, now_ms: i64) {
        self.display = on;
        self.apply(port, now_ms);
    }

    /// A timer fired: the expiry, or the 60 s tooltip refresh. At the end of the time Awake turns
    /// itself off.
    pub fn on_timer(&mut self, port: &mut dyn AwakePort, now_ms: i64) {
        self.expire(now_ms);
        self.apply(port, now_ms);
    }

    /// The PC woke up: the request is made again, and a Timed mode that ran out meanwhile is Off.
    pub fn on_resume(&mut self, port: &mut dyn AwakePort, now_ms: i64) {
        self.expire(now_ms);
        self.apply(port, now_ms);
    }

    fn expire(&mut self, now_ms: i64) {
        if matches!(self.mode, Mode::Timed(until) if until <= now_ms) {
            self.mode = Mode::Off;
        }
    }
}
