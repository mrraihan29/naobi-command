//! The themes folder (ADR-0007): reads the Custom theme files into a catalog. Parsing and the rules
//! live in `naobi_core::theme_file`.
use naobi_core::theme_file::{Catalog, FileContent, MAX_THEMES, MAX_THEME_BYTES};
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};

pub const FOLDER: &str = "themes";

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
}
