//! Settings model and strict TOML parser (D-009, D-015, D-022, T-004, T-022).
//! Never logs file content.
use std::collections::BTreeMap;

use serde::Deserialize;

use crate::chord::{KeyChord, ALT, CTRL, SHIFT, WIN};
use crate::error::SettingsError;
use crate::quicklink::Quicklink;
use crate::theme_file;

pub const MAX_FILE_BYTES: usize = 256 * 1024;
/// D-015: the file may lower retention, never raise it.
pub const MAX_RETENTION_DAYS: u32 = 30;
pub const MAX_ENTRIES: u32 = 1000;
pub const DEFAULT_REM_BASE_PX: f64 = 16.0;
pub const DEFAULT_FILE_IGNORE: [&str; 7] = [
    ".git",
    "node_modules",
    "target",
    ".cargo",
    ".venv",
    "dist",
    "build",
];
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
            ignored_apps: vec!["ProtonPass.exe".into()],
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

/// The Theme of each Windows app mode (R-032).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ThemeSlots {
    pub light: String,
    pub dark: String,
}
impl Default for ThemeSlots {
    fn default() -> Self {
        Self {
            light: "paper".into(),
            dark: "stone".into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Calculator {
    pub rem_base_px: f64,
}
impl Default for Calculator {
    fn default() -> Self {
        Self {
            rem_base_px: DEFAULT_REM_BASE_PX,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Currency {
    pub default_currency: String,
    pub number_format: String,
    pub auto_refresh: bool,
}
impl Default for Currency {
    fn default() -> Self {
        Self {
            default_currency: "IDR".into(),
            number_format: "us".into(),
            auto_refresh: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Files {
    pub roots: Vec<String>,
    pub ignore: Vec<String>,
}
impl Default for Files {
    fn default() -> Self {
        Self {
            roots: Vec::new(),
            ignore: DEFAULT_FILE_IGNORE.iter().map(|&s| s.to_owned()).collect(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    pub general: General,
    /// Keyed by hotkey id ("launcher", "topmost", ...).
    pub hotkeys: BTreeMap<String, KeyChord>,
    pub ui: Ui,
    pub theme: ThemeSlots,
    pub clipboard: Clipboard,
    pub aot: Aot,
    pub calculator: Calculator,
    pub currency: Currency,
    pub files: Files,
    pub quicklinks: Vec<Quicklink>,
    /// Custom item aliases mapping alias string -> `ItemId`.
    pub aliases: BTreeMap<String, String>,
}

#[derive(Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
struct Raw {
    general: General,
    hotkeys: BTreeMap<String, KeyChord>,
    ui: Ui,
    theme: ThemeSlots,
    /// Window snapping was removed; an old file may still hold the section, which is ignored.
    #[allow(dead_code)] // naobi-allow: read only to accept a retired settings section
    window: Option<toml::Table>,
    clipboard: Clipboard,
    aot: Aot,
    calculator: Calculator,
    currency: Currency,
    files: Files,
    quicklinks: Option<Vec<Quicklink>>,
    aliases: BTreeMap<String, String>,
}

/// Hotkeys of the removed window snapping; an old file may still bind them, and they are ignored.
const RETIRED_HOTKEYS: [&str; 7] = [
    "snap_left",
    "snap_right",
    "snap_up",
    "snap_down",
    "maximize",
    "center",
    "restore",
];

const DEFAULT_HOTKEYS: [(&str, u8, u32); 4] = [
    ("launcher", CTRL, 0x20),
    ("clipboard", ALT | SHIFT, 0x56),
    ("color_picker", WIN | SHIFT, 0x43),
    ("topmost", WIN | CTRL, 0x54),
];

/// Valid `[hotkeys]` slots that have no default global binding.
pub const OPTIONAL_HOTKEYS: [&str; 4] = ["window_switcher", "emoji", "snippets", "scratchpad"];

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
            theme: ThemeSlots::default(),
            clipboard: Clipboard::default(),
            aot: Aot::default(),
            calculator: Calculator::default(),
            currency: Currency::default(),
            files: Files::default(),
            quicklinks: DEFAULT_QUICKLINKS
                .iter()
                .map(|&(name, alias, url)| Quicklink {
                    name: name.into(),
                    alias: alias.into(),
                    url: url.into(),
                })
                .collect(),
            aliases: BTreeMap::new(),
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
        let mut raw: Raw = toml::from_str(text).map_err(|e| SettingsError::Syntax {
            line: e.span().and_then(|s| {
                let head = text.as_bytes().get(..s.start)?;
                Some(head.iter().filter(|&&b| b == b'\n').count() + 1)
            }),
            message: e.message().to_string(),
        })?;
        raw.hotkeys
            .retain(|k, _| !RETIRED_HOTKEYS.contains(&k.as_str()));
        // Hotkey ids must be known, and a global hotkey needs a modifier.
        for (k, c) in &raw.hotkeys {
            let known =
                DEFAULT_HOTKEYS.iter().any(|d| d.0 == k) || OPTIONAL_HOTKEYS.contains(&k.as_str());
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
            theme: raw.theme,
            clipboard: raw.clipboard,
            aot: raw.aot,
            calculator: raw.calculator,
            currency: raw.currency,
            files: raw.files,
            ..Settings::default()
        };
        if !TEXT_SCALES.contains(&s.ui.text_scale) {
            note("ui.text_scale must be 1.0, 1.125 or 1.25; using 1.0".into());
            s.ui = Ui::default();
        }
        let defaults = ThemeSlots::default();
        if !theme_file::is_theme_name(&s.theme.light) {
            note(format!(
                "theme.light is not a theme name; using {}",
                defaults.light
            ));
            s.theme.light = defaults.light;
        }
        if !theme_file::is_theme_name(&s.theme.dark) {
            note(format!(
                "theme.dark is not a theme name; using {}",
                defaults.dark
            ));
            s.theme.dark = defaults.dark;
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
        if !s.calculator.rem_base_px.is_finite() || s.calculator.rem_base_px <= 0.0 {
            note("calculator.rem_base_px must be a positive number; using 16.0".into());
            s.calculator = Calculator::default();
        }
        let currency_code = s.currency.default_currency.trim().to_ascii_uppercase();
        if currency_code.len() == 3 && currency_code.bytes().all(|b| b.is_ascii_alphabetic()) {
            s.currency.default_currency = currency_code;
        } else {
            note("currency.default_currency must be a 3-letter currency code; using IDR".into());
            s.currency.default_currency = Currency::default().default_currency;
        }
        let number_format = s.currency.number_format.trim().to_ascii_lowercase();
        if matches!(number_format.as_str(), "us" | "id") {
            s.currency.number_format = number_format;
        } else {
            note("currency.number_format must be \"us\" or \"id\"; using us".into());
            s.currency.number_format = Currency::default().number_format;
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
        let mut seen_aliases: Vec<String> = s
            .quicklinks
            .iter()
            .map(|q| q.alias.to_ascii_lowercase())
            .collect();
        for (alias, target) in raw.aliases {
            let alias_trimmed = alias.trim();
            let target_trimmed = target.trim();
            if alias_trimmed.is_empty() || alias_trimmed.chars().any(char::is_whitespace) {
                note(format!("alias '{alias}' rejected: cannot be empty or contain whitespace"));
                continue;
            }
            if target_trimmed.is_empty() {
                note(format!("alias '{alias_trimmed}' rejected: target item id is empty"));
                continue;
            }
            let lower = alias_trimmed.to_ascii_lowercase();
            if seen_aliases.contains(&lower) {
                note(format!("alias '{alias_trimmed}' rejected: duplicate alias"));
                continue;
            }
            seen_aliases.push(lower);
            s.aliases
                .insert(alias_trimmed.to_owned(), target_trimmed.to_owned());
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

/// `text` with Theme slot `slot` ("light" or "dark") set to `id`. Only that one line changes, so the
/// Owner's comments and order stay; a file that does not parse is refused (T-040).
pub fn with_theme_slot(text: &str, slot: &str, id: &str) -> Result<String, SettingsError> {
    let refused = |message: String| SettingsError::Syntax {
        line: None,
        message,
    };
    if !theme_file::is_theme_name(id) || !matches!(slot, "light" | "dark") {
        return Err(refused("that is not a theme name".to_owned()));
    }
    Settings::parse(text)?;
    let newline = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let assignment = format!("{slot} = \"{id}\"");
    let mut lines: Vec<String> = text.lines().map(str::to_owned).collect();
    let bare = |line: &str| line.split('#').next().unwrap_or("").trim().to_owned();
    let header = lines.iter().position(|l| bare(l) == "[theme]");
    match header {
        Some(start) => {
            let end = lines[start + 1..]
                .iter()
                .position(|l| bare(l).starts_with('['))
                .map_or(lines.len(), |i| start + 1 + i);
            let found = (start + 1..end).find(|&i| {
                bare(&lines[i])
                    .split('=')
                    .next()
                    .is_some_and(|k| k.trim() == slot)
            });
            match found {
                Some(i) => {
                    let comment = lines[i].find('#').map(|at| lines[i][at..].to_owned());
                    lines[i] = match comment {
                        Some(c) => format!("{assignment} {c}"),
                        None => assignment,
                    };
                }
                None => lines.insert(start + 1, assignment),
            }
        }
        None => {
            lines.push(String::new());
            lines.push("[theme]".to_owned());
            lines.push(assignment);
        }
    }
    let mut out = lines.join(newline);
    out.push_str(newline);
    let parsed = Settings::parse(&out)?;
    let written = if slot == "light" {
        &parsed.settings.theme.light
    } else {
        &parsed.settings.theme.dark
    };
    if written == id {
        Ok(out)
    } else {
        Err(refused("the theme slot could not be written".to_owned()))
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
launcher = "Ctrl+Space"
clipboard = "Alt+Shift+V"
color_picker = "Win+Shift+C"
topmost = "Win+Ctrl+T"
# window_switcher = "Alt+Shift+W" # optional, off by default
# emoji = "Alt+Shift+E" # optional, off by default
# snippets = "Alt+Shift+S" # optional, off by default
# scratchpad = "Alt+Shift+N"      # optional, off by default

[ui]
text_scale = 1.0 # 1.0, 1.125 or 1.25

[theme]
light = "paper" # used when Windows is in light mode
dark = "stone"  # used when Windows is in dark mode

[clipboard]
retention_days = 30 # may be lowered, never raised above 30
max_entries = 1000  # may be lowered, never raised above 1000
ignored_apps = ["ProtonPass.exe"]

[aot]
border = true
border_color = "auto" # or "#RRGGBB"

[calculator]
rem_base_px = 16.0 # root font size in pixels for px <-> rem conversions

[currency]
default_currency = "IDR" # target currency when none is typed (e.g. "53 usd")
number_format = "us"     # "us" (1,234.56) or "id" (1.234,56)
auto_refresh = true      # refresh cached rates at most once every 24h

[files]
roots = [] # empty uses %USERPROFILE% folders (Desktop, Documents, Downloads, Pictures, Videos, plus project folders)
ignore = [".git", "node_modules", "target", ".cargo", ".venv", "dist", "build"]

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

# Custom item aliases: run "Copy item ID" from the Action panel (Ctrl+K) and map an alias to it.
# [aliases]
# lock = "sys.lock"
"##;
