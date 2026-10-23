//! Settings model and strict TOML parser (D-009, D-015, D-022, T-004, T-022).
//! Never logs file content.
use std::collections::BTreeMap;

use serde::Deserialize;

use crate::chord::{KeyChord, ALT, CTRL, SHIFT, WIN};
use crate::error::SettingsError;
use crate::quicklink::Quicklink;

pub const MAX_FILE_BYTES: usize = 256 * 1024;
/// D-015: the file may lower retention, never raise it.
pub const MAX_RETENTION_DAYS: u32 = 30;
pub const MAX_ENTRIES: u32 = 1000;
const TEXT_SCALES: [f32; 3] = [1.0, 1.125, 1.25];

/// One line of the parse report. `line` is 1-based when known.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    pub line: Option<usize>,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Parsed {
    pub settings: Settings,
    pub reports: Vec<Report>,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct General {
    pub autostart: bool,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Ui {
    pub text_scale: f32,
}
impl Default for Ui {
    fn default() -> Self {
        Self { text_scale: 1.0 }
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Window {
    pub gap: u32,
}
impl Default for Window {
    fn default() -> Self {
        Self { gap: 8 }
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Clipboard {
    pub retention_days: u32,
    pub max_entries: u32,
    pub ignored_apps: Vec<String>,
}
impl Default for Clipboard {
    fn default() -> Self {
        Self {
            retention_days: MAX_RETENTION_DAYS,
            max_entries: MAX_ENTRIES,
            ignored_apps: vec!["Proton Pass.exe".into()],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Aot {
    pub border: bool,
    /// "auto" or "#RRGGBB".
    pub border_color: String,
}
impl Default for Aot {
    fn default() -> Self {
        Self {
            border: true,
            border_color: "auto".into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    pub general: General,
    /// Keyed by hotkey id ("launcher", "snap_left", ...).
    pub hotkeys: BTreeMap<String, KeyChord>,
    pub ui: Ui,
    pub window: Window,
    pub clipboard: Clipboard,
    pub aot: Aot,
    pub quicklinks: Vec<Quicklink>,
}

#[derive(Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
struct Raw {
    general: General,
    hotkeys: BTreeMap<String, KeyChord>,
    ui: Ui,
    window: Window,
    clipboard: Clipboard,
    aot: Aot,
    quicklinks: Option<Vec<Quicklink>>,
}

const DEFAULT_HOTKEYS: [(&str, u8, u32); 11] = [
    ("launcher", ALT, 0x20),
    ("clipboard", ALT | SHIFT, 0x56),
    ("color_picker", WIN | SHIFT, 0x43),
    ("topmost", WIN | CTRL, 0x54),
    ("snap_left", CTRL | ALT, 0x25),
    ("snap_right", CTRL | ALT, 0x27),
    ("snap_up", CTRL | ALT, 0x26),
    ("snap_down", CTRL | ALT, 0x28),
    ("maximize", CTRL | ALT, 0x0D),
    ("center", CTRL | ALT, 0x43),
    ("restore", CTRL | ALT, 0x08),
];

/// D-024: Google, GitHub, YouTube, naobi.dev.
const DEFAULT_QUICKLINKS: [(&str, &str, &str); 4] = [
    ("Google", "g", "https://www.google.com/search?q={query}"),
    ("GitHub", "gh", "https://github.com/search?q={query}"),
    (
        "YouTube",
        "yt",
        "https://www.youtube.com/results?search_query={query}",
    ),
    ("naobi.dev", "naobi", "https://naobi.dev/search?q={query}"),
];

impl Default for Settings {
    fn default() -> Self {
        Self {
            general: General::default(),
            hotkeys: DEFAULT_HOTKEYS
                .iter()
                .map(|&(k, mods, vk)| (k.to_string(), KeyChord { mods, vk }))
                .collect(),
            ui: Ui::default(),
            window: Window::default(),
            clipboard: Clipboard::default(),
            aot: Aot::default(),
            quicklinks: DEFAULT_QUICKLINKS
                .iter()
                .map(|&(name, alias, url)| Quicklink {
                    name: name.into(),
                    alias: alias.into(),
                    url: url.into(),
                })
                .collect(),
        }
    }
}

impl Settings {
    /// Commented default file written on first run.
    pub fn default_toml() -> &'static str {
        DEFAULT_TOML
    }

    /// Strict parse. Syntax errors, unknown keys and bad chords fail the whole file;
    /// out-of-range values and bad quicklinks are fixed up and listed in `reports`.
    pub fn parse(text: &str) -> Result<Parsed, SettingsError> {
        if text.len() > MAX_FILE_BYTES {
            return Err(SettingsError::TooLarge);
        }
        let raw: Raw = toml::from_str(text).map_err(|e| SettingsError::Syntax {
            line: e.span().and_then(|s| {
                let head = text.as_bytes().get(..s.start)?;
                Some(head.iter().filter(|&&b| b == b'\n').count() + 1)
            }),
            message: e.message().to_string(),
        })?;
        // Hotkey ids must be known, and a global hotkey needs a modifier.
        for (k, c) in &raw.hotkeys {
            let known = DEFAULT_HOTKEYS.iter().any(|d| d.0 == k);
            if !known || c.mods == 0 {
                let why = if known {
                    "needs a modifier"
                } else {
                    "is unknown"
                };
                return Err(SettingsError::Syntax {
                    line: None,
                    message: format!("hotkey `{k}` {why}"),
                });
            }
        }

        let mut reports = Vec::new();
        let mut note = |message: String| {
            reports.push(Report {
                line: None,
                message,
            })
        };
        let mut hotkeys = Settings::default().hotkeys;
        hotkeys.extend(raw.hotkeys);
        let mut s = Settings {
            general: raw.general,
            hotkeys,
            ui: raw.ui,
            window: raw.window,
            clipboard: raw.clipboard,
            aot: raw.aot,
            ..Settings::default()
        };
        if !TEXT_SCALES.contains(&s.ui.text_scale) {
            note("ui.text_scale must be 1.0, 1.125 or 1.25; using 1.0".into());
            s.ui = Ui::default();
        }
        if s.aot.border_color != "auto" && !is_hex_color(&s.aot.border_color) {
            note("aot.border_color must be \"auto\" or #RRGGBB; using auto".into());
            s.aot.border_color = "auto".into();
        }

        if s.clipboard.retention_days > MAX_RETENTION_DAYS {
            note(format!(
                "clipboard.retention_days cannot be raised above {MAX_RETENTION_DAYS}; using {MAX_RETENTION_DAYS}"
            ));
            s.clipboard.retention_days = MAX_RETENTION_DAYS;
        }
        if s.clipboard.max_entries > MAX_ENTRIES {
            note(format!(
                "clipboard.max_entries cannot be raised above {MAX_ENTRIES}; using {MAX_ENTRIES}"
            ));
            s.clipboard.max_entries = MAX_ENTRIES;
        }

        if let Some(list) = raw.quicklinks {
            s.quicklinks.clear();
            for q in list {
                match q.validate() {
                    Ok(()) if s.quicklinks.iter().any(|o| o.alias == q.alias) => {
                        note(format!("quicklink '{}' rejected: duplicate alias", q.alias));
                    }
                    Ok(()) => s.quicklinks.push(q),
                    Err(e) => note(format!("quicklink '{}' rejected: {e}", q.alias)),
                }
            }
        }
        Ok(Parsed {
            settings: s,
            reports,
        })
    }

    /// D-009: a file that fails to parse keeps `prev` and reports why.
    pub fn apply(prev: &Settings, text: &str) -> Parsed {
        Self::parse(text).unwrap_or_else(|e| Parsed {
            settings: prev.clone(),
            reports: vec![Report {
                line: match &e {
                    SettingsError::Syntax { line, .. } => *line,
                    SettingsError::TooLarge => None,
                },
                message: e.to_string(),
            }],
        })
    }
}

fn is_hex_color(s: &str) -> bool {
    s.len() == 7 && s.starts_with('#') && s[1..].bytes().all(|b| b.is_ascii_hexdigit())
}

const DEFAULT_TOML: &str = r##"# Naobi Command settings. Changes apply when the file is saved.
# An invalid file keeps the last valid settings.

[general]
autostart = false

[hotkeys]
launcher = "Alt+Space"
clipboard = "Alt+Shift+V"
color_picker = "Win+Shift+C"
topmost = "Win+Ctrl+T"
snap_left = "Ctrl+Alt+Left"
snap_right = "Ctrl+Alt+Right"
snap_up = "Ctrl+Alt+Up"
snap_down = "Ctrl+Alt+Down"
maximize = "Ctrl+Alt+Enter"
center = "Ctrl+Alt+C"
restore = "Ctrl+Alt+Backspace"

[ui]
text_scale = 1.0 # 1.0, 1.125 or 1.25

[window]
gap = 8

[clipboard]
retention_days = 30 # may be lowered, never raised above 30
max_entries = 1000  # may be lowered, never raised above 1000
ignored_apps = ["Proton Pass.exe"]

[aot]
border = true
border_color = "auto" # or "#RRGGBB"

# Defining any [[quicklinks]] replaces the four below. {query} is required and must not be in the host.
[[quicklinks]]
name = "Google"
alias = "g"
url = "https://www.google.com/search?q={query}"

[[quicklinks]]
name = "GitHub"
alias = "gh"
url = "https://github.com/search?q={query}"

[[quicklinks]]
name = "YouTube"
alias = "yt"
url = "https://www.youtube.com/results?search_query={query}"

[[quicklinks]]
name = "naobi.dev"
alias = "naobi"
url = "https://naobi.dev/search?q={query}"
"##;
