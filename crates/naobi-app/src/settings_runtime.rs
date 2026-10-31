//! Settings load and hot reload (D-009). A valid file applies. A file that does not parse keeps the last
//! valid settings and raises a Notice that names the line. Reads are capped (T-022).
use std::fs::{self, File, OpenOptions};
use std::io::{self, ErrorKind, Read, Write};
use std::path::{Path, PathBuf};

use naobi_core::error::SettingsError;
use naobi_core::notice::{Notice, NoticeKind};
use naobi_core::settings::{Settings, MAX_FILE_BYTES};

pub const FILE_NAME: &str = "settings.toml";

/// First run at start: creates `dir/settings.toml` from the commented defaults when it is missing,
/// then reads it. Returns the path (for the watch), the settings to use and the Notices to raise.
pub fn startup(dir: &Path) -> io::Result<(PathBuf, Settings, Vec<Notice>)> {
    fs::create_dir_all(dir)?;
    let path = dir.join(FILE_NAME);
    match OpenOptions::new().write(true).create_new(true).open(&path) {
        Ok(mut file) => file.write_all(Settings::default_toml().as_bytes())?,
        Err(e) if e.kind() == ErrorKind::AlreadyExists => {}
        Err(e) => return Err(e),
    }
    let (settings, notices) = reload(&path, &Settings::default());
    Ok((path, settings, notices))
}

/// The settings to use after the file at `path` changed, and the Notices to raise. A missing or
/// unreadable file keeps `last` without a Notice, since an editor's save can remove it for a moment.
pub fn reload(path: &Path, last: &Settings) -> (Settings, Vec<Notice>) {
    let Some(text) = read_capped(path) else {
        return (last.clone(), Vec::new());
    };
    match Settings::parse(&text) {
        Ok(parsed) => {
            let notices = parsed
                .reports
                .iter()
                .map(|r| notice(r.line, &r.message, NoticeKind::Warning))
                .collect();
            (parsed.settings, notices)
        }
        Err(e) => {
            let line = match &e {
                SettingsError::Syntax { line, .. } => *line,
                SettingsError::TooLarge => None,
            };
            (
                last.clone(),
                vec![notice(line, &e.to_string(), NoticeKind::Error)],
            )
        }
    }
}

fn notice(line: Option<usize>, message: &str, kind: NoticeKind) -> Notice {
    let text = match line {
        Some(n) => format!("{FILE_NAME} line {n}: {message}"),
        None => format!("{FILE_NAME}: {message}"),
    };
    Notice {
        subject: FILE_NAME.into(),
        message: text,
        kind,
    }
}

