//! Theme files (ADR-0007, D-043): a Custom theme is plain data over a read-only Built-in theme. This
//! module parses and resolves themes and touches no file, so every rule is tested with strings.
use crate::settings::{Report, ThemeSlots};
use crate::tokens::{
    contrast_failures, Palette, Rgba, DEEP_SEA, DUNE, KYOTO_DUSK, MATCHA, MIDNIGHT_INK, NEON_NIGHT,
    PAPER, SAKURA_DAWN, STONE, TOKYO_HAZE, WASHI,
};
use crate::ui::Theme;

/// Larger theme files are refused before they are parsed (T-038).
pub const MAX_THEME_BYTES: usize = 64 * 1024;
/// At most this many theme files are read, in name order (T-038).
pub const MAX_THEMES: usize = 100;
const MAX_ID_CHARS: usize = 40;
const MAX_NAME_CHARS: usize = 40;
const MAX_FONT_CHARS: usize = 64;

/// Corner radii in logical px.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Shape {
    pub window: u8,
    pub row: u8,
    pub chip: u8,
    pub control: u8,
}

impl Default for Shape {
    fn default() -> Self {
        Self {
            window: 8,
            row: 2,
            chip: 2,
            control: 4,
        }
    }
}

/// The result row height of a Theme.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Density {
    Compact,
    #[default]
    Normal,
    Comfortable,
}

impl Density {
    /// Row height at 100% text scale, logical px.
    pub fn row_height(self) -> f32 {
        match self {
            Self::Compact => 36.0,
            Self::Normal => 40.0,
            Self::Comfortable => 44.0,
        }
    }
}

/// Installed font families; None keeps the embedded font.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Fonts {
    pub interface: Option<String>,
    pub mono: Option<String>,
}

/// A Theme ready to draw with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedTheme {
    pub id: String,
    pub name: String,
    pub base: Theme,
    pub builtin: bool,
    pub palette: Palette,
    pub shape: Shape,
    pub density: Density,
    pub fonts: Fonts,
}

fn base_name(theme: Theme) -> &'static str {
    match theme {
        Theme::Paper => "paper",
        Theme::Stone => "stone",
    }
}

type BuiltinEntry = (&'static str, &'static str, Theme, Palette);

/// Built-in themes in Themes-view order: id, name, Windows-mode family, palette (D-046). The family is
/// what Duplicate writes as `base`.
const BUILTINS: &[BuiltinEntry] = &[
    ("paper", "Paper", Theme::Paper, PAPER),
    ("stone", "Stone", Theme::Stone, STONE),
    ("tokyo-haze", "Tokyo Haze", Theme::Stone, TOKYO_HAZE),
    ("neon-night", "Neon Night", Theme::Stone, NEON_NIGHT),
    ("washi", "Washi", Theme::Paper, WASHI),
    ("sakura-dawn", "Sakura Dawn", Theme::Paper, SAKURA_DAWN),
    ("matcha", "Matcha", Theme::Paper, MATCHA),
    ("dune", "Dune", Theme::Paper, DUNE),
    ("midnight-ink", "Midnight Ink", Theme::Stone, MIDNIGHT_INK),
    ("kyoto-dusk", "Kyoto Dusk", Theme::Stone, KYOTO_DUSK),
    ("deep-sea", "Deep Sea", Theme::Stone, DEEP_SEA),
];

fn resolved(&(id, name, base, palette): &BuiltinEntry) -> ResolvedTheme {
    ResolvedTheme {
        id: id.to_owned(),
        name: name.to_owned(),
        base,
        builtin: true,
        palette,
        shape: Shape::default(),
        density: Density::default(),
        fonts: Fonts::default(),
    }
}

/// The Built-in theme of Windows mode `theme`.
pub fn builtin(theme: Theme) -> ResolvedTheme {
    resolved(
        &BUILTINS[match theme {
            Theme::Paper => 0,
            Theme::Stone => 1,
        }],
    )
}

/// Every Built-in theme, in Themes-view order.
pub fn builtins() -> Vec<ResolvedTheme> {
    BUILTINS.iter().map(resolved).collect()
}

fn builtin_named(id: &str) -> Option<ResolvedTheme> {
    BUILTINS.iter().find(|b| b.0 == id).map(resolved)
}

