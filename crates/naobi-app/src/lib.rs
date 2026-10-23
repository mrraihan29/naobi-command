//! Composition root: starts the platform thread, then the pre-warmed Launcher window.
#![forbid(unsafe_code)]

#[cfg(feature = "perf")]
mod perf_report;

use naobi_core::ui::{Key, Row, UiEvent, ViewModel};
use naobi_ui::UiHandle;
use naobi_win32::{dwm, platform};
use std::sync::Mutex;

/// The window handle once it is ready, and a hotkey that arrived before that.
struct State {
    ui: Option<UiHandle>,
    early_hotkey: bool,
    view: ViewModel,
}

static STATE: Mutex<State> = Mutex::new(State {
    ui: None,
    early_hotkey: false,
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
        None => s.early_hotkey = true,
    }
}

fn on_ready(ui: UiHandle) {
    let mut s = state();
    s.view = placeholder();
    ui.post(s.view.clone());
    report("window_ready");
    if std::mem::take(&mut s.early_hotkey) {
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
        },
        UiEvent::RowActivated(_) | UiEvent::Dismissed => ui.hide(),
        // Search is not wired yet: the placeholder list stays as it is.
        UiEvent::TextChanged(q) => s.view.query = q,
    }
}

fn move_selection(s: &mut State, ui: &UiHandle, delta: isize) {
    s.view.move_selection(delta);
    ui.post(s.view.clone());
}

pub fn run() -> anyhow::Result<()> {
    // P1 first: hotkey registered and tray icon shown before any UI work.
    // ponytail: a hotkey conflict returns Err and exits; the visible conflict Notice is R-008, NCMD-12/NCMD-13.
    platform::start(on_hotkey, || std::process::exit(0))?;
    report("tray_ready");
    let theme = if dwm::system_uses_dark_theme() {
        naobi_core::ui::Theme::Stone
    } else {
        naobi_core::ui::Theme::Paper
    };
    naobi_ui::run(theme, on_event, on_ready)?;
    Ok(())
}
