//! Awake (NCMD-23): the commands, the power request, the timers and the tray, wired to the state
//! machine in `naobi-awake`. The request and the timers live on the platform thread; expiry and
//! resume arrive here as events.
use super::{EventKind, Modules};
use naobi_awake::{Awake, AwakeCommands, AwakePort, Cmd, Kind, View};
use naobi_core::event::AppEvent;
use naobi_win32::platform;
use naobi_win32::power::{self, EXPIRY_TIMER, TICK_TIMER};
use naobi_win32::tray::{self, Overlay, TrayState};
use std::sync::{Arc, Mutex};

static AWAKE: Mutex<Option<Awake>> = Mutex::new(None);
/// The Awake part of the tray and the active Notice's message. One lock covers composing and
/// posting, so two threads cannot post out of order.
static TRAY: Mutex<(TrayState, Option<String>)> = Mutex::new((
    TrayState {
        awake: tray::Awake::Off,
        overlay: Overlay::None,
        tooltip: String::new(),
    },
    None,
));

/// Posts the tray: the Awake state, plus the overlay and message of an active Notice.
fn publish(guard: &(TrayState, Option<String>)) {
    platform::set_tray_state(compose(&guard.0, guard.1.as_deref()));
}

/// The Awake tray with an active Notice laid over it: the overlay and the message win the tooltip, the
/// Awake icon state stays.
fn compose(base: &TrayState, notice: Option<&str>) -> TrayState {
    let mut tray = base.clone();
    if let Some(message) = notice {
        tray.overlay = Overlay::Notice;
        tray.tooltip = format!("Naobi Command: {message}");
    }
    tray
}

/// The banner's message while a Notice is active, `None` when it is gone.
pub fn set_notice(message: Option<String>) {
    let mut g = TRAY.lock().unwrap_or_else(|e| e.into_inner());
    g.1 = message;
    publish(&g);
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

struct Port;

impl AwakePort for Port {
    fn power(&mut self, on: bool, display: bool) {
        power::set_request(on, display);
    }

    fn timers(&mut self, expiry_ms: Option<u64>) {
        power::timers(expiry_ms);
    }

    fn tray(&mut self, view: View) {
        let base = TrayState {
            awake: match view.kind {
                Kind::Off => tray::Awake::Off,
                Kind::Indefinite => tray::Awake::Indefinite,
                Kind::Timed => tray::Awake::Timed,
            },
            overlay: Overlay::None,
            tooltip: view.tooltip,
        };
        let mut g = TRAY.lock().unwrap_or_else(|e| e.into_inner());
        g.0 = base;
        publish(&g);
    }
}

/// Runs `f` on the one Awake state.
fn with(f: impl FnOnce(&mut Awake, &mut Port, i64)) {
    let mut guard = AWAKE.lock().unwrap_or_else(|e| e.into_inner());
    f(
        guard.get_or_insert_with(Awake::default),
        &mut Port,
        now_ms(),
    );
}

fn run(cmd: Cmd) {
    with(|a, port, now| match cmd {
        Cmd::On => a.indefinite(port, now),
        Cmd::Off => a.off(port, now),
        Cmd::For(ms) => a.timed(ms, port, now),
        Cmd::Display(on) => a.set_display(on, port, now),
    });
}

pub fn register(m: &mut Modules) {
    m.register_source(Box::new(AwakeCommands::new(Arc::new(run))));
    m.subscribe_event(
        EventKind::Timer,
        Box::new(|ev, _| {
            if let AppEvent::Timer(id) = ev {
                if id.0 == EXPIRY_TIMER || id.0 == TICK_TIMER {
                    with(|a, port, now| a.on_timer(port, now));
                }
            }
        }),
    );
    m.subscribe_event(
        EventKind::Resume,
        Box::new(|_, _| with(|a, port, now| a.on_resume(port, now))),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    // @R-019
    #[test]
    fn r019_a_notice_is_laid_over_the_awake_tray_and_lifts_off_again() {
        let timed = TrayState {
            awake: tray::Awake::Timed,
            overlay: Overlay::None,
            tooltip: "Awake for 1 hour".into(),
        };
        let shown = compose(&timed, Some("Alt+Space is taken"));
        assert_eq!(shown.awake, tray::Awake::Timed, "the Awake icon stays");
        assert_eq!(shown.overlay, Overlay::Notice);
        assert_eq!(shown.tooltip, "Naobi Command: Alt+Space is taken");
        let lifted = compose(&timed, None);
        assert_eq!(lifted.overlay, Overlay::None);
        assert_eq!(lifted.tooltip, "Awake for 1 hour");
    }
}
