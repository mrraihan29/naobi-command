//! The themes folder (ADR-0007): reads the Custom theme files into a catalog. Parsing and the rules
//! live in `naobi_core::theme_file`.
use super::Modules;
use naobi_core::command::{CommandSource, Cx, Outcome, Query};
use naobi_core::effect::{Effect, LaunchTarget};
use naobi_core::ids::{ActionId, ItemId, ViewId};
use naobi_core::item::{Action, Item, ItemKind};
use naobi_core::notice::{Notice, NoticeKind};
use naobi_core::settings::ThemeSlots;
use naobi_core::theme_file::{
    self, builtins, color_tokens, is_custom_id, Catalog, Density, FileContent, ResolvedTheme,
    MAX_THEMES, MAX_THEME_BYTES,
};
use naobi_core::tokens::contrast_failures;
use naobi_core::ui::Theme;
use naobi_core::view_source::{Fact, Preview, Swatch, ViewSource};
use std::fs::{self, File, OpenOptions};
use std::io::{ErrorKind, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub const FOLDER: &str = "themes";
pub const VIEW_ID: &str = "themes";
const OPEN_ID: &str = "themes.open";
const ROW_PREFIX: &str = "theme:";

/// What the Themes view needs from the running app.
pub trait ThemeHost: Send + Sync {
    fn catalog(&self) -> Catalog;
    fn slots(&self) -> ThemeSlots;
    fn folder(&self) -> Option<PathBuf>;
    /// Reads the folder again, after a file was added.
    fn reload(&self);
}

struct AppThemes;

impl ThemeHost for AppThemes {
    fn catalog(&self) -> Catalog {
        crate::state().catalog.clone()
    }
    fn slots(&self) -> ThemeSlots {
        crate::state()
            .settings
            .as_ref()
            .map(|s| s.theme.clone())
            .unwrap_or_default()
    }
    fn folder(&self) -> Option<PathBuf> {
        naobi_core::paths::settings_dir().map(|dir| folder(&dir))
    }
    fn reload(&self) {
        if let Some(dir) = self.folder() {
            crate::reload_themes(&dir);
        }
    }
}

pub fn register(m: &mut Modules) {
    let host: Arc<dyn ThemeHost> = Arc::new(AppThemes);
    m.register_source(Box::new(ThemesCommand));
    m.register_view(Box::new(ThemesView { host }));
}

/// The root row that opens the Themes view.
struct ThemesCommand;

impl CommandSource for ThemesCommand {
    fn items(&self, _q: &Query) -> Arc<[Item]> {
        vec![Item {
            id: ItemId(OPEN_ID.into()),
            title: "Themes".into(),
            subtitle: String::new(),
            keywords: vec!["theme".into(), "appearance".into(), "colors".into()],
            alias: None,
            kind: ItemKind::Command,
            primary: ActionId("run".into()),
            secondary: None,
        }]
        .into()
    }
    fn actions(&self, _id: &ItemId) -> Vec<Action> {
        vec![action("run", "Open")]
    }
    fn run(&self, _id: &ItemId, _action: ActionId, _cx: &mut Cx) -> Outcome {
        Outcome::Effects(vec![Effect::PushView(ViewId(VIEW_ID.into()))])
    }
}

fn action(id: &str, label: &str) -> Action {
    Action {
        id: ActionId(id.into()),
        label: label.into(),
        shortcut: None,
        enabled: Ok(()),
    }
}

fn row_id(theme: &str) -> ItemId {
    ItemId(format!("{ROW_PREFIX}{theme}"))
}

fn failed(message: &str) -> Outcome {
    Outcome::Failed(Notice {
        subject: "Theme".into(),
        message: message.into(),
        kind: NoticeKind::Error,
    })
}

/// Writes a new Custom theme file from `theme` and returns its id. An existing file is never replaced.
fn duplicate(dir: &Path, theme: &ResolvedTheme) -> std::io::Result<String> {
    fs::create_dir_all(dir)?;
    let stem: String = theme.id.chars().take(30).collect();
    for n in 1..=MAX_THEMES {
        let id = match n {
            1 => format!("{stem}-copy"),
            _ => format!("{stem}-copy-{n}"),
        };
        let mut file = match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(dir.join(format!("{id}.toml")))
        {
            Ok(file) => file,
            Err(e) if e.kind() == ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        };
        let name: String = format!("{} copy", theme.name).chars().take(40).collect();
        file.write_all(theme_file::to_toml(theme, &name).as_bytes())?;
        return Ok(id);
    }
    Err(ErrorKind::AlreadyExists.into())
}

/// Every Theme with its live preview, and the actions to use and edit it (R-031).
pub struct ThemesView {
    host: Arc<dyn ThemeHost>,
}

impl ThemesView {
    fn all(&self) -> Vec<ResolvedTheme> {
        let mut themes = builtins();
        themes.extend(self.host.catalog().themes);
        themes
    }

    fn theme(&self, id: &ItemId) -> Option<ResolvedTheme> {
        let id = id.0.strip_prefix(ROW_PREFIX)?;
        theme_file::find(id, &self.host.catalog())
    }
}

/// The word at the right of a row: the slot it fills, or where it comes from.
fn row_note(theme: &ResolvedTheme, slots: &ThemeSlots) -> &'static str {
    match (theme.id == slots.light, theme.id == slots.dark) {
        (true, true) => "Light and dark",
        (true, false) => "Light",
        (false, true) => "Dark",
        _ if theme.builtin => "Built-in",
        _ => "Custom",
    }
}

