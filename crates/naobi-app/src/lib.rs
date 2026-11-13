//! Composition root: starts the platform thread, then the pre-warmed Launcher window.
#![forbid(unsafe_code)]

mod controller;
mod index_worker;
mod logging;
mod notices;
#[cfg(feature = "perf")]
mod perf_fixture;
#[cfg(feature = "perf")]
mod perf_report;
mod settings_runtime;

use controller::{Controller, Host, SystemClock};
use naobi_core::db::Db;
use naobi_core::effect::LaunchTarget;
use naobi_core::event::AppEvent;
use naobi_core::notice::{Notice, NoticeKind};
use naobi_core::settings::Settings;
use naobi_core::theme::{self, Look};
use naobi_core::ui::{Key, NoticeView, Screen, Theme, UiEvent, ViewModel};
use naobi_launcher::frecency::Frecency;
use naobi_launcher::{Clock, Launcher};
use naobi_ui::UiHandle;
use naobi_win32::theme::SystemTheme;
use naobi_win32::tray::{Overlay, TrayState};
use naobi_win32::{
    autostart, clipboard_marker, dwm, foreground, instance, notice, platform, shell,
};
use notices::NoticeCenter;
use std::cell::RefCell;
use std::path::Path;
use std::sync::Mutex;

/// The window handle once it is ready, and a show request that arrived before that. The settings are
/// set before P1 starts; the look is read from Windows at the same time.
struct State {
    ui: Option<UiHandle>,
    early_show: bool,
    /// The newest snapshot of the engine; every post carries the look and the banner on top of it.
    screen: Option<Screen>,
    settings: Option<Settings>,
    look: Look,
    notices: NoticeCenter,
}

static STATE: Mutex<State> = Mutex::new(State {
    ui: None,
    early_show: false,
    screen: None,
    settings: None,
    look: Look {
        theme: Theme::Paper,
        reduced_motion: false,
    },
    notices: NoticeCenter::new(),
});

fn state() -> std::sync::MutexGuard<'static, State> {
    STATE.lock().unwrap_or_else(|e| e.into_inner())
}

thread_local! {
    /// The controller lives on the UI thread, where the window and every UI event are.
    static CONTROLLER: RefCell<Option<Controller>> = const { RefCell::new(None) };
}

#[cfg(feature = "perf")]
fn report(ev: &str) {
    perf_report::emit(ev);
}
#[cfg(not(feature = "perf"))]
fn report(_ev: &str) {}

/// What the controller needs, backed by the window and Win32.
struct AppHost {
    ui: UiHandle,
}

impl Host for AppHost {
    fn hide(&mut self) {
        // On the UI thread already: cloak now, so a focus restore right after cannot race it.
        dwm::hide(self.ui.window());
    }
    fn show(&mut self) {
        self.ui.show();
    }
    fn is_visible(&self) -> bool {
        !dwm::is_cloaked(self.ui.window())
    }
    fn foreground(&self) -> Option<isize> {
        foreground::remember(self.ui.window())
    }
    fn restore_focus(&mut self, window: isize) {
        foreground::restore(window);
    }
    fn launch(&mut self, target: &LaunchTarget) -> std::io::Result<()> {
        match target {
            LaunchTarget::ShellItem(item) => shell::open(item),
            // A quicklink is a web search: nothing but http(s) is opened for a URL.
            LaunchTarget::Url(url) if shell::is_web_url(url) => shell::open(url),
            LaunchTarget::Url(_) => Err(std::io::ErrorKind::InvalidInput.into()),
        }
    }
    fn copy_text(&mut self, text: &str) -> std::io::Result<()> {
        clipboard_marker::write_text(text).map_err(std::io::Error::other)
    }
    fn notice(&mut self, n: Notice) {
        raise_notice(&mut state(), n);
    }
    fn post(&mut self, screen: Screen) {
        let mut s = state();
        s.screen = Some(screen);
        post_view(&self.ui, &s);
    }
}

/// Runs `f` with the controller and its host. Call it on the UI thread without holding the state
/// lock: the host takes the lock itself.
fn with_controller(ui: UiHandle, f: impl FnOnce(&mut Controller, &mut AppHost)) {
    CONTROLLER.with_borrow_mut(|c| {
        if let Some(c) = c {
            f(c, &mut AppHost { ui });
        }
    });
}

