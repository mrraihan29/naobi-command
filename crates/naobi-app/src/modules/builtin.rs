//! The built-in rows (NCMD-18): system commands, quicklinks and the calculator. The commands run
//! through `AppOps`, which logs each one by id (T-009) and calls Windows.
use super::clipboard_capture::DpapiCipher;
use super::{EventKind, Modules};
use crate::logging;
use naobi_core::command::Outcome;
use naobi_core::db::Db;
use naobi_core::event::AppEvent;
use naobi_core::quicklink::Quicklink;
use naobi_launcher::calc_history::CalcHistory;
use naobi_launcher::providers::calc::{
    refresh_rates, should_auto_refresh, CalcConfig, CalcHistoryCommands, CalcHistoryView,
    CalcSource, FxFeed, FxRefreshCommands, FX_REFRESH_INTERVAL,
};
use naobi_launcher::providers::color::ColorSource;
use naobi_launcher::providers::dev::{DevEnv, DevSource};
use naobi_launcher::providers::quicklink::{QuicklinkSet, QuicklinkSource};
use naobi_launcher::providers::run::{Program, ProgramLookup, RunSource};
use naobi_launcher::providers::system::{SystemCommand, SystemOps, SystemSource};
use naobi_launcher::run_history::RunHistory;
use naobi_win32::{clipboard_marker, fx, platform, random, run, shell_exec, system_commands};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::SystemTime;

/// The quicklinks of the live settings: the settings load and every hot reload `set` them.
fn quicklinks() -> &'static QuicklinkSet {
    static SET: OnceLock<QuicklinkSet> = OnceLock::new();
    SET.get_or_init(QuicklinkSet::default)
}

pub fn set_quicklinks(links: Vec<Quicklink>) {
    quicklinks().set(links);
}

fn calc_config() -> &'static CalcConfig {
    static CFG: OnceLock<CalcConfig> = OnceLock::new();
    CFG.get_or_init(CalcConfig::default)
}

pub fn set_calc_rem_base_px(px: f64) {
    calc_config().set_rem_base_px(px);
}

const FX_TIMER: usize = 0xB1;
const THEME_TIMER: usize = 0xB2;

#[derive(Default)]
struct ThemeScheduleRuntime {
    config: Option<naobi_core::settings::ThemeSchedule>,
    state: naobi_core::theme_schedule::ScheduleState,
}

fn theme_schedule_rt() -> &'static Mutex<ThemeScheduleRuntime> {
    static RT: OnceLock<Mutex<ThemeScheduleRuntime>> = OnceLock::new();
    RT.get_or_init(|| Mutex::new(ThemeScheduleRuntime::default()))
}

pub fn set_theme_schedule(schedule: Option<naobi_core::settings::ThemeSchedule>) {
    {
        let mut rt = theme_schedule_rt()
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if rt.config != schedule {
            rt.config = schedule;
            rt.state.reset();
        }
    }
    step_theme_schedule();
}

pub fn step_theme_schedule() {
    let (lt, offset) = naobi_win32::local_time::now_with_offset();
    let now = naobi_core::theme_schedule::ScheduleTime {
        year: i32::from(lt.year),
        month: u32::from(lt.month),
        day: u32::from(lt.day),
        hour: u32::from(lt.hour),
        minute: u32::from(lt.minute),
        second: u32::from(lt.second),
        utc_offset_minutes: offset,
    };
    let step = {
        let mut rt = theme_schedule_rt()
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let Some(cfg) = rt.config.clone() else {
            platform::set_timer(THEME_TIMER, None);
            return;
        };
        rt.state.step(&cfg, now)
    };
    match step {
        Some((apply, delay_ms)) => {
            if let Some((dark, include_system)) = apply {
                let _ = system_commands::set_dark_mode(dark, include_system);
            }
            platform::set_timer(THEME_TIMER, Some(delay_ms));
        }
        None => {
            platform::set_timer(THEME_TIMER, None);
        }
    }
}

#[derive(Default)]
struct FxScheduler {
    auto_refresh: bool,
    in_flight: bool,
    last_auto_attempt: Option<SystemTime>,
}

fn fx_scheduler() -> &'static Mutex<FxScheduler> {
    static SCHED: OnceLock<Mutex<FxScheduler>> = OnceLock::new();
    SCHED.get_or_init(|| {
        Mutex::new(FxScheduler {
            auto_refresh: true,
            in_flight: false,
            last_auto_attempt: None,
        })
    })
}

struct WinHttpFxFeed;

impl FxFeed for WinHttpFxFeed {
    fn fetch(&self, url: &str) -> Result<String, String> {
        fx::fetch_https(url).map_err(|e| e.to_string())
    }
}

struct InFlightGuard;

impl Drop for InFlightGuard {
    fn drop(&mut self) {
        fx_scheduler()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .in_flight = false;
    }
}

