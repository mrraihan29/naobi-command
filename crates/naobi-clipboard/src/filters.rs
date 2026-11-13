//! What never enters the history: the Launcher's own writes, copies that ask not to be recorded,
//! and copies made by an ignored app.
use crate::ports::Captured;

/// The password manager the Owner uses (R-015 v2). The exe name is confirmed on the Owner's PC:
/// `ProtonPass.exe` under `%LOCALAPPDATA%\ProtonPass` (the plan said "Proton Pass.exe", the window title).
pub const DEFAULT_IGNORED: [&str; 1] = ["ProtonPass.exe"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Skip {
    /// The Launcher's own CopyText or paste (D-019).
    Marker,
    /// A privacy format asks that the copy is not kept (R-009).
    Excluded,
    /// The owner exe is on the ignore list (R-015).
    IgnoredApp,
    /// Nothing worth storing: empty or blank text, no files.
    Empty,
    /// Over a size limit (D-015).
    TooLarge,
    /// A bitmap this build cannot convert.
    Unsupported,
}

/// Why `c` must not be stored, or None when it may be. `ignored` holds exe file names.
pub fn skip_reason(c: &Captured, ignored: &[String]) -> Option<Skip> {
    if c.marker {
        return Some(Skip::Marker);
    }
    // Any of the three formats can veto: exclusion present, history refused, or cloud refused.
    let f = &c.flags;
    if f.exclude_from_monitor
        || f.can_include_in_history == Some(0)
        || f.can_upload_to_cloud == Some(0)
    {
        return Some(Skip::Excluded);
    }
    let owner = c.owner_exe.as_deref()?;
    ignored
        .iter()
        .any(|i| i.eq_ignore_ascii_case(owner))
        .then_some(Skip::IgnoredApp)
}
