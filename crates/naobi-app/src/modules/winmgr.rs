//! Window management (NCMD-21): the snap commands as Launcher items, and each command's hotkey.
//! Both routes end in the same handler, which acts on the window in front.
use super::Modules;
use naobi_core::ids::HotkeyId;
use naobi_core::notice::{Notice, NoticeKind};
use naobi_core::settings::Settings;
use naobi_win32::window_ops::Ops;
use naobi_winmgr::plan::ALL;
use naobi_winmgr::{Config, Outcome, SnapCommands, Snapper};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;
use std::sync::OnceLock;

/// The gap and the chords of the live settings: the settings load and every hot reload `set` them.
fn config() -> &'static Config {
    static CONFIG: OnceLock<Config> = OnceLock::new();
    CONFIG.get_or_init(Config::default)
}

pub fn set_config(settings: &Settings) {
    let chords: BTreeMap<String, String> = settings
        .hotkeys
        .iter()
        .map(|(id, chord)| (id.clone(), chord.to_string()))
        .collect();
    config().set(settings.window.gap, chords);
}

pub fn register(m: &mut Modules) {
    m.register_source(Box::new(SnapCommands::new(config().clone())));
    let snapper = Rc::new(RefCell::new(Snapper::default()));
    for cmd in ALL {
        let snapper = snapper.clone();
        m.subscribe(
            HotkeyId(cmd.id().to_owned()),
            Box::new(move |_, cx| {
                let outcome = snapper.borrow_mut().run(cmd, &mut Ops, config().gap());
                // The command id only: never the window's title (T-010).
                tracing::info!(
                    command = cmd.id(),
                    moved = outcome == Outcome::Moved,
                    "window command"
                );
                if let Some(n) = notice(&outcome) {
                    cx.notice(n);
                }
            }),
        );
    }
}

fn named(title: &str) -> &str {
    if title.is_empty() {
        "this window"
    } else {
        title
    }
}

fn notice(outcome: &Outcome) -> Option<Notice> {
    let (subject, message, kind) = match outcome {
        // R-011: the Notice reads "Cannot move <window name>: it runs as administrator".
        Outcome::Elevated(title) => (
            format!("Cannot move {}", named(title)),
            "it runs as administrator",
            NoticeKind::Warning,
        ),
        Outcome::Refused(title) => (
            format!("Cannot move {}", named(title)),
            "Windows refused the move",
            NoticeKind::Warning,
        ),
        Outcome::NothingToRestore(title) => (
            "Window".to_owned(),
            if title.is_empty() {
                "nothing to restore"
            } else {
                "it was not moved by a command"
            },
            NoticeKind::Info,
        ),
        Outcome::OneDisplay => (
            "Window".to_owned(),
            "there is only one display",
            NoticeKind::Info,
        ),
        // Nothing in front to move: a key press with no effect needs no banner.
        Outcome::NoWindow | Outcome::Moved => return None,
    };
    Some(Notice {
        subject,
        message: message.to_owned(),
        kind,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // @R-011
    #[test]
    fn r011_elevated_notice_names_the_window_and_the_reason() {
        let n = notice(&Outcome::Elevated("Task Manager".into()));
        let n = n.expect("a notice");
        assert_eq!(
            format!("{}: {}", n.subject, n.message),
            "Cannot move Task Manager: it runs as administrator"
        );
        assert_eq!(n.kind, NoticeKind::Warning);
    }

    // @R-016
    #[test]
    fn r016_a_move_and_a_missing_window_raise_no_notice() {
        assert!(notice(&Outcome::Moved).is_none());
        assert!(notice(&Outcome::NoWindow).is_none());
    }
}