/// Hands `f` to the UI thread. Before the window is ready the request is kept, and served as soon
/// as it is.
fn dispatch(f: fn(&mut Controller, &mut AppHost)) {
    let mut s = state();
    match s.ui.clone() {
        Some(ui) => {
            drop(s);
            let on_ui = ui.clone();
            ui.run_on_ui(move || with_controller(on_ui, f));
        }
        None => s.early_show = true,
    }
}

/// The Launcher hotkey (R-023).
fn on_hotkey() {
    dispatch(|c, h| c.toggle(h));
}

/// Shows the Launcher (second instance, tray Open). Never toggles it off.
fn on_show() {
    dispatch(|c, h| c.show(h));
}

/// The sources the Launcher searches. The perf build searches the synthetic 10,000-item index
/// (`NAOBI_PERF_ITEMS` overrides the count), so the latency runs cross the real engine.
fn sources() -> Vec<Box<dyn naobi_core::command::CommandSource>> {
    #[cfg(feature = "perf")]
    {
        let n = std::env::var("NAOBI_PERF_ITEMS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(10_000);
        vec![Box::new(perf_fixture::PerfSource::new(n))]
    }
    #[cfg(not(feature = "perf"))]
    {
        // seam NCMD-16 to 18: the apps, files, commands, quicklinks and calculator sources.
        Vec::new()
    }
}

/// The engine with the Owner's frecency. When the store cannot be opened the history is kept in
/// memory for this run, with a Notice.
fn build_controller() -> (Option<Controller>, Option<Notice>) {
    let now = SystemClock.now_ms() / 1000;
    let on_disk = naobi_core::paths::data_dir().and_then(|dir| {
        std::fs::create_dir_all(&dir).ok()?;
        let random = naobi_win32::random::random16().ok()?;
        Db::open(&dir.join("naobi.db"), move || random).ok()
    });
    let (db, warn) = match on_disk {
        Some(db) => (db, None),
        None => match Db::open_in_memory() {
            Ok(db) => (
                db,
                Some(Notice {
                    subject: "History".into(),
                    message: "The data file could not be opened; usage is not saved this run."
                        .into(),
                    kind: NoticeKind::Warning,
                }),
            ),
            Err(e) => {
                tracing::error!(error = %e, "no data store: the Launcher is off");
                return (None, None);
            }
        },
    };
    match Frecency::open(db, now) {
        Ok(frecency) => {
            let launcher = Launcher::new(sources(), Vec::new(), frecency, Box::new(SystemClock));
            (Some(Controller::new(launcher)), warn)
        }
        Err(e) => {
            tracing::error!(error = %e, "frecency did not load: the Launcher is off");
            (None, None)
        }
    }
}

fn on_ready(ui: UiHandle) {
    #[cfg(feature = "perf")]
    // Same UI thread as the window: t0 of the keystroke metric is taken in this subclass.
    naobi_win32::subclass::install(ui.window());
    let (controller, warn) = build_controller();
    CONTROLLER.with_borrow_mut(|c| *c = controller);
    let show_now = {
        let mut s = state();
        s.ui = Some(ui.clone());
        if let Some(n) = warn {
            raise_notice(&mut s, n);
        }
        std::mem::take(&mut s.early_show)
    };
    report("window_ready");
    if show_now {
        // Pressed before the window was ready: served as soon as it is.
        with_controller(ui, |c, h| c.show(h));
    } else {
        with_controller(ui, |c, h| h.post(c.screen()));
    }
}

fn on_event(ev: UiEvent) {
    let Some(ui) = state().ui.clone() else { return };
    // Esc first dismisses the banner (D-018); with no banner it goes to the engine's ladder.
    if matches!(&ev, UiEvent::Key(c) if c.key == Key::Escape) {
        let mut s = state();
        if s.notices.banner().is_some() {
            s.notices.dismiss();
            sync_tray(&s.notices);
            post_view(&ui, &s);
            return;
        }
    }
    with_controller(ui, |c, h| c.on_ui_event(ev, h));
}

/// Posts the engine's snapshot with the Launcher's look and banner. Every post carries both, so the
/// banner shows on the next show.
fn post_view(ui: &UiHandle, s: &State) {
    let mut screen = s
        .screen
        .clone()
        .unwrap_or_else(|| Screen::from(ViewModel::default()));
    screen.reduced_motion = s.look.reduced_motion;
    screen.notice = s.notices.banner().map(|n| NoticeView {
        message: n.message.clone(),
    });
    ui.post_screen(screen);
}

/// Raises a Notice (D-018): the balloon, the tray tooltip and overlay, and the banner on the Launcher.
/// An equal Notice that is already active is ignored, so a repeated settings event adds nothing.
/// The caller holds the state lock.
fn raise_notice(s: &mut State, n: Notice) {
    if !s.notices.raise(n.clone()) {
        return;
    }
    notice::balloon(&n);
    sync_tray(&s.notices);
    if let Some(ui) = s.ui.clone() {
        post_view(&ui, s);
    }
}

/// While a Notice is active the tray shows the overlay and the banner's message as its tooltip.
fn sync_tray(notices: &NoticeCenter) {
    let tray = match notices.banner() {
        Some(n) => TrayState {
            overlay: Overlay::Notice,
            tooltip: format!("Naobi Command: {}", n.message),
            ..TrayState::default()
        },
        None => TrayState::default(),
    };
    platform::set_tray_state(tray);
}

/// R-008: another process owns the chord of `name`, so that hotkey is off. Names the chord when known.
fn on_conflict(name: &str) {
    let mut s = state();
    let chord = s
        .settings
        .as_ref()
        .and_then(|set| set.hotkeys.get(name))
        .copied();
    raise_notice(&mut s, notices::hotkey_conflict(name, chord));
}

/// R-007: Windows changed its theme or animation setting. The look follows it. The palette itself
/// waits for a `UiHandle` theme setter in naobi-ui, which NCMD-13 does not own.
fn on_theme() {
    let mut s = state();
    if let Some(look) = theme::follow(&AppEvent::ThemeChanged, &SystemTheme, s.look) {
        s.look = look;
        if let Some(ui) = s.ui.clone() {
            post_view(&ui, &s);
        }
    }
}

/// Runs on the index worker once the settings folder settled (D-009). A file that does not parse keeps
/// the last valid settings and raises a Notice. A changed `general.autostart` is written (D-007).
fn on_settings_changed(path: &Path) {
    let last = state().settings.clone().unwrap_or_default();
    let (next, notices) = settings_runtime::reload(path, &last);
    if next.general.autostart != last.general.autostart {
        sync_autostart(next.general.autostart);
    }
    let mut s = state();
    s.settings = Some(next);
    for n in notices {
        raise_notice(&mut s, n);
    }
}

/// Event routing into the controller belongs to NCMD-15. NCMD-13 adds the hotkey-conflict Notice and
/// the theme; the launcher hotkey and `ShowRequested` stay as NCMD-12 left them (R-023, tray Open).
// seam NCMD-15: every other hotkey, Resume, DisplayChanged, Timer, ClipboardChanged and SettingsChanged
// are routed here.
fn route(ev: AppEvent) {
    match ev {
        AppEvent::Hotkey(id) if id.0 == "launcher" => {
            logging::hotkey(&id.0);
            on_hotkey();
        }
        AppEvent::ShowRequested => on_show(),
        AppEvent::HotkeyConflict(id) => on_conflict(&id.0),
        AppEvent::ThemeChanged => on_theme(),
        _ => {}
    }
}

/// The Run key this run may write, or None for "write nothing". Pure: `perf` says which build this is,
/// `override_key` is the perf-only `NAOBI_TEST_RUN_KEY`. A perf build writes only to an override, so a
/// test or benchmark spawn never reaches the Owner's real Run key. The release build always uses it.
fn autostart_target(perf: bool, override_key: Option<String>) -> Option<String> {
    if perf {
        override_key
    } else {
        Some(autostart::RUN_KEY.into())
    }
}

/// The reader side of `autostart_target`. The variable is read only in perf builds.
fn autostart_key() -> Option<String> {
    #[cfg(feature = "perf")]
    let override_key = std::env::var("NAOBI_TEST_RUN_KEY").ok();
    #[cfg(not(feature = "perf"))]
    let override_key: Option<String> = None;
    autostart_target(cfg!(feature = "perf"), override_key)
}

/// D-007: the HKCU Run value follows `general.autostart`. A failure is logged with the system error
/// only, and never stops the app.
fn apply_autostart(enabled: bool, exe: &std::path::Path, key: &str) {
    if let Err(e) = autostart::set_enabled(key, exe, enabled) {
        tracing::warn!(error = %e, "autostart: the Run value was not updated");
    }
}

/// Writes the Run value for `enabled` at the program's own path, if this build writes one.
fn sync_autostart(enabled: bool) {
    let Some(key) = autostart_key() else { return };
    match std::env::current_exe() {
        Ok(exe) => apply_autostart(enabled, &exe, &key),
        Err(_) => {
            tracing::warn!("autostart: the program path is unknown; the Run value was not updated")
        }
    }
}

pub fn run() -> anyhow::Result<()> {
    // Single instance (D-013): a second launch shows the first one's Launcher and exits. The first
    // instance may still be creating its window (tray-ready takes tens of ms), so retry briefly.
    let Some(_claim) = instance::claim()? else {
        for _ in 0..20 {
            if instance::show_first() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        return Ok(());
    };
    // Best effort: the app runs without a log when its folder cannot be made.
    if let Some(dir) = naobi_core::paths::data_dir() {
        let _ = logging::init(&dir.join("logs"));
    }
    let settings_dir = naobi_core::paths::settings_dir();
    let (settings, boot_notices) = match settings_dir.as_deref().map(settings_runtime::startup) {
        Some(Ok((_, settings, notices))) => (settings, notices),
        Some(Err(e)) => {
            tracing::warn!(error = ?e.kind(), "settings file was not created");
            (Settings::default(), Vec::new())
        }
        None => (Settings::default(), Vec::new()),
    };
    sync_autostart(settings.general.autostart);
    {
        let mut s = state();
        s.settings = Some(settings.clone());
        s.look = theme::read(&SystemTheme);
    }
    // P1 first: hotkeys registered and tray icon shown before any UI work.
    platform::start_with(&settings, route, || std::process::exit(0))?;
    report("tray_ready");
    if let Some(dir) = settings_dir {
        let path = dir.join(settings_runtime::FILE_NAME);
        if let Err(e) = index_worker::start(&dir, move || on_settings_changed(&path)) {
            tracing::warn!(error = %e, "settings hot reload is off");
        }
    }
    {
        let mut s = state();
        for n in boot_notices {
            raise_notice(&mut s, n);
        }
    }
    let look = state().look;
    naobi_ui::run(look.theme, on_event, on_ready)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Removes the throwaway key tree on drop, so a failing assert leaves nothing behind.
    struct Cleanup(String);

    impl Drop for Cleanup {
        fn drop(&mut self) {
            autostart::remove_key_tree(&self.0);
        }
    }

    /// D-007 at start: the setting decides the Run value. A throwaway key stands in for the Owner's
    /// Run key, so this test never touches it.
    #[test]
    fn d007_setting_decides_the_run_value_on_a_throwaway_key() {
        let key = format!(r"Software\NaobiCommandTest\app-{}", std::process::id());
        let _cleanup = Cleanup(key.clone());
        let exe = std::path::Path::new(r"C:\Apps\naobi-app.exe");
        apply_autostart(true, exe, &key);
        assert_eq!(
            autostart::value(&key).as_deref(),
            Some(r#""C:\Apps\naobi-app.exe""#)
        );
        apply_autostart(false, exe, &key);
        assert_eq!(autostart::value(&key), None);
    }

    /// The Owner's Run key is written only by a release build. A perf build with no override writes
    /// nothing, and an override in a release build is ignored. Pure: no registry is touched.
    #[test]
    fn d007_perf_build_writes_only_to_an_override_and_release_always_uses_the_real_key() {
        let real = autostart::RUN_KEY.to_string();
        let throwaway = Some(r"Software\NaobiCommandTest\x".to_string());
        assert_eq!(autostart_target(true, None), None);
        assert_eq!(autostart_target(true, throwaway.clone()), throwaway);
        assert_eq!(autostart_target(false, None), Some(real.clone()));
        assert_eq!(autostart_target(false, throwaway), Some(real));
    }
}
