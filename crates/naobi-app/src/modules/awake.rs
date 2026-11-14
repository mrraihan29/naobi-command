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
/// The tray's Awake part as last set, for the Notice overlay to keep.
static TRAY: Mutex<Option<TrayState>> = Mutex::new(None);

/// The tray without a Notice: the Awake state and its tooltip.
pub fn tray_base() -> TrayState {
    TRAY.lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
        .unwrap_or_default()
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
        let state = TrayState {
            awake: match view.kind {
                Kind::Off => tray::Awake::Off,
                Kind::Indefinite => tray::Awake::Indefinite,
                Kind::Timed => tray::Awake::Timed,
            },
            overlay: Overlay::None,
            tooltip: view.tooltip,
        };
        *TRAY.lock().unwrap_or_else(|e| e.into_inner()) = Some(state.clone());
        platform::set_tray_state(state);
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