/// The Theme named `id`: a Built-in theme or one in `catalog`.
pub fn find(id: &str, catalog: &Catalog) -> Option<ResolvedTheme> {
    builtin_named(id).or_else(|| catalog.find(id).cloned())
}

/// True for an id a Custom theme file may have. Nothing else ever becomes part of a path (T-037).
pub fn is_custom_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= MAX_ID_CHARS
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        && builtin_named(id).is_none()
}

/// True for a name a Theme slot may hold.
pub fn is_theme_name(id: &str) -> bool {
    builtin_named(id).is_some() || is_custom_id(id)
}

/// The line of `key` in `section` (None is the top of the file), 1-based.
fn line_of(text: &str, section: Option<&str>, key: &str) -> Option<usize> {
    let mut current = None;
    for (i, raw) in text.lines().enumerate() {
        let line = raw.split('#').next().unwrap_or("").trim();
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            current = Some(name.trim());
        } else if current == section
            && line
                .split('=')
                .next()
                .is_some_and(|k| k.trim().trim_matches('"') == key)
        {
            return Some(i + 1);
        }
    }
    None
}

fn parse_color(value: &str, alpha_allowed: bool) -> Option<Rgba> {
    let hex = value.strip_prefix('#')?;
    if !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let number = u32::from_str_radix(hex, 16).ok()?;
    match hex.len() {
        6 => Some(Rgba::rgb(number)),
        8 if alpha_allowed => Some(Rgba::with_alpha(number >> 8, number as u8)),
        _ => None,
    }
}

/// The palette token behind a `[colors]` key, and whether it may be translucent.
fn token<'a>(p: &'a mut Palette, key: &str) -> Option<(&'a mut Rgba, bool)> {
    Some(match key {
        "window" => (&mut p.window, false),
        "raised" => (&mut p.raised, false),
        "selected" => (&mut p.selected, false),
        "well" => (&mut p.well, false),
        "text" => (&mut p.text_primary, true),
        "secondary" => (&mut p.text_secondary, true),
        "selected_text" => (&mut p.subtitle_selected, true),
        "muted" => (&mut p.text_muted, true),
        "accent" => (&mut p.accent, false),
        "focus" => (&mut p.focus, false),
        "danger" => (&mut p.danger, false),
        "success" => (&mut p.success, false),
        "hair" => (&mut p.hair, false),
        "strong" => (&mut p.strong, false),
        _ => return None,
    })
}

/// Collects the problems of one file, each with its line.
struct Reader<'a> {
    text: &'a str,
    reports: Vec<Report>,
}