/// Reads at most one byte over the cap, so an oversized file is caught without loading it whole.
fn read_capped(path: &Path) -> Option<String> {
    let mut bytes = Vec::new();
    File::open(path)
        .and_then(|f| f.take(MAX_FILE_BYTES as u64 + 1).read_to_end(&mut bytes))
        .ok()?;
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A folder for one test, emptied first and removed on drop, even when an assert fails.
    struct Dir(PathBuf);

    impl Dir {
        fn new(name: &str) -> Self {
            let p = std::env::temp_dir().join(format!("naobi-{name}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&p);
            Dir(p)
        }
    }

    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn d009_hot_reload_applies_valid_and_notices_invalid() {
        let dir = Dir::new("d009");
        let (path, first, notices) = startup(&dir.0).expect("first run");
        assert_eq!(
            fs::read_to_string(&path).expect("created"),
            Settings::default_toml(),
            "a missing file is created from the defaults"
        );
        assert!(notices.is_empty());
        assert!(!first.general.autostart);

        // A valid edit applies.
        fs::write(&path, "[general]\nautostart = true\n").expect("write");
        let (applied, notices) = reload(&path, &first);
        assert!(applied.general.autostart);
        assert!(notices.is_empty());

        // An invalid edit keeps the last valid settings and names the line.
        fs::write(
            &path,
            "[general]\nautostart = true\n[ui\ntext_scale = 1.0\n",
        )
        .expect("write");
        let (kept, notices) = reload(&path, &applied);
        assert_eq!(kept, applied);
        assert_eq!(notices.len(), 1);
        assert_eq!(notices[0].kind, NoticeKind::Error);
        assert!(
            notices[0].message.starts_with("settings.toml line 3:"),
            "{}",
            notices[0].message
        );
    }

    /// A file of exactly `len` bytes, valid TOML whose last byte matters: the cut-off last byte of a
    /// truncated read turns `true` into `tru`, a syntax error.
    fn capped_file(len: usize) -> String {
        const TAIL: &str = "[general]\nautostart = true";
        let pad = len - TAIL.len() - 2; // "#", the pad, then the newline before TAIL
        format!("#{}\n{TAIL}", "x".repeat(pad))
    }

    #[test]
    fn startup_keeps_an_existing_file_byte_identical() {
        let dir = Dir::new("existing");
        fs::create_dir_all(&dir.0).expect("dir");
        let path = dir.0.join(FILE_NAME);
        let owner_text =
            "# Owner's own file\n[general]\nautostart = true\n[ui]\ntext_scale = 1.25\n";
        fs::write(&path, owner_text).expect("write");
        let (_, settings, notices) = startup(&dir.0).expect("startup");
        assert_eq!(
            fs::read(&path).expect("read"),
            owner_text.as_bytes(),
            "an existing file is never overwritten"
        );
        assert!(settings.general.autostart, "the file's values apply");
        assert!(notices.is_empty());
    }

    /// Undoes the deny entry on drop, so the folder can be removed even when an assert fails.
    #[cfg(windows)]
    struct DenyCreate(PathBuf, String);

    #[cfg(windows)]
    impl Drop for DenyCreate {
        fn drop(&mut self) {
            let _ = std::process::Command::new("icacls")
                .arg(&self.0)
                .args(["/remove:d", &self.1])
                .output();
        }
    }

    /// A create that fails for a reason other than "already exists" must reach the caller, not be
    /// swallowed. The folder denies this user file creation (a deny ACE on the temp folder only).
    #[cfg(windows)]
    #[test]
    fn startup_reports_a_create_failure_other_than_exists() {
        let dir = Dir::new("denied");
        fs::create_dir_all(&dir.0).expect("dir");
        let user = std::env::var("USERNAME").expect("USERNAME");
        let status = std::process::Command::new("icacls")
            .arg(&dir.0)
            .args(["/deny", &format!("{user}:(WD)")])
            .output()
            .expect("icacls runs");
        assert!(
            status.status.success(),
            "icacls could not deny file creation"
        );
        let _undo = DenyCreate(dir.0.clone(), user);
        let err = startup(&dir.0).expect_err("the create is denied");
        assert_ne!(err.kind(), ErrorKind::AlreadyExists);
    }

    #[test]
    fn a_file_of_exactly_the_cap_is_read_in_full() {
        let dir = Dir::new("cap-exact");
        fs::create_dir_all(&dir.0).expect("dir");
        let path = dir.0.join(FILE_NAME);
        fs::write(&path, capped_file(MAX_FILE_BYTES)).expect("write");
        let (settings, notices) = reload(&path, &Settings::default());
        assert!(notices.is_empty(), "{notices:?}");
        assert!(settings.general.autostart, "the last byte was read");
    }

    #[test]
    fn a_file_one_byte_over_the_cap_is_rejected_with_a_notice() {
        let dir = Dir::new("cap-over");
        fs::create_dir_all(&dir.0).expect("dir");
        let path = dir.0.join(FILE_NAME);
        fs::write(&path, capped_file(MAX_FILE_BYTES + 1)).expect("write");
        let last = Settings::default();
        let (kept, notices) = reload(&path, &last);
        assert_eq!(kept, last, "the last valid settings stay");
        assert_eq!(notices.len(), 1);
        assert_eq!(notices[0].kind, NoticeKind::Error);
        assert!(
            notices[0].message.contains("larger than 256 KiB"),
            "{}",
            notices[0].message
        );
    }
}
