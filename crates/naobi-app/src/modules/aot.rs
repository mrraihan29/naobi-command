//! Always on top (NCMD-22): "Toggle Always on Top" as a Launcher item and the `topmost` hotkey. Both
//! end in the same handler, which acts on the window in front.
use super::Modules;
use naobi_aot::{style, toggle, AotCommands, Config, Outcome, TopmostOps};
use naobi_core::ids::HotkeyId;
use naobi_core::notice::{Notice, NoticeKind};
use naobi_core::settings::Settings;
use naobi_core::theme::ThemeSource;
use naobi_win32::theme::SystemTheme;
use naobi_win32::topmost::Ops;
use std::sync::{Mutex, OnceLock};

fn config() -> &'static Config {
    static CONFIG: OnceLock<Config> = OnceLock::new();
    CONFIG.get_or_init(Config::default)
}

/// `aot.border` and `aot.border_color` of the live settings.
static LOOK: Mutex<(bool, String)> = Mutex::new((true, String::new()));

/// The settings load and every hot reload call this: the chord for the item, and the border look.
pub fn set_config(settings: &Settings) {
    let chord = settings
        .hotkeys
        .get(naobi_aot::commands::HOTKEY)
        .map(ToString::to_string)
        .unwrap_or_default();
    config().set(chord);
    *LOOK.lock().unwrap_or_else(|e| e.into_inner()) =
        (settings.aot.border, settings.aot.border_color.clone());
    restyle();
}

/// Applies the border look under the Windows theme as it is now: called after a settings change and
/// when Windows switches between light and dark.
pub fn restyle() {
    let (show, setting) = LOOK.lock().unwrap_or_else(|e| e.into_inner()).clone();
    Ops.set_style(style(show, &setting, SystemTheme.current()));
}

pub fn register(m: &mut Modules) {
    m.register_source(Box::new(AotCommands::new(config().clone())));
    m.subscribe(
        HotkeyId(naobi_aot::commands::HOTKEY.to_owned()),
        Box::new(move |_, cx| {
            let outcome = toggle(&mut Ops);
            // No title in the log (T-010).
            tracing::info!(
                command = "aot.toggle",
                result = outcome_kind(&outcome),
                "window command"
            );
            if let Some(n) = notice(&outcome) {
                cx.notice(n);
            }
        }),
    );
}

fn outcome_kind(o: &Outcome) -> &'static str {
    match o {
        Outcome::On => "on",
        Outcome::Off => "off",
        Outcome::NoWindow => "no window",
        Outcome::Elevated(_) => "elevated",
        Outcome::Refused(_) => "refused",
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
    let (title, message) = match outcome {
        // R-011: the same wording as the snap commands.
        Outcome::Elevated(t) => (t, "it runs as administrator"),
        Outcome::Refused(t) => (t, "Windows refused the change"),
        // The border shows the state; a key press with no window in front needs no banner.
        Outcome::On | Outcome::Off | Outcome::NoWindow => return None,
    };
    Some(Notice {
        subject: format!("Cannot make {} topmost", named(title)),
        message: message.to_owned(),
        kind: NoticeKind::Warning,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // @R-011
    #[test]
    fn r011_elevated_notice_names_the_window_and_the_reason() {
        let n = notice(&Outcome::Elevated("Task Manager".into())).expect("a notice");
        assert_eq!(
            format!("{}: {}", n.subject, n.message),
            "Cannot make Task Manager topmost: it runs as administrator"
        );
        assert!(notice(&Outcome::On).is_none() && notice(&Outcome::Off).is_none());
    }
}