impl Reader<'_> {
    fn report(&mut self, section: Option<&str>, key: &str, message: String) {
        self.reports.push(Report {
            line: line_of(self.text, section, key),
            message,
        });
    }

    fn table<'t>(&mut self, root: &'t toml::Table, name: &str) -> Option<&'t toml::Table> {
        let value = root.get(name)?;
        let table = value.as_table();
        if table.is_none() {
            self.reports.push(Report {
                line: line_of(self.text, None, name),
                message: format!("{name} must be a table; it is ignored"),
            });
        }
        table
    }

    fn colors(&mut self, root: &toml::Table, palette: &mut Palette) {
        let Some(colors) = self.table(root, "colors") else {
            return;
        };
        for (key, value) in colors {
            let Some((slot, alpha_allowed)) = token(palette, key) else {
                self.report(Some("colors"), key, format!("unknown color `{key}`"));
                continue;
            };
            match value.as_str().and_then(|v| parse_color(v, alpha_allowed)) {
                Some(color) => *slot = color,
                None => self.report(
                    Some("colors"),
                    key,
                    format!("color `{key}` is not valid; using the base color"),
                ),
            }
        }
    }

    fn radius(&mut self, shape: &toml::Table, key: &str, allowed: &[i64], slot: &mut u8) {
        let Some(value) = shape.get(key) else { return };
        match value.as_integer().filter(|v| allowed.contains(v)) {
            Some(v) => *slot = v as u8,
            None => self.report(
                Some("shape"),
                key,
                format!("shape `{key}` is out of range; using the base value"),
            ),
        }
    }

    fn shape(&mut self, root: &toml::Table, shape: &mut Shape) {
        let Some(table) = self.table(root, "shape") else {
            return;
        };
        let up_to_12: Vec<i64> = (0..=12).collect();
        let up_to_8: Vec<i64> = (0..=8).collect();
        self.radius(table, "window", &[0, 4, 8], &mut shape.window);
        self.radius(table, "row", &up_to_12, &mut shape.row);
        self.radius(table, "chip", &up_to_8, &mut shape.chip);
        self.radius(table, "control", &up_to_12, &mut shape.control);
        for key in table.keys() {
            if !["window", "row", "chip", "control"].contains(&key.as_str()) {
                self.report(Some("shape"), key, format!("unknown shape `{key}`"));
            }
        }
    }

    fn density(&mut self, root: &toml::Table) -> Density {
        let Some(list) = self.table(root, "list") else {
            return Density::default();
        };
        match list.get("density").and_then(toml::Value::as_str) {
            None if !list.contains_key("density") => Density::default(),
            Some("compact") => Density::Compact,
            Some("normal") => Density::Normal,
            Some("comfortable") => Density::Comfortable,
            _ => {
                self.report(
                    Some("list"),
                    "density",
                    "density must be compact, normal or comfortable; using normal".to_owned(),
                );
                Density::default()
            }
        }
    }

    fn font(&mut self, fonts: &toml::Table, key: &str) -> Option<String> {
        let value = fonts.get(key)?;
        let family = value.as_str().map(str::trim).filter(|f| {
            !f.is_empty() && f.chars().count() <= MAX_FONT_CHARS && !f.chars().any(char::is_control)
        });
        if family.is_none() {
            self.report(
                Some("fonts"),
                key,
                format!("font `{key}` is not a font family name; using the embedded font"),
            );
        }
        family.map(str::to_owned)
    }

    fn fonts(&mut self, root: &toml::Table) -> Fonts {
        let Some(table) = self.table(root, "fonts") else {
            return Fonts::default();
        };
        Fonts {
            interface: self.font(table, "interface"),
            mono: self.font(table, "mono"),
        }
    }
}

/// A parsed theme and what was wrong with its values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThemeParse {
    pub theme: ResolvedTheme,
    pub reports: Vec<Report>,
}

/// Reads a theme file. An invalid value takes the base value and is reported (R-033); only a file that
/// is not TOML fails.
pub fn parse(id: &str, text: &str) -> Result<ThemeParse, Report> {
    let root: toml::Table = text.parse().map_err(|e: toml::de::Error| Report {
        line: e.span().and_then(|s| {
            let head = text.as_bytes().get(..s.start)?;
            Some(head.iter().filter(|&&b| b == b'\n').count() + 1)
        }),
        message: e.message().to_owned(),
    })?;
    let mut r = Reader {
        text,
        reports: Vec::new(),
    };
    let base = match root.get("base").map(toml::Value::as_str) {
        None | Some(Some("paper")) => Theme::Paper,
        Some(Some("stone")) => Theme::Stone,
        Some(_) => {
            r.report(
                None,
                "base",
                "base must be paper or stone; using paper".to_owned(),
            );
            Theme::Paper
        }
    };
    let mut theme = builtin(base);
    theme.id = id.to_owned();
    theme.builtin = false;
    theme.name = id.to_owned();
    match root.get("name").map(toml::Value::as_str) {
        None => {}
        Some(Some(name)) if !name.trim().is_empty() && name.chars().count() <= MAX_NAME_CHARS => {
            theme.name = name.trim().to_owned();
        }
        Some(_) => r.report(
            None,
            "name",
            "name is not valid; using the file name".to_owned(),
        ),
    }
    r.colors(&root, &mut theme.palette);
    r.shape(&root, &mut theme.shape);
    theme.density = r.density(&root);
    theme.fonts = r.fonts(&root);
    for key in root.keys() {
        if !["name", "base", "colors", "shape", "list", "fonts"].contains(&key.as_str()) {
            r.report(None, key, format!("unknown setting `{key}`"));
        }
    }
    Ok(ThemeParse {
        theme,
        reports: r.reports,
    })
}

