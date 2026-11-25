//! Window switcher module wiring (`naobi-launcher::providers::windows` + `naobi-win32`).
use super::Modules;
use naobi_core::ids::{HotkeyId, ViewId};
use naobi_launcher::providers::windows::{
    OpenWindow, WindowActionError, WindowCommands, WindowOps, WindowSource, WindowsView, VIEW_ID,
};
use naobi_win32::window_switcher::{self, ActionError};
use std::sync::Arc;

pub const HOTKEY: &str = "window_switcher";
pub const CYCLE_HOTKEY: &str = "cycle_app_windows";

struct WinWindowOps;

fn map_err(action: &'static str, err: ActionError) -> WindowActionError {
    // Content-free log (D-020): action and error kind only, never window title or process name.
    tracing::info!(action, error = ?err, "window switcher action refused");
    match err {
        ActionError::Elevated => WindowActionError::Elevated,
        ActionError::Refused => WindowActionError::Refused,
    }
}

impl WindowOps for WinWindowOps {
    fn list(&self) -> Vec<OpenWindow> {
        window_switcher::list()
            .into_iter()
            .map(|w| OpenWindow {
                hwnd: w.hwnd,
                title: w.title,
                process_name: w.process_name,
            })
            .collect()
    }

    fn focus(&self, hwnd: isize) -> Result<(), WindowActionError> {
        window_switcher::focus(hwnd).map_err(|e| map_err("focus", e))
    }

    fn minimize(&self, hwnd: isize) -> Result<(), WindowActionError> {
        window_switcher::minimize(hwnd).map_err(|e| map_err("minimize", e))
    }

    fn close(&self, hwnd: isize) -> Result<(), WindowActionError> {
        window_switcher::close(hwnd).map_err(|e| map_err("close", e))
    }

    fn force_quit(&self, hwnd: isize) -> Result<(), WindowActionError> {
        window_switcher::force_quit(hwnd).map_err(|e| map_err("force_quit", e))
    }
}

pub fn register(m: &mut Modules) {
    let ops: Arc<dyn WindowOps> = Arc::new(WinWindowOps);
    m.register_source(Box::new(WindowCommands));
    m.register_source(Box::new(WindowSource::new(ops.clone())));
    m.register_view(Box::new(WindowsView::new(ops)));
    m.subscribe(
        HotkeyId(HOTKEY.into()),
        Box::new(|_, cx| cx.open_view(ViewId(VIEW_ID.into()))),
    );
    m.subscribe(
        HotkeyId(CYCLE_HOTKEY.into()),
        Box::new(|_, cx| {
            let fg = naobi_win32::paste::foreground_window();
            if let Err(ActionError::Elevated) = window_switcher::cycle_same_app(fg) {
                cx.notice(naobi_core::notice::Notice {
                    subject: "Window Switcher".into(),
                    message: "That window runs as administrator.".into(),
                    kind: naobi_core::notice::NoticeKind::Error,
                });
            }
        }),
    );
}
