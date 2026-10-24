//! Composition root: starts the platform thread, then the pre-warmed Launcher window.
#![forbid(unsafe_code)]

#[cfg(feature = "perf")]
mod perf_fixture;
#[cfg(feature = "perf")]
mod perf_report;

use naobi_core::event::AppEvent;
use naobi_core::settings::Settings;
use naobi_core::ui::{Key, Row, UiEvent, ViewModel};
use naobi_ui::UiHandle;
use naobi_win32::{autostart, dwm, instance, platform};
use std::sync::Mutex;

/// The window handle once it is ready, and a show request that arrived before that.
struct State {
    ui: Option<UiHandle>,
    early_show: bool,
    view: ViewModel,
}

static STATE: Mutex<State> = Mutex::new(State {
    ui: None,
    early_show: false,
    view: ViewModel {
        query: String::new(),
        rows: Vec::new(),
        selected: 0,
    },
});

fn state() -> std::sync::MutexGuard<'static, State> {
    STATE.lock().unwrap_or_else(|e| e.into_inner())
}

#[cfg(feature = "perf")]
fn report(ev: &str) {
    perf_report::emit(ev);
}
#[cfg(not(feature = "perf"))]
fn report(_ev: &str) {}

/// The synthetic 10,000-item index (`NAOBI_PERF_ITEMS` overrides the count), the real event path's data.
#[cfg(feature = "perf")]
static FIXTURE: std::sync::OnceLock<Mutex<perf_fixture::Fixture>> = std::sync::OnceLock::new();

#[cfg(feature = "perf")]
fn fixture_rows(q: &str) -> Vec<Row> {
    let n = std::env::var("NAOBI_PERF_ITEMS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(10_000);
    let f = FIXTURE.get_or_init(|| Mutex::new(perf_fixture::Fixture::new(n)));
    f.lock().unwrap_or_else(|e| e.into_inner()).top(q)
}

fn placeholder() -> ViewModel {
    let row = |t: &str, s: &str| Row {
        title: t.into(),
        subtitle: s.into(),
    };
    ViewModel {
        query: String::new(),
        rows: vec![
            row("Visual Studio Code", "Application"),
            row("Windows Terminal", "Application"),
            row("Calculator", "Application"),
            row("Settings", "System"),
            row("Notepad", "Application"),
        ],
        selected: 0,
    }
}

fn on_hotkey() {
    let mut s = state();
    match &s.ui {
        Some(ui) => ui.toggle(),
        // Pressed before the window is ready: served as soon as it is.
        None => s.early_show = true,
    }
}

/// Shows the Launcher (second instance, tray Open). Never toggles it off.
fn on_show() {
    let mut s = state();
    match &s.ui {
        Some(ui) => ui.show(),
        None => s.early_show = true,
    }
}

fn on_ready(ui: UiHandle) {
    let mut s = state();
    s.view = placeholder();
    #[cfg(feature = "perf")]
    {
        s.view.rows = fixture_rows("");
        // Same UI thread as the window: t0 of the keystroke metric is taken in this subclass.
        naobi_win32::subclass::install(ui.window());
    }
    ui.post(s.view.clone());
    report("window_ready");
    if std::mem::take(&mut s.early_show) {
        ui.show();
    }
    s.ui = Some(ui);
}

fn on_event(ev: UiEvent) {
    let mut s = state();
    let Some(ui) = s.ui.clone() else { return };
    match ev {
        UiEvent::Key(c) => match c.key {
            Key::Escape | Key::Enter => ui.hide(),
            Key::Up => move_selection(&mut s, &ui, -1),
            Key::Down => move_selection(&mut s, &ui, 1),
            // Routed by NCMD-15 (page moves, Backspace pop, Ctrl+. pin, Ctrl+X delete, Ctrl+K panel).
            Key::PageUp | Key::PageDown | Key::Backspace | Key::Period | Key::X | Key::K => {}
        },
        UiEvent::RowActivated(_) | UiEvent::Dismissed => ui.hide(),
        // Search is not wired yet: the placeholder list stays as it is (perf: the fixture answers).
        UiEvent::TextChanged(q) => {
            #[cfg(feature = "perf")]
            {
                s.view.rows = fixture_rows(&q);
                s.view.selected = 0;
                s.view.query = q;
                ui.post(s.view.clone());
            }
            #[cfg(not(feature = "perf"))]
            {
                s.view.query = q;
            }
        }
    }
}

fn move_selection(s: &mut State, ui: &UiHandle, delta: isize) {
    s.view.move_selection(delta);
    ui.post(s.view.clone());
}

/// Settings from `%APPDATA%\NaobiCommand\settings.toml`. A missing or unreadable file means defaults.
// seam NCMD-13: the parse reports are dropped here until the Notice item shows them.
fn load_settings() -> Settings {
    let text = naobi_core::paths::settings_dir()
        .and_then(|dir| std::fs::read_to_string(dir.join("settings.toml")).ok())
        .unwrap_or_default();
    Settings::apply(&Settings::default(), &text).settings
}

/// Event routing into the controller belongs to NCMD-15. NCMD-12 needs only the launcher hotkey
/// and `ShowRequested` (R-023 second instance, tray Open).
// seam NCMD-15: every other hotkey, Resume, ThemeChanged, DisplayChanged, Timer, ClipboardChanged
// and SettingsChanged are routed here. The hotkey-conflict Notice is NCMD-13 (HotkeyConflict is dropped).
fn route(ev: AppEvent) {
    match ev {
        AppEvent::Hotkey(id) if id.0 == "launcher" => on_hotkey(),
        AppEvent::ShowRequested => on_show(),
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

/// D-007: the HKCU Run value follows `general.autostart`. A failure is reported without any path or
/// setting value, and never stops the app.
// seam NCMD-13: the same call runs again when settings are reloaded.
fn apply_autostart(enabled: bool, exe: &std::path::Path, key: &str) {
    if let Err(e) = autostart::set_enabled(key, exe, enabled) {
        // seam NCMD-13: the logging facility replaces this line; the text is the system error only.
        eprintln!("autostart: the Run value was not updated: {e}");
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
    let settings = load_settings();
    if let Some(key) = autostart_key() {
        match std::env::current_exe() {
            Ok(exe) => apply_autostart(settings.general.autostart, &exe, &key),
            Err(_) => {
                eprintln!("autostart: the program path is unknown; the Run value was not updated")
            }
        }
    }
    // P1 first: hotkeys registered and tray icon shown before any UI work.
    platform::start_with(&settings, route, || std::process::exit(0))?;
    report("tray_ready");
    let theme = if dwm::system_uses_dark_theme() {
        naobi_core::ui::Theme::Stone
    } else {
        naobi_core::ui::Theme::Paper
    };
    naobi_ui::run(theme, on_event, on_ready)?;
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
