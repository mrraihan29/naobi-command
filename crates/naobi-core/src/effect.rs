//! Side effects a command asks the app to perform.
use crate::ids::{EntryId, HotkeyId, ViewId};
use crate::notice::Notice;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LaunchTarget {
    /// A path, or `shell:AppsFolder\<AUMID>`.
    ShellItem(String),
    Url(String),
    /// A command line the Owner typed after `>` (R-024): runs in a new PowerShell console window that stays open.
    Console(String),
    /// A GUI-subsystem program named by the first token of a `>` line: started directly, `args` as typed.
    Program {
        path: String,
        args: String,
    },
}

impl LaunchTarget {
    /// For a URL that is not http or https, the scheme that was refused (T-006). Items and
    /// quicklinks are the only source of targets, and a quicklink is a web search.
    pub fn refused_scheme(&self) -> Option<String> {
        let Self::Url(url) = self else { return None };
        let url = url.trim_start();
        let lower = url.to_ascii_lowercase();
        if lower.starts_with("http://") || lower.starts_with("https://") {
            return None;
        }
        let scheme = url.split([':', '/']).next().unwrap_or("");
        Some(if scheme.is_empty() || scheme.len() > 20 {
            "(none)".to_owned()
        } else {
            scheme.to_ascii_lowercase()
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    Launch(LaunchTarget),
    Hide,
    CopyText(String),
    Notice(Notice),
    PushView(ViewId),
    Paste(EntryId),
    /// Puts a clipboard entry on the clipboard without pasting it (Ctrl+Enter in the history).
    CopyEntry(EntryId),
    /// Runs a module's hotkey handlers once the Launcher is hidden and focus is back, so a command
    /// from the list does what its key does to the window that was in front.
    RunHotkey(HotkeyId),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url(u: &str) -> LaunchTarget {
        LaunchTarget::Url(u.into())
    }

    #[test]
    fn only_http_and_https_urls_are_allowed() {
        assert_eq!(url("https://naobi.dev/work/").refused_scheme(), None);
        assert_eq!(url("  HTTP://example.com").refused_scheme(), None);
        assert_eq!(
            url("file:///C:/Windows/System32/calc.exe").refused_scheme(),
            Some("file".into())
        );
        assert_eq!(
            url("JavaScript:alert(1)").refused_scheme(),
            Some("javascript".into())
        );
        assert_eq!(
            url("ms-settings:display").refused_scheme(),
            Some("ms-settings".into())
        );
        assert_eq!(url("httpx://x").refused_scheme(), Some("httpx".into()));
        assert_eq!(url("").refused_scheme(), Some("(none)".into()));
        assert_eq!(
            url(r"C:\Windows\notepad.exe").refused_scheme(),
            Some("c".into())
        );
        assert_eq!(
            LaunchTarget::ShellItem("anything".into()).refused_scheme(),
            None
        );
        // A command line never reaches the URL scheme check (R-024).
        assert_eq!(
            LaunchTarget::Console("javascript:alert(1)".into()).refused_scheme(),
            None
        );
        assert_eq!(
            LaunchTarget::Program {
                path: r"C:\Windows\notepad.exe".into(),
                args: "file:///x".into()
            }
            .refused_scheme(),
            None
        );
    }
}
