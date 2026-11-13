//! Slint adapter (software renderer, accessibility on). The only crate that names Slint (ADR-0002).
#![deny(unsafe_code)]

mod model;

use naobi_core::tokens::{self, Palette};
use naobi_core::ui::{Key, KeyChord, Screen, Theme, UiEvent, ViewModel};
use naobi_win32::dwm;
use slint::platform::Key as SlintKey;
use slint::{ComponentHandle, ModelRc, VecModel, Weak};
use std::rc::Rc;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

// reason: generated code we do not write; it unwraps internally and cannot be edited.
#[allow(clippy::unwrap_used, clippy::expect_used)] // naobi-allow: generated Slint code, cannot be edited
mod generated {
    slint::include_modules!();
}
use generated::{AppWindow, Tokens};

/// Title of the Launcher window; tier 2 tests find it by this.
pub const TITLE: &str = "Naobi Command Launcher";
/// Launcher width in logical px (DESIGN_SYSTEM.md, size.window).
const WIDTH: f32 = 750.0;

/// Cheap, thread-safe handle to the Launcher window. Every call is applied on the UI thread.
#[derive(Clone)]
pub struct UiHandle {
    ui: Weak<AppWindow>,
    hwnd: isize,
    /// The newest screen not yet applied. Posts overwrite it, so the UI thread applies only the
    /// latest one (latest wins).
    pending: Arc<Mutex<Option<Screen>>>,
}

impl UiHandle {
    /// The native window handle as a plain value; constant for the life of the process (R-023).
    pub fn window(&self) -> isize {
        self.hwnd
    }

    /// Shows a list-only snapshot. Full screens go through `post_screen`.
    pub fn post(&self, vm: ViewModel) {
        self.post_screen(vm.into());
    }

    pub fn post_screen(&self, screen: Screen) {
        if offer(&self.pending, screen) {
            let pending = self.pending.clone();
            if self
                .ui
                .upgrade_in_event_loop(move |ui| flush(&ui, &pending))
                .is_err()
            {
                lock(&self.pending).take();
            }
        }
    }

    /// Work-area height of the Launcher's monitor, logical px. The window is clamped to 90% of it.
    pub fn set_work_area(&self, height: f32) {
        self.on_ui(move |ui, _| {
            ui.set_work_height(height);
            resize(ui);
        });
    }

    /// Switches the palette while the window lives (R-007: Windows changed light or dark).
    pub fn set_theme(&self, theme: Theme) {
        self.on_ui(move |ui, _| set_palette(ui, palette_of(theme)));
    }

    pub fn show(&self) {
        self.on_ui(show_now);
    }

    pub fn hide(&self) {
        self.on_ui(|ui, hwnd| {
            ui.set_active(false);
            dwm::hide(hwnd)
        });
    }

    /// Shows the window when it is cloaked, cloaks it otherwise.
    pub fn toggle(&self) {
        self.on_ui(|ui, hwnd| {
            if dwm::is_cloaked(hwnd) {
                show_now(ui, hwnd)
            } else {
                ui.set_active(false);
                dwm::hide(hwnd)
            }
        });
    }

    /// Marks the window as cloaked, so nothing animates while it cannot be seen. Used when the
    /// controller cloaks it directly.
    pub fn set_active(&self, active: bool) {
        self.on_ui(move |ui, _| ui.set_active(active));
    }

    /// Runs `f` on the UI thread after `ms` milliseconds.
    pub fn run_on_ui_after(&self, ms: u64, f: impl FnOnce() + Send + 'static) {
        let _ = self.ui.upgrade_in_event_loop(move |_| {
            slint::Timer::single_shot(Duration::from_millis(ms), f);
        });
    }

    /// Runs `f` on the UI thread, where the controller lives.
    pub fn run_on_ui(&self, f: impl FnOnce() + Send + 'static) {
        let _ = self.ui.upgrade_in_event_loop(move |_| f());
    }