/// The `[colors]` keys with their tokens, in file order.
pub fn color_tokens(p: &Palette) -> [(&'static str, Rgba); 14] {
    [
        ("window", p.window),
        ("raised", p.raised),
        ("selected", p.selected),
        ("well", p.well),
        ("text", p.text_primary),
        ("secondary", p.text_secondary),
        ("selected_text", p.subtitle_selected),
        ("muted", p.text_muted),
        ("accent", p.accent),
        ("focus", p.focus),
        ("danger", p.danger),
        ("success", p.success),
        ("hair", p.hair),
        ("strong", p.strong),
    ]
}

/// A complete, editable theme file for `theme` under the name `name`.
pub fn to_toml(theme: &ResolvedTheme, name: &str) -> String {
    use std::fmt::Write as _;
    let escaped = name.replace('\\', "\\\\").replace('"', "\\\"");
    let mut out = format!(
        "name = \"{escaped}\"\nbase = \"{}\"\n\n[colors]\n",
        base_name(theme.base)
    );
    for (key, c) in color_tokens(&theme.palette) {
        let _ = write!(out, "{key} = \"#{:06X}", c.to_rgb());
        if c.a != 255 {
            let _ = write!(out, "{:02X}", c.a);
        }
        out.push_str("\"\n");
    }
    let s = theme.shape;
    let density = match theme.density {
        Density::Compact => "compact",
        Density::Normal => "normal",
        Density::Comfortable => "comfortable",
    };
    let _ = write!(
        out,
        "\n[shape]\nwindow = {}\nrow = {}\nchip = {}\ncontrol = {}\n\n[list]\ndensity = \"{density}\"\n",
        s.window, s.row, s.chip, s.control
    );
    if theme.fonts.interface.is_some() || theme.fonts.mono.is_some() {
        out.push_str("\n[fonts]\n");
        for (key, family) in [
            ("interface", &theme.fonts.interface),
            ("mono", &theme.fonts.mono),
        ] {
            if let Some(family) = family {
                let family = family.replace('\\', "\\\\").replace('"', "\\\"");
                let _ = writeln!(out, "{key} = \"{family}\"");
            }
        }
    }
    out
}

/// One file of the themes folder, as the caller could read it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileContent {
    Text(String),
    TooLarge,
    Unreadable,
}

/// The Custom themes that could be read and what is wrong with the folder.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Catalog {
    pub themes: Vec<ResolvedTheme>,
    /// Messages for the Owner, each naming its file.
    pub problems: Vec<String>,
    /// Ids whose file exists but gave no theme, so a slot naming one needs no second message.
    pub broken: Vec<String>,
}

impl Catalog {
    pub const fn new() -> Self {
        Self {
            themes: Vec::new(),
            problems: Vec::new(),
            broken: Vec::new(),
        }
    }

    /// Builds the catalog from `(file name, content)` pairs.
    pub fn build(mut files: Vec<(String, FileContent)>) -> Self {
        files.sort_by(|a, b| a.0.cmp(&b.0));
        let mut catalog = Self::new();
        if files.len() > MAX_THEMES {
            catalog
                .problems
                .push(format!("Only the first {MAX_THEMES} theme files are read"));
            files.truncate(MAX_THEMES);
        }
        for (file, content) in files {
            catalog.add(&file, content);
        }
        catalog
    }

    fn add(&mut self, file: &str, content: FileContent) {
        let id = file.strip_suffix(".toml").unwrap_or(file);
        if !is_custom_id(id) {
            let reserved: Vec<&str> = BUILTINS.iter().map(|b| b.0).collect();
            self.problems.push(format!(
                "{file}: a theme file name uses a-z, 0-9 and -; these names are reserved: {}",
                reserved.join(", ")
            ));
            return;
        }
        let refused = |catalog: &mut Self, message: String| {
            catalog.problems.push(message);
            catalog.broken.push(id.to_owned());
        };
        let text = match content {
            FileContent::Text(text) if text.len() <= MAX_THEME_BYTES => text,
            FileContent::Text(_) | FileContent::TooLarge => {
                return refused(
                    self,
                    format!("{file}: larger than 64 KiB, so it is not read"),
                );
            }
            FileContent::Unreadable => {
                return refused(self, format!("{file}: the file could not be read"));
            }
        };
        match parse(id, &text) {
            Ok(parsed) => {
                self.problems
                    .extend(parsed.reports.iter().map(|r| located(file, r)));
                self.themes.push(parsed.theme);
            }
            Err(report) => refused(self, located(file, &report)),
        }
    }

    fn find(&self, id: &str) -> Option<&ResolvedTheme> {
        self.themes.iter().find(|t| t.id == id)
    }
}

