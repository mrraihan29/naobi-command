//! Composition root: starts the platform thread, then the pre-warmed Launcher window.
#![forbid(unsafe_code)]

mod controller;
mod index_worker;
mod logging;
mod modules;
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
use naobi_core::theme_file::{self, Catalog, ResolvedTheme};
use naobi_core::tokens::Palette;
use naobi_core::ui::{Key, NoticeView, Screen, Theme, UiEvent, ViewModel};
use naobi_launcher::frecency::Frecency;
use naobi_launcher::{Clock, Launcher};
use naobi_ui::UiHandle;
use naobi_win32::theme::SystemTheme;
use naobi_win32::{
    autostart, clipboard_marker, dwm, foreground, instance, notice, platform, shell_exec,
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
    /// The Custom themes on disk.
    catalog: Catalog,
    /// The Theme in use.
    theme: Option<ResolvedTheme>,
    /// The Theme on screen: the one in use, or the one previewed in the Themes view.
    drawn: RefCell<Option<ResolvedTheme>>,
    /// The Theme Notices that are raised now.
    theme_notices: Vec<String>,
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
    catalog: Catalog::new(),
    theme: None,
    drawn: RefCell::new(None),
    theme_notices: Vec::new(),
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
        self.ui.set_active(false);
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
        // The controller refused every URL that is not http or https before this.
        match target {
            LaunchTarget::ShellItem(t) | LaunchTarget::Url(t) => shell_exec::open(t),
            // The Owner's own command line or program, never through the URL scheme check (T-033).
            LaunchTarget::Console(line) => naobi_win32::run::run_in_console(line),
            LaunchTarget::Program { path, args } => naobi_win32::run::start_program(path, args),
        }
    }
    fn copy_text(&mut self, text: &str) -> std::io::Result<()> {
        clipboard_marker::write_text(text).map_err(std::io::Error::other)
    }
    fn notice(&mut self, n: Notice) {
        raise_notice(&mut state(), n);
    }
    fn close_later(&mut self, ms: u64) {
        let ui = self.ui.clone();
        self.ui
            .run_on_ui_after(ms, move || on_ui(ui, |c, h| c.finish_close(h)));
    }
    fn copy_entry(&mut self, id: naobi_core::ids::EntryId) -> std::io::Result<()> {
        modules::clipboard::copy_entry(id)
    }
    fn paste(&mut self, id: naobi_core::ids::EntryId, target: Option<isize>) {
        modules::clipboard::paste_entry(id, target, |n| raise_notice(&mut state(), n));
    }
    fn search_files(&mut self, generation: u64, query: &str) {
        modules::files::request(generation, query);
    }
    fn load_icons(&mut self, keys: Vec<String>) {
        modules::icons::request(keys);
    }
    fn apply_theme(&mut self, id: &str) -> Result<(), String> {
        apply_theme(id)
    }
    fn post(&mut self, screen: Screen) {
        if screen
            .sections
            .iter()
            .any(|s| s.header.as_deref() == Some("Files"))
        {
            report("files_shown");
        }
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
            on_ui(ui, f);
        }
        None => s.early_show = true,
    }
}

/// Runs `f` with the controller on the UI thread.
fn on_ui(ui: UiHandle, f: impl FnOnce(&mut Controller, &mut AppHost) + Send + 'static) {
    let handle = ui.clone();
    ui.run_on_ui(move || with_controller(handle, f));
}

