//! Notice state (D-018). A Notice stays active until its banner is dismissed with Esc. It reaches the
//! Owner three ways: a tray balloon when it is raised, the banner row on the next Launcher show (tray
//! Open included), and the tray tooltip and icon overlay for as long as it is active.
use std::collections::VecDeque;

use naobi_core::chord::{KeyChord, ALT, CTRL, SHIFT, WIN};
use naobi_core::notice::{Notice, NoticeKind};

use crate::logging;

/// The active Notices, oldest first. The oldest is the banner.
#[derive(Default)]
pub struct NoticeCenter {
    active: VecDeque<Notice>,
}

impl NoticeCenter {
    pub const fn new() -> Self {
        Self {
            active: VecDeque::new(),
        }
    }

    /// Adds `notice` behind the active ones. Returns false, and changes nothing, when an equal Notice is
    /// already active: a settings event re-reads the same file, and it must not stack the same banner.
    pub fn raise(&mut self, notice: Notice) -> bool {
        if self.active.contains(&notice) {
            return false;
        }
        logging::notice(notice.kind);
        self.active.push_back(notice);
        true
    }

    /// The banner row: the oldest active Notice.
    pub fn banner(&self) -> Option<&Notice> {
        self.active.front()
    }

    /// Esc on the banner dismisses it. False when there is no banner, so Esc hides the Launcher instead.
    pub fn dismiss(&mut self) -> bool {
        self.active.pop_front().is_some()
    }
}

/// The Notice for a hotkey another process owns. It names the chord, when the settings know it.
pub fn hotkey_conflict(name: &str, chord: Option<KeyChord>) -> Notice {
    let key = chord.map_or_else(|| name.to_string(), chord_text);
    Notice {
        subject: name.into(),
        message: format!("{key} is taken by another app, so the {name} hotkey is off"),
        kind: NoticeKind::Warning,
    }
}

/// A chord the way the settings file writes it, such as "Alt+Space" or "Ctrl+Alt+Left".
pub fn chord_text(chord: KeyChord) -> String {
    let mut parts: Vec<String> = Vec::new();
    for (bit, name) in [(WIN, "Win"), (CTRL, "Ctrl"), (ALT, "Alt"), (SHIFT, "Shift")] {
        if chord.mods & bit != 0 {
            parts.push(name.into());
        }
    }
    parts.push(key_name(chord.vk));
    parts.join("+")
}

/// The key part of a chord: the names `KeyChord::parse` accepts, for the keys it knows.
fn key_name(vk: u32) -> String {
    match vk {
        0x08 => "Backspace".into(),
        0x09 => "Tab".into(),
        0x0D => "Enter".into(),
        0x1B => "Esc".into(),
        0x20 => "Space".into(),
        0x25 => "Left".into(),
        0x26 => "Up".into(),
        0x27 => "Right".into(),
        0x28 => "Down".into(),
        0x30..=0x39 | 0x41..=0x5A => char::from(vk as u8).to_string(),
        0x70..=0x87 => format!("F{}", vk - 0x6F),
        other => format!("key {other:#x}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn notice(message: &str) -> Notice {
        Notice {
            subject: "launcher".into(),
            message: message.into(),
            kind: NoticeKind::Warning,
        }
    }

    // @R-008
    #[test]
    fn r008_hotkey_conflict_notice_names_alt_space() {
        let alt_space = KeyChord {
            mods: ALT,
            vk: 0x20,
        };
        let n = hotkey_conflict("launcher", Some(alt_space));
        assert!(n.message.starts_with("Alt+Space is taken"), "{}", n.message);
        assert_eq!(n.subject, "launcher");
        assert_eq!(n.kind, NoticeKind::Warning);
    }

    #[test]
    fn chords_read_back_as_the_settings_file_writes_them() {
        for text in [
            "Alt+Space",
            "Ctrl+Alt+Left",
            "Win+Shift+C",
            "Ctrl+Alt+Backspace",
            "Win+Ctrl+T",
        ] {
            let chord = KeyChord::parse(text).expect("parses");
            assert_eq!(chord_text(chord), text);
        }
    }

    #[test]
    fn d018_notice_sticky_until_dismissed() {
        let mut center = NoticeCenter::default();
        assert!(center.banner().is_none());
        assert!(
            !center.dismiss(),
            "Esc with no banner hides the Launcher instead"
        );
        assert!(center.raise(notice("first")));
        assert!(center.raise(notice("second")));
        // Showing the Launcher again does not clear it; only Esc does.
        for _ in 0..3 {
            assert_eq!(center.banner().map(|n| n.message.as_str()), Some("first"));
        }
        assert!(center.dismiss());
        assert_eq!(center.banner().map(|n| n.message.as_str()), Some("second"));
        assert!(center.dismiss());
        assert!(center.banner().is_none());
    }

    #[test]
    fn a_duplicate_raise_is_ignored_and_one_esc_clears_the_banner() {
        let mut center = NoticeCenter::default();
        assert!(center.raise(notice("same")));
        assert!(
            !center.raise(notice("same")),
            "an equal Notice is not raised twice"
        );
        assert!(!center.raise(notice("same")));
        assert!(center.dismiss());
        assert!(center.banner().is_none(), "one Esc clears the only banner");
        assert!(!center.dismiss());
    }

    #[test]
    fn distinct_notices_both_stay_active() {
        let mut center = NoticeCenter::default();
        assert!(center.raise(notice("unknown key")));
        assert!(center.raise(notice("bad line")));
        assert!(center.dismiss());
        assert_eq!(
            center.banner().map(|n| n.message.as_str()),
            Some("bad line"),
            "the second one shows once the first is dismissed"
        );
    }

    #[test]
    fn dismissing_a_notice_lets_it_be_raised_again() {
        let mut center = NoticeCenter::default();
        assert!(center.raise(notice("again")));
        assert!(center.dismiss());
        assert!(
            center.raise(notice("again")),
            "a dismissed Notice can come back"
        );
        assert_eq!(center.banner().map(|n| n.message.as_str()), Some("again"));
    }

    /// D-018 with D-009: every settings event re-reads the same invalid file. The same Notice comes back
    /// each time, so only the first one raises (and balloons); one Esc clears the single banner.
    #[test]
    fn repeated_settings_events_for_one_invalid_file_raise_one_notice() {
        let dir = std::env::temp_dir().join(format!("naobi-notices-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let (path, applied, _) = crate::settings_runtime::startup(&dir).expect("first run");
        std::fs::write(&path, "[general]\nautostart = true\n[ui\n").expect("save");
        let mut center = NoticeCenter::default();
        let mut balloons = 0;
        for _ in 0..4 {
            let (_, notices) = crate::settings_runtime::reload(&path, &applied);
            for n in notices {
                if center.raise(n) {
                    balloons += 1;
                }
            }
        }
        assert_eq!(balloons, 1, "one balloon for one invalid file");
        assert!(center.banner().is_some());
        assert!(center.dismiss());
        assert!(
            center.banner().is_none(),
            "one Esc clears it: exactly one banner"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn same_message_with_another_kind_is_a_different_notice() {
        let mut center = NoticeCenter::default();
        assert!(center.raise(notice("x")));
        let error = Notice {
            kind: NoticeKind::Error,
            ..notice("x")
        };
        assert!(center.raise(error));
    }
}