fn located(file: &str, report: &Report) -> String {
    match report.line {
        Some(line) => format!("{file} line {line}: {}", report.message),
        None => format!("{file}: {}", report.message),
    }
}

/// The contrast Notice text of a Custom theme, if any pair falls below WCAG 2.2 AA (R-033).
pub fn contrast_message(theme: &ResolvedTheme) -> Option<String> {
    if theme.builtin {
        return None;
    }
    let failures = contrast_failures(&theme.palette);
    let first = failures.first()?;
    let more = match failures.len() {
        1 => String::new(),
        n => format!(" ({} more)", n - 1),
    };
    Some(format!(
        "{}.toml: {} is {:.1}:1, below {}:1{more}",
        theme.id, first.pair, first.ratio, first.minimum
    ))
}

/// Clears the font families `installed` does not know, so the embedded fonts are used, and says which
/// ones (R-030).
pub fn check_fonts(theme: &mut ResolvedTheme, installed: impl Fn(&str) -> bool) -> Vec<String> {
    let id = theme.id.clone();
    let mut messages = Vec::new();
    for family in [&mut theme.fonts.interface, &mut theme.fonts.mono] {
        if let Some(name) = family.as_deref().filter(|name| !installed(name)) {
            messages.push(format!(
                "{id}.toml: the font {name} is not installed; using the embedded font"
            ));
            *family = None;
        }
    }
    messages
}