/// An event for the modules. Before the window is ready there is no controller, so it is dropped.
fn to_modules(f: impl FnOnce(&mut Controller, &mut AppHost) + Send + 'static) {
    let ui = state().ui.clone();
    if let Some(ui) = ui {
        on_ui(ui, f);
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

/// Every module's registration. The perf build adds the synthetic 10,000-item index
/// (`NAOBI_PERF_ITEMS` overrides the count), so the latency runs cross the real engine.
fn registry() -> modules::Modules {
    let mut m = modules::Modules::new();
    #[cfg(feature = "perf")]
    {
        let n = std::env::var("NAOBI_PERF_ITEMS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(10_000);
        m.register_source(Box::new(perf_fixture::PerfSource::new(n)));
        if let Ok(target) = std::env::var("NAOBI_TEST_TARGET") {
            m.register_source(Box::new(perf_fixture::TestTarget(target)));
        }
    }
    modules::register_all(&mut m);
    m
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
            let modules::Parts {
                sources,
                views,
                router,
            } = registry().into_parts();
            let launcher = Launcher::new(sources, views, frecency, Box::new(SystemClock));
            (Some(Controller::new(launcher, router)), warn)
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
        // A catalog scan may have finished before the controller existed.
        with_controller(ui, |c, h| c.refresh(h));
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
    // `ui.text_scale` from the settings (1.0, 1.125 or 1.25).
    screen.text_scale = s.settings.as_ref().map_or(1.0, |set| set.ui.text_scale);
    let preview = screen
        .theme_preview
        .as_deref()
        .and_then(|id| theme_file::find(id, &s.catalog));
    if let Some(theme) = preview.as_ref().or(s.theme.as_ref()) {
        screen.row_height = theme.density.row_height();
        let mut drawn = s.drawn.borrow_mut();
        if drawn.as_ref() != Some(theme) {
            ui.set_theme(theme);
            *drawn = Some(theme.clone());
        }
    }
    screen.notice = s.notices.banner().map(|n| NoticeView {
        message: n.message.clone(),
    });
    if screen.notice.is_none() {
        // A previewed Theme says the same as it would once applied (R-033).
        screen.notice = preview
            .as_ref()
            .and_then(theme_file::contrast_message)
            .map(|message| NoticeView { message });
    }
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
    modules::awake::set_notice(notices.banner().map(|n| n.message.clone()));
}

/// R-008: another process owns the chord of `name`, so that hotkey is off. Names the chord when known.
fn on_conflict(name: &str) {
    let mut s = state();
    let chord = s
        .settings
        .as_ref()
        .and_then(|set| set.hotkeys.get(name))
        .copied();
    let notice = notices::hotkey_conflict(name, chord);
    // A conflict on the chord this hotkey had before no longer applies.
    s.notices.clear_conflict(name, Some(&notice));
    raise_notice(&mut s, notice);
}

/// These hotkeys are registered now: a conflict Notice about them is out of date.
fn on_hotkeys_held(names: &[naobi_core::ids::HotkeyId]) {
    let mut s = state();
    let mut cleared = false;
    for id in names {
        cleared |= s.notices.clear_conflict(&id.0, None);
    }
    if cleared {
        sync_tray(&s.notices);
        if let Some(ui) = s.ui.clone() {
            post_view(&ui, &s);
        }
    }
}

const THEME_SUBJECT: &str = "Theme";

/// The palette of `theme`, or of the Built-in theme of Windows mode `mode` before one is resolved.
fn palette_of(theme: Option<&ResolvedTheme>, mode: Theme) -> Palette {
    theme.map_or_else(|| theme_file::builtin(mode).palette, |t| t.palette)
}

/// The colors the color picker overlay draws with (R-030).
pub(crate) fn active_palette() -> Palette {
    let s = state();
    palette_of(s.theme.as_ref(), s.look.theme)
}

/// Works out the Theme for the current slot and Windows mode, redraws when it changed, and keeps the
/// Theme Notices in step: one the Owner dismissed is not raised again until its text changes.
fn refresh_theme(s: &mut State) {
    let slots = s.settings.clone().unwrap_or_default().theme;
    let (active, mut messages) = theme_file::resolve(&slots, s.look.theme, &s.catalog);
    messages.extend(s.catalog.problems.iter().cloned());
    let old = std::mem::replace(&mut s.theme_notices, messages.clone());
    for gone in old.iter().filter(|m| !messages.contains(m)) {
        s.notices.clear(THEME_SUBJECT, gone);
    }
    if s.theme.as_ref() != Some(&active) {
        modules::aot::set_accent(active.palette.accent.to_rgb());
        s.theme = Some(active);
    }
    for message in messages.into_iter().filter(|m| !old.contains(m)) {
        raise_notice(
            s,
            Notice {
                subject: THEME_SUBJECT.into(),
                message,
                kind: NoticeKind::Warning,
            },
        );
    }
    sync_tray(&s.notices);
}

/// R-007: Windows changed its theme or animation setting. The look follows it, the Theme slot included.
fn on_theme() {
    let mut s = state();
    if let Some(look) = theme::follow(&AppEvent::ThemeChanged, &SystemTheme, s.look) {
        let mode_changed = look.theme != s.look.theme;
        s.look = look;
        if mode_changed {
            refresh_theme(&mut s);
        }
        if let Some(ui) = s.ui.clone() {
            post_view(&ui, &s);
        }
    }
}

/// Reads the themes folder again and draws the result.
fn reload_themes(dir: &Path) {
    let catalog = modules::themes::load(dir);
    let mut s = state();
    s.catalog = catalog;
    refresh_theme(&mut s);
    if let Some(ui) = s.ui.clone() {
        post_view(&ui, &s);
    }
}

/// Runs on the index worker once the themes folder settled (R-031): a saved file applies at once, and
/// an open Themes view lists it.
fn on_themes_changed(dir: &Path) {
    reload_themes(dir);
    to_modules(|c, h| c.refresh(h));
}

/// Puts `id` in the Theme slot of the current Windows mode (R-031).
fn apply_theme(id: &str) -> Result<(), String> {
    let dir = naobi_core::paths::settings_dir().ok_or("The settings folder is not known.")?;
    let slot = match state().look.theme {
        Theme::Paper => "light",
        Theme::Stone => "dark",
    };
    let settings =
        settings_runtime::write_theme_slot(&dir.join(settings_runtime::FILE_NAME), slot, id)?;
    let mut s = state();
    s.settings = Some(settings);
    refresh_theme(&mut s);
    Ok(())
}

/// Runs on the index worker once the settings folder settled (D-009). A file that does not parse keeps
/// the last valid settings and raises a Notice. A changed `general.autostart` is written (D-007).
fn on_settings_changed(path: &Path) {
    let last = state().settings.clone().unwrap_or_default();
    let (next, notices) = settings_runtime::reload(path, &last);
    if next.general.autostart != last.general.autostart {
        sync_autostart(next.general.autostart);
    }
    let chords_changed = next.hotkeys != last.hotkeys;
    // The settings are stored first, so a conflict event that follows names the new chord.
    {
        let mut s = state();
        s.settings = Some(next.clone());
        for n in notices {
            raise_notice(&mut s, n);
        }
        refresh_theme(&mut s);
    }
    // Changed chords are registered now, without a restart (D-009); a refused one raises the usual
    // conflict Notice.
    if chords_changed {
        naobi_win32::platform::reload_hotkeys(next.clone());
    }
    // Quicklinks are read live, so an edited alias works at once; an open Launcher ranks again.
    modules::aot::set_config(&next);
    modules::clipboard_capture::set_config(&next);
    modules::builtin::set_quicklinks(next.quicklinks);
    to_modules(|c, h| c.refresh(h));
}

/// A module's catalog was rebuilt on the index worker: a failure raises its Notice, and the Launcher
/// ranks the current query again. Before the window is ready the controller is not there; `on_ready`
/// ranks once when it is, so nothing is lost.
fn on_catalog(failure: Option<Notice>) {
    if let Some(n) = failure {
        raise_notice(&mut state(), n);
    }
    to_modules(|c, h| c.refresh(h));
}

/// Routes what P1 forwards: the Launcher's own events here, the modules' through the registry.
fn route(ev: AppEvent) {
    match ev {
        AppEvent::Hotkey(id) if id.0 == "launcher" => {
            logging::hotkey(&id.0);
            on_hotkey();
        }
        AppEvent::ShowRequested => on_show(),
        // A module's own hotkey, and the events modules subscribe to, run on the UI thread.
        AppEvent::Hotkey(id) => to_modules(move |c, h| {
            c.on_hotkey(&id, h);
        }),
        ev @ (AppEvent::ClipboardChanged(_)
        | AppEvent::Resume
        | AppEvent::DisplayChanged
        | AppEvent::SettingsChanged
        | AppEvent::Timer(_)) => to_modules(move |c, h| {
            c.on_app_event(&ev, h);
        }),
        AppEvent::HotkeyConflict(id) => on_conflict(&id.0),
        AppEvent::HotkeysHeld(names) => on_hotkeys_held(&names),
        AppEvent::ThemeChanged => on_theme(),
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

/// True for an exe in Cargo's `target/debug` or `target/release`: a build, not the installed program.
fn is_cargo_build(exe: &Path) -> bool {
    let mut up = exe.ancestors().skip(1).map(Path::file_name);
    up.next()
        .flatten()
        .is_some_and(|d| d == "debug" || d == "release")
        && up.next().flatten().is_some_and(|d| d == "target")
}

/// Writes the Run value for `enabled` at the program's own path, if this build writes one. A Cargo
/// build leaves the Owner's real Run key alone: `target` can be cleaned away.
fn sync_autostart(enabled: bool) {
    let Some(key) = autostart_key() else { return };
    match std::env::current_exe() {
        Ok(exe) if key == autostart::RUN_KEY && is_cargo_build(&exe) => {}
        Ok(exe) => apply_autostart(enabled, &exe, &key),
        Err(_) => {
            tracing::warn!("autostart: the program path is unknown; the Run value was not updated")
        }
    }
}

/// The argument of the scan child: the process lists the installed apps on stdout and exits.
pub use modules::apps::LIST_APPS_ARG;
/// The file search child's argument and entry point (`modules::files`).
pub use modules::files::{search_files, SEARCH_FILES_ARG};
/// The icon child's argument and entry point (`modules::icons`).
pub use modules::icons::{icons, ICONS_ARG};

/// The scan child (`--list-apps`): one line per app, `id<TAB>name`. It runs in its own process so the
/// shell and COM libraries the scan loads (about 19 MB of working set) never enter the resident one.
pub fn list_apps() -> anyhow::Result<()> {
    let apps = naobi_win32::apps::list()?;
    modules::apps::write_list(&mut std::io::stdout().lock(), &apps)?;
    Ok(())
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
    modules::builtin::set_quicklinks(settings.quicklinks.clone());
    modules::aot::set_config(&settings);
    {
        let mut s = state();
        s.settings = Some(settings.clone());
        s.look = theme::read(&SystemTheme);
    }
    // P1 first: hotkeys registered and tray icon shown before any UI work.
    platform::start_with(&settings, route, || std::process::exit(0))?;
    report("tray_ready");
    let mut jobs = Vec::new();
    if let Some(dir) = settings_dir {
        let themes = modules::themes::folder(&dir);
        if let Err(e) = std::fs::create_dir_all(&themes) {
            tracing::warn!(error = ?e.kind(), "the themes folder was not created");
        }
        state().catalog = modules::themes::load(&themes);
        jobs.push(index_worker::Job {
            dirs: vec![(themes.clone(), false)],
            initial: false,
            settle: None,
            run: Box::new(move || on_themes_changed(&themes)),
        });
        let path = dir.join(settings_runtime::FILE_NAME);
        jobs.push(index_worker::Job {
            dirs: vec![(dir, false)],
            initial: false,
            settle: None,
            run: Box::new(move || on_settings_changed(&path)),
        });
    }
    jobs.extend(modules::apps::job(on_catalog));
    if let Err(e) = index_worker::start(jobs) {
        tracing::warn!(error = %e, "the index worker did not start: hot reload and the app catalog are off");
    }
    {
        let mut s = state();
        for n in boot_notices {
            raise_notice(&mut s, n);
        }
        refresh_theme(&mut s);
    }
    let active = {
        let s = state();
        let active = s
            .theme
            .clone()
            .unwrap_or_else(|| theme_file::builtin(s.look.theme));
        *s.drawn.borrow_mut() = Some(active.clone());
        active
    };
    naobi_ui::run(&active, on_event, on_ready)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // @R-030
    #[test]
    fn r030_picker_and_border_use_the_active_theme() {
        let mut custom = theme_file::builtin(Theme::Stone);
        custom.palette.accent = naobi_core::tokens::Rgba::rgb(0x7ED7C1);
        assert_eq!(
            palette_of(Some(&custom), Theme::Paper).accent.to_rgb(),
            0x7ED7C1,
            "the active theme wins over the Windows mode"
        );
        assert_eq!(
            palette_of(None, Theme::Stone),
            theme_file::builtin(Theme::Stone).palette
        );
    }

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

    #[test]
    fn d007_only_an_exe_in_cargos_target_folder_counts_as_a_build() {
        for exe in [
            r"C:\p\target\debug\naobi-app.exe",
            r"C:\p\target\release\naobi-app.exe",
        ] {
            assert!(is_cargo_build(Path::new(exe)), "{exe}");
        }
        for exe in [
            r"C:\Users\x\AppData\Local\NaobiCommand\bin\naobi-app.exe",
            r"C:\release\naobi-app.exe",
        ] {
            assert!(!is_cargo_build(Path::new(exe)), "{exe}");
        }
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