fn fx_auto_enabled() -> bool {
    !cfg!(feature = "perf") || std::env::var_os("NAOBI_TEST_FX_REFRESH").is_some()
}

fn ensure_fx_path() {
    let cfg = calc_config();
    if cfg.fx_path().is_none() {
        cfg.set_fx_path(naobi_core::paths::data_dir().map(|d| d.join("fx-rates.json")));
    }
}

pub fn set_currency_config(cfg: &naobi_core::settings::Currency) {
    calc_config().set_currency(&cfg.default_currency, &cfg.number_format);
    fx_scheduler()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .auto_refresh = cfg.auto_refresh;
}

fn spawn_fx_refresh(is_auto: bool) {
    ensure_fx_path();
    {
        let mut sched = fx_scheduler().lock().unwrap_or_else(|e| e.into_inner());
        if sched.in_flight {
            return;
        }
        sched.in_flight = true;
        if is_auto {
            sched.last_auto_attempt = Some(SystemTime::now());
        }
    }
    let cfg = calc_config().clone();
    if std::thread::Builder::new()
        .name("fx-refresh".into())
        .spawn(move || {
            let _guard = InFlightGuard;
            match refresh_rates(&WinHttpFxFeed, &cfg) {
                Ok(_) => {
                    tracing::info!("currency rates refreshed");
                    crate::to_modules(|c, h| c.refresh(h));
                }
                Err(notice) => {
                    tracing::warn!("currency rates refresh failed");
                    crate::to_modules(move |c, h| c.on_notice(notice, h));
                }
            }
        })
        .is_err()
    {
        fx_scheduler()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .in_flight = false;
    }
}

/// Checks 24h cache freshness and triggers a background refresh if due and enabled.
pub fn maybe_auto_refresh_fx() {
    if !fx_auto_enabled() {
        return;
    }
    ensure_fx_path();
    let (auto_refresh, in_flight, last_attempt) = {
        let s = fx_scheduler().lock().unwrap_or_else(|e| e.into_inner());
        (s.auto_refresh, s.in_flight, s.last_auto_attempt)
    };
    if in_flight {
        return;
    }
    let path = calc_config().fx_path();
    if should_auto_refresh(
        path.as_deref(),
        auto_refresh,
        last_attempt,
        SystemTime::now(),
    ) {
        spawn_fx_refresh(true);
    }
}

/// Starts the daily FX auto-refresh timer and runs the initial freshness check.
pub fn start_fx_scheduler() {
    ensure_fx_path();
    if fx_auto_enabled() {
        platform::set_timer(FX_TIMER, Some(FX_REFRESH_INTERVAL.as_millis() as u64));
        maybe_auto_refresh_fx();
    }
}

pub fn register(m: &mut Modules) {
    ensure_fx_path();
    let cfg = calc_config().clone();
    let calc_history = open_data_db("calculation history")
        .map(|db| Arc::new(Mutex::new(CalcHistory::open(db, Box::new(DpapiCipher)))));
    m.register_source(Box::new(CalcSource::new(
        calc_history.clone(),
        cfg,
        Box::new(now_ms),
    )));
    m.register_source(Box::new(FxRefreshCommands::new(Arc::new(|| {
        spawn_fx_refresh(false);
        Outcome::Done
    }))));
    m.subscribe_event(
        EventKind::Timer,
        Box::new(|ev, _| {
            if let AppEvent::Timer(id) = ev {
                if id.0 == FX_TIMER {
                    maybe_auto_refresh_fx();
                } else if id.0 == THEME_TIMER {
                    step_theme_schedule();
                }
            }
        }),
    );
    m.subscribe_event(
        EventKind::Resume,
        Box::new(|_, _| {
            maybe_auto_refresh_fx();
            step_theme_schedule();
        }),
    );
    m.register_source(Box::new(ColorSource));
    m.register_source(Box::new(DevSource::new(Box::new(WinDevEnv))));
    if let Some(history) = calc_history {
        m.register_source(Box::new(CalcHistoryCommands));
        m.register_view(Box::new(CalcHistoryView::new(history, Box::new(now_ms))));
    }
    m.register_source(Box::new(QuicklinkSource::new(quicklinks().clone())));
    m.register_source(Box::new(SystemSource::new(Arc::new(AppOps))));
    m.register_source(Box::new(RunSource::new(
        open_run_history(),
        Box::new(WinLookup),
        Box::new(now_ms),
    )));
}

struct WinDevEnv;

impl DevEnv for WinDevEnv {
    fn clipboard_text(&self) -> Option<String> {
        clipboard_marker::read_text()
    }

    fn random16(&self) -> Option<[u8; 16]> {
        random::random16().ok()
    }