/// The Theme of the slot for Windows mode `mode`, and what the Owner must be told about it. A slot that
/// names no usable Theme gives the Built-in theme of that mode.
pub fn resolve(slots: &ThemeSlots, mode: Theme, catalog: &Catalog) -> (ResolvedTheme, Vec<String>) {
    let (slot, name) = match mode {
        Theme::Paper => ("light", slots.light.as_str()),
        Theme::Stone => ("dark", slots.dark.as_str()),
    };
    let found = find(name, catalog);
    let mut messages = Vec::new();
    let theme = found.unwrap_or_else(|| {
        if !catalog.broken.iter().any(|id| id == name) {
            messages.push(format!(
                "Theme slot {slot} names {name}, which does not exist; using {}",
                base_name(mode)
            ));
        }
        builtin(mode)
    });
    messages.extend(contrast_message(&theme));
    (theme, messages)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Settings;

    const KEYS: [&str; 14] = [
        "window",
        "raised",
        "selected",
        "well",
        "text",
        "secondary",
        "selected_text",
        "muted",
        "accent",
        "focus",
        "danger",
        "success",
        "hair",
        "strong",
    ];

    fn catalog(files: &[(&str, &str)]) -> Catalog {
        Catalog::build(
            files
                .iter()
                .map(|(name, text)| ((*name).to_owned(), FileContent::Text((*text).to_owned())))
                .collect(),
        )
    }

    fn slots(light: &str, dark: &str) -> ThemeSlots {
        ThemeSlots {
            light: light.into(),
            dark: dark.into(),
            schedule: None,
        }
    }

    // @R-030
    #[test]
    fn r030_theme_overrides_every_token() {
        for key in KEYS {
            let text = format!("base = \"paper\"\n[colors]\n{key} = \"#010203\"\n");
            let parsed = parse("t", &text).expect("parses");
            assert!(parsed.reports.is_empty(), "{key}: {:?}", parsed.reports);
            let mut expected = PAPER;
            *token(&mut expected, key).expect("known key").0 = Rgba::rgb(0x010203);
            assert_eq!(parsed.theme.palette, expected, "{key}");
        }
    }

    #[test]
    fn a_theme_reads_shape_density_fonts_and_its_name() {
        let text = "name = \"Harbor\"\nbase = \"stone\"\n[shape]\nwindow = 4\nrow = 6\nchip = 3\ncontrol = 12\n[list]\ndensity = \"compact\"\n[fonts]\ninterface = \"Segoe UI\"\n";
        let t = parse("harbor", text).expect("parses").theme;
        assert_eq!(
            (t.name.as_str(), t.base, t.builtin),
            ("Harbor", Theme::Stone, false)
        );
        assert_eq!(
            t.shape,
            Shape {
                window: 4,
                row: 6,
                chip: 3,
                control: 12
            }
        );
        assert_eq!(t.density, Density::Compact);
        assert_eq!(t.density.row_height(), 36.0);
        assert_eq!(t.fonts.interface.as_deref(), Some("Segoe UI"));
    }

    #[test]
    fn density_reads_each_word_and_reports_only_a_wrong_value() {
        for (list, want, reported) in [
            ("", Density::Normal, false),
            ("density = \"normal\"\n", Density::Normal, false),
            ("density = \"comfortable\"\n", Density::Comfortable, false),
            ("density = 5\n", Density::Normal, true),
            ("density = \"tight\"\n", Density::Normal, true),
        ] {
            let parsed = parse("d", &format!("[list]\n{list}")).expect("parses");
            assert_eq!(parsed.theme.density, want, "{list}");
            assert_eq!(!parsed.reports.is_empty(), reported, "{list}");
        }
    }

    #[test]
    fn a_font_that_is_empty_too_long_or_has_a_control_character_is_refused() {
        let long = "a".repeat(MAX_FONT_CHARS + 1);
        for family in [" ", long.as_str(), "Segoe\u{7}UI"] {
            let text = format!("[fonts]\ninterface = {}\n", toml::Value::from(family));
            let parsed = parse("f", &text).expect("parses");
            assert_eq!(parsed.theme.fonts.interface, None, "{family:?}");
            assert_eq!(parsed.reports.len(), 1, "{family:?}");
        }
        let longest = "a".repeat(MAX_FONT_CHARS);
        let text = format!("[fonts]\ninterface = \"{longest}\"\n");
        let parsed = parse("f", &text).expect("parses");
        assert_eq!(
            parsed.theme.fonts.interface.as_deref(),
            Some(longest.as_str())
        );
    }

    // @R-030
    #[test]
    fn r030_missing_font_falls_back_with_notice() {
        let text = "[fonts]\ninterface = \"Segoe UI\"\nmono = \"Gone Mono\"\n";
        let mut theme = parse("fonts", text).expect("parses").theme;
        let notes = check_fonts(&mut theme, |family| family == "Segoe UI");
        assert_eq!(theme.fonts.interface.as_deref(), Some("Segoe UI"));
        assert_eq!(theme.fonts.mono, None);
        assert_eq!(
            notes,
            ["fonts.toml: the font Gone Mono is not installed; using the embedded font"]
        );
    }

    // @R-032
    #[test]
    fn r032_default_slots_match_today() {
        let defaults = Settings::default().theme;
        let empty = Catalog::new();
        let (light, notes) = resolve(&defaults, Theme::Paper, &empty);
        assert_eq!(light.palette, PAPER);
        assert!(light.builtin && notes.is_empty());
        let (dark, _) = resolve(&defaults, Theme::Stone, &empty);
        assert_eq!(dark.palette, STONE);
    }

    // @R-032
    #[test]
    fn r032_mode_change_switches_slot() {
        let c = catalog(&[
            ("day.toml", "base = \"paper\""),
            ("night.toml", "base = \"stone\""),
        ]);
        let s = slots("day", "night");
        assert_eq!(resolve(&s, Theme::Paper, &c).0.id, "day");
        assert_eq!(resolve(&s, Theme::Stone, &c).0.id, "night");
    }

    // @R-032
    #[test]
    fn r032_same_theme_in_both_slots_does_not_change() {
        let c = catalog(&[("one.toml", "base = \"stone\"")]);
        let s = slots("one", "one");
        assert_eq!(
            resolve(&s, Theme::Paper, &c).0,
            resolve(&s, Theme::Stone, &c).0
        );
    }

    // @R-032
    #[test]
    fn r032_new_builtins_fill_a_slot_without_notice() {
        let empty = Catalog::new();
        for b in builtins() {
            let s = slots(&b.id, &b.id);
            for mode in [Theme::Paper, Theme::Stone] {
                let (t, notes) = resolve(&s, mode, &empty);
                assert_eq!((t.id.as_str(), t.palette), (b.id.as_str(), b.palette));
                assert!(t.builtin && notes.is_empty(), "{}", b.id);
                assert_eq!(contrast_message(&t), None, "{}", b.id);
            }
        }
        for (light, dark, expect) in [
            ("tokyo-haze", "neon-night", (TOKYO_HAZE, NEON_NIGHT)),
            ("washi", "deep-sea", (WASHI, DEEP_SEA)),
        ] {
            let parsed = Settings::parse(&format!(
                "[theme]
light = \"{light}\"
dark = \"{dark}\"
"
            ))
            .expect("parses");
            assert!(parsed.reports.is_empty(), "{:?}", parsed.reports);
            assert_eq!(parsed.settings.theme, slots(light, dark));
            let (l, _) = resolve(&parsed.settings.theme, Theme::Paper, &empty);
            let (d, _) = resolve(&parsed.settings.theme, Theme::Stone, &empty);
            assert_eq!((l.palette, d.palette), expect);
        }
    }

    // @R-032
    #[test]
    fn r032_builtin_registry_is_consistent() {
        let all = builtins();
        let ids: Vec<&str> = all.iter().map(|t| t.id.as_str()).collect();
        assert_eq!(
            ids,
            [
                "paper",
                "stone",
                "tokyo-haze",
                "neon-night",
                "washi",
                "sakura-dawn",
                "matcha",
                "dune",
                "midnight-ink",
                "kyoto-dusk",
                "deep-sea"
            ]
        );
        for (i, t) in all.iter().enumerate() {
            let light = i == 0 || (4..=7).contains(&i);
            assert_eq!(
                t.base,
                if light { Theme::Paper } else { Theme::Stone },
                "{}",
                t.id
            );
        }
        assert_eq!(all[0], builtin(Theme::Paper));
        assert_eq!(all[1], builtin(Theme::Stone));
        for (i, t) in all.iter().enumerate() {
            assert!(t.builtin, "{}", t.id);
            assert!(
                t.id.bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
                    && !t.id.is_empty()
                    && t.id.len() <= MAX_ID_CHARS,
                "{}",
                t.id
            );
            assert!(all[..i].iter().all(|o| o.id != t.id), "duplicate {}", t.id);
            assert!(is_theme_name(&t.id) && !is_custom_id(&t.id), "{}", t.id);
        }
    }

    // @D-043
    #[test]
    fn d043_new_builtin_ids_are_reserved_against_files() {
        for id in builtins().iter().map(|b| b.id.clone()) {
            let file = format!("{id}.toml");
            let c = catalog(&[(file.as_str(), "")]);
            assert!(c.themes.is_empty(), "{id}");
            assert_eq!(c.problems.len(), 1, "{:?}", c.problems);
            assert!(
                c.problems[0].starts_with(&format!("{file}:")),
                "{:?}",
                c.problems
            );
            assert!(c.problems[0].contains("reserved"), "{:?}", c.problems);
            for b in builtins() {
                assert!(c.problems[0].contains(&b.id), "{}: {:?}", b.id, c.problems);
            }
        }
    }

    // @R-033
    #[test]
    fn r033_invalid_values_use_base_and_name_the_line() {
        let c = catalog(&[(
            "bad.toml",
            "base = \"stone\"\n\n[colors]\naccent = \"red\"\nwindow = \"#11223344\"\n[shape]\nrow = 99\n",
        )]);
        let t = &c.themes[0];
        assert_eq!(t.palette, STONE);
        assert_eq!(t.shape, Shape::default());
        assert!(
            c.problems.iter().any(|m| m.starts_with("bad.toml line 4:")),
            "{:?}",
            c.problems
        );
        assert!(c.problems.iter().any(|m| m.starts_with("bad.toml line 5:")));
        assert!(c.problems.iter().any(|m| m.starts_with("bad.toml line 7:")));
    }

    // @R-033
    #[test]
    fn r033_broken_theme_uses_builtin_with_notice() {
        let c = catalog(&[("bad.toml", "[colors\naccent =")]);
        assert!(c.themes.is_empty());
        assert!(
            c.problems[0].starts_with("bad.toml line 1:"),
            "{:?}",
            c.problems
        );
        let (theme, notes) = resolve(&slots("bad", "stone"), Theme::Paper, &c);
        assert_eq!(theme.id, "paper");
        assert!(notes.is_empty(), "the file's own Notice says it: {notes:?}");
    }

    // @R-033
    #[test]
    fn r033_missing_slot_theme_uses_builtin() {
        let (theme, notes) = resolve(&slots("paper", "gone"), Theme::Stone, &Catalog::new());
        assert_eq!(theme.id, "stone");
        assert_eq!(
            notes,
            ["Theme slot dark names gone, which does not exist; using stone"]
        );
    }

    #[test]
    fn a_theme_name_is_trimmed_and_limited_to_40_characters() {
        let name = |text: &str| parse("harbor", &format!("name = {text}\n")).expect("parses");
        let longest = "n".repeat(MAX_NAME_CHARS);
        let ok = name(&format!("\"{longest}\""));
        assert_eq!(ok.theme.name, longest);
        assert!(ok.reports.is_empty());
        assert_eq!(name("\"  Harbor  \"").theme.name, "Harbor");
        for bad in [" ".to_owned(), "n".repeat(MAX_NAME_CHARS + 1)] {
            let parsed = name(&format!("\"{bad}\""));
            assert_eq!(parsed.theme.name, "harbor", "{bad:?}");
            assert_eq!(parsed.reports.len(), 1, "{bad:?}");
        }
    }

    #[test]
    fn exactly_the_most_themes_are_read_without_a_notice() {
        let files = (0..MAX_THEMES)
            .map(|i| (format!("t{i:03}.toml"), FileContent::Text(String::new())))
            .collect();
        let c = Catalog::build(files);
        assert_eq!(c.themes.len(), MAX_THEMES);
        assert!(c.problems.is_empty(), "{:?}", c.problems);
    }

    // @R-033
    #[test]
    fn r033_low_contrast_applies_with_a_named_notice() {
        let c = catalog(&[(
            "dim.toml",
            "base = \"paper\"\n[colors]\ntext = \"#FAF8F2\"\n",
        )]);
        let (theme, notes) = resolve(&slots("dim", "stone"), Theme::Paper, &c);
        assert_eq!(theme.id, "dim", "it still applies");
        assert!(
            notes[0].starts_with("dim.toml: primary on window is 1.0:1, below 4.5:1"),
            "{notes:?}"
        );
    }

    // @R-033
    #[test]
    fn r033_oversized_theme_is_refused_before_parsing() {
        let c = Catalog::build(vec![
            ("big.toml".into(), FileContent::TooLarge),
            (
                "long.toml".into(),
                FileContent::Text("#".repeat(MAX_THEME_BYTES + 1)),
            ),
        ]);
        assert!(c.themes.is_empty());
        assert_eq!(c.broken, ["big", "long"]);
        assert!(c.problems[0].contains("64 KiB"));
    }

    // @R-033
    #[test]
    fn r033_no_more_than_the_limit_of_themes_is_read() {
        let files = (0..MAX_THEMES + 1)
            .map(|i| (format!("t{i:03}.toml"), FileContent::Text(String::new())))
            .collect();
        let c = Catalog::build(files);
        assert_eq!(c.themes.len(), MAX_THEMES);
        assert_eq!(c.problems.len(), 1);
    }

    #[test]
    fn a_written_theme_reads_back_the_same() {
        let mut t = builtin(Theme::Stone);
        t.id = "copy".into();
        t.name = "My \"copy\"".into();
        t.builtin = false;
        t.density = Density::Comfortable;
        t.shape.row = 7;
        t.fonts.mono = Some("Cascadia Mono".into());
        let parsed = parse("copy", &to_toml(&t, &t.name)).expect("parses");
        assert!(parsed.reports.is_empty(), "{:?}", parsed.reports);
        assert_eq!(parsed.theme, t);
    }

    // @D-043
    #[test]
    fn d043_theme_id_rejects_paths_and_reserved_names() {
        for bad in builtins()
            .iter()
            .map(|b| b.id.as_str())
            .chain(["", "..", r"..\x", "a/b", "A", "a b", "a.b"])
        {
            assert!(!is_custom_id(bad), "{bad}");
        }
        assert!(!is_custom_id(&"x".repeat(41)));
        assert!(is_custom_id("harbor-2"));
        assert!(is_theme_name("paper") && is_theme_name("harbor-2") && !is_theme_name(r"..\x"));
        let c = catalog(&[("Bad Name.toml", "")]);
        assert!(c.themes.is_empty() && c.problems[0].starts_with("Bad Name.toml:"));
        let parsed = Settings::parse("[theme]\nlight = \"../x\"\n").expect("parses");
        assert_eq!(parsed.settings.theme.light, "paper");
        assert_eq!(parsed.reports.len(), 1);
    }
}