fn facts(theme: &ResolvedTheme) -> Vec<Fact> {
    let fact = |label: &str, value: String| Fact {
        label: label.into(),
        value,
    };
    let base = match theme.base {
        Theme::Paper => "Paper",
        Theme::Stone => "Stone",
    };
    let density = match theme.density {
        Density::Compact => "Compact",
        Density::Normal => "Normal",
        Density::Comfortable => "Comfortable",
    };
    let fonts = format!(
        "{}, {}",
        theme.fonts.interface.as_deref().unwrap_or("Inter"),
        theme.fonts.mono.as_deref().unwrap_or("JetBrains Mono")
    );
    let contrast = match contrast_failures(&theme.palette).len() {
        0 => "Passes WCAG AA".to_owned(),
        1 => "1 pair below AA".to_owned(),
        n => format!("{n} pairs below AA"),
    };
    vec![
        fact("Base", base.to_owned()),
        fact("Density", density.to_owned()),
        fact("Fonts", fonts),
        fact("Contrast", contrast),
    ]
}

impl CommandSource for ThemesView {
    fn items(&self, q: &Query) -> Arc<[Item]> {
        let wanted = q.text.trim().to_lowercase();
        let slots = self.host.slots();
        self.all()
            .into_iter()
            .filter(|t| t.name.to_lowercase().contains(&wanted))
            .map(|t| Item {
                id: row_id(&t.id),
                subtitle: row_note(&t, &slots).to_owned(),
                title: t.name,
                keywords: Vec::new(),
                alias: None,
                kind: ItemKind::Theme,
                primary: ActionId("apply".into()),
                secondary: None,
            })
            .collect()
    }

    fn actions(&self, id: &ItemId) -> Vec<Action> {
        let mut actions = vec![
            action("apply", "Use theme"),
            action("duplicate", "Duplicate"),
        ];
        if self.theme(id).is_some_and(|t| !t.builtin) {
            actions.push(action("edit", "Edit file"));
        }
        actions
    }

    fn run(&self, id: &ItemId, act: ActionId, _cx: &mut Cx) -> Outcome {
        let Some(theme) = self.theme(id) else {
            return failed("That theme is no longer there.");
        };
        match act.0.as_str() {
            "apply" => Outcome::Stay {
                effects: vec![Effect::ApplyTheme(theme.id)],
                select: None,
            },
            "duplicate" => {
                let copied = self
                    .host
                    .folder()
                    .and_then(|dir| duplicate(&dir, &theme).ok());
                match copied {
                    Some(new_id) => {
                        self.host.reload();
                        Outcome::Stay {
                            effects: Vec::new(),
                            select: Some(row_id(&new_id)),
                        }
                    }
                    None => failed("The theme file could not be created."),
                }
            }
            // Only a Custom theme with a valid id has a file (T-042).
            "edit" if !theme.builtin && is_custom_id(&theme.id) => match self.host.folder() {
                Some(dir) => {
                    let path = dir.join(format!("{}.toml", theme.id));
                    Outcome::Effects(vec![Effect::Launch(LaunchTarget::ShellItem(
                        path.to_string_lossy().into_owned(),
                    ))])
                }
                None => failed("The themes folder is not known."),
            },
            _ => Outcome::Done,
        }
    }
}

impl ViewSource for ThemesView {
    fn view_id(&self) -> ViewId {
        ViewId(VIEW_ID.into())
    }

