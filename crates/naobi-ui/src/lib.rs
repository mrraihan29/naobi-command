//! Slint adapter (software renderer, accessibility on). The only crate that names Slint (ADR-0002).
#![deny(unsafe_code)]

use naobi_core::ui::{Key, KeyChord, Theme, UiEvent, ViewModel};
use naobi_win32::dwm;
use slint::platform::Key as SlintKey;
use slint::{ComponentHandle, ModelRc, VecModel, Weak};
use std::rc::Rc;
use std::time::Duration;

// reason: generated code we do not write; it unwraps internally and cannot be edited.
#[allow(clippy::unwrap_used, clippy::expect_used)] // naobi-allow: generated Slint code, cannot be edited
mod generated {
    slint::include_modules!();
}
use generated::{AppWindow, Row, Tokens};

/// Title of the Launcher window; tier 2 tests find it by this.
pub const TITLE: &str = "Naobi Command Launcher";

/// Cheap, thread-safe handle to the Launcher window. Every call is applied on the UI thread.
#[derive(Clone)]
pub struct UiHandle {
    ui: Weak<AppWindow>,
    hwnd: isize,
}

impl UiHandle {
    /// The native window handle as a plain value; constant for the life of the process (R-023).
    pub fn window(&self) -> isize {
        self.hwnd
    }

    pub fn post(&self, vm: ViewModel) {
        let _ = self.ui.upgrade_in_event_loop(move |ui| apply(&ui, vm));
    }

    pub fn show(&self) {
        self.on_ui(show_now);
    }

    pub fn hide(&self) {
        self.on_ui(|_, hwnd| dwm::hide(hwnd));
    }

    /// Shows the window when it is cloaked, cloaks it otherwise.
    pub fn toggle(&self) {
        self.on_ui(|ui, hwnd| {
            if dwm::is_cloaked(hwnd) {
                show_now(ui, hwnd)
            } else {
                dwm::hide(hwnd)
            }
        });
    }

    fn on_ui(&self, f: impl FnOnce(&AppWindow, isize) + Send + 'static) {
        let hwnd = self.hwnd;
        let _ = self.ui.upgrade_in_event_loop(move |ui| f(&ui, hwnd));
    }
}

fn show_now(ui: &AppWindow, hwnd: isize) {
    dwm::show(hwnd);
    ui.invoke_focus_input();
}

fn apply(ui: &AppWindow, vm: ViewModel) {
    let rows: Vec<Row> = vm
        .rows
        .into_iter()
        .map(|r| Row {
            title: r.title.into(),
            subtitle: r.subtitle.into(),
        })
        .collect();
    ui.set_rows(ModelRc::new(VecModel::from(rows)));
    ui.set_query(vm.query.into());
    ui.set_selected(vm.selected as i32);
}

fn key_from(text: &str) -> Option<Key> {
    let c = text.chars().next()?;
    [
        (SlintKey::UpArrow, Key::Up),
        (SlintKey::DownArrow, Key::Down),
        (SlintKey::Return, Key::Enter),
        (SlintKey::Escape, Key::Escape),
    ]
    .into_iter()
    .find(|(k, _)| char::from(*k) == c)
    .map(|(_, key)| key)
}

/// Creates the Launcher window hidden, wires its events and runs the main-thread loop until quit.
/// Once the native window exists and is cloaked, `on_ready` receives the handle.
pub fn run(
    theme: Theme,
    on_event: impl Fn(UiEvent) + 'static,
    on_ready: impl FnOnce(UiHandle) + Send + 'static,
) -> Result<(), slint::PlatformError> {
    let ui = AppWindow::new()?;
    ui.global::<Tokens>().set_dark(theme == Theme::Stone);
    let on_event = Rc::new(on_event);
    {
        let f = on_event.clone();
        ui.on_text_changed(move |q| f(UiEvent::TextChanged(q.into())));
        let f = on_event.clone();
        ui.on_row_activated(move |i| f(UiEvent::RowActivated(i.max(0) as usize)));
        ui.on_key_pressed(move |text, ctrl, alt, shift| match key_from(&text) {
            Some(key) => {
                on_event(UiEvent::Key(KeyChord {
                    key,
                    ctrl,
                    alt,
                    shift,
                }));
                true
            }
            None => false,
        });
    }
    // The native window exists only once the loop runs: park it off-screen, then cloak it in setup.
    ui.window()
        .set_position(slint::PhysicalPosition::new(-32000, -32000));
    ui.show()?;
    let weak = ui.as_weak();
    slint::invoke_from_event_loop(move || setup(weak, Box::new(on_ready)))
        .map_err(|e| slint::PlatformError::Other(e.to_string()))?;
    slint::run_event_loop_until_quit()
}

type Ready = Box<dyn FnOnce(UiHandle) + Send>;

/// Finds the native window (retrying every 2 ms until winit has created it), cloaks it, reports ready.
fn setup(ui: Weak<AppWindow>, on_ready: Ready) {
    let Some(hwnd) = dwm::find_window(TITLE) else {
        slint::Timer::single_shot(Duration::from_millis(2), move || setup(ui, on_ready));
        return;
    };
    dwm::prepare(hwnd);
    on_ready(UiHandle { ui, hwnd });
}