    fn on_ui(&self, f: impl FnOnce(&AppWindow, isize) + Send + 'static) {
        let hwnd = self.hwnd;
        let _ = self.ui.upgrade_in_event_loop(move |ui| f(&ui, hwnd));
    }
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Stores `item` as the newest pending one. Returns true when nothing was pending, which means
/// the UI thread has to be woken; a later item replaces an unapplied earlier one.
fn offer<T>(slot: &Mutex<Option<T>>, item: T) -> bool {
    lock(slot).replace(item).is_none()
}

fn flush(ui: &AppWindow, pending: &Mutex<Option<Screen>>) {
    let next = lock(pending).take();
    if let Some(screen) = next {
        apply(ui, screen);
    }
}

fn show_now(ui: &AppWindow, hwnd: isize) {
    // The size follows the monitor the Launcher is about to open on.
    ui.set_work_height(dwm::work_height_logical());
    let height = resize(ui);
    ui.set_active(true);
    dwm::show(hwnd, height);
    ui.invoke_focus_input();
}

fn palette_of(theme: Theme) -> &'static Palette {
    match theme {
        Theme::Paper => &tokens::PAPER,
        Theme::Stone => &tokens::STONE,
    }
}

fn set_palette(ui: &AppWindow, p: &Palette) {
    let t = ui.global::<Tokens>();
    t.set_surface_window(model::color(p.window));
    t.set_surface_raised(model::color(p.raised));
    t.set_surface_selected(model::color(p.selected));
    t.set_surface_well(model::color(p.well));
    t.set_text_primary(model::color(p.text_primary));
    t.set_subtitle_selected(model::color(p.subtitle_selected));
    t.set_text_muted(model::color(p.text_muted));
    t.set_accent(model::color(p.accent));
    t.set_focus(model::color(p.focus));
    t.set_danger(model::color(p.danger));
    t.set_line_hair(model::color(p.hair));
    t.set_line_strong(model::color(p.strong));
}

/// Sets the window height from the text scale and the work area (model::frame_height).
fn resize(ui: &AppWindow) -> f32 {
    let h = model::frame_height(ui.global::<Tokens>().get_scale(), ui.get_work_height());
    ui.window().set_size(slint::LogicalSize::new(WIDTH, h));
    h
}

fn apply(ui: &AppWindow, s: Screen) {
    let t = ui.global::<Tokens>();
    t.set_scale(s.text_scale);
    t.set_motion(if s.reduced_motion { 0.0 } else { 1.0 });
    resize(ui);

    let lines = model::lines(&s);
    ui.set_lines(ModelRc::new(VecModel::from(lines.items)));
    ui.set_selected(lines.selected);
    ui.set_query(s.query.as_str().into());
    ui.set_count_text(model::count_text(&s).into());
    ui.set_notice(s.notice.as_ref().map_or("", |n| n.message.as_str()).into());
    ui.set_loading(s.loading);

    let empty = s.empty.as_ref();
    ui.set_empty_visible(empty.is_some());
    ui.set_empty_title(empty.map_or("", |e| e.title.as_str()).into());
    ui.set_empty_helper(empty.map_or("", |e| e.helper.as_str()).into());

    ui.set_footer_primary(s.footer.primary.as_deref().unwrap_or_default().into());
    ui.set_footer_actions(s.footer.actions);
    ui.set_footer_confirmation(s.footer.confirmation.as_deref().unwrap_or_default().into());

    let (kind, text, picture) = model::preview(s.preview.as_ref());
    ui.set_preview_kind(kind);
    ui.set_preview_text(text);
    ui.set_preview_image(picture);

    let panel = s.action_panel.as_ref();
    ui.set_panel_open(panel.is_some());
    ui.set_panel_filter(panel.map_or("", |p| p.filter.as_str()).into());
    let actions = panel.map(model::actions).unwrap_or_default();
    ui.set_actions(ModelRc::new(VecModel::from(actions)));
}

/// Maps a key event to a `Key`. Named keys need no modifier; Ctrl chords arrive as the letter or
/// as its control character, so the letters count only with Ctrl (typing "k" stays in the field).
fn key_from(text: &str, ctrl: bool) -> Option<Key> {
    let c = text.chars().next()?;
    let named = [
        (SlintKey::UpArrow, Key::Up),
        (SlintKey::DownArrow, Key::Down),
        (SlintKey::PageUp, Key::PageUp),
        (SlintKey::PageDown, Key::PageDown),
        (SlintKey::Return, Key::Enter),
        (SlintKey::Escape, Key::Escape),
        (SlintKey::Backspace, Key::Backspace),
    ];
    if let Some((_, key)) = named.into_iter().find(|(k, _)| char::from(*k) == c) {
        return Some(key);
    }
    if !ctrl {
        return None;
    }
    match c {
        '.' => Some(Key::Period),
        'k' | 'K' | '\u{b}' => Some(Key::K),
        'x' | 'X' | '\u{18}' => Some(Key::X),
        _ => None,
    }
}