    fn title(&self) -> &'static str {
        "Themes"
    }

    fn opened_by_hotkey(&self) -> bool {
        false
    }

    fn ranked(&self) -> bool {
        false
    }

    fn placeholder(&self) -> &'static str {
        "Search themes..."
    }

    fn preview(&self, id: &ItemId) -> Option<Preview> {
        self.pane(id).map(|(preview, _)| preview)
    }

    fn pane(&self, id: &ItemId) -> Option<(Preview, Vec<Fact>)> {
        let theme = self.theme(id)?;
        let swatches = color_tokens(&theme.palette)
            .into_iter()
            .map(|(key, color)| Swatch {
                label: key.replace('_', " "),
                color,
            })
            .collect();
        Some((Preview::Swatches(swatches), facts(&theme)))
    }

    fn look(&self, id: &ItemId) -> Option<String> {
        self.theme(id).map(|t| t.id)
    }
}

/// The themes folder under the settings folder.
pub fn folder(settings_dir: &Path) -> PathBuf {
    settings_dir.join(FOLDER)
}

/// Reads at most one byte over the limit, so a huge file is refused without loading it (T-038).
fn read_capped(path: &Path) -> FileContent {
    let mut bytes = Vec::new();
    let read =
        File::open(path).and_then(|f| f.take(MAX_THEME_BYTES as u64 + 1).read_to_end(&mut bytes));
    match read {
        Err(_) => FileContent::Unreadable,
        Ok(n) if n > MAX_THEME_BYTES => FileContent::TooLarge,
        Ok(_) => String::from_utf8(bytes).map_or(FileContent::Unreadable, FileContent::Text),
    }
}

/// The catalog of `dir`. A missing folder is an empty catalog.
pub fn load(dir: &Path) -> Catalog {
    let mut names: Vec<String> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|name| name.ends_with(".toml"))
        .collect();
    names.sort();
    let extra = names.len().saturating_sub(MAX_THEMES);
    // One more than the limit is enough for the catalog to say that files were left out.
    names.truncate(MAX_THEMES + usize::from(extra > 0));
    let files = names
        .into_iter()
        .map(|name| {
            let content = read_capped(&dir.join(&name));
            (name, content)
        })
        .collect();
    let catalog = Catalog::build(files);
    tracing::info!("{}", log_line(&catalog));
    catalog
}