    fn now_ms(&self) -> i64 {
        now_ms()
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

fn open_data_db(label: &str) -> Option<Db> {
    let opened = (|| {
        let path = naobi_core::paths::data_dir()?.join("naobi.db");
        std::fs::create_dir_all(path.parent()?).ok()?;
        let seed = random::random16().ok()?;
        Db::open(&path, move || seed).ok()
    })();
    if opened.is_none() {
        tracing::warn!(
            store = label,
            "the history store is off: naobi.db could not be opened"
        );
    }
    opened
}

/// The run history on its own connection to the data file (D-038). None when the store cannot be
/// opened: `>` still runs lines, without history.
fn open_run_history() -> Option<RunHistory> {
    let db = open_data_db("run history")?;
    let opened = RunHistory::open(db, Box::new(DpapiCipher), now_ms()).ok();
    if opened.is_none() {
        tracing::warn!("the run history is off: the store could not be opened");
    }
    opened
}

/// The first token of a `>` line against PATH and App Paths (T-035: no network paths).
struct WinLookup;

impl ProgramLookup for WinLookup {
    fn find(&self, token: &str) -> Option<Program> {
        run::find_program(token).map(|f| Program {
            path: f.path,
            gui: f.gui,
        })
    }
}

struct AppOps;

/// A Windows failure is logged with its system error; the Notice stays a plain sentence.
fn windows_failed(command: SystemCommand, e: &std::io::Error) -> String {
    tracing::warn!(command = command.id(), error = %e, "system command failed");
    "Windows refused the command.".to_owned()
}

impl SystemOps for AppOps {
    fn run(&self, command: SystemCommand) -> Result<Option<String>, String> {
        logging::system_command(command.id());
        let done =
            |r: std::io::Result<()>| r.map(|()| None).map_err(|e| windows_failed(command, &e));
        let open_settings = |uri: &str| {
            shell_exec::open(uri)
                .map(|()| None)
                .map_err(|_| "Windows could not open Settings.".to_owned())
        };
        match command {
            SystemCommand::Lock => done(system_commands::lock()),
            SystemCommand::Sleep => done(system_commands::suspend(false)),
            SystemCommand::SleepDisplays => done(system_commands::sleep_displays()),
            SystemCommand::Hibernate => done(system_commands::suspend(true)),
            SystemCommand::Restart => done(system_commands::restart()),
            SystemCommand::RestartFrontmostApp => {
                naobi_win32::window_switcher::restart_frontmost_app()
                    .map(|()| None)
                    .map_err(|e| match e {
                        naobi_win32::window_switcher::ActionError::Elevated => {
                            "That window runs as administrator.".to_owned()
                        }
                        naobi_win32::window_switcher::ActionError::Refused => {
                            "Could not restart the frontmost app.".to_owned()
                        }
                    })
            }
            SystemCommand::ShutDown => done(system_commands::shut_down()),
            SystemCommand::SignOut => done(system_commands::sign_out()),
            SystemCommand::EmptyRecycleBin => done(system_commands::empty_recycle_bin()),
            SystemCommand::ToggleDarkMode => done(system_commands::toggle_dark_mode()),
            SystemCommand::ToggleMute => done(system_commands::toggle_mute()),
            SystemCommand::Volume0 => done(system_commands::set_volume(0)),
            SystemCommand::Volume25 => done(system_commands::set_volume(25)),
            SystemCommand::Volume50 => done(system_commands::set_volume(50)),
            SystemCommand::Volume75 => done(system_commands::set_volume(75)),
            SystemCommand::Volume100 => done(system_commands::set_volume(100)),
            SystemCommand::CopyLocalIp => match system_commands::local_ip() {
                Ok(Some(ip)) => Ok(Some(ip)),
                Ok(None) => Err("No active local IP address found.".to_owned()),
                Err(e) => Err(windows_failed(command, &e)),
            },
            SystemCommand::OpenWindowsSettings => open_settings("ms-settings:"),
            SystemCommand::OpenNightLightSettings => open_settings("ms-settings:nightlight"),
            SystemCommand::OpenSoundSettings => open_settings("ms-settings:sound"),
            SystemCommand::OpenWifiSettings => open_settings("ms-settings:network-wifi"),
            SystemCommand::OpenBluetoothSettings => open_settings("ms-settings:bluetooth"),
            SystemCommand::OpenSettingsFile => platform::open_settings()
                .then_some(None)
                .ok_or_else(|| "Windows could not open the settings file.".to_owned()),
            // Resuming is in the tray menu: the Launcher hotkey is off while they are paused.
            SystemCommand::PauseHotkeys => platform::set_hotkeys_paused(true)
                .then_some(None)
                .ok_or_else(|| "The hotkeys could not be paused.".to_owned()),
            SystemCommand::Quit => platform::quit()
                .then_some(None)
                .ok_or_else(|| "The app could not quit.".to_owned()),
        }
    }
}