/// Creates the Launcher window hidden, wires its events and runs the main-thread loop until quit.
/// Once the native window exists and is cloaked, `on_ready` receives the handle.
pub fn run(
    theme: Theme,
    on_event: impl Fn(UiEvent) + 'static,
    on_ready: impl FnOnce(UiHandle) + Send + 'static,
) -> Result<(), slint::PlatformError> {
    let ui = AppWindow::new()?;
    set_palette(&ui, palette_of(theme));
    // The window size comes from Rust only: 750 x frame_height, here at 100% and no work area.
    resize(&ui);
    let on_event = Rc::new(on_event);
    {
        let f = on_event.clone();
        ui.on_text_changed(move |q| f(UiEvent::TextChanged(q.into())));
        let f = on_event.clone();
        ui.on_row_activated(move |i| f(UiEvent::RowActivated(i.max(0) as usize)));
        ui.on_key_pressed(move |text, ctrl, alt, shift| match key_from(&text, ctrl) {
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
    on_ready(UiHandle {
        ui,
        hwnd,
        pending: Arc::default(),
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offer_wakes_the_ui_once_and_keeps_only_the_latest() {
        let slot = Mutex::new(None);
        assert!(offer(&slot, 1), "first item wakes the UI thread");
        assert!(
            !offer(&slot, 2),
            "a pending item is replaced, no second wake"
        );
        assert!(!offer(&slot, 3));
        assert_eq!(lock(&slot).take(), Some(3));
        assert!(
            offer(&slot, 4),
            "after the flush, the next item wakes again"
        );
    }

    #[test]
    fn letters_are_keys_only_with_ctrl() {
        assert_eq!(key_from("k", false), None, "typing k stays in the field");
        assert_eq!(key_from("x", false), None);
        assert_eq!(key_from(".", false), None);
        assert_eq!(key_from("k", true), Some(Key::K));
        assert_eq!(key_from("K", true), Some(Key::K));
        assert_eq!(
            key_from("\u{b}", true),
            Some(Key::K),
            "Ctrl+K as control character"
        );
        assert_eq!(key_from("x", true), Some(Key::X));
        assert_eq!(
            key_from("\u{18}", true),
            Some(Key::X),
            "Ctrl+X as control character"
        );
        assert_eq!(key_from(".", true), Some(Key::Period));
        assert_eq!(key_from("j", true), None);
    }

    #[test]
    fn named_keys_map_with_or_without_modifiers() {
        let named = |k: SlintKey| char::from(k).to_string();
        assert_eq!(key_from(&named(SlintKey::PageUp), false), Some(Key::PageUp));
        assert_eq!(
            key_from(&named(SlintKey::PageDown), false),
            Some(Key::PageDown)
        );
        assert_eq!(
            key_from(&named(SlintKey::Backspace), false),
            Some(Key::Backspace)
        );
        assert_eq!(key_from(&named(SlintKey::Return), true), Some(Key::Enter));
        assert_eq!(key_from(&named(SlintKey::UpArrow), false), Some(Key::Up));
        assert_eq!(
            key_from(&named(SlintKey::DownArrow), false),
            Some(Key::Down)
        );
        assert_eq!(key_from(&named(SlintKey::Escape), false), Some(Key::Escape));
        assert_eq!(key_from("", false), None);
    }
}

/// UI-CHECK: renders the main views in paper and stone with Slint's software renderer into BMP
/// files under `NAOBI_UI_CHECK_DIR`. Runs with the tier-1 tests; set NAOBI_UI_CHECK_DIR to write the BMP frames.
#[cfg(test)]
mod ui_check {
    use super::*;
    use naobi_core::ui::{
        ActionPanelView, ActionView, EmptyView, FooterView, NoticeView, PreviewView, RowView,
        Section,
    };
    use slint::platform::software_renderer::{
        MinimalSoftwareWindow, PremultipliedRgbaColor, RepaintBufferType,
    };
    use slint::platform::{Platform, WindowAdapter};
    use std::path::Path;

    struct Headless(Rc<MinimalSoftwareWindow>);

    impl Platform for Headless {
        fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, slint::PlatformError> {
            Ok(self.0.clone())
        }
    }

    fn row(kind: &str, title: &str, subtitle: &str) -> RowView {
        RowView {
            kind: kind.into(),
            title: title.into(),
            subtitle: subtitle.into(),
            hint: None,
            calculator: false,
            disabled: false,
            armed: false,
            pinned: false,
            swatch: None,
        }
    }

    fn section(header: &str, rows: Vec<RowView>) -> Section {
        Section {
            header: Some(header.into()),
            rows,
        }
    }

    fn screen(sections: Vec<Section>) -> Screen {
        let mut s = Screen::from(ViewModel::default());
        s.sections = sections;
        s
    }

    /// (file name, screen): the main states of DESIGN_SYSTEM.md and the ticket's UI-CHECK list.
    fn views() -> Vec<(&'static str, Screen)> {
        let mut calc = row("Calculator", "42", "6 x 7 = 42");
        calc.calculator = true;
        calc.hint = Some("Copy".into());
        let mut results = screen(vec![
            section("Calculator", vec![calc]),
            section(
                "Applications",
                vec![
                    row(
                        "Application",
                        "Visual Studio Code",
                        "C:\\Program Files\\Microsoft VS Code\\Code.exe",
                    ),
                    row(
                        "Application",
                        "Windows Terminal",
                        "C:\\Program Files\\WindowsApps\\Terminal.exe",
                    ),
                ],
            ),
            section(
                "Files",
                vec![row("File", "budget.xlsx", "C:\\Users\\Raihan\\Documents")],
            ),
        ]);
        results.query = "code".into();
        results.selected = 1;
        results.footer = FooterView {
            primary: Some("Open".into()),
            actions: true,
            ..FooterView::default()
        };

        let mut panel = results.clone();
        panel.action_panel = Some(ActionPanelView {
            filter: String::new(),
            actions: vec![
                ActionView {
                    label: "Open".into(),
                    shortcut: Some("Enter".into()),
                    reason: None,
                },
                ActionView {
                    label: "Copy path".into(),
                    shortcut: Some("Ctrl Enter".into()),
                    reason: None,
                },
                ActionView {
                    label: "Run as administrator".into(),
                    shortcut: None,
                    reason: Some("Not available for this app".into()),
                },
            ],
            selected: 1,
        });

        let mut clip = row("Clipboard entry", "Image 4 x 3", "3 min ago");
        clip.pinned = true;
        let mut clipboard = screen(vec![section(
            "Clipboard",
            vec![
                row(
                    "Clipboard entry",
                    "git commit -m \"Add views\"",
                    "1 min ago",
                ),
                clip,
                row("Clipboard entry", "3 files", "5 min ago"),
            ],
        )]);
        clipboard.selected = 1;
        clipboard.preview = Some(PreviewView::Image {
            width: 4,
            height: 3,
            rgba: (0..12u8)
                .flat_map(|i| [i * 20, 255 - i * 15, 128, 255])
                .collect(),
        });

        let mut armed_row = row(
            "System command",
            "Restart: press Enter again to confirm",
            "Restart",
        );
        armed_row.armed = true;
        let mut notice = screen(vec![section("System", vec![armed_row])]);
        notice.notice = Some(NoticeView {
            message: "Alt+Space is taken by another app".into(),
        });

        // An armed row or a Notice keeps the primary action and Actions chips (DESIGN_SYSTEM.md).
        notice.footer = FooterView {
            primary: Some("Confirm".into()),
            actions: true,
            ..FooterView::default()
        };
        let mut empty = screen(vec![]);
        empty.empty = Some(EmptyView {
            title: "Nothing copied yet".into(),
            helper: "Copy text or an image and it appears here".into(),
        });
        empty.preview = Some(PreviewView::Empty);

        let mut loading = results.clone();
        loading.action_panel = None;
        loading.loading = true;
        loading.text_scale = 1.25;
        loading.preview = Some(PreviewView::Loading);

        vec![
            ("results", results),
            ("action-panel", panel),
            ("clipboard-image", clipboard),
            ("notice-armed", notice),
            ("empty", empty),
            ("loading-scale-125", loading),
        ]
    }

    /// Writes `px` (rows top to bottom) as a 24-bit bottom-up BMP.
    fn write_bmp(path: &Path, width: u32, height: u32, px: &[PremultipliedRgbaColor]) {
        let stride = (width * 3 + 3) & !3;
        let data_len = stride * height;
        let mut out = Vec::with_capacity(54 + data_len as usize);
        out.extend_from_slice(b"BM");
        out.extend_from_slice(&(54 + data_len).to_le_bytes());
        out.extend_from_slice(&[0; 4]);
        out.extend_from_slice(&54u32.to_le_bytes());
        out.extend_from_slice(&40u32.to_le_bytes());
        out.extend_from_slice(&(width as i32).to_le_bytes());
        out.extend_from_slice(&(height as i32).to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&24u16.to_le_bytes());
        out.extend_from_slice(&[0; 4]); // BI_RGB
        out.extend_from_slice(&data_len.to_le_bytes());
        out.extend_from_slice(&[0; 16]);
        for y in (0..height).rev() {
            for x in 0..width {
                let p = px[(y * width + x) as usize];
                out.extend_from_slice(&[p.blue, p.green, p.red]);
            }
            out.resize(out.len() + (stride - width * 3) as usize, 0);
        }
        std::fs::write(path, out).expect("write screenshot");
    }

    #[test]
    fn ui_check_renders_main_views_and_checks_the_125_percent_frame() {
        // BMP frames are written only when NAOBI_UI_CHECK_DIR is set (the owner's UI-CHECK).
        let dir = std::env::var_os("NAOBI_UI_CHECK_DIR").map(std::path::PathBuf::from);
        if let Some(dir) = &dir {
            std::fs::create_dir_all(dir).expect("screenshot dir");
        }
        let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
        slint::platform::set_platform(Box::new(Headless(window.clone())))
            .expect("platform is set once per test process");
        let ui = AppWindow::new().expect("window");
        ui.show().expect("show");
        for (theme, palette) in [("paper", &tokens::PAPER), ("stone", &tokens::STONE)] {
            set_palette(&ui, palette);
            for (name, s) in views() {
                apply(&ui, s);
                // Let the 120 ms panel fade and rise finish (the animations run on wall-clock time).
                slint::platform::update_timers_and_animations();
                std::thread::sleep(std::time::Duration::from_millis(250));
                slint::platform::update_timers_and_animations();
                let size = ui.window().size();
                let mut px =
                    vec![PremultipliedRgbaColor::default(); (size.width * size.height) as usize];
                window.draw_if_needed(|renderer| {
                    renderer.render(&mut px, size.width as usize);
                });
                if name == "loading-scale-125" {
                    assert_footer_on_bottom_edge(&px, size, palette, theme);
                }
                if let Some(dir) = &dir {
                    write_bmp(
                        &dir.join(format!("{theme}-{name}.bmp")),
                        size.width,
                        size.height,
                        &px,
                    );
                }
            }
        }
    }

    /// At 125% the window must be 475 x 1.25 tall and the footer must sit on its bottom edge. A
    /// window taller than its content would show the window colour under the footer.
    fn assert_footer_on_bottom_edge(
        px: &[PremultipliedRgbaColor],
        size: slint::PhysicalSize,
        palette: &Palette,
        theme: &str,
    ) {
        assert_eq!(
            size.height,
            (model::BASE_HEIGHT * 1.25) as u32,
            "{theme}: window height is not 475 x 1.25"
        );
        // Five rows above the bottom edge are inside the 42 px footer.
        let p = px[((size.height - 5) * size.width + 10) as usize];
        assert_eq!(
            (p.red, p.green, p.blue),
            (palette.raised.r, palette.raised.g, palette.raised.b),
            "{theme}: no footer at the bottom edge (empty strip under it)"
        );
    }
}