/// Counts only: theme and font names are the Owner's and stay out of the log (D-020, T-041).
fn log_line(catalog: &Catalog) -> String {
    format!(
        "themes loaded: {} usable, {} problems",
        catalog.themes.len(),
        catalog.problems.len()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index_worker;
    use naobi_core::db::Db;
    use naobi_core::tokens::TOKYO_HAZE;
    use naobi_core::ui::{Key, KeyChord, UiEvent};
    use naobi_launcher::frecency::Frecency;
    use naobi_launcher::{Clock, Launcher};
    use std::sync::{mpsc, Mutex};
    use std::time::Duration;

    /// Paper, Stone, Tokyo Haze and Neon Night come before the first Custom theme.
    const BUILTIN_ROWS: usize = 4;

    struct Dir(PathBuf);

    impl Dir {
        fn new(name: &str) -> Self {
            let p =
                std::env::temp_dir().join(format!("naobi-themes-{name}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&p);
            fs::create_dir_all(&p).expect("folder");
            Dir(p)
        }
    }

    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    // @R-030
    #[test]
    fn r030_the_folder_gives_one_theme_per_file() {
        let dir = Dir::new("load");
        fs::write(dir.0.join("harbor.toml"), "base = \"stone\"\n").expect("write");
        fs::write(dir.0.join("notes.txt"), "not a theme").expect("write");
        fs::create_dir(dir.0.join("sub.toml")).expect("folder");
        let c = load(&dir.0);
        assert_eq!(c.themes.len(), 1);
        assert_eq!(c.themes[0].id, "harbor");
        assert!(c.problems.is_empty());
    }

    // @R-033
    #[test]
    fn r033_a_file_over_the_limit_is_not_read_whole() {
        let dir = Dir::new("big");
        let path = dir.0.join("big.toml");
        fs::write(&path, vec![b'#'; MAX_THEME_BYTES + 10]).expect("write");
        assert_eq!(read_capped(&path), FileContent::TooLarge);
        assert!(load(&dir.0).themes.is_empty());
    }

    #[test]
    fn a_missing_folder_is_an_empty_catalog() {
        let c = load(Path::new(r"C:\no\such\naobi\themes"));
        assert!(c.themes.is_empty() && c.problems.is_empty());
    }

    // @D-020
    #[test]
    fn d020_theme_reload_logs_no_names() {
        let c = Catalog::build(vec![(
            "secret-name.toml".into(),
            FileContent::Text("name = \"Private\"\n[fonts]\nmono = \"Hidden Font\"\n".into()),
        )]);
        let line = log_line(&c);
        for word in ["secret", "Private", "Hidden"] {
            assert!(!line.contains(word), "{line}");
        }
        assert_eq!(line, "themes loaded: 1 usable, 0 problems");
    }

    struct FakeHost {
        dir: PathBuf,
        catalog: Mutex<Catalog>,
    }

    impl ThemeHost for FakeHost {
        fn catalog(&self) -> Catalog {
            self.catalog.lock().expect("catalog").clone()
        }
        fn slots(&self) -> ThemeSlots {
            ThemeSlots::default()
        }
        fn folder(&self) -> Option<PathBuf> {
            Some(self.dir.clone())
        }
        fn reload(&self) {
            *self.catalog.lock().expect("catalog") = load(&self.dir);
        }
    }

    struct Zero;
    impl Clock for Zero {
        fn now_ms(&self) -> i64 {
            1_000_000
        }
    }

    fn engine(dir: &Path) -> Launcher {
        let host = Arc::new(FakeHost {
            dir: dir.to_owned(),
            catalog: Mutex::new(load(dir)),
        });
        let db = Db::open_in_memory().expect("db");
        Launcher::new(
            vec![Box::new(ThemesCommand)],
            vec![Box::new(ThemesView { host })],
            Frecency::open(db, 0).expect("frecency"),
            Box::new(Zero),
        )
    }

    fn press(l: &mut Launcher, key: Key, ctrl: bool) -> Vec<Effect> {
        l.handle(UiEvent::Key(KeyChord {
            key,
            ctrl,
            alt: false,
            shift: false,
        }))
    }

    fn open(l: &mut Launcher) {
        l.handle(UiEvent::TextChanged("themes".into()));
        assert_eq!(l.screen().sections[0].rows[0].title, "Themes");
        press(l, Key::Enter, false);
        assert_eq!(l.screen().view.name, "Themes");
    }

    fn run_panel_action(l: &mut Launcher, label: &str) -> Vec<Effect> {
        press(l, Key::K, true);
        l.handle(UiEvent::PanelFilter(label.into()));
        press(l, Key::Enter, false)
    }

    // @R-031
    #[test]
    fn r031_selection_previews_and_esc_restores() {
        let dir = Dir::new("preview");
        let mut l = engine(&dir.0);
        assert_eq!(l.screen().theme_preview, None);
        open(&mut l);
        assert_eq!(l.screen().theme_preview.as_deref(), Some("paper"));
        press(&mut l, Key::Down, false);
        let s = l.screen();
        assert_eq!(s.theme_preview.as_deref(), Some("stone"));
        assert_eq!(s.sections[0].rows[1].right.as_deref(), Some("Dark"));
        assert_eq!(s.facts[0].value, "Stone");
        press(&mut l, Key::Escape, false);
        assert_eq!(l.screen().theme_preview, None, "the applied theme returns");
    }

    // @R-031
    #[test]
    fn r031_enter_keeps_the_view_open_and_asks_for_the_theme() {
        let dir = Dir::new("enter");
        let mut l = engine(&dir.0);
        open(&mut l);
        press(&mut l, Key::Down, false);
        assert_eq!(
            press(&mut l, Key::Enter, false),
            [Effect::ApplyTheme("stone".into())]
        );
        assert_eq!(l.screen().view.name, "Themes");
    }

    // @R-031
    #[test]
    fn r031_duplicate_creates_and_selects_a_theme() {
        let dir = Dir::new("duplicate");
        let mut l = engine(&dir.0);
        open(&mut l);
        assert!(run_panel_action(&mut l, "Duplicate").is_empty());
        assert!(dir.0.join("paper-copy.toml").is_file());
        let s = l.screen();
        assert_eq!(s.theme_preview.as_deref(), Some("paper-copy"));
        assert_eq!(s.sections[0].rows[BUILTIN_ROWS].title, "Paper copy");
        // A second copy never replaces the first.
        for _ in 0..BUILTIN_ROWS {
            press(&mut l, Key::Up, false);
        }
        assert!(run_panel_action(&mut l, "Duplicate").is_empty());
        assert!(dir.0.join("paper-copy-2.toml").is_file());
    }

    // @R-031
    #[test]
    fn r031_edit_file_opens_only_the_theme_path() {
        let dir = Dir::new("edit");
        fs::write(dir.0.join("harbor.toml"), "base = \"stone\"\n").expect("write");
        let mut l = engine(&dir.0);
        open(&mut l);
        for _ in 0..BUILTIN_ROWS {
            press(&mut l, Key::Down, false);
        }
        let expected = dir.0.join("harbor.toml").to_string_lossy().into_owned();
        assert_eq!(
            run_panel_action(&mut l, "Edit"),
            [
                Effect::Launch(LaunchTarget::ShellItem(expected)),
                Effect::Hide
            ]
        );
    }

    // @R-032
    #[test]
    fn r032_builtin_theme_has_no_edit_action() {
        let dir = Dir::new("builtin");
        let mut l = engine(&dir.0);
        open(&mut l);
        press(&mut l, Key::K, true);
        let labels: Vec<String> = l
            .screen()
            .action_panel
            .expect("panel")
            .actions
            .into_iter()
            .map(|a| a.label)
            .collect();
        assert_eq!(labels, ["Use theme", "Duplicate"]);
    }

    // @R-032
    #[test]
    fn r032_new_builtins_have_no_edit_action() {
        let dir = Dir::new("newedit");
        let mut l = engine(&dir.0);
        open(&mut l);
        for _ in 0..2 {
            press(&mut l, Key::Down, false);
        }
        for row in 2..BUILTIN_ROWS {
            press(&mut l, Key::K, true);
            let labels: Vec<String> = l
                .screen()
                .action_panel
                .expect("panel")
                .actions
                .into_iter()
                .map(|a| a.label)
                .collect();
            assert_eq!(labels, ["Use theme", "Duplicate"], "row {row}");
            press(&mut l, Key::Escape, false);
            press(&mut l, Key::Down, false);
        }
    }

    // @R-031
    #[test]
    fn r031_new_builtins_preview_apply_and_duplicate() {
        let dir = Dir::new("newbuiltins");
        let mut l = engine(&dir.0);
        open(&mut l);
        let rows: Vec<(String, Option<String>)> = l.screen().sections[0]
            .rows
            .iter()
            .map(|r| (r.title.clone(), r.right.clone()))
            .collect();
        let note = |t: &str, r: &str| (t.to_owned(), Some(r.to_owned()));
        assert_eq!(
            rows,
            [
                note("Paper", "Light"),
                note("Stone", "Dark"),
                note("Tokyo Haze", "Built-in"),
                note("Neon Night", "Built-in"),
            ]
        );
        for (id, effect) in [("tokyo-haze", "tokyo-haze"), ("neon-night", "neon-night")] {
            press(&mut l, Key::Down, false);
            press(&mut l, Key::Down, false);
            if id == "neon-night" {
                press(&mut l, Key::Down, false);
            }
            let s = l.screen();
            assert_eq!(s.theme_preview.as_deref(), Some(id));
            assert_eq!(s.facts[0].value, "Stone");
            assert_eq!(s.facts[3].value, "Passes WCAG AA");
            assert_eq!(
                press(&mut l, Key::Enter, false),
                [Effect::ApplyTheme(effect.into())]
            );
            press(&mut l, Key::Escape, false);
            assert_eq!(l.screen().theme_preview, None, "the applied theme returns");
            open(&mut l);
        }
        // Duplicate keeps the palette and names the family as base.
        press(&mut l, Key::Down, false);
        press(&mut l, Key::Down, false);
        assert!(run_panel_action(&mut l, "Duplicate").is_empty());
        let text = fs::read_to_string(dir.0.join("tokyo-haze-copy.toml")).expect("copy");
        assert!(text.starts_with(
            "name = \"Tokyo Haze copy\"
base = \"stone\"
"
        ));
        let parsed = theme_file::parse("tokyo-haze-copy", &text).expect("parses");
        assert!(parsed.reports.is_empty(), "{:?}", parsed.reports);
        assert_eq!(parsed.theme.palette, TOKYO_HAZE);
        assert_eq!(
            l.screen().sections[0].rows[BUILTIN_ROWS].title,
            "Tokyo Haze copy"
        );
    }

    // @R-031
    #[test]
    fn r031_saved_theme_applies_within_one_second() {
        let dir = Dir::new("watch");
        let (tx, rx) = mpsc::channel();
        let watched = dir.0.clone();
        index_worker::start(vec![index_worker::Job {
            dirs: vec![(dir.0.clone(), false)],
            initial: false,
            settle: None,
            run: Box::new(move || {
                let _ = tx.send(load(&watched));
            }),
        }])
        .expect("worker");
        fs::write(dir.0.join("harbor.toml"), "name = \"Harbor\"\n").expect("save");
        let catalog = rx.recv_timeout(Duration::from_secs(1)).expect("reloaded");
        assert_eq!(catalog.themes[0].name, "Harbor");
    }
}
