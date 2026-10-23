//! Where settings and data live: %APPDATA%\NaobiCommand and %LOCALAPPDATA%\NaobiCommand.
use std::ffi::OsString;
use std::path::PathBuf;

const APP_DIR: &str = "NaobiCommand";

/// `override_dir` wins when set (tests, portable runs); otherwise `<base>\NaobiCommand`.
pub fn resolve(override_dir: Option<OsString>, base: Option<OsString>) -> Option<PathBuf> {
    // Empty counts as unset: a relative path would land next to the cwd.
    let override_dir = override_dir.filter(|s| !s.is_empty());
    let base = base.filter(|s| !s.is_empty());
    match override_dir {
        Some(d) => Some(PathBuf::from(d)),
        None => base.map(|b| PathBuf::from(b).join(APP_DIR)),
    }
}

/// Override with `NAOBI_SETTINGS_DIR`.
pub fn settings_dir() -> Option<PathBuf> {
    resolve(
        std::env::var_os("NAOBI_SETTINGS_DIR"),
        std::env::var_os("APPDATA"),
    )
}

/// Override with `NAOBI_DATA_DIR`.
pub fn data_dir() -> Option<PathBuf> {
    resolve(
        std::env::var_os("NAOBI_DATA_DIR"),
        std::env::var_os("LOCALAPPDATA"),
    )
}
